//! The MCP handler, and what has to be true before it can be built.
//!
//! Startup is where the two surfaces are discovered. Neither is required: a
//! machine with the CLI and no credential offers the local tools, a container
//! with a credential and no CLI offers the tailnet tools, and both are ordinary
//! configurations rather than degraded ones. What is not permitted is offering
//! nothing, which is a configuration mistake and is reported as one.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, InitializeResult,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use serde_json::Value;
use tailscale_cli::{CliBackend, LocalBackend, Unavailable};
use tailscale_rest::Credentials;

use crate::config::Config;
use crate::context::{PathPolicy, SelfIdentity, ToolContext};
use crate::error::{ToolError, ToolResult};
use crate::gating::{ConfigError, Gate};
use crate::meta::Surface;
use crate::registry::{Registry, RegistryError, ToolEntry};
use crate::version::SUPPORTED_FLOOR;
use crate::{cli, instructions};

/// The server could not be built.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

/// What the two surfaces turned out to be.
///
/// Separated from [`build`] so that the tests can supply both without a binary
/// on the machine or a credential in the environment.
#[derive(Debug)]
pub struct Backends {
    pub local: Arc<dyn LocalBackend>,
    /// Whether a usable local backend was found. An absent one is still a
    /// backend — it reports the binary as missing — so this is what says
    /// whether the surface should be offered.
    pub local_available: bool,
    pub credentials: Option<Credentials>,
}

impl Backends {
    /// Look for both surfaces on this machine.
    pub fn discover(config: &Config) -> Self {
        let (local, local_available): (Arc<dyn LocalBackend>, bool) =
            if config.is_disabled(Surface::Local) {
                (Arc::new(Unavailable::new("disabled by --no-local")), false)
            } else {
                match CliBackend::discover_with(config.cli_path.as_ref().map(AsRef::as_ref)) {
                    Ok(backend) => (Arc::new(backend), true),
                    Err(e) => (Arc::new(Unavailable::new(e.to_string())), false),
                }
            };
        let credentials = if config.is_disabled(Surface::Tailnet) {
            None
        } else {
            Credentials::from_env()
        };
        Self {
            local,
            local_available,
            credentials,
        }
    }
}

/// A built server, and what the operator should be told about how it was built.
#[derive(Debug)]
pub struct Startup {
    pub server: TailscaleMcpServer,
    /// Lines for standard error. Never standard output: on the stdio transport
    /// that is the protocol stream, and a stray line there ends the session.
    pub notes: Vec<String>,
}

/// Build a server from a settled configuration and the tool table.
pub async fn build(
    config: &Config,
    entries: Vec<ToolEntry>,
    backends: Backends,
) -> Result<Startup, StartupError> {
    let registry = Registry::new(entries)?;
    let mut notes = Vec::new();

    // A surface the operator switched off is hidden whether or not the machine
    // could have offered it. The check is here rather than only in `discover`
    // so that a caller assembling `Backends` itself cannot bypass the flag.
    let mut unavailable = std::collections::BTreeSet::new();
    if config.is_disabled(Surface::Local) {
        unavailable.insert(Surface::Local);
        notes.push(
            "The local surface is switched off; the tools that drive this node are hidden."
                .to_owned(),
        );
    } else if !backends.local_available {
        unavailable.insert(Surface::Local);
        notes.push(
            "No `tailscale` binary was found, so the tools that drive this node are hidden. \
             Set TAILSCALE_MCP_CLI_PATH or pass --cli-path to point at one."
                .to_owned(),
        );
    }
    if config.is_disabled(Surface::Tailnet) {
        unavailable.insert(Surface::Tailnet);
        notes.push(
            "The tailnet surface is switched off; the tools that act on the tailnet are hidden."
                .to_owned(),
        );
    } else if backends.credentials.is_none() {
        unavailable.insert(Surface::Tailnet);
        notes.push(
            "No control-plane credential was found, so the tools that act on the tailnet are \
             hidden. Set TAILSCALE_API_KEY, or TAILSCALE_OAUTH_CLIENT_ID and \
             TAILSCALE_OAUTH_CLIENT_SECRET."
                .to_owned(),
        );
    }

    // Probing a node the operator asked us to leave alone would be a surprise,
    // so both probes are skipped when the local surface is not on offer.
    let local_offered = backends.local_available && !config.is_disabled(Surface::Local);
    let (cli_version, identity) = if local_offered {
        let version = cli::probe_version(backends.local.as_ref()).await;
        match version {
            Some(found) if found < SUPPORTED_FLOOR && !found.is_unstable() => notes.push(format!(
                "The `tailscale` binary reports {found}, which is older than {SUPPORTED_FLOOR}, \
                 the oldest release this server is written against. Nothing is hidden; commands \
                 this build does not have will report the version they need."
            )),
            None => notes.push(
                "Could not read the version of the `tailscale` binary; commands will be attempted \
                 regardless."
                    .to_owned(),
            ),
            Some(_) => {}
        }
        (version, cli::probe_identity(backends.local.as_ref()).await)
    } else {
        (None, SelfIdentity::default())
    };

    let gate = Gate::new(
        config.toolsets.clone(),
        config.max_tier,
        unavailable,
        &registry.metas(),
    )?;

    let ctx = ToolContext {
        local: Arc::clone(&backends.local),
        redactor: crate::error::Redactor::default(),
        max_result_bytes: config.max_result_bytes,
        identity,
        cli_version,
        paths: PathPolicy::default(),
        max_tier: config.max_tier,
    };

    let visible = registry.visible(&gate).len();
    notes.push(format!(
        "Offering {visible} tools: {} at the {} tier and above.",
        describe_toolsets(&gate),
        gate.max_tier()
    ));

    Ok(Startup {
        server: TailscaleMcpServer::new(Arc::new(registry), gate, Arc::new(ctx)),
        notes,
    })
}

