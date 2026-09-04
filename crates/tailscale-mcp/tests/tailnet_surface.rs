//! The tailnet surface, as a session actually gets it.
//!
//! What is checked here is not any one tool's behaviour — the contract table
//! covers that — but the things every tailnet tool inherits and none of them
//! implements: that a call carries the credential, means the tailnet the
//! environment named, is held to the session's result cap, and says something
//! useful when there is no credential or the control plane cannot be reached.
//!
//! This replaces `tests/control_plane.rs`, which asserted the same properties
//! by reaching into the session's context and driving the client directly.
//! That was a provisional third seam, kept because ticket 15 built a transport
//! with no tool above it (Q59) and re-aimed at this ticket when ticket 16 also
//! landed none (Q63). Every assertion below is now an ordinary tool call, so
//! nothing here knows how the server is wired.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use serde_json::json;
use tailscale_mcp::config::API_BASE_URL_ENV;
use tailscale_rest::credentials::TAILNET_ENV;
use tailscale_rest::fake::{FakeControlPlane, Response};

use harness::{Setup, TEST_API_KEY};

const DEVICES: &str = "/api/v2/tailnet/-/devices";

#[tokio::test]
async fn a_tool_call_reaches_the_control_plane_with_the_session_credential() {
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers("GET", DEVICES, Response::json(json!({"devices": []})))
        .await
        .start()
        .await;

    let answer = harness.call_ok("tailnet_device_list", json!({})).await;
    assert_eq!(answer["devices"], json!([]));

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
async fn the_tailnet_a_tool_acts_on_is_the_one_the_environment_named() {
    let path = "/api/v2/tailnet/example.com/devices";
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .env(TAILNET_ENV, "example.com")
        .api_answers("GET", path, Response::json(json!({"devices": []})))
        .await
        .start()
        .await;

    harness.call_ok("tailnet_device_list", json!({})).await;

    assert_eq!(harness.control_plane().only_request().path, path);

    harness.shutdown().await;
}

#[tokio::test]
async fn an_answer_over_the_session_cap_is_refused_rather_than_truncated() {
    // The same ceiling a tool result is held to. A control-plane answer that
    // could not be returned anyway is better refused before it is parsed.
    let big = json!({
        "devices": (0..50)
            .map(|n| json!({"name": format!("example-node-{n}")}))
            .collect::<Vec<_>>(),
    });
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .env("TAILSCALE_MCP_MAX_RESULT_BYTES", "128")
        .api_answers("GET", DEVICES, Response::json(&big))
        .await
        .start()
        .await;

    let error = harness.call_err("tailnet_device_list", json!({})).await;
    assert_eq!(error["code"], json!("result_too_large"));
    // The session's cap refuses an oversized *result* with the same code, so
    // the code alone would not say which check fired. The message does: the
    // transport's names the request and the ceiling it was held to, while the
    // result cap's states an exact size, which the transport never learns
    // because it stops reading.
    let message = error["message"].as_str().expect("a message");
    assert!(
        message.contains(DEVICES) && message.contains("more than 128 bytes"),
        "the refusal should be the transport's, before the body was parsed: {error:#?}"
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

    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .env(API_BASE_URL_ENV, &address)
        .start()
        .await;

    let error = harness.call_err("tailnet_device_list", json!({})).await;
    assert_eq!(error["code"], json!("backend_unavailable"));
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|m| m.contains("the control plane is unavailable")),
        "the refusal should say which backend: {error:#?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn without_a_credential_the_tailnet_tools_are_absent_and_the_session_is_told_why() {
    // `control_plane.rs` asserted this through the context, where the missing
    // client carries a sentence naming `TAILSCALE_API_KEY`. Through a session
    // that sentence is unreachable: a surface with no credential is switched
    // off at startup, so its tools are never offered and no call can arrive to
    // be refused. What a caller can see is the absence and the instructions,
    // so that is what is checked. The sentence still exists, for the one case
    // that can reach it — a credential that stops working mid-session — and
    // `context.rs`'s
    // `a_context_with_no_credential_names_the_variables_that_would_give_it_one`
    // is where it is asserted.
    let harness = Setup::new()
        .toolsets("local-status,tailnet-devices")
        .without_credentials()
        .start()
        .await;

    let offered = harness.tool_names().await;
    assert!(
        offered.iter().any(|name| name.starts_with("tailscale_")),
        "the local tools are unaffected: {offered:?}"
    );
    assert!(
        !offered.iter().any(|name| name.starts_with("tailnet_")),
        "no tailnet tool should be offered: {offered:?}"
    );
    assert!(
        harness
            .instructions()
            .contains("tailnet surface is not available"),
        "a session that cannot reach the control plane should be told so"
    );

    harness.shutdown().await;
}

// ---------------------------------------------------------------------------
// What the surface adds on top of the transport
// ---------------------------------------------------------------------------

#[tokio::test]
async fn both_device_identifier_forms_reach_the_device_they_name() {
    // The API takes either the node id or the numeric one, and a tool that
    // preferred one would be a tool that could not address half the tailnet.
    for id in ["n1111111CNTRL", "123456789"] {
        let path = format!("/api/v2/device/{id}");
        let harness = Setup::new()
            .toolsets("tailnet-devices")
            .api_answers("GET", &path, Response::json(json!({"nodeId": id})))
            .await
            .start()
            .await;

        let answer = harness
            .call_ok("tailnet_device_get", json!({"device_id": id}))
            .await;

        assert_eq!(answer["nodeId"], json!(id));
        assert_eq!(harness.control_plane().only_request().path, path);
        harness.shutdown().await;
    }
}

#[tokio::test]
async fn field_selection_and_filters_travel_as_the_api_spells_them() {
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers("GET", DEVICES, Response::json(json!({"devices": []})))
        .await
        .start()
        .await;

    harness
        .call_ok(
            "tailnet_device_list",
            json!({"fields": "all", "filters": {"isEphemeral": "true", "tags": "tag:example"}}),
        )
        .await;

    let query = harness.control_plane().only_request().query.clone();
    assert_eq!(
        query.get("fields").map(String::as_str),
        Some("all"),
        "{query:?}"
    );
    assert_eq!(
        query.get("isEphemeral").map(String::as_str),
        Some("true"),
        "a filter travels as its own query parameter: {query:?}"
    );
    assert_eq!(
        query.get("tags").map(String::as_str),
        Some("tag:example"),
        "{query:?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_window_slices_the_listing_without_changing_its_shape() {
    let devices: Vec<_> = (0..5)
        .map(|n| json!({"nodeId": format!("n111111{n}CNTRL")}))
        .collect();
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers("GET", DEVICES, Response::json(json!({"devices": devices})))
        .await
        .start()
        .await;

    let answer = harness
        .call_ok("tailnet_device_list", json!({"offset": 1, "limit": 2}))
        .await;

    let listed = answer["devices"].as_array().expect("still a device list");
    assert_eq!(listed.len(), 2, "{answer:#?}");
    assert_eq!(listed[0]["nodeId"], json!("n1111111CNTRL"));
    assert_eq!(
        answer["window"],
        json!({"total": 5, "returned": 2, "offset": 1, "limit": 2}),
        "a windowed answer says how much it left out, and what was asked for"
    );

    // The window is this server's, not the API's: the request is unchanged.
    let query = harness.control_plane().only_request().query.clone();
    assert!(!query.contains_key("limit"), "{query:?}");
    assert!(!query.contains_key("offset"), "{query:?}");

    harness.shutdown().await;
}

#[tokio::test]
async fn an_unwindowed_listing_is_the_api_answer_and_nothing_else() {
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers(
            "GET",
            DEVICES,
            Response::json(json!({"devices": [{"nodeId": "n1111111CNTRL"}]})),
        )
        .await
        .start()
        .await;

    let answer = harness.call_ok("tailnet_device_list", json!({})).await;

    assert_eq!(answer, json!({"devices": [{"nodeId": "n1111111CNTRL"}]}));

    harness.shutdown().await;
}

#[tokio::test]
async fn a_posture_attribute_round_trips_through_the_control_plane() {
    let device = "n1111111CNTRL";
    let attribute = format!("/api/v2/device/{device}/attributes/custom:example");
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .tier(tailscale_mcp::meta::Tier::Destructive)
        .api_answers("POST", &attribute, Response::empty())
        .await
        .api_answers(
            "GET",
            &format!("/api/v2/device/{device}/attributes"),
            Response::json(json!({
                "attributes": {"custom:example": 80},
                "expiries": {"custom:example": "2027-01-01T00:00:00Z"},
            })),
        )
        .await
        .api_answers("DELETE", &attribute, Response::empty())
        .await
        .start()
        .await;

    harness
        .call_ok(
            "tailnet_device_attribute_set",
            json!({"device_id": device, "attribute_key": "custom:example", "value": 80,
                   "expiry": "2027-01-01T00:00:00Z"}),
        )
        .await;

    let read = harness
        .call_ok(
            "tailnet_device_attributes_get",
            json!({"device_id": device}),
        )
        .await;
    assert_eq!(read["attributes"]["custom:example"], json!(80));

    let deleted = harness
        .call_ok(
            "tailnet_device_attribute_delete",
            json!({"device_id": device, "attribute_key": "custom:example"}),
        )
        .await;
    assert_eq!(deleted["done"], json!("attribute deleted"));

    let sent = harness.control_plane().recorded();
    assert_eq!(sent.len(), 3, "one call each: {sent:#?}");
    assert_eq!(
        sent[0].json(),
        json!({"value": 80, "expiry": "2027-01-01T00:00:00Z"}),
        "the body is Tailscale's own shape, and carries only what was given"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_batched_attribute_update_refuses_a_key_the_api_would_reject() {
    // Refused here rather than by the control plane, because the batch is
    // all-or-nothing: one bad key in a hundred devices would fail the whole
    // call after it had been sent, and the refusal names the device.
    // An answer is arranged that the call must not reach: if the check were
    // missing, the tool would send the batch and answer with this instead.
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .tier(tailscale_mcp::meta::Tier::Write)
        .api_answers(
            "PATCH",
            "/api/v2/tailnet/-/device-attributes",
            Response::empty(),
        )
        .await
        .start()
        .await;

    let error = harness
        .call_err(
            "tailnet_device_attributes_update",
            json!({"nodes": {"n1111111CNTRL": {"node:os": "linux"}}}),
        )
        .await;

    assert_eq!(error["code"], json!("invalid_args"));
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|m| m.contains("custom:") && m.contains("n1111111CNTRL")),
        "the refusal should name the key and the device: {error:#?}"
    );
    assert_eq!(
        harness.control_plane().request_count(),
        0,
        "nothing should have been sent"
    );

    harness.shutdown().await;
}

// ---------------------------------------------------------------------------
// DNS and policy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_split_dns_update_and_replace_forms_are_different_calls() {
    // The API's own distinction, and the one this toolset's naming exists to
    // keep visible: `PATCH` merges, `PUT` overwrites. A tool that sent one
    // where the caller meant the other would silently drop every domain it was
    // not told about.
    let path = "/api/v2/tailnet/-/dns/split-dns";
    let domains = json!({"domains": {"example.com": ["10.0.0.1"]}});
    let harness = Setup::new()
        .toolsets("tailnet-dns")
        .tier(tailscale_mcp::meta::Tier::Write)
        .api_answers(
            "PATCH",
            path,
            Response::json(json!({"example.com": ["10.0.0.1"]})),
        )
        .await
        .api_answers(
            "PUT",
            path,
            Response::json(json!({"example.com": ["10.0.0.1"]})),
        )
        .await
        .start()
        .await;

    harness
        .call_ok("tailnet_dns_split_update", domains.clone())
        .await;
    harness.call_ok("tailnet_dns_split_replace", domains).await;

    let sent = harness.control_plane().recorded();
    assert_eq!(
        sent.iter().map(|r| r.method.as_str()).collect::<Vec<_>>(),
        ["PATCH", "PUT"],
        "the merge and the replace reach different verbs: {sent:#?}"
    );
    // Both send the map itself, not a wrapper: `domains` is this server's
    // parameter name and the body is Tailscale's shape (ADR-0004).
    for request in &sent {
        assert_eq!(request.json(), json!({"example.com": ["10.0.0.1"]}));
    }

    harness.shutdown().await;
}

#[tokio::test]
async fn reading_the_policy_answers_with_its_version_and_the_document_as_written() {
    // `spec.md`'s one documented exception to answering with the body alone:
    // the version is a header, so it cannot travel in a body that has no room
    // for one.
    let hujson = "{\n  // a comment, which JSON does not have\n  \"acls\": [],\n}";
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", hujson).with_header("etag", "\"e0b2816b418\""),
        )
        .await
        .start()
        .await;

    let answer = harness.call_ok("tailnet_policy_get", json!({})).await;

    assert_eq!(answer["format"], json!("hujson"));
    assert_eq!(answer["etag"], json!("\"e0b2816b418\""));
    assert_eq!(
        answer["policy"],
        json!(hujson),
        "the comments are the part a person wrote, so the document comes back as written"
    );
    assert_eq!(
        harness.control_plane().only_request().header("accept"),
        Some("application/hujson"),
        "and that is what was asked for"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_json_spelling_is_asked_for_and_comes_back_parsed() {
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/json", "{\"acls\": [{\"action\": \"accept\"}]}"),
        )
        .await
        .start()
        .await;

    let answer = harness
        .call_ok("tailnet_policy_get", json!({"format": "json"}))
        .await;

    assert_eq!(answer["format"], json!("json"));
    assert_eq!(
        answer["policy"],
        json!({"acls": [{"action": "accept"}]}),
        "asked for as JSON, so handed back parsed rather than as a string"
    );
    assert_eq!(
        harness.control_plane().only_request().header("accept"),
        Some("application/json")
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_detailed_report_is_asked_for_without_an_accept_and_is_its_own_shape() {
    // Two rules in one call. The description: "If using this, do not supply an
    // `Accept` parameter in the header." And the report is not the policy, so
    // it does not arrive under `policy` — a `format` of `"details"` would be a
    // value `format` does not accept back.
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::json(json!({
                "acl": "eyJhY2xzIjogW119",
                "warnings": ["a group is not syncing"],
                "errors": [],
            })),
        )
        .await
        .start()
        .await;

    let answer = harness
        .call_ok("tailnet_policy_get", json!({"details": true}))
        .await;

    assert_eq!(
        answer["details"]["warnings"][0],
        json!("a group is not syncing")
    );
    assert_eq!(answer["details"]["acl"], json!("eyJhY2xzIjogW119"));
    assert!(answer.get("policy").is_none(), "{answer:#?}");
    assert!(answer.get("format").is_none(), "{answer:#?}");

    let request = harness.control_plane().only_request();
    assert_eq!(
        request.query.get("details").map(String::as_str),
        Some("true")
    );
    // "If using this, do not supply an `Accept` parameter in the header."
    // What reaches the wire is the HTTP client's own `*/*`, which every client
    // sends and which asks for nothing in particular. What matters is that
    // this server named neither format: `application/hujson` alongside
    // `details` is the combination the description warns about.
    assert_eq!(
        request.header("accept"),
        Some("*/*"),
        "no format should have been asked for: {:?}",
        request.headers
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn asking_for_a_format_and_the_report_at_once_is_refused() {
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::json(json!({"acl": "", "warnings": [], "errors": []})),
        )
        .await
        .start()
        .await;

    let error = harness
        .call_err(
            "tailnet_policy_get",
            json!({"details": true, "format": "json"}),
        )
        .await;
    assert_eq!(error["code"], json!("invalid_args"));
    assert_eq!(harness.control_plane().request_count(), 0);

    harness.shutdown().await;
}

#[tokio::test]
async fn a_policy_write_without_a_guard_never_reaches_the_control_plane() {
    // An answer is arranged that the call must not reach: without the check
    // the write would land, and a write with no `If-Match` overwrites whatever
    // is there — including a change the caller never saw.
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .tier(tailscale_mcp::meta::Tier::Destructive)
        .api_answers(
            "POST",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", "{}"),
        )
        .await
        .start()
        .await;

    let error = harness
        .call_err("tailnet_policy_set", json!({"policy": "{\"acls\": []}"}))
        .await;
    assert_eq!(error["code"], json!("invalid_args"));
    assert_eq!(
        harness.control_plane().request_count(),
        0,
        "nothing should have been sent"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_stale_version_is_a_conflict_that_says_to_read_it_again() {
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .tier(tailscale_mcp::meta::Tier::Destructive)
        .api_answers(
            "POST",
            "/api/v2/tailnet/-/acl",
            Response::status(
                412,
                json!({"message": "precondition failed, invalid old hash"}),
            ),
        )
        .await
        .start()
        .await;

    let error = harness
        .call_err(
            "tailnet_policy_set",
            json!({"policy": "{\"acls\": []}", "etag": "\"stale\""}),
        )
        .await;

    assert_eq!(error["code"], json!("conflict"));
    assert_eq!(error["status"], json!(412));
    assert!(
        error["hint"]
            .as_str()
            .is_some_and(|h| h.contains("tailnet_policy_get")),
        "the remedy is to read it again: {error:#?}"
    );
    assert_eq!(
        harness.control_plane().only_request().header("if-match"),
        Some("\"stale\""),
        "and the version the caller gave is what was on the wire"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn writing_over_the_default_is_the_other_way_to_pass_the_guard() {
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .tier(tailscale_mcp::meta::Tier::Destructive)
        .api_answers(
            "POST",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", "{}"),
        )
        .await
        .start()
        .await;

    harness
        .call_ok(
            "tailnet_policy_set",
            json!({"policy": "{\n  // written by hand\n  \"acls\": [],\n}",
                   "over_default": true}),
        )
        .await;

    let request = harness.control_plane().only_request();
    assert_eq!(request.header("if-match"), Some("\"ts-default\""));
    assert_eq!(
        request.header("content-type"),
        Some("application/hujson"),
        "a document given as a string is sent as HuJSON, comments and all"
    );
    assert!(
        request.body.contains("// written by hand"),
        "and unescaped: {:?}",
        request.body
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_two_validation_modes_are_told_apart_by_what_was_given() {
    // One endpoint, two meanings: an array of tests runs against the policy in
    // force, an object is a hypothetical document. The API reads the body's
    // own JSON type, so sending the wrong one runs the wrong check.
    let path = "/api/v2/tailnet/-/acl/validate";
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .api_answers("POST", path, Response::empty())
        .await
        .api_answers("POST", path, Response::empty())
        .await
        .start()
        .await;

    let tests = json!([{"src": "someone@example.com", "accept": ["10.0.0.1:80"]}]);
    let passed = harness
        .call_ok("tailnet_policy_validate", json!({"tests": tests}))
        .await;
    assert_eq!(
        passed["passed"],
        json!(true),
        "an empty answer is a pass, and says so rather than answering nothing"
    );

    harness
        .call_ok("tailnet_policy_validate", json!({"policy": {"acls": []}}))
        .await;

    let sent = harness.control_plane().recorded();
    assert!(
        sent[0].json().is_array(),
        "the tests go as an array: {sent:#?}"
    );
    assert!(
        sent[1].json().is_object(),
        "the policy goes as an object: {sent:#?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_posture_integration_secret_is_sent_and_never_answered_with() {
    let harness = Setup::new()
        .toolsets("tailnet-posture")
        .tier(tailscale_mcp::meta::Tier::Write)
        .api_answers(
            "POST",
            "/api/v2/tailnet/-/posture/integrations",
            // What the control plane really does: the secret it was given is
            // absent from the answer.
            Response::json(json!({"id": "pi-example", "provider": "falcon"})),
        )
        .await
        .start()
        .await;

    let answer = harness
        .call_ok(
            "tailnet_posture_integration_create",
            json!({"provider": "falcon", "client_secret": "example-secret-value"}),
        )
        .await;

    assert_eq!(answer["id"], json!("pi-example"));
    assert!(
        !serde_json::to_string(&answer)
            .expect("the answer serialises")
            .contains("example-secret-value"),
        "the answer should not carry the secret back: {answer:#?}"
    );
    assert_eq!(
        harness.control_plane().only_request().json()["clientSecret"],
        json!("example-secret-value"),
        "and it should have reached the control plane"
    );

    harness.shutdown().await;
}
