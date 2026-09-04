//! Typed client for the Tailscale control-plane REST API v2.
//!
//! Written here rather than taken from a crate (ADR-0002), so this is where
//! authentication, retry policy, pagination and the response shapes live.

// A panic in a test is a failed test, which is what these lints exist to
// prevent elsewhere.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod credentials;
pub mod secret;

pub use credentials::{Credentials, DEFAULT_TAILNET};
pub use secret::Secret;
