//! What a tool handler is given.
//!
//! Deliberately a plain struct of backends and limits rather than the server
//! itself: a handler that could reach the server could reach the router, and
//! then the tests would have to construct one to call anything.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tailscale_cli::LocalBackend;

use crate::error::Redactor;
use crate::meta::Tier;
use crate::version::Version;

/// Where a tool may write when the caller names a path on this machine.
///
/// In this release the tier is what confines host filesystem access: those
/// tools sit at the write tier and no higher, so a read-only session reaches
/// none of them. The allow-list is the mechanism meant to confine them further,
/// and it is here rather than in a comment so that switching it on is a matter
/// of populating one value: every tool that takes a path already asks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PathPolicy {
    /// Any path the caller names. What this release ships.
    #[default]
    Unrestricted,
    /// Only paths under one of these roots.
    Within(Vec<PathBuf>),
}

impl PathPolicy {
    /// Whether this policy would let a tool write to `path`.
    #[must_use]
    pub fn permits(&self, path: &Path) -> bool {
        match self {
            Self::Unrestricted => true,
            // A `..` walks out of whatever root it is checked against, so a
            // path carrying one is refused rather than resolved. Resolving
            // would have to touch the filesystem, and the path a caller names
            // here is usually one that does not exist yet.
            Self::Within(roots) => {
                !path.components().any(|c| c == Component::ParentDir)
                    && roots.iter().any(|root| path.starts_with(root))
            }
        }
    }
}

/// The identity of the node this server runs on, read from status at startup.
///
/// Used to recognise a control-plane operation aimed at ourselves, which is the
/// difference between deleting a device and severing the connection the caller
/// is talking over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelfIdentity {
    /// The control-plane device id of this node, when it could be determined.
    pub device_id: Option<String>,
    /// The node's stable id as the CLI reports it.
    pub node_id: Option<String>,
    /// Tailscale addresses assigned to this node.
    pub addresses: Vec<String>,
    /// The node's MagicDNS name.
    pub dns_name: Option<String>,
}

impl SelfIdentity {
    /// Whether `target` names this node. Matching is generous on purpose: a
    /// caller may refer to the node by any of the identifiers the API accepts,
    /// and a missed match is the expensive direction.
    pub fn matches(&self, target: &str) -> bool {
        let target = target.trim().trim_end_matches('.');
        if target.is_empty() {
            return false;
        }
        let same = |candidate: &Option<String>| {
            candidate
                .as_deref()
                .is_some_and(|c| c.trim_end_matches('.').eq_ignore_ascii_case(target))
        };
        same(&self.device_id)
            || same(&self.node_id)
            || same(&self.dns_name)
            || self.addresses.iter().any(|a| a == target)
            // A MagicDNS name may be given unqualified.
            || self
                .dns_name
                .as_deref()
                .and_then(|n| n.split('.').next())
                .is_some_and(|short| short.eq_ignore_ascii_case(target))
    }
}

/// Everything a handler may reach.
#[derive(Clone)]
pub struct ToolContext {
    /// The local node. Present even when the local surface is disabled, in
    /// which case it is a backend that reports the binary as missing.
    pub local: Arc<dyn LocalBackend>,
    /// Removes secrets from anything on its way out.
    pub redactor: Redactor,
    /// The size above which a result is refused rather than truncated.
    pub max_result_bytes: usize,
    /// Who we are on the tailnet, when we could find out.
    pub identity: SelfIdentity,
    /// The version the local CLI reports, when it could be read.
    pub cli_version: Option<Version>,
    /// Where the tools that take a path are allowed to write.
    pub paths: PathPolicy,
    /// The most dangerous tier this session permits.
    ///
    /// The gate is what normally applies this, before a handler is reached, so
    /// no typed tool has to look at it. The passthrough does: its row carries a
    /// floor rather than its real tier, so it is the one tool that has to make
    /// the same decision the gate makes, against the command it was given.
    pub max_tier: Tier,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No credential-bearing field is printed, and none should be added.
        f.debug_struct("ToolContext")
            .field("max_result_bytes", &self.max_result_bytes)
            .field("identity", &self.identity)
            .field("cli_version", &self.cli_version)
            .field("max_tier", &self.max_tier)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> SelfIdentity {
        SelfIdentity {
            device_id: Some("n1234567CNTRL".to_owned()),
            node_id: Some("nodekey:abc".to_owned()),
            addresses: vec!["100.64.0.1".to_owned(), "fd7a::1".to_owned()],
            dns_name: Some("workstation.example-tailnet.ts.net.".to_owned()),
        }
    }

    #[test]
    fn a_node_is_recognised_by_any_name_the_api_accepts() {
        let id = identity();
        for name in [
            "n1234567CNTRL",
            "nodekey:abc",
            "100.64.0.1",
            "fd7a::1",
            "workstation.example-tailnet.ts.net",
            "workstation.example-tailnet.ts.net.",
            "workstation",
            "WORKSTATION",
        ] {
            assert!(id.matches(name), "{name} should name this node");
        }
    }

    #[test]
    fn another_node_is_not() {
        let id = identity();
        for name in [
            "n7654321CNTRL",
            "100.64.0.2",
            "laptop.example-tailnet.ts.net",
            "laptop",
            "",
            "   ",
        ] {
            assert!(!id.matches(name), "{name} should not name this node");
        }
    }

    #[test]
    fn an_unknown_identity_matches_nothing() {
        assert!(!SelfIdentity::default().matches("anything"));
    }
}
