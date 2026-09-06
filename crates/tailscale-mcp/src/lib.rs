//! MCP server for Tailscale.
//!
//! Two surfaces, deliberately kept apart. The *local* surface drives the node
//! this server runs on through the `tailscale` command-line interface
//! (ADR-0001). The *tailnet* surface acts on the whole tailnet through the
//! control-plane REST API (ADR-0002). A tool belongs to exactly one of them.
//!
//! The crate is a library as well as a binary so that the tests can build a
//! whole server in-process and drive it as a client, which is where nearly all
//! of the behaviour is observable.
//!
//! That is the whole of why the library is public, and so it carries no
//! stability guarantee: the compatible thing to depend on is the protocol this
//! server speaks, which the contract tests pin, rather than the Rust surface
//! the tests happen to reach through. A signature here may change in any
//! release, and `cargo semver-checks` will say so — the question that answers
//! is whether the change was meant, not whether it is allowed.

// A panic in a test is a failed test, which is what these lints exist to
// prevent elsewhere.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod cli;
pub mod completion;
pub mod config;
pub mod context;
pub mod error;
pub mod gating;
pub mod http;
pub mod instructions;
pub mod meta;
pub mod registry;
pub mod resources;
pub mod server;
pub mod subcommands;
pub mod tools;
pub mod version;

/// Test helpers, re-exported so that the unit tests and the integration tests
/// reach for the same fake.
#[cfg(test)]
pub(crate) mod testing {
    use std::sync::Arc;

    #[allow(unused_imports)]
    pub(crate) use tailscale_cli::stub::{Reply, StubBackend};

    use crate::context::{Identity, PathPolicy, ToolContext};
    use crate::error::Redactor;
    use crate::meta::Tier;

    /// A context wired to a scripted client and to nothing else.
    ///
    /// Every toolset's unit tests want the same one, and used to spell it out
    /// six identical times. The tier is the most permissive: a handler called
    /// directly has already passed the gate, so anything less would be testing
    /// a check that is not this code's to make.
    pub(crate) fn context(backend: Arc<StubBackend>) -> ToolContext {
        ToolContext {
            local: backend as Arc<dyn tailscale_cli::LocalBackend>,
            // The tailnet surface has its own fake; a local handler
            // never reaches for it.
            tailnet: None,
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: Identity::default(),
            cli_version: None,
            paths: PathPolicy::default(),
            devices: Default::default(),
            max_tier: Tier::Destructive,
        }
    }
}
