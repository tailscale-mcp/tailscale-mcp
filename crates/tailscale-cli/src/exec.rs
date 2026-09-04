//! Spawning the `tailscale` binary.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::RwLock;

use crate::backend::{BoxFuture, Concurrency, Invocation, LocalBackend, Output};

/// How long a call may take before it is cut off.
///
/// Generous, because a few commands legitimately wait on the network, and the
/// tools that need longer say so.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a timed-out child is given to exit on its own before it is killed.
pub const GRACE_PERIOD: Duration = Duration::from_secs(2);

/// The environment variable that overrides binary discovery.
pub const BINARY_ENV: &str = "TAILSCALE_MCP_CLI_PATH";

/// Something went wrong before, or instead of, the command producing a result.
///
/// A command that ran and exited non-zero is not an error here: that is an
/// [`Output`] with a non-zero code, and the layer above turns it into the
/// `cli_failed` result.
#[derive(Debug, Error)]
pub enum ExecError {
    #[error("no `tailscale` binary found; looked at {}", .searched.join(", "))]
    BinaryNotFound { searched: Vec<String> },

    #[error("`{path}` was set as the CLI path but is not an executable file")]
    BinaryNotExecutable { path: String },

    #[error("could not start `{binary}`: {source}")]
    Spawn {
        binary: String,
        #[source]
        source: std::io::Error,
    },

    #[error("`{command}` did not finish within {}s", .timeout.as_secs())]
    Timeout {
        command: String,
        timeout: Duration,
        /// Whatever the child had printed by the time it was killed.
        ///
        /// A command that hangs usually says why first — `tailscale funnel`
        /// prints the URL that enables Funnel and then waits for someone to
        /// visit it — so the words are kept and handed to the caller. Empty
        /// when the child said nothing.
        printed: String,
    },

    #[error("failed talking to `{command}`: {source}")]
    Io {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("could not create a private file for a secret: {0}")]
    SecretFile(#[source] std::io::Error),
}

/// The real backend: finds the binary once, then spawns it per call.
#[derive(Debug)]
pub struct CliBackend {
    binary: PathBuf,
    /// Read-locked by shared calls, write-locked by exclusive ones. An
    /// `RwLock` says precisely what the design wants: reads overlap each other,
    /// a mutation overlaps nothing.
    lock: RwLock<()>,
}

impl CliBackend {
    /// Find the binary and build a backend around it.
    ///
    /// Order: the explicit override, then the search path, then the macOS
    /// application bundle. The override is first because an operator who names
    /// a path means it — if it is wrong, that is an error rather than a quiet
    /// fallback to some other Tailscale on the machine.
    pub fn discover() -> Result<Self, ExecError> {
        Self::discover_with(std::env::var_os(BINARY_ENV).as_deref())
    }

    /// [`Self::discover`], with the override supplied rather than read from the
    /// environment, so a test can drive every branch.
    pub fn discover_with(override_path: Option<&std::ffi::OsStr>) -> Result<Self, ExecError> {
        if let Some(path) = override_path.filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            if !is_executable_file(&path) {
                return Err(ExecError::BinaryNotExecutable {
                    path: path.display().to_string(),
                });
            }
            return Ok(Self::at(path));
        }

        let mut searched = Vec::new();
        for candidate in search_path_candidates().chain(bundle_candidates()) {
            if is_executable_file(&candidate) {
                return Ok(Self::at(candidate));
            }
            searched.push(candidate.display().to_string());
        }
        Err(ExecError::BinaryNotFound { searched })
    }

