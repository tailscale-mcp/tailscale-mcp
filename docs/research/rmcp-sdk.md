<!-- Research note. Notes on the official Rust MCP SDK (rmcp 3.2.0): features, macros, transports, verified probe code.
     Produced by a research sub-agent on 2026-09-03 during the design interview; facts were verified against the sources named inside. Not a spec. -->

# rmcp research report (verified against rmcp 3.2.0, 2026-09-03)

## 0. What I actually read / ran

- Local: `rustc --version`, `cargo --version`, `rustup show`, `cargo search`/`cargo info`, `~/.cargo/registry`.
- Shallow clone of https://github.com/modelcontextprotocol/rust-sdk at HEAD `3b5ca4d` (2026-09-02) in `<scratchpad>/rust-sdk`. `diff -rq` of published `rmcp-3.2.0/src` (registry) vs clone `crates/rmcp/src`: **identical**, so main HEAD == published 3.2.0 for the core crate. Files read: root `README.md`, `crates/rmcp/README.md`, `crates/rmcp/Cargo.toml`, `crates/rmcp/CHANGELOG.md`, `crates/rmcp-macros/{Cargo.toml,README.md,CHANGELOG.md,src/{lib,tool,tool_router,tool_handler}.rs}`, `crates/rmcp/src/{lib.rs,model.rs,model/tool.rs,model/mrtr.rs,model/capabilities.rs,error.rs,handler/server.rs,handler/server/{tool.rs,common.rs,tool_name_validation.rs,router.rs,router/tool.rs,router/prompt.rs,wrapper/{json,parameters}.rs},transport/io.rs,transport/streamable_http_server/tower.rs}`, tests `test_tool_macro_annotations.rs`, `test_tool_routers.rs`, `test_tool_disable_notification.rs`, examples `counter_stdio.rs`, `counter_streamhttp.rs`, `common/counter.rs`, `common/calculator.rs`, `structured_output.rs`, `docs/OAUTH_SUPPORT.md`.
- WebFetch: docs.rs features page (https://docs.rs/crate/rmcp/latest/features) and the 3.x migration guide (https://github.com/modelcontextprotocol/rust-sdk/discussions/969).
- **Built and ran a probe crate** against the published crate on the local toolchain: `<scratchpad>/rmcp-probe` (two tool routers, annotations, `Json<T>` output, read-only toggle, stdio + axum bins). Compiles in 16 s, unit test passes, JSON-RPC smoke tests over stdio and HTTP pass (outputs quoted below). The code blocks in section 4 are that verified code.

## 1. Local facts

- `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1`; rustup default `stable-aarch64-apple-darwin` (also has a `1.81` toolchain). MSRV of rmcp (1.88) is satisfied.
- No local checkout at `~/github.com/modelcontextprotocol/rust-sdk`; registry had no rmcp until `cargo info rmcp` fetched it (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rmcp-3.2.0`).
- crates.io:
  - `rmcp` latest = **3.2.0** (rust-version 1.88, Apache-2.0) https://crates.io/crates/rmcp/3.2.0 ; `rmcp-macros` 3.2.0 (rmcp pins `rmcp-macros = "3.2.0"`). Note `cargo search rmcp` buries rmcp itself (shows rmcp-memex 0.5.0, rmcp-mux 0.4.0, "120 crates more"); `cargo info rmcp` is the reliable check.
  - `tailscale` **TAKEN**: 0.5.0 "A work-in-progress Tailscale implementation", BSD-3-Clause, https://github.com/tailscale/tailscale-rs.
  - `tailscale-api` **TAKEN**: 0.1.5 "An API client for Tailscale" (oxidecomputer/cio).
  - `tailscale-mcp` / `tailscale_mcp` **available** (crates.io treats `-`/`_` as the same name), `tailscale-cli` **available**, `tsmcp` no results.

## 2. Versions, MSRV, tokio, protocol versions

- Published manifest (`rmcp-3.2.0/Cargo.toml`): `edition = "2024"`, `rust-version = "1.88"`, `tokio = { version = "1", features = ["sync", "macros", "rt", "time"] }` (features `transport-async-rw` adds `tokio/io-util` + `tokio-util/codec`; `transport-io` adds `tokio/io-std`). Your binary additionally needs `rt-multi-thread` (for `#[tokio::main]`), `io-std` (stdio), `signal` (ctrl_c for HTTP shutdown); the SDK's own examples use `["macros","rt","rt-multi-thread","io-std","signal"]`. Repo `rust-toolchain.toml` pins channel 1.96 for development.
- `schemars = { version = "1.0", features = ["chrono04"] }` (dev-deps use 1.1.0). `axum` is not a dependency of rmcp; examples use `axum = "0.8"`. `tokio-util = "0.7"` for `CancellationToken`.
- Protocol versions (`crates/rmcp/src/model.rs:170-175`): `ProtocolVersion::V_2026_07_28`, `V_2025_11_25`, `V_2025_06_18`, `V_2025_03_26`, `V_2024_11_05`; `LATEST = V_2025_11_25` (what `ServerInfo`/`get_info` defaults to); `KNOWN_VERSIONS` = default of `ServerHandler::supported_protocol_versions()`. README: "implements the stable MCP `2026-07-28` specification while remaining fully compatible with `2025-11-25` and earlier". Negotiation is automatic (verified: client offered `2025-06-18`, server answered `2025-06-18`).

## 3. Feature flags (published 3.2.0, verbatim from registry manifest; docs.rs page agrees)

- `default = ["base64", "macros", "server"]`
- `server = ["transport-async-rw", "schemars", "dep:pastey", "uuid"]`
- `macros = ["dep:rmcp-macros", "dep:pastey"]`
- `schemars = ["dep:schemars"]`
- `transport-io = ["transport-async-rw", "tokio/io-std"]` (stdio, client + server)
- `transport-streamable-http-server = ["transport-streamable-http-server-session", "server-side-http", "transport-worker"]` (Tower service; mount in axum 0.8 or hyper)
- `auth = ["dep:async-trait", "dep:oauth2", "__reqwest", "dep:url"]` (oauth2 5.0, reqwest 0.13.2). Per `docs/OAUTH_SUPPORT.md` this is the OAuth 2.1 **client** side (PKCE, RFC 9728/8414 discovery, dynamic registration, CIMD, refresh). Server-side protection is shown in examples `simple_auth_streamhttp.rs`, `complex_auth_streamhttp.rs`, `cimd_auth_streamhttp.rs` (not read in detail; flagged).
- Others: `client`, `elicitation = ["dep:url"]`, `request-state`, `local` (non-Send handlers), `reqwest` / `reqwest-native-tls` / `reqwest-tls-no-provider` (TLS backend for client transports), `transport-child-process`, `transport-worker`.
- For your server: `features = ["server", "macros", "transport-io", "transport-streamable-http-server"]` (first two are default anyway).

## 4. Declaring tools: macros and verified examples

Macro grammar (from `crates/rmcp-macros/src/*.rs`):
- `#[tool(name = "...", title = "...", description = "...", input_schema = <expr>, output_schema = <expr>, annotations(title = "...", read_only_hint = bool, destructive_hint = bool, idempotent_hint = bool, open_world_hint = bool), icons = <expr>, meta = <expr>, local)]`. Name defaults to the fn ident; description defaults to the doc comment (also `#[doc = include_str!(..)]`). Generates `fn <name>_tool_attr() -> rmcp::model::Tool` via `Tool::new_with_raw(..).with_title(..).with_raw_output_schema(..).with_annotations(ToolAnnotations::from_raw(title, read_only_hint, destructive_hint, idempotent_hint, open_world_hint))`. Input schema: explicit expr, else `schema_for_input::<T>()` from the `Parameters<T>` argument, else `schema_for_empty_input()` (`{"type":"object","properties":{}}`). Output schema: explicit, or auto from return type `Json<T>` / `Result<Json<T>, E>`.
- `#[tool_router(router = <ident, default tool_router>, vis = "pub", server_handler)]` on an inherent `impl` generates `<vis> fn <router>() -> ToolRouter<Self>` registering every `#[tool]` in that impl block; `server_handler` additionally emits `#[tool_handler(router = Self::<router>())] impl ServerHandler for T {}`.
- `#[tool_handler(router = <expr, default Self::tool_router()>, meta, name, version, instructions)]` on `impl ServerHandler` generates `call_tool`, `list_tools` (returns **all** tools in one page, `next_cursor: None`; cache hints only for 2026-07-28 peers), `get_tool`, and `get_info` (only if you did not write one; `name`/`version`/`instructions` fill `Implementation` and instructions; with a sibling `#[prompt_handler]` it also enables prompts).
- Handler args are extractors (up to 16): `Parameters<T>` (T: DeserializeOwned + JsonSchema), `Parameters<JsonObject>`, `RequestContext<RoleServer>` (`.id`, `.meta`, `.extensions`, `.peer`, `.ct` cancellation token, `.protocol_version()`, `.client_capabilities()`), `Peer<RoleServer>`, `CancellationToken`, `Extensions`, `Extension<T>`, `RequestId`, `ToolName`, `RequestState`, `InputResponses`. Sync or async fns; async ones become `Pin<Box<dyn Future + Send>>`.
- Handler return types: anything `IntoCallToolResult`: `String`/`IntoContents` -> `CallToolResult::success`; `CallToolResult`; `Json<T>`; `CallToolResponse`; `Result<T, E>` where `Err(E)`: if `E` is `ErrorData` it becomes a JSON-RPC protocol error, otherwise (e.g. `String`) it becomes a tool result with `is_error = Some(true)`.

Verified `Cargo.toml` (probe):
```toml
[package]
name = "tailscale-mcp"
version = "0.1.0"
edition = "2024"
rust-version = "1.88"

[dependencies]
rmcp = { version = "3.2.0", features = ["server", "macros", "transport-io", "transport-streamable-http-server"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "io-std", "signal"] }
tokio-util = "0.7"
axum = "0.8"
schemars = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

Verified server (`src/lib.rs`) — two routers, annotations, structured output, tool-level error, read-only mode:
```rust
use std::sync::{Arc, Mutex};
use rmcp::{
    ErrorData as McpError, Json, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*, schemars, service::RequestContext, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetDeviceArgs {
    /// Device ID (nodeId) as shown by `tailscale status --json`.
    pub device_id: String,
    /// Include routes and tags in the result.
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Device { pub id: String, pub hostname: String, pub online: bool }

#[derive(Clone)]
pub struct TailscaleServer {
    tool_router: ToolRouter<Self>,
    calls: Arc<Mutex<u64>>,
}

// Toolset A: read-only tools
#[tool_router(router = read_tools, vis = "pub")]
impl TailscaleServer {
    /// Get a device by ID.
    #[tool(name = "get_device",
           annotations(title = "Get device", read_only_hint = true, destructive_hint = false,
                       idempotent_hint = true, open_world_hint = true))]
    async fn get_device(&self, Parameters(args): Parameters<GetDeviceArgs>)
        -> Result<Json<Device>, McpError> {
        *self.calls.lock().unwrap() += 1;
        Ok(Json(Device { id: args.device_id, hostname: "mac-mini".into(), online: true }))
    }
}

// Toolset B: mutating tools
#[tool_router(router = write_tools, vis = "pub")]
impl TailscaleServer {
    #[tool(name = "delete_device", description = "Delete a device from the tailnet",
           annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true))]
    async fn delete_device(&self, Parameters(args): Parameters<GetDeviceArgs>,
                           _ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        if args.device_id.is_empty() {
            return Ok(CallToolResult::error(vec![ContentBlock::text("device_id must not be empty")]));
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(format!("deleted {}", args.device_id))]))
    }
}

