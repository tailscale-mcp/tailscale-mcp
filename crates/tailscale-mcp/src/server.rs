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
    CallToolRequestParams, CallToolResponse, CallToolResult, GetPromptRequestParams,
    GetPromptResponse, GetPromptResult, Implementation, InitializeResult, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use serde_json::Value;
use tailscale_cli::{CliBackend, LocalBackend, Unavailable};
use tailscale_rest::Credentials;

use crate::config::Config;
use crate::context::{Identity, PathPolicy, SelfIdentity, ToolContext};
use crate::error::{ToolError, ToolResult};
use crate::gating::{ConfigError, Gate};
use crate::meta::Surface;
use crate::registry::{Registry, RegistryError, ToolEntry};
use crate::version::SUPPORTED_FLOOR;
use crate::{cli, instructions, resources};

/// The server could not be built.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A control-plane client that cannot be built is a startup failure rather
    /// than a note. In practice this is the base URL having been set to
    /// somewhere a credential must not be sent, and carrying on would mean
    /// running with the tailnet surface silently missing.
    #[error(transparent)]
    ControlPlane(#[from] tailscale_rest::ApiError),
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
    /// What this node calls the nodes at these addresses, read from the same
    /// status the identity came from. The HTTP transport names its callers
    /// with it; a stdio session has one caller and no use for it.
    pub peers: std::collections::HashMap<std::net::IpAddr, String>,
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
    let (cli_version, identity, peers) = if local_offered {
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
        let (identity, peers) = cli::probe_node(backends.local.as_ref()).await;
        (
            version,
            // Live: it may be read again as it ages, because a node can be
            // renamed or moved onto a different address under a running
            // server (Q87).
            Identity::probed(identity),
            peers,
        )
    } else {
        // Nothing was read, so there is nothing to re-read: a fixed identity
        // that matches nothing rather than a timer asking a missing binary.
        (
            None,
            Identity::fixed(SelfIdentity::default()),
            std::collections::HashMap::new(),
        )
    };

    // Only when the surface is on offer: a credential in the environment is
    // not a reason to build a client for tools nobody can call. The address is
    // judged either way, so that an operator who has redirected the control
    // plane hears about it now rather than on the day they add a credential.
    let tailnet = if unavailable.contains(&Surface::Tailnet) {
        None
    } else {
        tailscale_rest::checked_base_url(&config.api_base_url)?;
        match &backends.credentials {
            Some(credentials) => {
                let mut client = tailscale_rest::ClientConfig::new(credentials.clone());
                client.base_url = config.api_base_url.clone();
                client.tailnet = config.tailnet.clone();
                client.max_response_bytes = config.max_result_bytes;
                // One number for both surfaces: a control-plane call is a tool
                // call, and a tool call has one timeout.
                client.budget = tailscale_cli::DEFAULT_TIMEOUT;
                Some(tailscale_rest::Client::new(client)?)
            }
            None => None,
        }
    };

    let gate = Gate::new(
        config.toolsets.clone(),
        config.max_tier,
        unavailable,
        &registry.metas(),
    )?;

    let ctx = ToolContext {
        local: Arc::clone(&backends.local),
        tailnet,
        redactor: crate::error::Redactor::for_credentials(backends.credentials.as_ref()),
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
        peers,
        notes,
    })
}