    /// A backend over a known binary. The stub-binary tests use this.
    pub fn at(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            lock: RwLock::new(()),
        }
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    async fn spawn(&self, invocation: Invocation) -> Result<Output, ExecError> {
        // Held for the whole call. Dropped on every exit path, including the
        // timeout, because it lives in this scope.
        let _guard = match invocation.concurrency {
            Concurrency::Shared => Guard::Shared(self.lock.read().await),
            Concurrency::Exclusive => Guard::Exclusive(self.lock.write().await),
        };

        let command = invocation.display();
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.args(&invocation.args)
            .env_clear()
            .envs(minimal_env())
            .stdin(if invocation.stdin.is_some() {
                Stdio::piped()
            } else {
                // Closed, so a command that would prompt fails instead of
                // hanging on a terminal that is not there.
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // If this future is dropped — a cancelled request, a shutdown —
            // the child does not outlive it.
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|source| ExecError::Spawn {
            binary: self.binary.display().to_string(),
            source,
        })?;

        let mut stdin_pipe = child.stdin.take();
        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();
        let stdin_bytes = invocation.stdin;

        // Owned out here rather than inside the reading futures, so that a
        // child killed for taking too long still leaves behind whatever it had
        // said. `read_to_end` fills the buffer as it goes, and cancelling it
        // keeps what it filled.
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();

        let collected = {
            let feed = async {
                if let (Some(pipe), Some(bytes)) = (stdin_pipe.as_mut(), stdin_bytes.as_ref()) {
                    pipe.write_all(bytes).await?;
                    pipe.shutdown().await?;
                }
                // Closing the pipe is what tells the child there is no more.
                drop(stdin_pipe.take());
                Ok::<(), std::io::Error>(())
            };
            let read_out = async {
                if let Some(pipe) = stdout_pipe.as_mut() {
                    pipe.read_to_end(&mut stdout_buf).await?;
                }
                Ok::<(), std::io::Error>(())
            };
            let read_err = async {
                if let Some(pipe) = stderr_pipe.as_mut() {
                    pipe.read_to_end(&mut stderr_buf).await?;
                }
                Ok::<(), std::io::Error>(())
            };

            let work = async {
                let (fed, out, err, status) = tokio::join!(feed, read_out, read_err, child.wait());
                fed?;
                out?;
                err?;
                status
            };

            tokio::time::timeout(invocation.timeout, work).await.ok()
        };

        match collected {
            Some(Ok(status)) => Ok(Output {
                exit_code: status.code(),
                stdout: stdout_buf,
                stderr: String::from_utf8_lossy(&stderr_buf).into_owned(),
            }),
            Some(Err(source)) => Err(ExecError::Io { command, source }),
            None => {
                terminate(&mut child).await;
                Err(ExecError::Timeout {
                    command,
                    timeout: invocation.timeout,
                    printed: printed(&stdout_buf, &stderr_buf),
                })
            }
        }
    }
}

impl LocalBackend for CliBackend {
    fn run<'a>(&'a self, invocation: Invocation) -> BoxFuture<'a, Result<Output, ExecError>> {
        Box::pin(self.spawn(invocation))
    }
}

/// Ask the child to stop, then insist.
///
/// The polite request matters: `tailscale` cleans up state on the way out, and
/// a killed process can leave a half-applied preference behind.
/// What a killed child had said, both streams together and in the order a
/// person reading a terminal would have seen them: standard output first,
/// since that is where the client puts the thing it wants acted on.
///
/// Kept short, because it goes into an error message rather than into a result.
fn printed(stdout: &[u8], stderr: &[u8]) -> String {
    const LIMIT: usize = 2_000;
    let mut out = String::new();
    for stream in [stdout, stderr] {
        let text = String::from_utf8_lossy(stream);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(text);
    }
    if out.len() > LIMIT {
        // On a character boundary, so the result is still a string.
        let end = (0..=LIMIT)
            .rev()
            .find(|i| out.is_char_boundary(*i))
            .unwrap_or(0);
        out.truncate(end);
        out.push('\u{2026}');
    }
    out
}

async fn terminate(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // A failure here means the child is already gone, which is the outcome
        // we wanted anyway.
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGTERM,
        );
        if tokio::time::timeout(GRACE_PERIOD, child.wait())
            .await
            .is_ok()
        {
            return;
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
}