fn describe_toolsets(gate: &Gate) -> String {
    let names: Vec<&str> = gate.toolsets().iter().map(|t| t.as_str()).collect();
    if names.is_empty() {
        "no toolsets".to_owned()
    } else {
        names.join(", ")
    }
}

/// The MCP server.
#[derive(Debug, Clone)]
pub struct TailscaleMcpServer {
    registry: Arc<Registry>,
    gate: Gate,
    ctx: Arc<ToolContext>,
}

impl TailscaleMcpServer {
    pub fn new(registry: Arc<Registry>, gate: Gate, ctx: Arc<ToolContext>) -> Self {
        Self {
            registry,
            gate,
            ctx,
        }
    }

    pub fn gate(&self) -> &Gate {
        &self.gate
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn context(&self) -> &Arc<ToolContext> {
        &self.ctx
    }

    /// The tools this server offers, in listing order.
    pub fn tools(&self) -> Result<Vec<Tool>, McpError> {
        self.registry
            .visible(&self.gate)
            .into_iter()
            .map(|entry| {
                entry.describe().map_err(|reason| {
                    McpError::internal_error(
                        format!("the schema for `{}` is invalid: {reason}", entry.meta.name),
                        None,
                    )
                })
            })
            .collect()
    }

    /// Run one tool, all policy applied.
    async fn dispatch(&self, request: CallToolRequestParams) -> ToolResult<Value> {
        let arguments = request.arguments.unwrap_or_default();
        let (entry, arguments) = self
            .registry
            .resolve(&request.name, arguments, &self.gate)?;
        if entry.meta.surface() == Surface::Local {
            if !entry.meta.runs_here() {
                return Err(ToolError::unsupported_platform(
                    entry.meta.name,
                    std::env::consts::OS,
                ));
            }
            cli::version_permits(&self.ctx, &entry.meta)?;
        }
        let invoke = entry.invoke;
        let value = invoke(Arc::clone(&self.ctx), arguments).await?;
        self.check_size(&value)?;
        Ok(value)
    }

    /// Refuse a result too large to be useful, rather than truncating it into
    /// something that looks complete and is not.
    fn check_size(&self, value: &Value) -> ToolResult<()> {
        let size = serde_json::to_vec(value).map_or(0, |v| v.len());
        if size > self.ctx.max_result_bytes {
            return Err(ToolError::result_too_large(size, self.ctx.max_result_bytes));
        }
        Ok(())
    }
}

impl ServerHandler for TailscaleMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(instructions::render(&self.gate, &self.ctx));
        // Not `from_build_env`: that reads the *SDK*'s package metadata.
        info.server_info = Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
            .with_title("Tailscale")
            .with_website_url("https://github.com/tailscale-mcp/tailscale-mcp");
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        // Not paginated: the largest possible listing is under two hundred
        // tools, and a client that has to page through its own tool list is
        // worse off than one that receives it in a single message.
        Ok(ListToolsResult::with_all_items(self.tools()?))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        // Only ever a visible tool: this is used to validate a call, and a
        // hidden tool must not become discoverable through it.
        self.registry
            .visible(&self.gate)
            .into_iter()
            .find(|entry| entry.meta.name == name)
            .and_then(|entry| entry.describe().ok())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        // A tool that ran and failed is a result, not a protocol error: the
        // model has to see it to choose what to do next. Protocol errors are
        // reserved for requests that could not be routed at all, and the
        // registry does not produce those.
        let result = match self.dispatch(request).await {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(error.to_value()),
        };
        Ok(CallToolResponse::Complete(result))
    }
}

