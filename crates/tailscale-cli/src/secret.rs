//! Passing a secret to the CLI without putting it on the command line.
//!
//! An argument list is world-readable on every platform we support: `ps` shows
//! it, `/proc` shows it, and a crash reporter will happily upload it. Tailscale
//! anticipates this and accepts `file:<path>` wherever it accepts a key, so the
//! secret goes into a file only this process can read, for only as long as the
//! call takes.

use std::io::Write;
use std::path::Path;

use crate::exec::ExecError;

/// A private file holding one secret, removed when this value is dropped.
///
/// Dropping is enough for the ordinary paths, including a timeout: the guard
/// lives in the same scope as the call, so unwinding or an early return takes
/// the file with it. A hard kill of the server leaves it behind, which is why
/// it is created under the system temporary directory with a private mode
/// rather than somewhere durable.
#[derive(Debug)]
pub struct SecretFile {
    file: tempfile::NamedTempFile,
}

impl SecretFile {
    /// Write `secret` to a new private file.
    pub fn new(secret: &str) -> Result<Self, ExecError> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("tailscale-mcp-").suffix(".key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::Permissions::from_mode(0o600);
            builder.permissions(std::fs::Permissions::from_mode(0o600));
        }
        let mut file = builder.tempfile().map_err(ExecError::SecretFile)?;
        file.write_all(secret.as_bytes())
            .and_then(|()| file.flush())
            .map_err(ExecError::SecretFile)?;
        Ok(Self { file })
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The argument value the CLI expects: a path behind the `file:` scheme.
    pub fn arg(&self) -> String {
        format!("file:{}", self.file.path().display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_reaches_the_cli_by_reference_not_by_value() {
        let secret = "tskey-auth-notreal-notreal";
        let file = SecretFile::new(secret).expect("a temporary file");
        let arg = file.arg();
        assert!(arg.starts_with("file:"));
        assert!(
            !arg.contains(secret),
            "the argument must not carry the secret: {arg}"
        );
        assert_eq!(
            std::fs::read_to_string(file.path()).expect("readable"),
            secret
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_this_user() {
        use std::os::unix::fs::PermissionsExt as _;
        let file = SecretFile::new("tskey-auth-notreal-notreal").expect("a temporary file");
        let mode = std::fs::metadata(file.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "group and other must have no access");
    }

    #[test]
    fn the_file_goes_away_with_the_call() {
        let path = {
            let file = SecretFile::new("tskey-auth-notreal-notreal").expect("a temporary file");
            file.path().to_path_buf()
        };
        assert!(!path.exists(), "{} outlived its guard", path.display());
    }
}