/// Held for the duration of a call and never inspected: the lock is released
/// by dropping it, which is the whole point.
#[allow(dead_code)]
enum Guard<'a> {
    Shared(tokio::sync::RwLockReadGuard<'a, ()>),
    Exclusive(tokio::sync::RwLockWriteGuard<'a, ()>),
}

/// The environment the child gets: an allow-list, not the parent's.
///
/// Two reasons. Our own credentials — API keys, OAuth secrets — are in this
/// process's environment and have no business in a child that does not need
/// them. And `TS_DEBUG_*` and friends change the CLI's behaviour, so inheriting
/// whatever the launching shell happened to have makes the server's behaviour
/// depend on how it was started.
fn minimal_env() -> BTreeMap<OsString, OsString> {
    const KEEP: &[&str] = &[
        // Needed to find helpers and, on macOS, the app bundle.
        "PATH",
        // The CLI reads and writes per-user state.
        "HOME",
        "USER",
        "LOGNAME",
        // Where a secret file may live.
        "TMPDIR",
        // Windows cannot start a process without these.
        "SystemRoot",
        "SystemDrive",
        "COMSPEC",
        "PATHEXT",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramData",
        "ProgramFiles",
        "TEMP",
        "TMP",
        "windir",
    ];

    let mut env: BTreeMap<OsString, OsString> = KEEP
        .iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect();
    // Stable, parseable output regardless of the operator's locale.
    env.insert(OsString::from("LC_ALL"), OsString::from("C"));
    env
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// `tailscale` as found on `PATH`, resolved by hand so that the error message
/// can say where we looked.
fn search_path_candidates() -> impl Iterator<Item = PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["tailscale.exe"]
    } else {
        &["tailscale"]
    };
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.into_iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
}

/// Where the macOS applications put their CLI. Neither is on `PATH` unless the
/// app has been asked to install its symlink.
fn bundle_candidates() -> impl Iterator<Item = PathBuf> {
    let paths: &[&str] = if cfg!(target_os = "macos") {
        &[
            // The standalone build, and the App Store build's own copy.
            "/Applications/Tailscale.app/Contents/MacOS/tailscale",
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ]
    } else {
        &[]
    };
    let mut candidates: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if cfg!(target_os = "macos")
        && let Some(home) = std::env::var_os("HOME")
    {
        candidates
            .push(PathBuf::from(home).join("Applications/Tailscale.app/Contents/MacOS/tailscale"));
    }
    candidates.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_child_environment_is_an_allow_list() {
        let env = minimal_env();
        assert!(!env.contains_key(std::ffi::OsStr::new("TAILSCALE_API_KEY")));
        assert!(!env.contains_key(std::ffi::OsStr::new("TS_DEBUG_MUCK")));
        assert_eq!(
            env.get(std::ffi::OsStr::new("LC_ALL"))
                .map(|v| v.as_os_str()),
            Some(std::ffi::OsStr::new("C"))
        );
    }

    #[test]
    fn a_named_binary_that_is_not_there_is_an_error_not_a_fallback() {
        let err =
            CliBackend::discover_with(Some(std::ffi::OsStr::new("/definitely/not/here/tailscale")))
                .expect_err("a missing override must fail");
        assert!(
            matches!(err, ExecError::BinaryNotExecutable { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn an_empty_override_is_treated_as_unset() {
        // Falls through to discovery rather than failing on the empty path.
        let result = CliBackend::discover_with(Some(std::ffi::OsStr::new("")));
        match result {
            Ok(_) | Err(ExecError::BinaryNotFound { .. }) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn an_invocation_renders_without_a_shell_anywhere_near_it() {
        let inv = Invocation::read(["ping", "--c=1", "host with spaces"]);
        assert_eq!(inv.display(), "tailscale ping --c=1 host with spaces");
        assert_eq!(inv.args.len(), 3, "the arguments stay separate");
    }
}