/// The name a client sees. Kept here so the tests can assert on it.
pub fn server_name() -> Cow<'static, str> {
    Cow::Borrowed(env!("CARGO_PKG_NAME"))
}

#[cfg(test)]
mod tests {
    use rmcp::schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;
    use tailscale_rest::Secret;

    use super::*;
    use crate::config::Cli;
    use crate::error::ErrorCode;
    use crate::meta::Tier;
    use crate::registry::CONFIRM_PARAM;
    use crate::testing::{Reply, StubBackend};

    /// A table standing in for the real one, so that these tests exercise the
    /// gate, the confirmation rule and both surfaces before the toolsets that
    /// will fill them have been written. It is deliberately small and covers
    /// one tool per case that the server itself has to get right.
    mod fixture {
        use super::{Deserialize, JsonSchema, ToolContext, ToolResult, Value};
        use tailscale_cli::Invocation;

        #[derive(Debug, Deserialize, JsonSchema)]
        pub struct NoParams {}

        crate::tools! {
            /// Read something about this node.
            tailscale_fixture_read => NoParams, run_local,
                toolset: LocalStatus, tier: Read, idempotent: true;

            /// Change something about this node.
            tailscale_fixture_write => NoParams, run_local,
                toolset: LocalPrefs, tier: Write;

            /// Disconnect this node, which cuts off this server.
            tailscale_fixture_sever => NoParams, run_local,
                toolset: LocalPrefs, tier: Destructive, severing: true;

            /// Something only a much newer binary has.
            tailscale_fixture_new => NoParams, run_local,
                toolset: LocalStatus, tier: Read, since: "1.94";

            /// Something the debug toolset offers, which no preset includes.
            tailscale_fixture_debug => NoParams, run_local,
                toolset: LocalDebug, tier: Read;

            /// Read something about the tailnet.
            tailnet_fixture_read => NoParams, run_tailnet,
                toolset: TailnetDevices, tier: Read, idempotent: true;

            /// Delete something from the tailnet.
            tailnet_fixture_delete => NoParams, run_tailnet,
                toolset: TailnetDevices, tier: Destructive, confirm: true;
        }

        async fn run_local(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
            let text = crate::cli::run_text(
                ctx,
                &metas::tailscale_fixture_read,
                Invocation::read(["fixture"]),
            )
            .await?;
            Ok(serde_json::json!({ "text": text.trim() }))
        }

        async fn run_tailnet(_ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
            Ok(serde_json::json!({ "ok": true }))
        }
    }

    fn config(cli: Cli) -> Config {
        Config::resolve_with(cli, |_| None).expect("test configuration resolves")
    }

    fn backends(local: Option<StubBackend>, credentialled: bool) -> Backends {
        let local_available = local.is_some();
        Backends {
            local: local.map_or_else(
                || Arc::new(Unavailable::default()) as Arc<dyn LocalBackend>,
                |b| Arc::new(b) as Arc<dyn LocalBackend>,
            ),
            local_available,
            credentials: credentialled
                .then(|| Credentials::ApiKey(Secret::new("tskey-api-example"))),
        }
    }

    /// A stub that answers the startup probes the way a healthy node does.
    fn healthy_node() -> StubBackend {
        StubBackend::ok("")
            .on(["version"], Reply::ok("1.102.2\n"))
            .on(
                ["status", "--json"],
                Reply::ok(
                    json!({
                        "Self": {
                            "ID": "n1234567CNTRL",
                            "PublicKey": "nodekey:abc",
                            "TailscaleIPs": ["100.64.0.1", "fd7a::1"],
                            "DNSName": "workstation.example-tailnet.ts.net."
                        }
                    })
                    .to_string(),
                ),
            )
    }