impl TailscaleServer {
    pub fn new(read_only: bool) -> Self {
        let mut router = Self::read_tools() + Self::write_tools();
        if read_only { router.disable_route("delete_device"); }
        Self { tool_router: router, calls: Arc::new(Mutex::new(0)) }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TailscaleServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
            .with_instructions("Tailscale MCP server")
    }
}
```

Verified stdio main (`src/bin/stdio.rs`; mirrors `examples/servers/src/counter_stdio.rs`, which additionally installs `tracing_subscriber` writing to **stderr**):
```rust
use rmcp::{ServiceExt, transport::stdio};
use tailscale_mcp::TailscaleServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let read_only = std::env::var_os("READ_ONLY").is_some();
    let service = TailscaleServer::new(read_only).serve(stdio()).await?; // stdio() -> (tokio::io::Stdin, tokio::io::Stdout)
    service.waiting().await?;
    Ok(())
}
```

Verified Streamable HTTP main (`src/bin/http.rs`; mirrors `examples/servers/src/counter_streamhttp.rs`):
```rust
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tailscale_mcp::TailscaleServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ct = tokio_util::sync::CancellationToken::new();
    let service = StreamableHttpService::new(
        || Ok(TailscaleServer::new(false)),              // Fn() -> Result<S, std::io::Error>, called per session/request
        LocalSessionManager::default().into(),           // Arc<M>
        StreamableHttpServerConfig::default()
            .with_cancellation_token(ct.child_token())
            .with_allowed_hosts(["localhost", "127.0.0.1", "::1"]), // add your tailnet hostname here
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000").await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { let _ = tokio::signal::ctrl_c().await; ct.cancel(); })
        .await?;
    Ok(())
}
```

Verified wire behaviour (stdio, JSON-RPC over stdin/stdout):
- `tools/list` entry for `get_device`: `"inputSchema": {"$schema": "https://json-schema.org/draft/2020-12/schema", "properties": {"device_id": {"description": "Device ID ...", "type": "string"}, "verbose": {"default": false, "description": "...", "type": "boolean"}}, "required": ["device_id"], "type": "object"}`, `"outputSchema": {... "properties": {"hostname","id","online"} ...}`, `"annotations": {"title": "Get device", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true}`. Doc comments became `description`s; the struct's own name/doc are stripped. Unset hints are omitted (`delete_device` shows only the three set).
- `tools/call get_device {"device_id":"n123"}` -> `{"content":[{"type":"text","text":"{\"hostname\":\"mac-mini\",\"id\":\"n123\",\"online\":true}"}],"structuredContent":{...},"isError":false}` (`Json<T>` emits both text and `structuredContent`).
- Missing arg -> `{"content":[{"type":"text","text":"failed to deserialize parameters: missing field `device_id`"}],"isError":true}` (tool-level error, not a protocol error; since 1.8.0).
- Empty id -> `{"content":[{"type":"text","text":"device_id must not be empty"}],"isError":true}`.
- Unknown or disabled tool -> `{"error":{"code":-32602,"message":"tool not found"}}`.
- With `READ_ONLY=1`: `tools/list` -> `["get_device"]`, and calling `delete_device` -> `-32602 "tool not found"`.
- HTTP: `POST /mcp` initialize -> `200`, `content-type: text/event-stream`, `mcp-session-id: <uuid>`, SSE priming event then the JSON-RPC result. Same request with `Host: evil.example.com` -> `403 Forbidden: Host header is not allowed`. `GET /mcp` with no session -> 400.

## 5. Tool annotations

`rmcp::model::ToolAnnotations { title: Option<String>, read_only_hint: Option<bool>, destructive_hint: Option<bool>, idempotent_hint: Option<bool>, open_world_hint: Option<bool> }` (camelCase on the wire; `#[non_exhaustive]`; spec defaults documented as read_only=false, destructive=true, idempotent=false, open_world=true when absent). Set via `#[tool(annotations(...))]` as above, or by hand: `ToolAnnotations::new().with_title("..").read_only(true).destructive(false).idempotent(true).open_world(true)` and `Tool::new(name, description, input_schema).with_annotations(a)` / `.annotate(a)`; also `.with_title`, `.with_output_schema::<T>()`, `.with_input_schema::<T>()`, `.with_icons`, `.with_meta`. Test reference: `crates/rmcp/tests/test_tool_macro_annotations.rs`.

