//! The four completable slots, and what each of them offers.
//!
//! Completion covers a prompt's argument and a resource template's variable,
//! and nothing else — the protocol has no reference type for a tool argument.
//! So the reach of this is four slots, and the one that matters is the device
//! template's, which asks for exactly the identifier the tools ask for.
//!
//! What holds it: every value offered is one the server would accept back, the
//! slots that have no knowable set say so by offering nothing, and no failure
//! anywhere underneath turns into an error the client has to explain.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use serde_json::{Value, json};
use tailscale_cli::stub::Reply;
use tailscale_rest::fake::Response;

use harness::Setup;

const TEMPLATE: &str = "tailnet://device/{device_id}";

/// Three devices, one of them sharing another's hostname.
fn listing() -> Value {
    json!({"devices": [
        {
            "nodeId": "n1111111CNTRL",
            "name": "workstation.example-tailnet.ts.net",
            "hostname": "workstation",
            "addresses": ["100.64.0.1"],
            "user": "alice@example.com",
            "tags": ["tag:redacted-server"]
        },
        {
            "nodeId": "n2222222CNTRL",
            "name": "laptop.example-tailnet.ts.net",
            "hostname": "shared-hostname",
            "addresses": ["100.64.0.2"],
            "user": "alice@example.com"
        },
        {
            "nodeId": "n3333333CNTRL",
            "name": "tablet.example-tailnet.ts.net",
            "hostname": "shared-hostname",
            "addresses": ["100.64.0.3"],
            "user": "bob@example.com"
        }
    ]})
}

/// Two peers this node can see, one of them offline and one with no MagicDNS
/// name at all.
fn status_with_peers() -> Value {
    let mut status = harness::status_json();
    status["Peer"] = json!({
        "nodekey:2222222222222222222222222222222222222222222222222222222222222222": {
            "HostName": "laptop",
            "DNSName": "laptop.example-tailnet.ts.net.",
            "TailscaleIPs": ["100.64.0.2"],
            "Online": false
        },
        "nodekey:3333333333333333333333333333333333333333333333333333333333333333": {
            "HostName": "printer",
            "DNSName": "",
            "TailscaleIPs": ["100.64.0.3"],
            "Online": true
        },
        "nodekey:4444444444444444444444444444444444444444444444444444444444444444": {
            "HostName": "a-laptop",
            "DNSName": "a-laptop.example-tailnet.ts.net.",
            "TailscaleIPs": ["100.64.0.4"],
            "Online": true
        }
    });
    status
}

async fn tailnet() -> harness::Harness {
    Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .start()
        .await
}

/// The slot that carries the feature: a device, by any of its names.
#[tokio::test]
async fn a_device_is_offered_by_the_name_that_will_resolve() {
    let harness = tailnet().await;

    let offered = harness
        .complete_resource(TEMPLATE, "device_id", "lap")
        .await;
    assert_eq!(
        offered.values,
        vec!["laptop.example-tailnet.ts.net"],
        "typing part of a name should find the device that wears it"
    );

    // Found by hostname and by address, but still offered as the MagicDNS
    // name — because what the client inserts is the string in `values`, and a
    // hostname two devices share is one `resolve` refuses.
    let by_host = harness
        .complete_resource(TEMPLATE, "device_id", "shared-hostname")
        .await;
    assert_eq!(
        by_host.values,
        vec![
            "laptop.example-tailnet.ts.net",
            "tablet.example-tailnet.ts.net"
        ],
        "a shared hostname should offer both devices by their own names"
    );
    let by_address = harness
        .complete_resource(TEMPLATE, "device_id", "100.64.0.3")
        .await;
    assert_eq!(by_address.values, vec!["tablet.example-tailnet.ts.net"]);

    harness.shutdown().await;
}

