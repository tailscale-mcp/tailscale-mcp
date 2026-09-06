//! The tailnet surface accepts the identifiers the instructions promise.
//!
//! Before this, the instructions told every session that "a device can be named
//! by its node ID, one of its Tailscale IP addresses, or its MagicDNS name",
//! and only the first of those was true of the `tailnet_*` tools: the other two
//! came back `not_found` from the control plane. The local surface had always
//! been fine, because the CLI resolves names itself.
//!
//! What holds it now: a name is looked up in the tailnet's own listing, an
//! identifier is passed through untouched, and a name matching two devices is
//! refused rather than guessed at — which is the case that would otherwise end
//! with the wrong device deleted.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;
mod repo;

use serde_json::json;
use tailscale_rest::fake::Response;

use harness::Setup;
use tailscale_mcp::meta::Tier;

/// How many tools take a device. Pinned so that the sweep below cannot pass by
/// finding nothing.
const DEVICE_ID_PARAMETERS: usize = 17;

/// Two devices, the second sharing the first's *hostname* but not its name.
///
/// Hostnames are not unique — two machines called `macbook-air` is an ordinary
/// state of affairs — which is the whole reason resolution has to refuse.
fn listing() -> serde_json::Value {
    json!({"devices": [
        {
            "nodeId": "n1111111CNTRL",
            "id": "111",
            "name": "laptop.example-tailnet.ts.net",
            "hostname": "shared-hostname",
            "addresses": ["100.64.0.1", "fd7a:115c:a1e0::1"]
        },
        {
            "nodeId": "n2222222CNTRL",
            "id": "222",
            "name": "desktop.example-tailnet.ts.net",
            "hostname": "shared-hostname",
            "addresses": ["100.64.0.2"]
        }
    ]})
}

async fn harness_with_listing() -> harness::Harness {
    Setup::new()
        .toolsets("tailnet-devices")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/device/n1111111CNTRL",
            Response::json(
                json!({"nodeId": "n1111111CNTRL", "name": "laptop.example-tailnet.ts.net"}),
            ),
        )
        .await
        .start()
        .await
}

/// Every name a person might use for a device reaches the same device.
#[tokio::test]
async fn each_way_of_naming_one_device_reaches_that_device() {
    for named_by in [
        "laptop.example-tailnet.ts.net", // the MagicDNS name
        "laptop",                        // the short name, which is what people type
        "100.64.0.1",                    // an address
        "fd7a:115c:a1e0::1",             // and the other kind of address
        "LAPTOP",                        // case is not part of a name
    ] {
        let harness = harness_with_listing().await;
        let answer = harness
            .call_ok("tailnet_device_get", json!({"device_id": named_by}))
            .await;
        assert_eq!(
            answer["nodeId"], "n1111111CNTRL",
            "`{named_by}` should reach the device it names"
        );
        harness.shutdown().await;
    }
}

/// An identifier is not looked up, because it is already the answer.
///
/// This is what keeps every call that worked before working identically: a node
/// id costs no listing, so the change is confined to values that used to fail.
#[tokio::test]
async fn an_identifier_is_used_as_given_and_costs_no_listing() {
    for identifier in ["n1111111CNTRL", "111"] {
        let harness = Setup::new()
            .toolsets("tailnet-devices")
            .api_answers(
                "GET",
                &format!("/api/v2/device/{identifier}"),
                Response::json(json!({"nodeId": "n1111111CNTRL"})),
            )
            .await
            .start()
            .await;

        harness
            .call_ok("tailnet_device_get", json!({"device_id": identifier}))
            .await;
        let paths: Vec<String> = harness
            .control_plane()
            .recorded()
            .into_iter()
            .map(|request| request.path)
            .collect();
        assert!(
            !paths.iter().any(|path| path.contains("/devices")),
            "`{identifier}` needed no listing, but asked for one: {paths:?}"
        );
        harness.shutdown().await;
    }
}

