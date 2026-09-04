//! The tailnet surface's transport, as a session actually gets it.
//!
//! The client is built in `server::build` from the configuration and whatever
//! credential was discovered, and handed to every handler through the context.
//! No tool uses it yet, so what is checked here is the wiring: that a call made
//! through the context reaches the fake, carries the credential, means the
//! tailnet the environment named, and is held to the same size cap as
//! everything else a session can ask for.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use serde_json::json;
use tailscale_mcp::config::API_BASE_URL_ENV;
use tailscale_mcp::error::ToolError;
use tailscale_rest::ApiError;
use tailscale_rest::credentials::TAILNET_ENV;
use tailscale_rest::fake::{FakeControlPlane, Response};

use harness::{Setup, TEST_API_KEY};

const DEVICES: &str = "/api/v2/tailnet/-/devices";

#[tokio::test]
async fn a_call_through_the_session_reaches_the_control_plane_with_the_credential() {
    let harness = Setup::new()
        .api_answers("GET", DEVICES, Response::json(json!({"devices": []})))
        .await
        .start()
        .await;

    let client = harness.context.tailnet().expect("a credential was found");
    let answer = client
        .get(client.tailnet_path(None, "/devices"))
        .send()
        .await
        .expect("the fake answers");

    assert_eq!(answer, json!({"devices": []}));
    let request = harness.control_plane().only_request();
    assert_eq!(request.path, DEVICES);
    assert_eq!(
        request.authorization(),
        Some(format!("Bearer {TEST_API_KEY}").as_str()),
        "the credential the session was built with should be the one on the wire"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_tailnet_a_path_means_is_the_one_the_environment_named() {
    let path = "/api/v2/tailnet/example.com/devices";
    let harness = Setup::new()
        .env(TAILNET_ENV, "example.com")
        .api_answers("GET", path, Response::json(json!({"devices": []})))
        .await
        .start()
        .await;

    let client = harness.context.tailnet().expect("a credential was found");
    assert_eq!(client.tailnet(), "example.com");
    client
        .get(client.tailnet_path(None, "/devices"))
        .send()
        .await
        .expect("the fake answers");

    assert_eq!(harness.control_plane().only_request().path, path);

    harness.shutdown().await;
}

#[tokio::test]
async fn an_answer_over_the_session_cap_is_refused_rather_than_truncated() {
    // The same ceiling a tool result is held to. A control-plane answer that
    // could not be returned anyway is better refused before it is parsed.
    let big = json!({"devices": (0..50).map(|n| json!({"name": format!("node-{n}")})).collect::<Vec<_>>()});
    let harness = Setup::new()
        .env("TAILSCALE_MCP_MAX_RESULT_BYTES", "128")
        .api_answers("GET", DEVICES, Response::json(&big))
        .await
        .start()
        .await;

    let client = harness.context.tailnet().expect("a credential was found");
    let error = client
        .get(client.tailnet_path(None, "/devices"))
        .send()
        .await
        .expect_err("the answer is over the cap");

    assert!(
        matches!(error, ApiError::TooLarge { cap: 128, .. }),
        "unexpected error: {error:?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_control_plane_that_cannot_be_reached_is_the_surface_being_unavailable() {
    // The one arm of the mapping that cannot be built by hand: a transport
    // failure carries a `reqwest::Error`, which only a real failed request
    // produces. So one is produced — the fake is started and then dropped, so
    // its port is a place nothing is listening.
    let address = {
        let fake = FakeControlPlane::start().await.expect("a loopback socket");
        fake.base_url().to_owned()
    };

    let harness = Setup::new().env(API_BASE_URL_ENV, &address).start().await;
    let client = harness.context.tailnet().expect("a credential was found");
    let error = client
        .get(client.tailnet_path(None, "/devices"))
        .send()
        .await
        .expect_err("nothing is listening there");

    let reported = ToolError::from(error).to_value();
    assert_eq!(reported["code"], json!("backend_unavailable"));
    assert!(
        reported["message"]
            .as_str()
            .is_some_and(|m| m.contains("the control plane is unavailable")),
        "the refusal should say which backend: {reported:#?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn without_a_credential_there_is_no_client_and_a_reason_a_caller_can_read() {
    let harness = Setup::new().without_credentials().start().await;

    let error = harness
        .context
        .tailnet()
        .expect_err("there is no credential");
    let reported = serde_json::to_value(&error).expect("the error is reportable");

    assert_eq!(reported["code"], json!("backend_unavailable"));
    assert!(
        reported["message"]
            .as_str()
            .is_some_and(|m| m.contains("TAILSCALE_API_KEY")),
        "the refusal should name what to set: {reported:#?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_surface_switched_off_leaves_no_client_even_with_a_credential() {
    // The credential is there — the harness supplies one — and the operator
    // said not to use it. Building a client anyway would leave the session one
    // mistake away from using a surface that was switched off on purpose.
    let harness = Setup::new().without_tailnet().start().await;

    assert!(harness.context.tailnet().is_err());

    harness.shutdown().await;
}