/// The toolsets this session actually offers something from.
///
/// Selected is not the same as offered: a toolset whose surface has no backend
/// — no `tailscale` binary, no control-plane credential — contributes no tools,
/// and naming it here beside a count that excludes it describes a session
/// nobody is in. [`Gate::offers`] is the question the instructions already ask
/// for exactly this reason, so the note asks it too.
fn describe_toolsets(gate: &Gate) -> String {
    let names: Vec<&str> = gate
        .offered_toolsets()
        .map(|toolset| toolset.as_str())
        .collect();
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
        let mut info = InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                // Resources and prompts, but not `enable_resources_subscribe`:
                // `spec.md` puts subscriptions out of scope, and advertising
                // one this server does not serve is a lie a client acts on.
                .enable_resources()
                .enable_prompts()
                .build(),
        )
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

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        // A resource whose surface is not offered is absent, not listed and
        // refused — the same rule the tool listing follows, for the same
        // reason: a client should not be shown something it cannot have.
        Ok(ListResourcesResult::with_all_items(
            resources::all()
                .iter()
                .filter(|entry| !entry.templated && self.gate.offers(entry.surface))
                .map(resources::ResourceEntry::describe)
                .collect(),
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resources::all()
                .iter()
                .filter(|entry| entry.templated && self.gate.offers(entry.surface))
                .map(resources::ResourceEntry::describe_template)
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let read = resources::read(&self.ctx, |surface| self.gate.offers(surface), &request.uri);
        match read.await {
            Ok(result) => Ok(ReadResourceResponse::Complete(result)),
            // Unlike a tool call, a resource read has no result shape to carry
            // a failure in, so this one really is a protocol error.
            Err(error) => Err(McpError::resource_not_found(
                error.message.clone(),
                Some(serde_json::json!({"uri": request.uri, "error": error.to_value()})),
            )),
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(
            resources::prompts()
                .iter()
                .map(resources::PromptEntry::describe)
                .collect(),
        ))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let prompts = resources::prompts();
        let prompt = prompts
            .iter()
            .find(|prompt| prompt.name == request.name)
            .ok_or_else(|| {
                McpError::invalid_params(format!("`{}` is not a prompt", request.name), None)
            })?;
        let (argument, _) = prompt.argument;
        let given = request
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get(argument))
            .and_then(Value::as_str);
        Ok(GetPromptResponse::Complete(
            GetPromptResult::new(prompt.expand(given)).with_description(prompt.description),
        ))
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
                            "PublicKey": "nodekey:aaa",
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

    /// A server pointed somewhere in particular, so that the base URL is the
    /// only thing the pair below varies. Everything else is a healthy node
    /// with a credential, which is the state in which the address matters.
    async fn build_pointed_at(base_url: &str) -> Result<Startup, StartupError> {
        let config = Config::resolve_with(Cli::default(), |key| {
            (key == crate::config::API_BASE_URL_ENV).then(|| base_url.to_owned())
        })
        .expect("the configuration itself resolves");

        build(
            &config,
            fixture::entries(),
            backends(Some(healthy_node()), true),
        )
        .await
    }

    #[tokio::test]
    async fn a_base_url_that_is_not_the_control_plane_stops_the_server_starting() {
        // Not a note and not a hidden failure: the only way to reach this is
        // to have redirected every credential this server holds, and starting
        // anyway would mean running with the tailnet surface quietly missing.
        let error = build_pointed_at("http://control.example.com")
            .await
            .expect_err("plaintext to another host is refused");

        let reported = error.to_string();
        assert!(
            reported.contains("loopback") && reported.contains("control.example.com"),
            "the failure should say what was wrong and with what: {reported}"
        );
    }

    #[tokio::test]
    async fn a_loopback_address_is_the_one_thing_that_may_stand_in_for_it() {
        // The other half of the pair, and what the whole suite rides on: the
        // fake control plane is reachable because it is on this machine, and
        // for no other reason. Same scheme, same everything, different host.
        let fake = tailscale_rest::fake::FakeControlPlane::start()
            .await
            .expect("a loopback socket");

        let startup = build_pointed_at(fake.base_url())
            .await
            .expect("a fake on this machine is a place a credential may go");
        assert!(startup.server.context().tailnet().is_ok());
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
        let identity = startup.server.context().identity.last_known();
        assert!(identity.matches("n1234567CNTRL"));
        assert!(identity.matches("100.64.0.1"));
        assert!(identity.matches("workstation"));
    }

    #[tokio::test]
    async fn without_a_local_surface_we_claim_no_identity() {
        let startup = server(Cli::default(), backends(None, true)).await;
        let identity = startup.server.context().identity.last_known();
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

    /// The count and the list are one sentence and have to agree.
    ///
    /// With no credential the tailnet toolsets are still *selected* — the
    /// preset picks them and nothing in the configuration says otherwise — but
    /// they contribute nothing, and the count beside them already knows that.
    /// Naming them anyway told a reader that `tailnet-devices` was on offer in
    /// a session where every call to it would be refused, which is the one
    /// thing a startup note exists to prevent.
    #[tokio::test]
    async fn the_summary_names_no_toolset_that_offers_nothing() {
        let startup = server(Cli::default(), backends(Some(healthy_node()), false)).await;
        let summary = startup
            .notes
            .last()
            .expect("a summary is always the last note");
        assert!(
            summary.contains("local-status"),
            "the local surface is there and should be named: {summary}"
        );
        assert!(
            !summary.contains("tailnet-"),
            "no credential, so no tailnet toolset offers anything; naming one \
             contradicts the count in the same sentence: {summary}"
        );
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