/// A name matching two devices is refused, and says which two.
#[tokio::test]
async fn a_name_matching_two_devices_is_refused_with_both_named() {
    let harness = harness_with_listing().await;
    let refusal = harness
        .call_err(
            "tailnet_device_get",
            json!({"device_id": "shared-hostname"}),
        )
        .await;

    assert_eq!(refusal["code"], "invalid_args");
    let said = refusal["message"].as_str().expect("a message");
    for candidate in ["n1111111CNTRL", "n2222222CNTRL"] {
        assert!(
            said.contains(candidate),
            "the refusal should name `{candidate}` so the caller can retry: {said}"
        );
    }
    harness.shutdown().await;
}

/// A name matching nothing says what was actually searched.
#[tokio::test]
async fn a_name_matching_nothing_says_which_fields_were_searched() {
    let harness = harness_with_listing().await;
    let refusal = harness
        .call_err(
            "tailnet_device_get",
            json!({"device_id": "no-such-machine"}),
        )
        .await;

    assert_eq!(refusal["code"], "not_found");
    let said = refusal["message"].as_str().expect("a message");
    assert!(
        said.contains("hostname") && said.contains("MagicDNS"),
        "a caller should learn which names were tried, not just that it failed: {said}"
    );
    harness.shutdown().await;
}

/// A burst of calls reads the listing once.
#[tokio::test]
async fn a_burst_of_named_calls_reads_the_listing_once() {
    let harness = harness_with_listing().await;
    for _ in 0..4 {
        harness
            .call_ok("tailnet_device_get", json!({"device_id": "laptop"}))
            .await;
    }
    let listings = harness
        .control_plane()
        .recorded()
        .into_iter()
        .filter(|request| request.path.contains("/devices"))
        .count();
    assert_eq!(
        listings, 1,
        "four calls by name should share one listing, not fetch one each"
    );
    harness.shutdown().await;
}

/// No handler addresses a device straight from what the caller wrote.
///
/// The behavioural tests above prove one path. This proves there is only one
/// path: a tool added later that builds its own device path from
/// `params.device_id` would accept node ids and refuse names, which is the
/// inconsistency this ticket exists to remove, and it would do so silently.
#[test]
fn no_handler_builds_a_device_path_from_what_the_caller_wrote() {
    let tools = repo::root().join("crates/tailscale-mcp/src/tools");
    let mut offences = Vec::new();
    for entry in std::fs::read_dir(&tools).expect("the tools directory") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a source file");
        for (number, line) in source.lines().enumerate() {
            let builds_a_path = ["device_path(", "device_invites_path(", "approval_suffix("]
                .iter()
                .any(|builder| line.contains(builder));
            if builds_a_path && line.contains("&params.device_id") {
                offences.push(format!(
                    "{}:{}: {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "these address a device without resolving what the caller named first:\n{}",
        offences.join("\n")
    );
}

/// What the session is told matches what the tools take.
#[tokio::test]
async fn the_instructions_and_the_parameters_agree_on_how_to_name_a_device() {
    // Every toolset, every tier: the parameters that take a device live in
    // four different files, and a narrower surface would leave some of them
    // uninspected — which is exactly the blindness this test exists to avoid.
    let harness = Setup::new()
        .preset("full")
        .tier(Tier::Destructive)
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .start()
        .await;

    let said = harness.instructions();
    for promised in ["MagicDNS name", "hostname", "node ID"] {
        assert!(
            said.contains(promised),
            "the instructions should name `{promised}` as a way to name a device: {said}"
        );
    }

    // And every parameter that takes one says the same, because a model that
    // reads the schema rather than the preamble has to arrive at the same place.
    let mut inspected = 0_usize;
    for tool in harness.tools().await {
        let Some(device_id) = tool
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("device_id"))
        else {
            continue;
        };
        let description = device_id
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(
            description.contains("MagicDNS"),
            "`{}`'s `device_id` does not say a name will do: {description}",
            tool.name
        );
        inspected += 1;
    }

    // And say how many were looked at, so that a surface which stopped
    // offering them would fail here rather than pass vacuously.
    assert_eq!(
        inspected, DEVICE_ID_PARAMETERS,
        "the number of tools taking a device changed; if that is intended, \
         move the count, but check the new ones say a name will do"
    );

    harness.shutdown().await;
}
