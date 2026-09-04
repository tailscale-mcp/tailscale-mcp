//! Typed client for the Tailscale control-plane REST API v2.
//!
//! Written here rather than taken from a crate (ADR-0002), so this is where
//! authentication, retry policy, pagination and the response shapes live.

// A panic in a test is a failed test, which is what these lints exist to
// prevent elsewhere.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod client;
pub mod credentials;
pub mod error;
#[cfg(any(test, feature = "testing"))]
pub mod fake;
pub mod secret;
mod token;

pub use client::{
    Client, ClientConfig, DEFAULT_BASE_URL, RequestBuilder, TextBody, checked_base_url,
};
pub use credentials::{Credentials, DEFAULT_TAILNET};
pub use error::ApiError;
pub use secret::Secret;