    async fn server(cli: Cli, backends: Backends) -> Startup {
        build(&config(cli), fixture::entries(), backends)
            .await
            .expect("the server builds")
    }

    fn names(startup: &Startup) -> Vec<String> {
        startup
            .server
            .tools()
            .expect("tools describe")
            .iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    async fn call(server: &TailscaleMcpServer, name: &str, arguments: Value) -> CallToolResult {
        let arguments = arguments
            .as_object()
            .cloned()
            .expect("arguments must be an object");
        let response = server
            .respond(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
            .await;
        match response {
            CallToolResponse::Complete(result) => result,
            other => panic!("expected a completed call, got {other:?}"),
        }
    }

    fn error_of(result: &CallToolResult) -> Value {
        assert_eq!(result.is_error, Some(true), "expected a failed call");
        result
            .structured_content
            .clone()
            .expect("a failed call carries structured content")
    }

    #[tokio::test]
    async fn a_server_with_both_surfaces_offers_both() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        let names = names(&startup);
        assert!(
            names.iter().any(|n| n.starts_with("tailscale_")),
            "{names:?}"
        );
        assert!(names.iter().any(|n| n.starts_with("tailnet_")), "{names:?}");
    }

    #[tokio::test]
    async fn without_a_binary_the_local_tools_are_hidden_and_the_tailnet_tools_remain() {
        let startup = server(Cli::default(), backends(None, true)).await;
        let names = names(&startup);
        assert!(
            !names.is_empty(),
            "the tailnet surface should still be offered"
        );
        assert!(
            names.iter().all(|n| n.starts_with("tailnet_")),
            "a local tool survived a missing binary: {names:?}"
        );
        assert!(
            startup
                .notes
                .iter()
                .any(|n| n.contains("`tailscale` binary")),
            "{:?}",
            startup.notes
        );
    }

    #[tokio::test]
    async fn without_a_credential_the_tailnet_tools_are_hidden_and_the_local_tools_remain() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), false)).await;
        let names = names(&startup);
        assert!(
            !names.is_empty(),
            "the local surface should still be offered"
        );
        assert!(
            names.iter().all(|n| n.starts_with("tailscale_")),
            "a tailnet tool survived a missing credential: {names:?}"
        );
        assert!(
            startup.notes.iter().any(|n| n.contains("credential")),
            "{:?}",
            startup.notes
        );
    }

    #[tokio::test]
    async fn a_surface_switched_off_is_reported_as_a_choice_not_a_failure() {
        let startup = server(
            Cli {
                no_tailnet: true,
                ..Cli::default()
            },
            backends(Some(healthy_node()), true),
        )
        .await;
        assert!(
            startup.notes.iter().any(|n| n.contains("switched off")),
            "{:?}",
            startup.notes
        );
    }

    #[tokio::test]
    async fn a_surface_switched_off_stays_off_even_when_it_could_have_worked() {
        // The flag is the operator's decision, not a description of the machine.
        let startup = server(
            Cli {
                no_tailnet: true,
                ..Cli::default()
            },
            backends(Some(healthy_node()), true),
        )
        .await;
        let names = names(&startup);
        assert!(!names.is_empty());
        assert!(
            names.iter().all(|n| n.starts_with("tailscale_")),
            "a tailnet tool survived --no-tailnet: {names:?}"
        );
    }

    #[tokio::test]
    async fn a_local_surface_switched_off_is_not_probed() {
        let backend = Arc::new(healthy_node());
        let startup = build(
            &config(Cli {
                no_local: true,
                ..Cli::default()
            }),
            fixture::entries(),
            Backends {
                local: Arc::clone(&backend) as Arc<dyn LocalBackend>,
                local_available: true,
                credentials: Some(Credentials::ApiKey(Secret::new("tskey-api-example"))),
            },
        )
        .await
        .expect("the tailnet surface still starts");
        assert!(backend.calls().is_empty(), "{:?}", backend.calls());
        assert_eq!(startup.server.context().cli_version, None);
    }

    #[tokio::test]
    async fn neither_surface_is_a_startup_error() {
        let err = build(
            &config(Cli::default()),
            fixture::entries(),
            backends(None, false),
        )
        .await
        .expect_err("a server that can do nothing should not start");
        assert!(matches!(
            err,
            StartupError::Config(ConfigError::NoToolsEnabled)
        ));
    }

    #[tokio::test]
    async fn an_old_binary_is_warned_about_and_hides_nothing() {
        let old = StubBackend::ok("").on(["version"], Reply::ok("1.72.0\n"));
        let startup = server(Cli::default(), backends(Some(old), true)).await;
        assert!(
            startup
                .notes
                .iter()
                .any(|n| n.contains("1.72.0") && n.contains("Nothing is hidden")),
            "{:?}",
            startup.notes
        );
        assert!(
            names(&startup).iter().any(|n| n.starts_with("tailscale_")),
            "an old binary should hide nothing"
        );
    }

    #[tokio::test]
    async fn an_unstable_build_is_not_warned_about() {
        let unstable = StubBackend::ok("").on(["version"], Reply::ok("1.77.0\n"));
        let startup = server(Cli::default(), backends(Some(unstable), true)).await;
        assert!(
            !startup.notes.iter().any(|n| n.contains("older than")),
            "{:?}",
            startup.notes
        );
    }

    #[tokio::test]
    async fn an_unreadable_version_is_noted_and_blocks_nothing() {
        let odd = StubBackend::ok("").on(["version"], Reply::failed(1, "no"));
        let startup = server(Cli::default(), backends(Some(odd), true)).await;
        assert!(
            startup
                .notes
                .iter()
                .any(|n| n.contains("Could not read the version")),
            "{:?}",
            startup.notes
        );
        assert!(names(&startup).iter().any(|n| n.starts_with("tailscale_")));
    }

    #[tokio::test]
    async fn the_server_information_names_this_server_and_carries_instructions() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        let info = startup.server.get_info();
        assert_eq!(info.server_info.name, server_name());
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert!(
            info.capabilities.tools.is_some(),
            "tools must be advertised"
        );
        let instructions = info.instructions.expect("instructions are sent");
        assert!(instructions.contains("tailscale_*"), "{instructions}");
        assert!(instructions.contains("1.102.2"), "{instructions}");
    }

    #[tokio::test]
    async fn the_identity_probe_fills_in_who_we_are() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        let identity = &startup.server.context().identity;
        assert!(identity.matches("n1234567CNTRL"));
        assert!(identity.matches("100.64.0.1"));
        assert!(identity.matches("workstation"));
    }

    #[tokio::test]
    async fn without_a_local_surface_we_claim_no_identity() {
        let startup = server(Cli::default(), backends(None, true)).await;
        let identity = &startup.server.context().identity;
        assert!(!identity.matches("n1234567CNTRL"));
        assert_eq!(startup.server.context().cli_version, None);
    }

    #[tokio::test]
    async fn a_hidden_tool_is_not_reachable_by_name() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        // The debug toolset is outside every preset.
        assert!(startup.server.get_tool("tailscale_fixture_debug").is_none());
        let error = error_of(&call(&startup.server, "tailscale_fixture_debug", json!({})).await);
        assert_eq!(error["code"], ErrorCode::NotPermitted.as_str());
        assert!(
            error["hint"]
                .as_str()
                .is_some_and(|h| h.contains("local-debug")),
            "the refusal should say what to change: {error}"
        );
    }

    #[tokio::test]
    async fn a_tool_nobody_declared_is_not_found() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        let error = error_of(&call(&startup.server, "tailscale_invented", json!({})).await);
        assert_eq!(error["code"], ErrorCode::NotFound.as_str());
    }

    #[tokio::test]
    async fn a_write_tool_is_hidden_until_writing_is_permitted() {
        let read_only = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        assert!(!names(&read_only).contains(&"tailscale_fixture_write".to_owned()));
        assert_eq!(read_only.server.gate().max_tier(), Tier::Read);

        let writable = server(
            Cli {
                allow_write: true,
                ..Cli::default()
            },
            backends(Some(healthy_node()), true),
        )
        .await;
        assert!(names(&writable).contains(&"tailscale_fixture_write".to_owned()));
        assert!(
            !names(&writable).contains(&"tailscale_fixture_sever".to_owned()),
            "writing does not permit destruction"
        );
    }

    #[tokio::test]
    async fn a_tool_that_fails_answers_with_a_result_not_a_protocol_error() {
        let broken = StubBackend::failure(1, "something went wrong")
            .on(["version"], Reply::ok("1.102.2\n"))
            .on(["status", "--json"], Reply::ok("{}"));
        let startup = server(Cli::default(), backends(Some(broken), true)).await;
        let error = error_of(&call(&startup.server, "tailscale_fixture_read", json!({})).await);
        assert_eq!(error["code"], ErrorCode::CliFailed.as_str());
        assert_eq!(error["exit_code"], 1);
    }

    #[tokio::test]
    async fn a_tool_that_succeeds_answers_with_structured_content() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        let result = call(&startup.server, "tailnet_fixture_read", json!({})).await;
        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result.structured_content.expect("structured content")["ok"],
            true
        );
    }

    #[tokio::test]
    async fn a_confirmable_tool_refuses_until_it_is_confirmed() {
        let startup = server(
            Cli {
                allow_destructive: true,
                ..Cli::default()
            },
            backends(Some(healthy_node()), true),
        )
        .await;
        let error = error_of(&call(&startup.server, "tailnet_fixture_delete", json!({})).await);
        assert_eq!(error["code"], ErrorCode::ConfirmationRequired.as_str());

        let result = call(
            &startup.server,
            "tailnet_fixture_delete",
            json!({ CONFIRM_PARAM: true }),
        )
        .await;
        assert_eq!(result.is_error, Some(false), "{result:?}");
    }

    #[tokio::test]
    async fn a_tool_newer_than_the_binary_reports_the_version_it_needs() {
        let old = StubBackend::ok("")
            .on(["version"], Reply::ok("1.80.0\n"))
            .on(["status", "--json"], Reply::ok("{}"));
        let startup = server(Cli::default(), backends(Some(old), true)).await;
        let error = error_of(&call(&startup.server, "tailscale_fixture_new", json!({})).await);
        assert_eq!(error["code"], ErrorCode::UnsupportedVersion.as_str());
        assert!(
            error["message"]
                .as_str()
                .is_some_and(|m| m.contains("1.94")),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_result_larger_than_the_cap_is_refused() {
        let startup = server(
            Cli {
                max_result_bytes: Some(4),
                ..Cli::default()
            },
            backends(Some(healthy_node()), true),
        )
        .await;
        let error = error_of(&call(&startup.server, "tailnet_fixture_read", json!({})).await);
        assert_eq!(error["code"], ErrorCode::ResultTooLarge.as_str());
    }

    #[tokio::test]
    async fn the_listing_is_stable_and_every_entry_is_annotated() {
        let startup = server(
            Cli {
                allow_destructive: true,
                ..Cli::default()
            },
            backends(Some(healthy_node()), true),
        )
        .await;
        let first = names(&startup);
        assert_eq!(first, names(&startup), "the listing order must not vary");
        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(first, sorted, "tools are listed in name order");

        for tool in startup.server.tools().expect("describe") {
            let annotations = tool.annotations.expect("every tool is annotated");
            assert_eq!(annotations.open_world_hint, Some(true), "{}", tool.name);
            let destructive = tool.name.contains("sever") || tool.name.contains("delete");
            assert_eq!(
                annotations.destructive_hint,
                Some(destructive),
                "{}",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn a_selection_naming_only_hidden_surfaces_does_not_start() {
        let err = build(
            &config(Cli {
                toolsets: Some("tailnet-devices".to_owned()),
                ..Cli::default()
            }),
            fixture::entries(),
            backends(Some(healthy_node()), false),
        )
        .await
        .expect_err("nothing would be offered");
        assert!(matches!(
            err,
            StartupError::Config(ConfigError::NoToolsEnabled)
        ));
    }

    #[tokio::test]
    async fn the_notes_say_what_is_on_offer() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), true)).await;
        let summary = startup
            .notes
            .last()
            .expect("a summary is always the last note");
        assert!(summary.contains("Offering"), "{summary}");
        assert!(summary.contains("local-status"), "{summary}");
        assert!(summary.contains("read"), "{summary}");
    }

    /// `call_tool` without a [`RequestContext`], which only a live session can
    /// produce. Every policy the handler applies lives in `dispatch`, so this
    /// exercises the same path.
    impl TailscaleMcpServer {
        async fn respond(&self, request: CallToolRequestParams) -> CallToolResponse {
            CallToolResponse::Complete(match self.dispatch(request).await {
                Ok(value) => CallToolResult::structured(value),
                Err(error) => CallToolResult::structured_error(error.to_value()),
            })
        }
    }
}
