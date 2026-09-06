//! What a session says it offers is what it offers.
//!
//! Two places describe the selection to somebody: the startup note, which
//! reaches an operator's terminal, and the instructions, which reach the
//! model. Both listed `gate.toolsets()` — what was *asked for* — while the
//! count and the tool listing beside them reported what came of it. In a
//! session without a credential the instructions said "The tailnet surface is
//! not available in this session, so no `tailnet_*` tool is offered" and then,
//! eleven lines later, "Toolsets offered: ..., tailnet-devices, ...". A model
//! reading the second calls a tool the first said was not there.
//!
//! Both now go through `Gate::offered_toolsets`. This holds the property at
//! the seam a caller actually sees, so a third renderer that reaches for the
//! unfiltered list fails here rather than in somebody's session.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use harness::Setup;
use serde_json::Value;
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::meta::Tier;
use tailscale_mcp::subcommands;

/// Every toolset named in either rendering is one with a tool behind it.
#[tokio::test]
async fn a_session_without_a_tailnet_names_no_tailnet_toolset() {
    let harness = Setup::new()
        .preset("full")
        .without_credentials()
        .start()
        .await;

    let offered: Vec<String> = harness
        .tools()
        .await
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    assert!(
        offered.iter().all(|name| !name.starts_with("tailnet_")),
        "the premise of this test is a session with no tailnet tools: {offered:?}"
    );

    let instructions = harness.instructions();
    assert!(
        !instructions.contains("tailnet-"),
        "the instructions name a tailnet toolset in a session that offers none, \
         which is what a model reads before calling one: {instructions}"
    );
}

/// And the converse, so the filter is not simply hiding everything.
#[tokio::test]
async fn a_session_with_a_tailnet_still_names_its_toolsets() {
    let harness = Setup::new().preset("full").start().await;

    let instructions = harness.instructions();
    assert!(
        instructions.contains("tailnet-devices"),
        "a session that offers the tailnet should say so: {instructions}"
    );
    assert!(
        instructions.contains("local-status"),
        "and should not have dropped the local half either: {instructions}"
    );
}

/// The `tools` listing agrees with itself.
///
/// Its `count` and `tools` come from the gate and its `toolsets` came from the
/// configuration, so `--no-tailnet` produced one document reporting 29 tools,
/// every one of them local, under a list naming nine tailnet toolsets.
#[test]
fn the_tools_listing_names_no_toolset_its_own_tools_do_not_come_from() {
    let cli = <Cli as clap::Parser>::try_parse_from(["tailscale-mcp", "--no-tailnet"])
        .expect("the arguments parse");
    let config = Config::resolve_with(cli, |_| None).expect("the configuration resolves");
    let report = subcommands::tools(&config, true);
    let parsed: Value = serde_json::from_str(&report.text).expect("JSON");

    let named: Vec<&str> = parsed["toolsets"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|toolset| toolset.as_str().expect("a name"))
        .collect();
    let surfaces: Vec<&str> = parsed["tools"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|tool| tool["surface"].as_str().expect("a surface"))
        .collect();

    assert!(
        surfaces.iter().all(|surface| *surface == "local"),
        "the premise is a listing whose tools are all local: {surfaces:?}"
    );
    assert!(
        named.iter().all(|name| !name.starts_with("tailnet-")),
        "the listing names a tailnet toolset none of its own tools came from: {named:?}"
    );
}

/// Guidance for a tool is offered only where the tool is.
///
/// `tailscale_run` gets a paragraph of its own, and the paragraph appeared
/// whenever the passthrough toolset was *selected* — so a session with the
/// local surface switched off introduced it four lines after saying no
/// `tailscale_*` tool is offered.
#[tokio::test]
async fn the_passthrough_paragraph_follows_the_tool_it_describes() {
    let without = Setup::new()
        .preset("full")
        .toolsets("+local-passthrough")
        .without_cli()
        .start()
        .await;
    assert!(
        !without.instructions().contains("tailscale_run"),
        "no local surface, so no `tailscale_run` to introduce: {}",
        without.instructions()
    );

    let with = Setup::new()
        .preset("full")
        .toolsets("+local-passthrough")
        .start()
        .await;
    assert!(
        with.instructions().contains("tailscale_run"),
        "and where it is offered it still gets its paragraph: {}",
        with.instructions()
    );
}

