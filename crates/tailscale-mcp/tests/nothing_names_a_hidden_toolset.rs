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