/// Every value offered is one the server would take back.
///
/// This is the property the whole design turns on: a popup that inserts a
/// string the next call refuses is worse than a popup that offers nothing.
#[tokio::test]
async fn every_value_offered_resolves_to_exactly_one_device() {
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/device/n1111111CNTRL",
            Response::json(json!({"nodeId": "n1111111CNTRL"})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/device/n2222222CNTRL",
            Response::json(json!({"nodeId": "n2222222CNTRL"})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/device/n3333333CNTRL",
            Response::json(json!({"nodeId": "n3333333CNTRL"})),
        )
        .await
        .start()
        .await;

    let offered = harness.complete_resource(TEMPLATE, "device_id", "").await;
    assert_eq!(offered.values.len(), 3, "every device, on an empty input");
    for value in &offered.values {
        let answer = harness
            .call_ok("tailnet_device_get", json!({"device_id": value}))
            .await;
        assert!(
            answer["nodeId"].as_str().is_some(),
            "`{value}` was offered but does not address a device: {answer}"
        );
    }

    harness.shutdown().await;
}

/// Ranked, and the same order every time.
#[tokio::test]
async fn an_exact_match_comes_before_a_prefix_and_a_prefix_before_the_rest() {
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(json!({"devices": [
                {"nodeId": "n1111111CNTRL", "name": "zeta.example-tailnet.ts.net",
                 "hostname": "box", "addresses": []},
                {"nodeId": "n2222222CNTRL", "name": "box.example-tailnet.ts.net",
                 "hostname": "box-two", "addresses": []},
                {"nodeId": "n3333333CNTRL", "name": "alpha-box.example-tailnet.ts.net",
                 "hostname": "alpha", "addresses": []}
            ]})),
        )
        .await
        .start()
        .await;

    let offered = harness
        .complete_resource(TEMPLATE, "device_id", "box")
        .await;
    assert_eq!(
        offered.values,
        vec![
            // `box` is the first one's hostname exactly, and the second's own
            // short name exactly; alphabetical order separates those two.
            "box.example-tailnet.ts.net",
            "zeta.example-tailnet.ts.net",
            // and this one merely contains it.
            "alpha-box.example-tailnet.ts.net",
        ],
        "exact, then prefix, then contained — alphabetical inside each"
    );

    harness.shutdown().await;
}

/// The peer slot reads the node, not the tailnet.
#[tokio::test]
async fn a_peer_is_offered_by_the_name_a_person_would_type() {
    let harness = Setup::new()
        .cli_answers(
            &["status", "--json"],
            Reply::ok(status_with_peers().to_string()),
        )
        .start()
        .await;

    let offered = harness
        .complete_prompt("diagnose_connectivity", "peer", "")
        .await;
    assert_eq!(
        offered.values,
        vec!["a-laptop", "laptop", "printer"],
        "the short name, and the hostname when there is no MagicDNS name; an \
         offline peer is exactly what this prompt is asked about, so it stays"
    );

    // This node is not a peer of itself, and `diagnose_connectivity` asked
    // about it would have nothing to diagnose.
    assert!(
        !offered.values.iter().any(|value| value == "workstation"),
        "this node should not be offered as a peer: {:?}",
        offered.values
    );

    // `DNSName` is fully qualified and carries the root label, which nobody
    // types. Stripped, the name a person writes is an exact match and sorts
    // first; kept, it would merely be contained in both and `a-laptop` would
    // come first on the alphabet.
    // Either spelling, because `tailscale status` prints the root label and a
    // caller may well have pasted what it printed.
    for typed in [
        "laptop.example-tailnet.ts.net",
        "laptop.example-tailnet.ts.net.",
    ] {
        let qualified = harness
            .complete_prompt("diagnose_connectivity", "peer", typed)
            .await;
        assert_eq!(
            qualified.values,
            vec!["laptop", "a-laptop"],
            "`{typed}` names one peer exactly, and it should come first"
        );
    }

    harness.shutdown().await;
}