## 6. Structured output

`rmcp::Json<T>` (`handler/server/wrapper/json.rs`): `impl<T: Serialize + JsonSchema + 'static> IntoCallToolResult for Json<T>` -> `CallToolResult::structured(value)`; the macro derives `output_schema` from `Json<T>` / `Result<Json<T>, E>` using `schema_for_output::<T>()` (top-level title/description stripped; any root type allowed per SEP-2106, whereas `inputSchema` must be root `type: "object"` or `schema_for_input` returns `Err`). `CallToolResult` (`model.rs:3785`): fields `result_type: Option<ResultType>`, `content: Vec<ContentBlock>`, `structured_content: Option<serde_json::Value>`, `is_error: Option<bool>`, `meta`; constructors `success(Vec<ContentBlock>)`, `error(Vec<ContentBlock>)`, `structured(Value)`, `structured_error(Value)` (also puts `value.to_string()` in text content). `ContentBlock::text/image/audio/resource`. Example: `examples/servers/src/structured_output.rs`.

## 7. Errors

- `rmcp::ErrorData` (re-exported at crate root; `pub struct ErrorData` at `model.rs:565`) is the JSON-RPC error type. **There is no public `McpError` type** — `crates/rmcp/src/handler/server.rs` does `use crate::error::ErrorData as McpError;` privately, and every example writes `use rmcp::ErrorData as McpError;`. `rmcp::RmcpError` (`error.rs:34`) is the transport/service error enum. Constructors: `ErrorData::invalid_params(msg, data)`, `internal_error`, `resource_not_found`, `method_not_found`, `ErrorCode::{INVALID_PARAMS, INTERNAL_ERROR, METHOD_NOT_FOUND, RESOURCE_NOT_FOUND}`.
- Trait signature (3.x): `async fn call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData>` where `CallToolResponse::{Complete(CallToolResult), InputRequired(..), Task(..)}` (`model/mrtr.rs:105`) and `impl From<CallToolResult> for CallToolResponse` (so `Ok(result.into())`). Doc guidance (`CallToolResult::error` docs, README "Error handling"): `Ok(CallToolResult::error(...))` for "tool ran and failed" (client shows your text); `Err(ErrorData)` only for unroutable/malformed requests or server-internal failure (clients render opaquely).

