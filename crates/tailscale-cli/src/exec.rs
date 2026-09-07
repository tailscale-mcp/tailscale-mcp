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
    /// Order: the explicit override, then the search path, then the shim the
    /// macOS applications install, and last the executable inside the
    /// application bundle. The override is first because an operator who names
    /// a path means it — if it is wrong, that is an error rather than a quiet
    /// fallback to some other Tailscale on the machine. The bundle is last
    /// because it is the only candidate that can be present and not be a
    /// command-line interface; `bundle_candidates` says why.
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

        first_usable(candidates())
            .map(Self::at)
            .map_err(|searched| ExecError::BinaryNotFound { searched })
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
/// Two reasons. Our own credentials — API access tokens, OAuth secrets — are in
/// this process's environment and have no business in a child that does not need
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

/// The first candidate that can be used, or every place that was looked.
///
/// Takes the list rather than calling [`candidates`] so that a test can drive
/// both kinds of belief over stubs it controls; the real list is absolute paths
/// on this machine, which no test can arrange.
fn first_usable(
    candidates: impl IntoIterator<Item = (PathBuf, Believe)>,
) -> Result<PathBuf, Vec<String>> {
    let mut searched = Vec::new();
    for (candidate, believe) in candidates {
        if !is_executable_file(&candidate) {
            searched.push(candidate.display().to_string());
            continue;
        }
        // Reached only when nothing earlier was there, so the cost of asking is
        // paid on the machines that would otherwise be handed a surface on
        // which every call fails.
        if believe == Believe::OnceItAnswers && !answers_as_cli(&candidate) {
            searched.push(format!(
                "{} (present, but does not answer `version` as the CLI)",
                candidate.display()
            ));
            continue;
        }
        return Ok(candidate);
    }
    Err(searched)
}

/// Whether a candidate is believed on sight, or only once it has answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Believe {
    /// A `tailscale` on `PATH`, or the shim beside it. Nothing else on a
    /// machine is called that, and both are the command-line interface.
    OnSight,
    /// The executable inside the application bundle: see [`answers_as_cli`].
    OnceItAnswers,
}

/// Everywhere a `tailscale` might be, in the order they are considered.
///
/// Gathered into one ordered list rather than chained at the point of use so
/// that a test can hold the order, which is half of what this fix is: the shim
/// has to be reached before the executable inside the bundle (Q157).
fn candidates() -> Vec<(PathBuf, Believe)> {
    search_path_candidates()
        .chain(shim_candidates())
        .map(|path| (path, Believe::OnSight))
        .chain(bundle_candidates().map(|path| (path, Believe::OnceItAnswers)))
        .collect()
}

/// The shim the macOS applications install for command-line use.
///
/// Both builds offer to put this there, and it is what a person means by "the
/// `tailscale` command" on a Mac. It is a two-line `/bin/sh` script in front of
/// the executable inside the bundle and, unlike that executable, it acts as the
/// command-line interface whatever environment it is handed — which is the
/// property that matters here (Q157).
///
/// Looked for by absolute path rather than left to `PATH`, because
/// `/usr/local/bin` is not in the environment a launcher hands a server started
/// outside a login shell, and that is how an MCP client starts one.
fn shim_candidates() -> impl Iterator<Item = PathBuf> {
    let paths: &[&str] = if cfg!(target_os = "macos") {
        &["/usr/local/bin/tailscale"]
    } else {
        &[]
    };
    paths.iter().map(PathBuf::from)
}

/// Whether a candidate answers as the command-line interface.
///
/// The executable inside the standalone application's bundle is the
/// application's own. Handed the environment [`minimal_env`] builds, it starts
/// the GUI rather than running the command, prints `The Tailscale GUI failed to
/// start` **on standard output**, and **exits 0** — so neither the exit status
/// nor the stream it chose tells it apart from a version. Only the shape of
/// what it printed does: `version` opens with the number and nothing before it,
/// so a first line not starting with a digit is not one (Q157).
///
/// `version` is the right question to ask: it reads the build stamped into the
/// binary, so it is local, prompt, and changes nothing.
fn answers_as_cli(path: &Path) -> bool {
    let Ok(output) = std::process::Command::new(path)
        .arg("version")
        .env_clear()
        .envs(minimal_env())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .is_some_and(|line| line.starts_with(|c: char| c.is_ascii_digit()))
}

