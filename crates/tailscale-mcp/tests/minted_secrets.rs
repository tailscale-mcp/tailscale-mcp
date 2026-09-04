//! A minted secret reaches the caller and reaches no log line.
//!
//! Two halves of one rule, and the second is why this file exists on its own.
//! Asserting "nothing logs a body" by reading the code is an argument, not a
//! test: it holds until somebody adds a `tracing::debug!` with a response in
//! it. So a subscriber is installed over the whole process, at `TRACE`, and
//! the assertion is made against what it actually collected.
//!
//! Process-wide is the reason for a file of its own. `set_global_default` can
//! be called once, and a test binary is one process — so this binary contains
//! only tests that want that subscriber, and every one of them contributes its
//! output to the same buffer that gets asserted on.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::io;
use std::sync::{Arc, Mutex};

use serde_json::json;
use tailscale_rest::fake::Response;

use harness::Setup;

/// A secret nobody holds, in the shape the control plane really sends.
///
/// Documentation values only: `nExAmPlE` is not a tailnet's key, and the
/// suffix is the fixture convention `tests/fixtures_are_redacted.rs` enforces.
const MINTED: &str = "tskey-auth-nExAmPlE1-redactedSecretValue";

/// Everything anything logged during this process.
#[derive(Clone, Default)]
struct Collected(Arc<Mutex<Vec<u8>>>);

impl io::Write for Collected {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("the log buffer")
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Collected {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn logged() -> &'static Collected {
    use std::sync::OnceLock;
    static COLLECTED: OnceLock<Collected> = OnceLock::new();
    COLLECTED.get_or_init(|| {
        let collected = Collected::default();
        // The filter an operator gets by asking for everything, not a raw
        // `TRACE`: what matters is what *this server* would write, and this
        // server caps the SDK (Q79). A raw `TRACE` would be asserting against
        // a configuration nobody runs.
        let filter = tailscale_mcp::config::bounded_log_filter("trace");
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(collected.clone())
            .with_ansi(false)
            .init();
        collected
    })
}

fn everything_logged() -> String {
    String::from_utf8_lossy(&logged().0.lock().expect("the log buffer")).into_owned()
}

#[tokio::test]
async fn a_minted_key_reaches_the_caller_and_no_log_line() {
    logged();
    let harness = Setup::new()
        .toolsets("tailnet-keys")
        .tier(tailscale_mcp::meta::Tier::Write)
        .api_answers(
            "POST",
            "/api/v2/tailnet/-/keys",
            Response::json(json!({
                "id": "kexample1",
                "key": MINTED,
                "keyType": "auth",
                "capabilities": {"devices": {"create": {"reusable": false}}},
            })),
        )
        .await
        .start()
        .await;

    let answer = harness
        .call_ok(
            "tailnet_key_create",
            json!({
                "key_type": "auth",
                "description": "example",
                "capabilities": {"devices": {"create": {
                    "reusable": false, "ephemeral": false,
                    "preauthorized": false, "tags": ["tag:example"],
                }}},
            }),
        )
        .await;

    // Half one: the caller gets it. There is no second chance to read it, so a
    // server that redacted here would have thrown the key away.
    assert_eq!(
        answer["key"],
        json!(MINTED),
        "the minted secret is the whole point of the call: {answer:#?}"
    );

    // And the documented capabilities example reached the wire unchanged.
    assert_eq!(
        harness.control_plane().only_request().json()["capabilities"],
        json!({"devices": {"create": {
            "reusable": false, "ephemeral": false,
            "preauthorized": false, "tags": ["tag:example"],
        }}}),
    );

    harness.shutdown().await;

    // Half two, against what was collected rather than against a reading of
    // the code.
    let log = everything_logged();
    assert!(
        !log.contains(MINTED),
        "the minted secret reached a log line:\n{log}"
    );
    assert!(
        !log.contains("redactedSecretValue"),
        "and no part of it did either:\n{log}"
    );
}

#[tokio::test]
async fn an_invite_url_reaches_the_caller_and_no_log_line() {
    // The same rule for the other credential this surface hands out: anyone
    // holding an invite URL can accept it, so it is a secret that happens to
    // look like a link.
    const INVITE: &str = "https://login.tailscale.com/admin/invite/example-redacted-code";
    logged();
    let harness = Setup::new()
        .toolsets("tailnet-invites")
        .tier(tailscale_mcp::meta::Tier::Write)
        .api_answers(
            "POST",
            "/api/v2/tailnet/-/user-invites",
            Response::json(json!([{"id": "ui-example", "inviteUrl": INVITE}])),
        )
        .await
        .start()
        .await;

    let answer = harness
        .call_ok(
            "tailnet_user_invite_create",
            json!({"invites": [{"role": "member", "email": "someone@example.com"}]}),
        )
        .await;

    assert_eq!(answer["invites"][0]["inviteUrl"], json!(INVITE));

    harness.shutdown().await;

    let log = everything_logged();
    assert!(
        !log.contains("example-redacted-code"),
        "the invite URL reached a log line:\n{log}"
    );
}