## 8. Composing routers and runtime toolsets / read-only mode

`ToolRouter<S>` (`handler/server/router/tool.rs`, `#[non_exhaustive]`, Clone + Default + Debug): `new()`, `with_route((attr, handler))`, `with_sync_tool::<T>()` / `with_async_tool::<T>()` (trait-based tools `SyncTool`/`AsyncTool`), `add_route(ToolRoute)`, `merge(other)`, `impl Add`/`AddAssign` (`router_a + router_b`; same name = later overwrites, HashMap), `remove_route(name)`, `has_route(name)` (registered and not disabled), `disable_route(name) -> bool`, `enable_route(name) -> bool`, `is_disabled(name)`, builder `with_disabled(name)` (works even before the route is added), `list_all() -> Vec<Tool>` (sorted by name, excludes disabled), `get(name) -> Option<&Tool>`, `call(ToolCallContext)`, `transparent_when_not_found` field, plus `set_notifier`/`clear_notifier`/`bind_peer_notifier(&Peer<RoleServer>)` which spawns `peer.notify_tool_list_changed()` on disable/enable. `ToolRoute::new(attr, handler)` / `ToolRoute::new_dyn(attr, |ctx| Box::pin(async {...}))`. Multi-module pattern from macro docs: `mod a { #[tool_router(router = tool_router_a, vis = "pub")] impl S {..} }`, `mod b { ... tool_router_b ... }`, `Self { tool_router: Self::tool_router_a() + Self::tool_router_b() }`, `#[tool_handler(router = self.tool_router)]`.