/// Where the macOS applications keep the executable itself.
///
/// Tried last and only when it answers, because this is the one candidate that
/// can be present, executable, and still not be a command-line interface:
/// see [`answers_as_cli`]. Neither path is on `PATH` at all.
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

    /// What the standalone application's own executable prints when it is run
    /// outside a login environment: no version, and a zero exit.
    #[cfg(unix)]
    const STARTS_THE_GUI: &str = "echo 'The Tailscale GUI failed to start: The operation couldn\u{2019}t be completed. \
             (Tailscale.CLIError error 3.)'";

    /// A stub `tailscale` that runs `script` and exits however it exits.
    #[cfg(unix)]
    fn stub_named(dir: &tempfile::TempDir, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write the stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make the stub executable");
        path
    }

    #[cfg(unix)]
    #[test]
    fn a_candidate_that_starts_the_gui_does_not_answer_as_the_cli() {
        // Verbatim what the standalone application's own executable does when
        // it is handed the environment `minimal_env` builds: that message, on
        // standard output, and a zero exit. Neither the status nor the stream
        // it chose tells it apart from a version, so only the shape can.
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = stub_named(&dir, "tailscale", STARTS_THE_GUI);
        assert!(!answers_as_cli(&path));
    }

    #[cfg(unix)]
    #[test]
    fn a_bundled_executable_that_starts_the_gui_is_passed_over_for_one_that_answers() {
        // The affected machine in one list: the application is there, and so
        // is the shim, and only the shim is the command-line interface.
        // Two directories, because the names differ only in case and a Mac's
        // filesystem does not: in one directory the second would be the first.
        let bundle = tempfile::tempdir().expect("a temp dir");
        let usr_local = tempfile::tempdir().expect("a temp dir");
        let bundled = stub_named(&bundle, "Tailscale", STARTS_THE_GUI);
        let shim = stub_named(&usr_local, "tailscale", "echo '1.102.2'");
        let found = first_usable(vec![
            (bundled, Believe::OnceItAnswers),
            (shim.clone(), Believe::OnSight),
        ])
        .expect("the shim is usable");
        assert_eq!(found, shim);
    }

    #[cfg(unix)]
    #[test]
    fn a_bundled_executable_that_starts_the_gui_is_not_a_binary_we_found() {
        // With nothing else on the machine the honest answer is that there is
        // no command-line interface here, so the surface is not offered at all
        // rather than offered and failing on every call (Q157).
        let bundle = tempfile::tempdir().expect("a temp dir");
        let bundled = stub_named(&bundle, "Tailscale", STARTS_THE_GUI);
        let searched =
            first_usable(vec![(bundled, Believe::OnceItAnswers)]).expect_err("nothing is usable");
        assert!(
            searched.iter().any(|line| line.contains("does not answer")),
            "the reason has to name itself: {searched:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_candidate_that_reports_a_version_answers_as_the_cli() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = stub_named(
            &dir,
            "tailscale",
            "echo '1.102.2'\necho '  tailscale commit: 6cac9181'",
        );
        assert!(answers_as_cli(&path));
    }

    #[cfg(unix)]
    #[test]
    fn a_candidate_that_says_nothing_at_all_does_not_answer_as_the_cli() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = stub_named(&dir, "tailscale", "exit 0");
        assert!(!answers_as_cli(&path));
    }

    #[test]
    fn the_shim_is_reached_before_the_executable_inside_the_bundle() {
        // The whole of the macOS fix is this order: on an affected machine both
        // are present, and only the one reached first acts as the CLI.
        let all = candidates();
        let at = |wanted: Vec<PathBuf>| {
            all.iter()
                .position(|(path, _)| wanted.iter().any(|w| w == path))
        };
        match (
            at(shim_candidates().collect()),
            at(bundle_candidates().collect()),
        ) {
            (Some(shim), Some(bundle)) => assert!(shim < bundle, "{all:?}"),
            (None, None) if cfg!(target_os = "macos") => panic!("macOS offers both"),
            (None, None) => {}
            other => panic!("one list reached without the other: {other:?}"),
        }
    }

    #[test]
    fn only_the_executable_inside_the_bundle_has_to_answer_before_it_is_believed() {
        let bundled: Vec<PathBuf> = bundle_candidates().collect();
        for (path, believe) in candidates() {
            assert_eq!(
                believe == Believe::OnceItAnswers,
                bundled.contains(&path),
                "{} is believed the wrong way round",
                path.display()
            );
        }
    }

    #[test]
    fn an_invocation_renders_without_a_shell_anywhere_near_it() {
        let inv = Invocation::read(["ping", "--c=1", "host with spaces"]);
        assert_eq!(inv.display(), "tailscale ping --c=1 host with spaces");
        assert_eq!(inv.args.len(), 3, "the arguments stay separate");
    }
}