/// The subject slot draws on users and on tags from both ends.
#[tokio::test]
async fn a_subject_is_a_user_or_a_tag() {
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/users",
            Response::json(json!({"users": [
                {"loginName": "alice@example.com"},
                {"loginName": "bob@example.com"}
            ]})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::json(json!({"tagOwners": {"tag:redacted-ci": ["alice@example.com"]}})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .start()
        .await;

    let offered = harness
        .complete_prompt("audit_tailnet_access", "subject", "")
        .await;
    assert_eq!(
        offered.values,
        vec![
            "alice@example.com",
            "bob@example.com",
            // declared in the policy, worn by nothing
            "tag:redacted-ci",
            // worn by a device, whatever the policy says
            "tag:redacted-server",
        ],
        "users, tags declared, and tags in use"
    );

    harness.shutdown().await;
}

/// A free-text argument has no set to draw from, and says so.
#[tokio::test]
async fn a_slot_with_nothing_to_offer_offers_nothing() {
    // Every source answers, so that a slot which started drawing on one would
    // come back with values rather than with an empty list for its own reasons.
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/users",
            Response::json(json!({"users": [{"loginName": "alice@example.com"}]})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::json(json!({"tagOwners": {"tag:redacted-ci": []}})),
        )
        .await
        .start()
        .await;

    for (prompt, argument) in [
        // A sentence saying what a policy change is for. Completing this would
        // be inventing the caller's intent.
        ("review_policy_change", "goal"),
        // Not an argument these prompts have.
        ("diagnose_connectivity", "subject"),
        ("audit_tailnet_access", "peer"),
    ] {
        let offered = harness.complete_prompt(prompt, argument, "").await;
        assert!(
            offered.values.is_empty(),
            "`{prompt}`'s `{argument}` should offer nothing, and offered {:?}",
            offered.values
        );
    }

    // And a reference this server does not serve at all.
    let unknown = harness
        .complete_resource("tailnet://nothing/{id}", "id", "")
        .await;
    assert!(unknown.values.is_empty());

    harness.shutdown().await;
}

/// A source that cannot answer is an empty popup, never an error.
#[tokio::test]
async fn a_source_that_fails_completes_to_nothing_rather_than_failing() {
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::status(429, json!({"message": "slow down"})),
        )
        .await
        .start()
        .await;

    // `complete_resource` panics if the request itself errors, which is the
    // assertion: the control plane refused and the client still got an answer.
    let offered = harness
        .complete_resource(TEMPLATE, "device_id", "lap")
        .await;
    assert!(offered.values.is_empty());

    harness.shutdown().await;
}

/// A slot whose surface this session does not have is not attempted.
#[tokio::test]
async fn a_slot_needing_a_surface_this_session_lacks_offers_nothing() {
    // A control plane that would answer, and a session that will not ask it:
    // the recorder is what turns "offers nothing" into "did not even try".
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .without_tailnet()
        .start()
        .await;

    let offered = harness.complete_resource(TEMPLATE, "device_id", "").await;
    assert!(
        offered.values.is_empty(),
        "a session with no control plane cannot list devices"
    );
    assert_eq!(
        harness.control_plane().request_count(),
        0,
        "and should not have tried"
    );

    harness.shutdown().await;
}

/// A tailnet larger than the cap is cut, and says so rather than pretending.
///
/// The protocol caps a completion at a hundred values. A server that silently
/// returned its first hundred would tell a caller it had seen everything.
#[tokio::test]
async fn more_devices_than_fit_are_counted_even_though_they_are_not_sent() {
    let many: Vec<Value> = (0..150)
        .map(|n| {
            json!({
                "nodeId": format!("n{n:07}CNTRL"),
                "name": format!("device-{n:03}.example-tailnet.ts.net"),
                "hostname": format!("device-{n:03}"),
                "addresses": []
            })
        })
        .collect();
    let harness = Setup::new()
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(json!({"devices": many})),
        )
        .await
        .start()
        .await;

    let offered = harness.complete_resource(TEMPLATE, "device_id", "").await;
    assert_eq!(offered.values.len(), 100, "the cap the protocol sets");
    assert_eq!(offered.total, Some(150), "and the truth about the rest");
    assert_eq!(offered.has_more, Some(true));

    // Narrowed to something that fits, nothing is being held back.
    let narrowed = harness
        .complete_resource(TEMPLATE, "device_id", "device-14")
        .await;
    assert_eq!(narrowed.values.len(), 10, "device-140 through device-149");
    assert_eq!(narrowed.total, Some(10));
    assert_eq!(narrowed.has_more, Some(false));

    harness.shutdown().await;
}

/// A surface the operator switched off is not completed from either.
///
/// This is the case the gate exists for, and it is not the same as having no
/// credential: a session started with only local toolsets has a working
/// control-plane client and offers no tailnet tools. Completing a device there
/// would hand back the contents of a tailnet whose tools the operator
/// deliberately did not select.
#[tokio::test]
async fn a_surface_whose_toolsets_were_not_selected_offers_nothing() {
    let harness = Setup::new()
        .toolsets("local-status")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(listing()),
        )
        .await
        .start()
        .await;

    let offered = harness.complete_resource(TEMPLATE, "device_id", "").await;
    assert!(
        offered.values.is_empty(),
        "no tailnet toolset was selected, so there is nothing to complete: {:?}",
        offered.values
    );
    assert_eq!(
        harness.control_plane().request_count(),
        0,
        "and the credential should not have been spent finding that out"
    );

    harness.shutdown().await;
}

/// The capability is declared, because a client that cannot see it will not ask.
#[tokio::test]
async fn the_server_says_it_completes() {
    let harness = Setup::new().start().await;
    assert!(
        harness.info().capabilities.completions.is_some(),
        "`completions` should be declared: {:?}",
        harness.info().capabilities
    );
    harness.shutdown().await;
}

/// A session asking faster than the limit is answered emptily, not endlessly.
///
/// The specification says a server SHOULD rate limit this method and its
/// security section says MUST, and the reason is upstream: every keystroke is
/// a request, and the device slot's source is a control plane that answers 429.
#[tokio::test]
async fn a_session_asking_too_fast_is_slowed_down() {
    let harness = tailnet().await;

    // Far more than the reserve, and far more than any plausible refill over
    // the time an in-process burst takes: at twenty a second, four hundred
    // requests would need nineteen seconds of elapsed time to be allowed.
    let mut refused = 0;
    for _ in 0..400 {
        if harness
            .complete_resource(TEMPLATE, "device_id", "lap")
            .await
            .values
            .is_empty()
        {
            refused += 1;
        }
    }
    assert!(
        refused > 0,
        "a burst of four hundred should have run out of budget, and none did"
    );

    harness.shutdown().await;
}