Three ways to do toolsets/read-only:
1. Build-time selection: compose only the routers you want, or `with_disabled(..)` — what the probe does (verified).
2. Runtime toggling: keep `Arc<RwLock<ToolRouter<Self>>>`, hand-write `call_tool`/`list_tools` (pattern in `tests/test_tool_disable_notification.rs`: `let tcc = ToolCallContext::new(self, request, context); router.call(tcc).await` and `ListToolsResult { tools: router.list_all(), ..Default::default() }`), call `bind_peer_notifier(&context.peer)` in `on_initialized`, then `disable_route`/`enable_route` emit `notifications/tools/list_changed`; advertise via `ServerCapabilities::builder().enable_tools().enable_tool_list_changed()`.
3. `rmcp::handler::server::router::Router<S>` wrapper (`Router::new(service).with_tool(..)`, exposes `tool_router`/`prompt_router` fields, implements `Service<RoleServer>`, auto-sets `tools.list_changed = Some(true)` and wires the peer notifier) — not exercised by me.
Overriding `list_tools` to filter on request context (e.g. per-client toolsets) is also fine: the macro only generates `list_tools`/`call_tool`/`get_tool`/`get_info` if you did not write them.

Tool-name rules (`tool_name_validation.rs`): 1..=128 chars, `[A-Za-z0-9_.-]`; anything else logs `tracing::warn!` at registration but does not fail. `snake_case` names are ideal for ~100 tools.

