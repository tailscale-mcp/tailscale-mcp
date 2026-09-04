//! A real client, over a real transport, against a real server.
//!
//! The unit tests reach into the server directly; this one goes through the
//! protocol, which is the only place the handshake, the schemas and the
//! serialisation of a failed call can be observed the way a client sees them.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ErrorCode};
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tailscale_cli::stub::{Reply, StubBackend};
use tailscale_cli::{Invocation, LocalBackend};
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::context::ToolContext;
use tailscale_mcp::error::ToolResult;
use tailscale_mcp::server::{self, Backends};
use tailscale_rest::{Credentials, Secret};

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

    /// Act on the tailnet, irreversibly.
    tailnet_session_delete => NoParams, deleted,
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

async fn deleted(_ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    Ok(json!({ "deleted": true }))
}

fn node() -> StubBackend {
    StubBackend::failure(1, "no such device")
        .on(["version"], Reply::ok("1.102.2\n"))
        .on(
            ["status", "--json"],
            Reply::ok(
                json!({
                    "Self": {
                        "ID": "n1234567CNTRL",
                        "TailscaleIPs": ["100.64.0.1"],
                        "DNSName": "workstation.example-tailnet.ts.net."
                    }
                })
                .to_string(),
            ),
        )
        .on(["ask"], Reply::ok("answered\n"))
}

/// A client and a server talking over a pipe, which is what stdio is.
async fn session(cli: Cli) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let config = Config::resolve_with(cli, |_| None).expect("configuration resolves");
    let backends = Backends {
        local: Arc::new(node()) as Arc<dyn LocalBackend>,
        local_available: true,
        credentials: Some(Credentials::ApiKey(Secret::new("tskey-api-example"))),
    };
    let startup = server::build(&config, entries(), backends)
        .await
        .expect("the server builds");

    let (client_side, server_side) = tokio::io::duplex(8 * 1024);
    let (server_read, server_write) = tokio::io::split(server_side);
    tokio::spawn(async move {
        let service = startup
            .server
            .serve((server_read, server_write))
            .await
            .expect("the server accepts the connection");
        let _ = service.waiting().await;
    });

    let (client_read, client_write) = tokio::io::split(client_side);
    ().serve((client_read, client_write))
        .await
        .expect("the client connects")
}

fn structured(result: &rmcp::model::CallToolResult) -> Value {
    result
        .structured_content
        .clone()
        .expect("every result carries structured content")
}

fn args(value: Value) -> rmcp::model::JsonObject {
    value.as_object().cloned().expect("an object")
}

#[tokio::test]
async fn a_client_connects_and_is_told_what_this_server_is() {
    let client = session(Cli::default()).await;
    let info = client.peer_info().expect("the handshake completed");

    let implementation = info.server_info.as_ref().expect("the server names itself");
    assert_eq!(implementation.name, "tailscale-mcp");
    assert_eq!(implementation.version, env!("CARGO_PKG_VERSION"));
    assert!(info.capabilities.tools.is_some(), "tools are advertised");

    let instructions = info
        .instructions
        .as_deref()
        .expect("instructions are part of the handshake");
    assert!(instructions.contains("tailnet_*"), "{instructions}");
    assert!(
        instructions.contains("Permitted tier: read."),
        "{instructions}"
    );
    assert!(instructions.contains("1.102.2"), "{instructions}");

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn the_listing_describes_each_tool_well_enough_to_call_it() {
    let client = session(Cli::default()).await;
    let tools = client.list_all_tools().await.expect("tools are listed");

    let echo = tools
        .iter()
        .find(|t| t.name == "tailscale_session_echo")
        .expect("the read tool is offered");
    assert!(
        echo.description.as_deref().is_some_and(|d| !d.is_empty()),
        "a tool without a description cannot be chosen"
    );
    let schema = serde_json::to_value(&*echo.input_schema).expect("the schema serialises");
    assert_eq!(schema["properties"]["message"]["type"], "string");
    assert_eq!(schema["required"][0], "message");

    // Destructive tools are hidden by default, so nothing here can change anything.
    assert!(
        !tools.iter().any(|t| t.name == "tailnet_session_delete"),
        "a destructive tool was offered without --allow-destructive"
    );

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn a_call_round_trips_its_arguments_and_its_result() {
    let client = session(Cli::default()).await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("tailscale_session_echo".to_owned())
                .with_arguments(args(json!({ "message": "hello" }))),
        )
        .await
        .expect("the call is answered");

    assert_eq!(result.is_error, Some(false));
    assert_eq!(structured(&result)["message"], "hello");

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn a_tool_reaches_the_local_binary() {
    let client = session(Cli::default()).await;
    let result = client
        .call_tool(CallToolRequestParams::new(
            "tailscale_session_ask".to_owned(),
        ))
        .await
        .expect("the call is answered");

    assert_eq!(structured(&result)["text"], "answered");

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn a_failing_tool_is_a_result_the_client_can_read_not_a_dropped_session() {
    let client = session(Cli::default()).await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("tailscale_session_echo".to_owned())
                .with_arguments(args(json!({ "message": 7 }))),
        )
        .await
        .expect("a bad argument is answered, not dropped");

    assert_eq!(result.is_error, Some(true));
    assert_eq!(structured(&result)["code"], "invalid_args");

    // The session survives it, which is the point.
    let after = client
        .call_tool(
            CallToolRequestParams::new("tailscale_session_echo".to_owned())
                .with_arguments(args(json!({ "message": "still here" }))),
        )
        .await
        .expect("the session is still usable");
    assert_eq!(structured(&after)["message"], "still here");

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn a_hidden_tool_cannot_be_reached_by_naming_it() {
    let client = session(Cli::default()).await;
    let result = client
        .call_tool(CallToolRequestParams::new(
            "tailnet_session_delete".to_owned(),
        ))
        .await
        .expect("the refusal is a result, not a transport error");

    assert_eq!(result.is_error, Some(true));
    let error = structured(&result);
    assert_eq!(error["code"], "not_permitted");
    assert!(
        error["hint"]
            .as_str()
            .is_some_and(|h| h.contains("--allow-destructive")),
        "{error}"
    );

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn a_destructive_tool_still_needs_confirming() {
    let client = session(Cli {
        allow_destructive: true,
        ..Cli::default()
    })
    .await;

    let refused = client
        .call_tool(CallToolRequestParams::new(
            "tailnet_session_delete".to_owned(),
        ))
        .await
        .expect("the call is answered");
    assert_eq!(structured(&refused)["code"], "confirmation_required");

    let done = client
        .call_tool(
            CallToolRequestParams::new("tailnet_session_delete".to_owned())
                .with_arguments(args(json!({ "confirm": true }))),
        )
        .await
        .expect("the call is answered");
    assert_eq!(done.is_error, Some(false), "{done:?}");
    assert_eq!(structured(&done)["deleted"], true);

    client.cancel().await.expect("the session closes cleanly");
}

#[tokio::test]
async fn an_unknown_tool_is_reported_without_ending_the_session() {
    let client = session(Cli::default()).await;
    let result = client
        .call_tool(CallToolRequestParams::new("tailscale_invented".to_owned()))
        .await;

    match result {
        Ok(result) => assert_eq!(structured(&result)["code"], "not_found"),
        // Some transports validate the name before the handler sees it.
        Err(rmcp::ServiceError::McpError(e)) => {
            assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        }
        Err(other) => panic!("unexpected transport failure: {other:?}"),
    }

    client.cancel().await.expect("the session closes cleanly");
}
