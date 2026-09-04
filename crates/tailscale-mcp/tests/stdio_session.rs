//! What a client sees, over a transport, with a table it can be sure of.
//!
//! The contract test covers the real tools; this one covers the machinery they
//! run on, using a table declared here so that a case can exist before the
//! tool that would exercise it does. What it checks — the handshake, schemas,
//! hiding, confirmation, and that a failed call is a result rather than a
//! dropped session — has to hold for every tool that will ever be added.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tailscale_cli::Invocation;
use tailscale_mcp::context::ToolContext;
use tailscale_mcp::error::ToolResult;
use tailscale_mcp::meta::Tier;

use harness::{Harness, Setup};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

/// Arguments a client has to get right, so that the schema can be checked.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EchoParams {
    /// What to say back.
    pub message: String,
}

tailscale_mcp::tools! {
    /// Say something back.
    tailscale_session_echo => EchoParams, echo,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Ask the local binary something.
    tailscale_session_ask => NoParams, ask,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Change something about this node.
    tailscale_session_set => NoParams, done,
        toolset: LocalPrefs, tier: Write;

    /// Act on the tailnet, irreversibly.
    tailnet_session_delete => NoParams, done,
        toolset: TailnetDevices, tier: Destructive, confirm: true;
}

async fn echo(_ctx: &ToolContext, params: EchoParams) -> ToolResult<Value> {
    Ok(json!({ "message": params.message }))
}

async fn ask(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let text = tailscale_mcp::cli::run_text(
        ctx,
        &metas::tailscale_session_ask,
        Invocation::read(["ask"]),
    )
    .await?;
    Ok(json!({ "text": text.trim() }))
}

async fn done(_ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    Ok(json!({ "done": true }))
}

/// A session offering the table above, at the given tier.
async fn session(tier: Tier) -> Harness {
    Setup::new()
        .entries(entries())
        .toolsets("local-status,local-prefs,tailnet-devices")
        .tier(tier)
        .cli_answers(&["ask"], tailscale_cli::stub::Reply::ok("answered\n"))
        .start()
        .await
}

#[tokio::test]
async fn a_client_connects_and_is_told_what_this_server_is() {
    let harness = session(Tier::Read).await;
    let info = harness.info();

    let implementation = info.server_info.as_ref().expect("the server names itself");
    assert_eq!(implementation.name, "tailscale-mcp");
    assert_eq!(implementation.version, env!("CARGO_PKG_VERSION"));
    assert!(info.capabilities.tools.is_some(), "tools are advertised");

    let instructions = harness.instructions();
    assert!(instructions.contains("tailnet_*"), "{instructions}");
    assert!(
        instructions.contains("Permitted tier: read."),
        "{instructions}"
    );
    assert!(
        instructions.contains(harness::TEST_CLI_VERSION),
        "{instructions}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn the_listing_describes_each_tool_well_enough_to_call_it() {
    let harness = session(Tier::Read).await;

    let echo = harness
        .tool("tailscale_session_echo")
        .await
        .expect("the read tool is offered");
    assert!(
        echo.description.as_deref().is_some_and(|d| !d.is_empty()),
        "a tool without a description cannot be chosen"
    );
    let schema = serde_json::to_value(&*echo.input_schema).expect("the schema serialises");
    assert_eq!(schema["properties"]["message"]["type"], "string");
    assert_eq!(schema["required"][0], "message");

    // Nothing that changes anything is on offer at the read tier.
    let names = harness.tool_names().await;
    assert_eq!(
        names,
        vec!["tailscale_session_ask", "tailscale_session_echo"]
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_call_round_trips_its_arguments_and_its_result() {
    let harness = session(Tier::Read).await;
    let answer = harness
        .call_ok("tailscale_session_echo", json!({ "message": "hello" }))
        .await;
    assert_eq!(answer["message"], "hello");
    harness.shutdown().await;
}

#[tokio::test]
async fn a_tool_reaches_the_local_binary() {
    let harness = session(Tier::Read).await;
    let answer = harness.call_ok("tailscale_session_ask", json!({})).await;

    assert_eq!(answer["text"], "answered");
    assert!(
        harness.cli_calls().contains(&vec!["ask".to_owned()]),
        "{:?}",
        harness.cli_calls()
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_failing_call_is_a_result_the_client_can_read_not_a_dropped_session() {
    let harness = session(Tier::Read).await;
    let error = harness
        .call_err("tailscale_session_echo", json!({ "message": 7 }))
        .await;
    assert_eq!(error["code"], "invalid_args");

    // The session survives it, which is the point.
    let answer = harness
        .call_ok("tailscale_session_echo", json!({ "message": "still here" }))
        .await;
    assert_eq!(answer["message"], "still here");

    harness.shutdown().await;
}

#[tokio::test]
async fn a_hidden_tool_cannot_be_reached_by_naming_it() {
    let harness = session(Tier::Read).await;
    let error = harness.call_err("tailnet_session_delete", json!({})).await;

    assert_eq!(error["code"], "not_permitted");
    assert!(
        error["hint"]
            .as_str()
            .is_some_and(|h| h.contains("--allow-destructive")),
        "{error}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_tier_permits_everything_below_it_and_nothing_above() {
    let harness = session(Tier::Write).await;
    let names = harness.tool_names().await;

    assert!(
        names.contains(&"tailscale_session_set".to_owned()),
        "{names:?}"
    );
    assert!(
        !names.contains(&"tailnet_session_delete".to_owned()),
        "writing does not permit destruction: {names:?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn a_destructive_tool_still_needs_confirming() {
    let harness = session(Tier::Destructive).await;

    let error = harness.call_err("tailnet_session_delete", json!({})).await;
    assert_eq!(error["code"], "confirmation_required");

    let answer = harness
        .call_ok("tailnet_session_delete", json!({ "confirm": true }))
        .await;
    assert_eq!(answer["done"], true);

    harness.shutdown().await;
}

#[tokio::test]
async fn an_unknown_tool_is_reported_without_ending_the_session() {
    let harness = session(Tier::Read).await;
    let error = harness.call_err("tailscale_invented", json!({})).await;
    assert_eq!(error["code"], "not_found");

    let answer = harness
        .call_ok("tailscale_session_echo", json!({ "message": "still here" }))
        .await;
    assert_eq!(answer["message"], "still here");

    harness.shutdown().await;
}

#[tokio::test]
async fn a_surface_with_nothing_behind_it_is_hidden_rather_than_broken() {
    let harness = Setup::new()
        .entries(entries())
        .toolsets("local-status,local-prefs,tailnet-devices")
        .tier(Tier::Destructive)
        .without_credentials()
        .start()
        .await;

    let names = harness.tool_names().await;
    assert!(
        !names.is_empty(),
        "the local surface should still be offered"
    );
    assert!(
        names.iter().all(|n| n.starts_with("tailscale_")),
        "a tailnet tool survived a missing credential: {names:?}"
    );
    assert!(
        harness.notes.iter().any(|n| n.contains("credential")),
        "{:?}",
        harness.notes
    );

    harness.shutdown().await;
}
