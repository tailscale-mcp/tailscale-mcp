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

// A panic in a test is a failed test, which is what these lints exist to
// prevent elsewhere.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod cli;
pub mod config;
pub mod context;
pub mod error;
pub mod gating;
pub mod instructions;
pub mod meta;
pub mod registry;
pub mod server;
pub mod tools;
pub mod version;

/// Test helpers, re-exported so that the unit tests and the integration tests
/// reach for the same fake.
#[cfg(test)]
pub(crate) mod testing {
    #[allow(unused_imports)]
    pub(crate) use tailscale_cli::stub::{Reply, StubBackend};
}