## 9. Resources and prompts

- Resources: no macro; implement on `ServerHandler`: `async fn list_resources(&self, request: Option<PaginatedRequestParams>, ctx: RequestContext<RoleServer>) -> Result<ListResourcesResult, ErrorData>`, `async fn read_resource(&self, request: ReadResourceRequestParams, ctx) -> Result<ReadResourceResponse, ErrorData>` (return `Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]).into())`, or `.blob(base64, uri).with_mime_type(..)`; not found -> `Err(ErrorData::resource_not_found("resource_not_found", Some(json!({"uri": uri}))))`), optional `list_resource_templates` (`ResourceTemplate::new("users://{user_id}/profile", "user-profile")`), `Resource::new(uri, name)`; enable with `.enable_resources()` (`.enable_resources_subscribe()`, `.enable_resources_list_changed()` exist). Flag: the root README Resources snippet still writes `-> Result<ReadResourceResult, McpError>`; the trait (and `examples/servers/src/common/counter.rs`) use `ReadResourceResponse` — the root README is not doc-tested (`lib.rs` includes `crates/rmcp/README.md`, not the root one), so trust the trait.
- Prompts: `#[prompt_router(router = .., vis = ..)]`, `#[prompt(name, description, arguments, meta)]`, `#[prompt_handler(router = self.prompt_router, meta)]` all exist (`crates/rmcp-macros`, README "Prompts", `common/counter.rs`). Prompt fns return `Vec<PromptMessage>`, `GetPromptResult`, or `Result<_, ErrorData>`; args are `Parameters<T: JsonSchema>`. `PromptRouter<S>` has `with_route/add_route/merge/remove_route/has_route/list_all` and `+`, but **no disable/enable**. Gotcha: MCP prompt arguments are `Record<string,string>`, so non-string fields need a custom deserializer (counter.rs does a string-or-int one).

## 10. schemars: version and gotchas

- Pinned major: **schemars 1.x** (`"1.0"`, chrono04 feature); use `rmcp::schemars` re-export or your own `schemars = "1"` (verified both resolve to the same crate). Schemas are generated with `SchemaSettings::draft2020_12()` (JSON Schema 2020-12 default since 0.9.1) and cached per type; `nullable` is deliberately not emitted.
- `inputSchema` must have root `type: "object"` (a `Parameters<Vec<_>>` or primitive fails at router construction with the `Err(String)` from `schema_for_input`); top-level `title`/`description` are stripped since 1.8.0, field docs kept. Output shows a `"$schema"` key (harmless).
- Field docs via `///` or `#[schemars(description = "...")]`; `#[serde(default)]` makes fields optional (verified `verbose` not in `required`, with `"default": false`). Enums used as elicitation/tool params: README notes `#[schemars(inline)]` and `#[schemars(extend("type" = "string"))]` because schemars omits `type` for unit enums.
- `Json<T>` output types need `Serialize + JsonSchema + 'static`.

## 11. Notable recent breaking changes (CHANGELOG + migration guide #969)