/// The instructions explain `confirm` exactly where a tool takes one.
///
/// This is the same defect as the one above, one paragraph further down. Every
/// session was told what a `confirm` argument is for; at the read tier — the
/// default — no offered tool has one, so the advice described a tool the
/// session could not see. The check is written as an equivalence rather than
/// two assertions so that the paragraph cannot drift away from the tool table
/// in either direction.
#[tokio::test]
async fn confirm_is_explained_in_exactly_the_sessions_that_offer_one() {
    for tier in [Tier::Read, Tier::Write, Tier::Destructive] {
        let harness = Setup::new().preset("full").tier(tier).start().await;
        let explained = harness.instructions().contains("`confirm` argument");
        let offered = harness
            .tools()
            .await
            .iter()
            .filter(|tool| {
                tool.input_schema
                    .get("properties")
                    .and_then(|properties| properties.get("confirm"))
                    .is_some()
            })
            .count();
        assert_eq!(
            explained,
            offered > 0,
            "at {tier:?} the instructions {} `confirm` while {offered} tools take one",
            if explained {
                "explain"
            } else {
                "do not explain"
            }
        );
        harness.shutdown().await;
    }
}

/// And the identifier advice describes the surfaces the session has.
#[tokio::test]
async fn the_identifier_advice_names_no_surface_this_session_lacks() {
    let harness = Setup::new().preset("full").start().await;
    let said = harness.instructions();
    assert!(said.contains("node ID"), "both surfaces are here: {said}");
    harness.shutdown().await;

    // The contradiction: "no `tailnet_*` tool is offered", and two paragraphs
    // later what the `tailnet_*` tools accept.
    let harness = Setup::new().preset("full").without_tailnet().start().await;
    let said = harness.instructions();
    assert!(said.contains("no `tailnet_*` tool is offered"), "{said}");
    assert!(
        !said.contains("node ID") && !said.contains("tailnet's device list"),
        "a session with no tailnet surface should not describe the tailnet tools: {said}"
    );
    harness.shutdown().await;
}

/// Nothing tells a session that defaulting a tailnet to our own is fine.
///
/// One tool in the table takes a `tailnet` argument and it deletes one, so
/// "`-` means the tailnet the credential belongs to and is almost always
/// right" was advice pointing the wrong way in the only place it applied. The
/// sweep is over every tier because the sentence was unconditional.
#[tokio::test]
async fn no_session_is_told_to_default_a_tailnet_to_our_own() {
    for tier in [Tier::Read, Tier::Write, Tier::Destructive] {
        let harness = Setup::new().preset("full").tier(tier).start().await;
        let said = harness.instructions();
        assert!(
            !said.contains("almost always right") && !said.contains("Where a tailnet must be"),
            "at {tier:?}: {said}"
        );
        // And the tool that does take one still asks for it by name.
        if let Some(tool) = harness.tool("tailnet_organization_tailnet_delete").await {
            let description = tool.input_schema["properties"]["tailnet"]["description"]
                .as_str()
                .unwrap_or_default();
            assert!(
                description.contains("its id") && !description.contains('-'),
                "the one tool taking a tailnet should ask for it explicitly: {description}"
            );
        }
        harness.shutdown().await;
    }
}

/// The metadata and the generated schema agree about which tools ask for one.
///
/// `Offered` reads the three metadata flags; the client sees a `confirm`
/// property on a schema. Those are two descriptions of the same fact, and the
/// paragraph above is only as honest as their agreement — a tool that grew a
/// `confirm` parameter without a flag would be a tool the instructions never
/// mention. Checked per tool rather than per session, because a session-wide
/// count hides a mismatch as long as one other tool still matches.
#[tokio::test]
async fn a_tool_asks_for_confirmation_exactly_when_its_metadata_says_so() {
    let harness = Setup::new()
        .preset("full")
        .tier(Tier::Destructive)
        .start()
        .await;
    let registry = tailscale_mcp::registry::Registry::new(tailscale_mcp::tools::entries())
        .expect("a registry");

    let mut disagreed = Vec::new();
    for tool in harness.tools().await {
        let in_schema = tool
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("confirm"))
            .is_some();
        let in_metadata = registry
            .get(&tool.name)
            .expect("a listed tool is registered")
            .meta
            .takes_confirmation();
        if in_schema != in_metadata {
            disagreed.push(format!(
                "{}: schema says {in_schema}, metadata says {in_metadata}",
                tool.name
            ));
        }
    }
    assert!(
        disagreed.is_empty(),
        "the metadata and the schema disagree about confirmation:\n  {}",
        disagreed.join("\n  ")
    );
    harness.shutdown().await;
}
