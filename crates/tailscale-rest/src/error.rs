//! What can go wrong between here and the control plane.
//!
//! This crate does not know about MCP error codes, so the variants are named
//! for what happened rather than for what a caller should be told. The server
//! crate maps them; keeping the mapping there is what lets this crate be used
//! without it.

use std::path::PathBuf;
use std::time::Duration;

/// A call that did not produce the answer it was asked for.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The control plane answered, and the answer was not a success.
    #[error("{request} failed with {status}: {message}")]
    Status {
        /// The method and path, for saying which call this was.
        request: String,
        status: u16,
        /// The API's own `message` field where it sent one, the body where it
        /// did not, and the status's reason where the body was empty.
        message: String,
        /// What the server asked us to wait, from `Retry-After`.
        retry_after: Option<Duration>,
    },

    /// The request never became a response: no route, refused, TLS, a reset.
    #[error("{request} could not be sent: {source}")]
    Transport {
        request: String,
        #[source]
        source: reqwest::Error,
    },

    /// The call used up the budget it was given, across every attempt.
    #[error("{request} did not finish within {}s", budget.as_secs())]
    Timeout { request: String, budget: Duration },

    /// The body is larger than this server will hold in memory.
    #[error("{request} answered with more than {cap} bytes")]
    TooLarge { request: String, cap: usize },

    /// The body arrived and is not what it should be.
    #[error("{request} answered with a body this server could not read: {source}")]
    Malformed {
        request: String,
        #[source]
        source: serde_json::Error,
    },

    /// A token could not be minted, so no call can be made at all.
    #[error("the control-plane credential could not be exchanged for a token: {0}")]
    Token(String),

    /// The federated identity's JWT could not be read from disk.
    #[error("the federated identity file {} could not be read: {source}", path.display())]
    JwtFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The client was built wrong. Never depends on the network, so it is
    /// worth telling apart from everything above.
    #[error("the control-plane client is misconfigured: {0}")]
    Config(String),
}

impl ApiError {
    /// The status the control plane sent, where there was one.
    ///
    /// The server crate turns this into the tool error's `status` field, which
    /// is the number a caller needs to look the failure up in Tailscale's own
    /// documentation.
    pub const fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Whether asking again could plausibly work.
    ///
    /// This is about the failure, not about the request: whether *this* call
    /// may be sent twice is [`Idempotence`], and both have to agree before
    /// anything is retried.
    pub const fn is_transient(&self) -> bool {
        match self {
            Self::Status { status, .. } => matches!(status, 429 | 500 | 502 | 503 | 504),
            // A request that never reached a response was not acted on.
            Self::Transport { .. } => true,
            _ => false,
        }
    }
}

/// What to tell a caller about a failure, from the status and the body.
///
/// The API answers a failure with `{"message": "..."}` most of the time and
/// with something else the rest of it, so all three fallbacks are needed: the
/// field, then whatever the body says, then the status's own reason for a body
/// that is empty.
pub(crate) fn describe(status: reqwest::StatusCode, body: &str) -> String {
    let body = body.trim();
    if let Ok(serde_json::Value::Object(fields)) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(serde_json::Value::String(message)) = fields.get("message")
        && !message.trim().is_empty()
    {
        return message.trim().to_owned();
    }
    if body.is_empty() {
        return status
            .canonical_reason()
            .unwrap_or("no reason given")
            .to_owned();
    }
    body.to_owned()
}

/// Whether a request may be sent a second time.
///
/// HTTP's own answer is the method, and it is the answer used here: `GET`,
/// `HEAD`, `PUT` and `DELETE` are defined to have the same effect done once or
/// done twice, and `POST` and `PATCH` are not. Minting an auth key is a `POST`,
/// and a retried mint is a second key nobody asked for and nobody sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Idempotence {
    /// Repeating it is defined to be the same as doing it once.
    Repeatable,
    /// Repeating it may do the thing twice.
    Once,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: u16) -> ApiError {
        ApiError::Status {
            request: "GET /api/v2/tailnet/-/devices".to_owned(),
            status,
            message: "nope".to_owned(),
            retry_after: None,
        }
    }

    #[test]
    fn the_statuses_worth_asking_again_about_are_the_ones_that_mean_later() {
        for code in [429, 500, 502, 503, 504] {
            assert!(status(code).is_transient(), "{code} should be transient");
        }
        // A refusal, a bad request or a missing thing will say the same next
        // time; asking again is a second failure and a second wait.
        for code in [400, 401, 403, 404, 409, 412, 501] {
            assert!(!status(code).is_transient(), "{code} should be permanent");
        }
    }

    #[test]
    fn a_failure_is_described_from_the_field_the_api_uses() {
        let reason = |body| describe(reqwest::StatusCode::BAD_REQUEST, body);
        assert_eq!(
            reason(r#"{"message": "invalid tailnet"}"#),
            "invalid tailnet"
        );
        // Not every failure is that shape, and the body still says more than
        // the status does.
        assert_eq!(reason("plain trouble"), "plain trouble");
        assert_eq!(reason(r#"{"error": "nope"}"#), r#"{"error": "nope"}"#);
        // An empty body leaves only the status.
        assert_eq!(reason("   "), "Bad Request");
    }

    #[test]
    fn only_a_status_carries_a_status() {
        assert_eq!(status(429).status(), Some(429));
        assert_eq!(ApiError::Token("no".to_owned()).status(), None);
    }
}