- **3.0.0 (2026-07-28)**: handler return types changed to MRTR wrappers `call_tool -> CallToolResponse`, `get_prompt -> GetPromptResponse`, `read_resource -> ReadResourceResponse` (`.into()` from the old result types); `structured_content` is `Option<Value>` not `Option<JsonObject>`; `Annotations.last_modified` is `Option<String>`; `Meta` split into `MetaObject`/`RequestMetaObject`/`NotificationMetaObject`; the six protocol union enums are `#[non_exhaustive]`; `StreamableHttpService<S, M>` now requires `S: ServerHandler`; `stateful_mode`/`with_stateful_mode` renamed `legacy_session_mode`/`with_legacy_session_mode`; experimental Tasks API replaced by SEP-2663 (`enable_tasks()`, `CallToolResponse::Task`); subscriptions moved to `subscriptions/listen`; OAuth `discover_metadata` -> `resolve_metadata`, `start_authorization(AuthorizationRequest::new(..))`; `result_type`, `ttl_ms`, `cache_scope` fields added to results; MSRV declared 1.88; deprecated v3 APIs removed; `DiscoverResult.server_info` removed; "flag schema derive on schemars feature".
- 3.1.x (Jul 31 – Aug 18): honor `supported_protocol_versions`, strict stateless protocol metadata validation, cache hints emitted from handler macros, MRTR state exposed to tool handlers, async-trait optional; rmcp-macros 3.1.1 upgraded to syn 3 / darling 0.24. 3.2.0 (2026-08-31): auth credential-store refresh, request-state key rotation, bug fixes — no API breaks noted.
- **2.0.0 (2026-06-27)**: model types aligned with MCP 2025-11-25; roots/sampling/logging deprecated. **1.8.0 (06-22)**: invalid tool arguments now return `is_error` tool results instead of protocol errors; tool schemas stripped/validated. 1.6.0: runtime tool disabling + Origin validation. 1.4.0: macros auto-generate `get_info` and default router. 1.3.0: `local` feature; `StreamableHttpService` lost its default type param. **0.10.0 (2025-12-01)**: SSE transport removed (README: legacy HTTP+SSE is a permanent non-goal). 0.4.0: outputSchema/structuredContent.

## 12. Gotchas worth designing around

1. `Implementation::from_build_env()` is compiled inside rmcp, so it reports `name = "rmcp", version = "3.2.0"` (verified by the probe). Use `Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))` in your crate or `#[tool_handler(name = "...", version = "...")]` (macro output expands in your crate).
2. Streamable HTTP defaults (`tower.rs:164`): `allowed_hosts = ["localhost","127.0.0.1","::1"]` — any other `Host` header gets **403** (verified). For a server reached over Tailscale you must `with_allowed_hosts([...tailnet hostname/IP, optionally with port...])` or `disable_allowed_hosts()`; `allowed_origins` is empty (Origin checks off) by default; `legacy_session_mode = true` (sessions + SSE for < 2026-07-28 clients), `json_response = false`, `sse_keep_alive = 15s`, `max_request_body_bytes` capped, `stateless_protocol_metadata_required = false`. The service factory closure runs per session/request, so keep shared state in cloned `Arc`s.
3. stdio: never print to stdout; use `tracing_subscriber` with `.with_writer(std::io::stderr)` as the example does.
4. Generated `list_tools` ignores cursors (fine for ~100 tools; each page is the full list).
5. `ErrorData` from a handler surfaces as an opaque JSON-RPC error to the model; prefer `CallToolResult::error` for Tailscale API/CLI failures.
6. `RequestContext<RoleServer>.extensions` carries `axum::http::request::Parts` in HTTP mode (counter.rs reads it) — usable for per-request auth headers.

## 13. Could not verify / caveats

- docs.rs main page was only seen through a lossy WebFetch summary earlier; feature lists above come from the registry manifest and the docs.rs features page (which agree).
- `Router<S>` wrapper and `#[prompt_router]` were read but not compiled in the probe; `examples/servers/src/*auth_streamhttp.rs` (server-side OAuth) were not read.
- `crates/rmcp/tests/test_tool_macros.rs` does not exist (the annotation test is `test_tool_macro_annotations.rs`).
- Release dates: CHANGELOG dates are authoritative; an earlier GitHub releases WebFetch returned wrong dates.
- Clone is shallow (no tags), so "main == 3.2.0" rests on the `diff -rq` of `crates/rmcp/src`; I did not diff `rmcp-macros` sources against the published macro crate (registry copy not present), though the probe exercised the published macros successfully.
