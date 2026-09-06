//! The nine resources and three prompts, as a client actually gets them.
//!
//! Everything here goes through the in-process client and a fully constructed
//! server, which is the seam `spec.md` names for exactly this: what a resource
//! returns and what a prompt expands to are observable there and nowhere else
//! as honestly.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use serde_json::json;
use tailscale_rest::fake::Response;

use harness::Setup;

/// The URIs of everything listed, templates included.
async fn listed(harness: &harness::Harness) -> Vec<String> {
    let mut uris: Vec<String> = harness
        .resources()
        .await
        .into_iter()
        .map(|resource| resource.uri)
        .collect();
    uris.extend(
        harness
            .resource_templates()
            .await
            .into_iter()
            .map(|template| template.uri_template),
    );
    uris.sort();
    uris
}

#[tokio::test]
async fn every_resource_is_listed_when_both_surfaces_are_there() {
    let harness = Setup::new()
        .toolsets("local-status,tailnet-devices")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(json!({})),
        )
        .await
        .start()
        .await;

    assert_eq!(
        listed(&harness).await,
        vec![
            "tailnet://device/{device_id}",
            "tailnet://devices",
            "tailnet://dns",
            "tailnet://policy",
            "tailnet://settings",
            "tailscale://lock",
            "tailscale://netcheck",
            "tailscale://prefs",
            "tailscale://status",
        ],
        "nine, across the two schemes"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_resource_whose_surface_is_not_there_is_absent_rather_than_refused() {
    // A tailnet-only session: no local toolset is selected, so no local
    // backend is on offer and no `tailscale://` resource is either.
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(json!({})),
        )
        .await
        .start()
        .await;

    let uris = listed(&harness).await;
    assert!(
        uris.iter().all(|uri| uri.starts_with("tailnet://")),
        "a local resource should not be listed by a tailnet-only session: {uris:?}"
    );
    assert_eq!(uris.len(), 5);

    // And asking for one by name says why rather than answering it.
    let refused = harness
        .read_resource("tailscale://status")
        .await
        .expect_err("not on offer");
    assert!(
        refused.contains("local"),
        "the refusal should name the missing surface: {refused}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn reading_a_local_resource_answers_what_the_command_printed() {
    let harness = Setup::new().toolsets("local-status").start().await;

    let result = harness
        .read_resource("tailscale://status")
        .await
        .expect("on offer");
    let contents = &result.contents[0];
    let (uri, mime, text) = match contents {
        rmcp::model::ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => (uri, mime_type, text),
        other => panic!("a status resource is text: {other:?}"),
    };
    assert_eq!(uri, "tailscale://status");
    assert_eq!(mime.as_deref(), Some("application/json"));
    assert!(
        text.contains("n1111111CNTRL"),
        "the node's own status, as the command printed it: {text}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_policy_resource_is_served_as_the_document_it_is() {
    let hujson = "{\n  // Who may reach what.\n  \"acls\": [],\n}";
    let harness = Setup::new()
        .toolsets("tailnet-policy")
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", hujson),
        )
        .await
        .start()
        .await;

    let result = harness
        .read_resource("tailnet://policy")
        .await
        .expect("on offer");
    let rmcp::model::ResourceContents::TextResourceContents {
        mime_type, text, ..
    } = &result.contents[0]
    else {
        panic!("a policy is text");
    };
    assert_eq!(
        mime_type.as_deref(),
        Some("application/hujson"),
        "not `application/json`: the comments are the part a person wrote"
    );
    assert!(
        text.contains("// Who may reach what."),
        "and they survive the trip: {text}"
    );

    // The `Accept` says the same thing on the wire.
    assert_eq!(
        harness.control_plane().only_request().header("accept"),
        Some("application/hujson")
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_template_resolves_for_a_device_and_says_so_for_anything_else() {
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers(
            "GET",
            "/api/v2/device/n1111111CNTRL",
            Response::json(json!({"nodeId": "n1111111CNTRL", "hostname": "example-node"})),
        )
        .await
        .start()
        .await;

    let result = harness
        .read_resource("tailnet://device/n1111111CNTRL")
        .await
        .expect("a device");
    let rmcp::model::ResourceContents::TextResourceContents { text, .. } = &result.contents[0]
    else {
        panic!("a device is text");
    };
    assert!(text.contains("example-node"), "{text}");

    // An identifier that is not one: the refusal names the URI rather than
    // reaching the control plane with a path it invented.
    let refused = harness
        .read_resource("tailnet://device/")
        .await
        .expect_err("not a device");
    assert!(refused.contains("tailnet://device/"), "{refused}");
    assert_eq!(
        harness.control_plane().request_count(),
        1,
        "only the real read should have gone out"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn an_unknown_device_is_the_control_planes_answer_rather_than_a_silence() {
    let harness = Setup::new()
        .toolsets("tailnet-devices")
        .api_answers(
            "GET",
            "/api/v2/device/n9999999CNTRL",
            Response::status(404, json!({"message": "device not found"})),
        )
        .await
        .start()
        .await;

    let refused = harness
        .read_resource("tailnet://device/n9999999CNTRL")
        .await
        .expect_err("no such device");
    assert!(
        refused.contains("404") || refused.to_lowercase().contains("not found"),
        "the refusal should carry what the control plane said: {refused}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn no_resource_answers_with_something_a_tool_result_would_have_redacted() {
    // `status --json` carries this node's key material, and a resource is
    // exactly as public as a tool result. Documentation values only, in the
    // shapes the redactor is written for.
    let leaky = json!({
        "Self": {"ID": "n1111111CNTRL"},
        "AuthKey": "tskey-auth-nExAmPlE1-redactedSecretValue",
        "PrivateKey": "privkey:0000000000000000000000000000000000000000000000000000000000000000",
    })
    .to_string();
    let harness = Setup::new()
        .toolsets("local-status,local-lock")
        .cli_answers(
            &["status", "--json"],
            tailscale_cli::stub::Reply::ok(&leaky),
        )
        .cli_answers(
            &["lock", "status", "--json"],
            tailscale_cli::stub::Reply::ok(&leaky),
        )
        .start()
        .await;

    for uri in ["tailscale://status", "tailscale://lock"] {
        let result = harness.read_resource(uri).await.expect("on offer");
        let rmcp::model::ResourceContents::TextResourceContents { text, .. } = &result.contents[0]
        else {
            panic!("text");
        };
        assert!(
            text.contains("n1111111CNTRL"),
            "the rest of the document should survive: {text}"
        );
        for secret in ["tskey-auth-nExAmPlE1", "privkey:0000"] {
            assert!(
                !text.contains(secret),
                "`{uri}` answered with `{secret}`, which a tool result would have removed:\n{text}"
            );
        }
    }

    harness.shutdown().await;
}

#[tokio::test]
async fn all_three_prompts_expand_with_and_without_their_argument() {
    // Under the read tier, which is the point: validation and preview do not
    // mutate, so a read-only session can follow all three to the end. Both
    // surfaces, because a prompt is listed only where the surface it needs is,
    // and this test is about the argument rather than about that.
    let harness = Setup::new()
        .toolsets("local-status,tailnet-policy")
        .start()
        .await;

    let listed: Vec<String> = harness
        .prompts()
        .await
        .into_iter()
        .map(|prompt| prompt.name)
        .collect();
    assert_eq!(
        listed,
        vec![
            "diagnose_connectivity",
            "review_policy_change",
            "audit_tailnet_access"
        ]
    );

    for (name, argument) in [
        ("diagnose_connectivity", "peer"),
        ("review_policy_change", "goal"),
        ("audit_tailnet_access", "subject"),
    ] {
        let without = harness.prompt(name, json!({})).await;
        assert_eq!(without.messages.len(), 1, "`{name}` expands without one");

        let with = harness
            .prompt(name, json!({ argument: "something-particular" }))
            .await;
        assert!(
            format!("{:?}", with.messages).contains("something-particular"),
            "`{name}` should use the `{argument}` it was given"
        );
    }

    harness.shutdown().await;
}

#[tokio::test]
async fn the_policy_prompt_puts_the_write_last_and_only_as_the_operators_call() {
    let harness = Setup::new().toolsets("tailnet-policy").start().await;

    let expanded = harness.prompt("review_policy_change", json!({})).await;
    let text = format!("{:?}", expanded.messages);
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("`{needle}` should be named: {text}"))
    };
    assert!(at("tailnet_policy_get") < at("tailnet_policy_validate"));
    assert!(at("tailnet_policy_validate") < at("tailnet_policy_preview"));
    assert!(at("tailnet_policy_preview") < at("tailnet_policy_set"));

    harness.shutdown().await;
}

/// Every resource, read through the client, answering with what its own
/// backend was told to say.
///
/// The listing test above proves the nine are offered; this proves the nine
/// are *readable*, which is a different claim and the one that catches a
/// resource wired to a path or a command that does not exist. It caught two.
#[tokio::test]
async fn reading_every_resource_reaches_the_backend_behind_it() {
    let hujson = "{\n  // Who may reach what.\n  \"acls\": [],\n}";
    let harness = Setup::new()
        .toolsets("local-status,tailnet-devices")
        .cli_answers(
            &["get", "--json"],
            tailscale_cli::stub::Reply::ok("{\"RouteAll\":true}\n"),
        )
        .cli_answers(
            &["netcheck", "--format=json"],
            tailscale_cli::stub::Reply::ok("{\"UDP\":true}\n"),
        )
        .cli_answers(
            &["lock", "status", "--json"],
            tailscale_cli::stub::Reply::ok("{\"Enabled\":false}\n"),
        )
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", hujson),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(json!({"devices": [{"nodeId": "n2222222CNTRL"}]})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/device/n2222222CNTRL",
            Response::json(json!({"nodeId": "n2222222CNTRL", "name": "peer"})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/dns/configuration",
            Response::json(json!({"magicDNS": true})),
        )
        .await
        .api_answers(
            "GET",
            "/api/v2/tailnet/-/settings",
            Response::json(json!({"devicesApprovalOn": false})),
        )
        .await
        .start()
        .await;

    for (uri, expected) in [
        ("tailscale://status", "n1111111CNTRL"),
        ("tailscale://prefs", "RouteAll"),
        ("tailscale://netcheck", "UDP"),
        ("tailscale://lock", "Enabled"),
        ("tailnet://policy", "Who may reach what"),
        ("tailnet://devices", "n2222222CNTRL"),
        ("tailnet://device/n2222222CNTRL", "peer"),
        ("tailnet://dns", "magicDNS"),
        ("tailnet://settings", "devicesApprovalOn"),
    ] {
        let result = harness
            .read_resource(uri)
            .await
            .unwrap_or_else(|error| panic!("`{uri}` should be readable: {error}"));
        let rmcp::model::ResourceContents::TextResourceContents { text, .. } = &result.contents[0]
        else {
            panic!("`{uri}` should answer with text");
        };
        assert!(
            text.contains(expected),
            "`{uri}` should have reached its backend, and answered `{text}`"
        );
    }

    harness.shutdown().await;
}

/// The preferences resource does not go round an exclusion.
///
/// `debug prefs` prints this node's private keys, which is why the passthrough
/// and the tool surface both refuse it. A resource reading the same thing by
/// another door would undo that, so the resource is held to the same argv the
/// preference tool uses (Q89).
#[tokio::test]
async fn the_preferences_resource_does_not_run_an_excluded_command() {
    let harness = Setup::new()
        .toolsets("local-status")
        .cli_answers(
            &["get", "--json"],
            tailscale_cli::stub::Reply::ok("{\"RouteAll\":true}\n"),
        )
        .start()
        .await;

    harness
        .read_resource("tailscale://prefs")
        .await
        .expect("the sanctioned command answers");
    let ran = harness.cli_calls();
    assert!(
        !ran.iter()
            .any(|argv| argv.starts_with(&["debug".to_owned()])),
        "no `debug` subcommand should have run: {ran:?}"
    );

    harness.shutdown().await;
}

/// A resource is held to the same ceiling as the tool beside it.
///
/// `tailscale://status` and `tailscale_status` answer with the same bytes, so
/// a cap on one and not the other is a cap on neither: a caller wanting the
/// document the tool had just refused could read the resource instead. The
/// module makes the same argument for redaction — "a resource is not a way
/// around that" — and the cap is the other thing a result passes on the way
/// out.
#[tokio::test]
async fn a_resource_is_refused_when_it_is_over_the_result_cap() {
    let harness = Setup::new()
        .toolsets("local-status")
        .env(tailscale_mcp::config::MAX_RESULT_BYTES_ENV, "64")
        .start()
        .await;

    let refused = harness
        .read_resource("tailscale://status")
        .await
        .expect_err("the status document is far over a 64 byte cap");
    assert!(
        refused.contains("64 byte cap"),
        "the refusal should name the ceiling it hit: {refused}"
    );

    harness.shutdown().await;
}

/// And is not refused under the cap this server ships with.
///
/// The default is a mebibyte, so the check above must not be something every
/// session meets: a resource that stopped working by default would be a worse
/// bug than the one being fixed.
#[tokio::test]
async fn the_same_resource_is_answered_under_the_default_cap() {
    let harness = Setup::new().toolsets("local-status").start().await;

    let answered = harness
        .read_resource("tailscale://status")
        .await
        .expect("the default cap is a mebibyte and this document is nowhere near it");
    assert!(
        !answered.contents.is_empty(),
        "the resource should still answer with its document"
    );

    harness.shutdown().await;
}

/// A prompt is listed where its surface is, and nowhere else.
///
/// The two resource listings already work this way, and the reason is the same
/// one `Gate::offers` was written for: a session shown `audit_tailnet_access`
/// with no credential is shown a five-step procedure it would refuse at every
/// step.
#[tokio::test]
async fn a_prompt_is_not_listed_where_its_surface_is_missing() {
    for (toolsets, expected) in [
        (
            "local-status,tailnet-policy",
            vec![
                "diagnose_connectivity",
                "review_policy_change",
                "audit_tailnet_access",
            ],
        ),
        (
            "tailnet-policy",
            vec!["review_policy_change", "audit_tailnet_access"],
        ),
        ("local-status", vec!["diagnose_connectivity"]),
    ] {
        let harness = Setup::new().toolsets(toolsets).start().await;
        let listed: Vec<String> = harness
            .prompts()
            .await
            .into_iter()
            .map(|prompt| prompt.name)
            .collect();
        assert_eq!(listed, expected, "with the toolsets `{toolsets}`");
        harness.shutdown().await;
    }
}

/// And asking for one anyway says which surface it wanted.
#[tokio::test]
async fn asking_for_a_prompt_whose_surface_is_missing_says_which_one() {
    let harness = Setup::new().toolsets("local-status").start().await;

    let refusal = harness.prompt_refusal("audit_tailnet_access").await;
    assert!(
        refusal.contains("tailnet"),
        "the refusal should name the surface it wanted: {refusal}"
    );

    // A name that is not a prompt at all is still the other answer.
    let unknown = harness.prompt_refusal("no_such_prompt").await;
    assert!(
        unknown.contains("is not a prompt"),
        "an unknown name is not a missing surface: {unknown}"
    );

    harness.shutdown().await;
}

/// The one prompt that reads from both surfaces stops at what it has.
///
/// `diagnose_connectivity` is offered wherever the local surface is, including
/// the commonest session of all: a machine with the CLI and no credential. Its
/// last two steps are control-plane reads, and naming them there would send
/// the model at two tools this session was never offered.
#[tokio::test]
async fn the_diagnosis_drops_its_control_plane_steps_without_a_credential() {
    let local = Setup::new().toolsets("local-status").start().await;
    let text = format!(
        "{:?}",
        local.prompt("diagnose_connectivity", json!({})).await
    );
    assert!(
        text.contains("tailscale_status") && text.contains("tailscale_ping"),
        "the local steps are the ones that survive: {text}"
    );
    for absent in ["tailnet_device_list", "tailnet_policy_preview"] {
        assert!(
            !text.contains(absent),
            "`{absent}` is not a tool this session has: {text}"
        );
    }
    local.shutdown().await;

    // With a credential they come back, which is what makes the omission a
    // property of the session rather than of the prompt.
    let both = Setup::new()
        .toolsets("local-status,tailnet-devices")
        .start()
        .await;
    let text = format!(
        "{:?}",
        both.prompt("diagnose_connectivity", json!({})).await
    );
    for present in ["tailnet_device_list", "tailnet_policy_preview"] {
        assert!(
            text.contains(present),
            "`{present}` belongs in a session that has the control plane: {text}"
        );
    }
    both.shutdown().await;
}
