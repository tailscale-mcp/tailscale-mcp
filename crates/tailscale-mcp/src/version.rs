//! Reading, comparing and reporting the local Tailscale version.
//!
//! Upstream publishes no end-of-life policy: stable releases carry even minor
//! numbers and land about every four weeks, and Tailscale's stated position is
//! that they do not break clients people are still running. The floor below is
//! therefore ours, and it is a statement about what this server models rather
//! than about what Tailscale supports.

use std::fmt;
use std::str::FromStr;

/// The oldest release this server models faithfully.
///
/// Chosen as the newest release that introduced a command belonging to the
/// default preset (`tailscale metrics`, 1.78). Everything the core preset
/// exposes exists at or below it; everything added afterwards carries its own
/// minimum on its metadata row.
///
/// Below the floor the server still starts and still offers every tool. It
/// warns once, on standard error, and lets the individual commands report what
/// they need. Hiding tools on a version guess would be worse: the guess would
/// be wrong for anyone running a fork or a distribution build.
pub const SUPPORTED_FLOOR: Version = Version {
    major: 1,
    minor: 78,
    patch: 0,
};

/// A Tailscale version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Whether this is a development build.
    ///
    /// Odd minor numbers are the unstable track. Such a build is newer than the
    /// stable release with the next even minor, so it is treated as such rather
    /// than warned about.
    pub const fn is_unstable(self) -> bool {
        self.minor % 2 == 1
    }

    /// Read the version out of what `tailscale version` prints.
    ///
    /// The first line is the client version; the remaining lines are the Go
    /// toolchain and commit hashes, which are of no interest here. A build with
    /// a suffix — `1.102.2-t01a2b3c4` — parses as its release.
    pub fn parse_cli_output(output: &str) -> Option<Self> {
        output.lines().find_map(|line| line.trim().parse().ok())
    }
}

impl FromStr for Version {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().trim_start_matches('v');
        // Stop at the first character that cannot be part of a version, so a
        // build suffix or trailing prose does not defeat the parse.
        let end = s
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(s.len());
        let mut parts = s[..end].split('.');
        let mut next = || parts.next().and_then(|p| p.parse::<u32>().ok());
        let major = next().ok_or(())?;
        let minor = next().ok_or(())?;
        let patch = next().unwrap_or(0);
        if parts.next().is_some() {
            return Err(());
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Whether `found` satisfies a tool's `min_version`.
///
/// An unparseable or unknown version satisfies everything: we would rather
/// attempt a command and let the CLI refuse it than refuse a command because
/// we could not read a version string.
pub fn satisfies(found: Option<Version>, required: Option<&str>) -> bool {
    match (found, required.and_then(|r| r.parse::<Version>().ok())) {
        (Some(found), Some(required)) => found >= required,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_version_parses() {
        assert_eq!("1.102.2".parse(), Ok(Version::new(1, 102, 2)));
        assert_eq!("1.78".parse(), Ok(Version::new(1, 78, 0)));
        assert_eq!("v1.80.0".parse(), Ok(Version::new(1, 80, 0)));
    }

    #[test]
    fn a_build_suffix_parses_as_its_release() {
        assert_eq!("1.102.2-t01a2b3c4".parse(), Ok(Version::new(1, 102, 2)));
        assert_eq!("1.99.0-dev".parse(), Ok(Version::new(1, 99, 0)));
    }

    #[test]
    fn prose_is_not_a_version() {
        for text in ["", "tailscale", "go1.24.1 (rev abcdef)", "1", "1.2.3.4"] {
            assert_eq!(text.parse::<Version>(), Err(()), "{text} parsed");
        }
    }

    #[test]
    fn the_first_line_of_the_cli_output_is_the_client_version() {
        let output = "1.102.2\n  tailscale commit: 0123456789abcdef\n  go version: go1.24.1\n";
        assert_eq!(
            Version::parse_cli_output(output),
            Some(Version::new(1, 102, 2))
        );
    }

    #[test]
    fn versions_order_by_component() {
        assert!(Version::new(1, 102, 0) > Version::new(1, 78, 9));
        assert!(Version::new(1, 78, 1) > Version::new(1, 78, 0));
        assert!(Version::new(1, 78, 0) >= SUPPORTED_FLOOR);
        assert!(Version::new(1, 72, 0) < SUPPORTED_FLOOR);
    }

    #[test]
    fn odd_minors_are_the_unstable_track() {
        assert!(Version::new(1, 103, 0).is_unstable());
        assert!(!Version::new(1, 102, 0).is_unstable());
    }

    #[test]
    fn a_requirement_is_met_by_a_newer_release() {
        assert!(satisfies(Some(Version::new(1, 102, 2)), Some("1.80")));
        assert!(satisfies(Some(Version::new(1, 80, 0)), Some("1.80")));
        assert!(!satisfies(Some(Version::new(1, 78, 0)), Some("1.80")));
    }

    #[test]
    fn an_unknown_version_blocks_nothing() {
        assert!(satisfies(None, Some("1.80")));
        assert!(satisfies(Some(Version::new(1, 50, 0)), None));
        // An unparseable requirement is a bug in our own table, and refusing
        // the call would hide it behind a confusing message.
        assert!(satisfies(Some(Version::new(1, 50, 0)), Some("soon")));
    }
}
