//! Wrapper around the `tailscale` command-line interface.
//!
//! The local node is driven through its CLI rather than through the LocalAPI
//! socket (ADR-0001), so this crate is where every one of those decisions
//! lives: where the binary is found, what environment it inherits, how long it
//! is given, how a secret reaches it without passing through the argument list,
//! and which calls may overlap.
//!
//! Everything above this crate sees [`LocalBackend`], a one-method trait, so
//! the server can be built over a fake in tests without spawning anything.

// A panic in a test is a failed test, which is what these lints exist to
// prevent elsewhere.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod backend;
pub mod exec;
pub mod secret;

/// A scriptable backend for tests. Not built into a release binary.
#[cfg(feature = "testing")]
pub mod stub;

pub use backend::{BoxFuture, Concurrency, Invocation, LocalBackend, Output, Unavailable};
pub use exec::{CliBackend, DEFAULT_TIMEOUT, ExecError, GRACE_PERIOD};
pub use secret::SecretFile;
