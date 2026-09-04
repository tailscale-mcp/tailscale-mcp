//! The seam every integration test works through: a whole server, built the
//! way the binary builds it, with a fake `tailscale` and a fake control plane
//! underneath, driven by a real client over a real transport.
//!
//! Nothing here reaches into the server's own types to assert on. If a
//! behaviour cannot be seen from the client's side of the connection, a client
//! cannot rely on it either.
//!
//! Shared by several test binaries, each of which uses a different part of it.
#![allow(dead_code)]

use std::sync::Arc;

use rmcp::ServiceExt as _;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::RunningService;
use serde_json::{Value, json};
use tailscale_cli::LocalBackend;
use tailscale_cli::stub::{Reply, StubBackend};
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::meta::Tier;
use tailscale_mcp::registry::ToolEntry;
use tailscale_mcp::server::{self, Backends};
use tailscale_rest::fake::FakeControlPlane;
use tailscale_rest::{Credentials, Secret};

/// The credential every test runs with. Not a real key shape by accident: it
/// is the shape the redactor is expected to recognise.
pub const TEST_API_KEY: &str = "tskey-api-redacted-example";

/// The version the fake `tailscale` reports unless a test says otherwise.
/// Above the supported floor, so the version warning stays out of the way.
pub const TEST_CLI_VERSION: &str = "1.102.2";

/// What `tailscale status --json` answers with by default: a node with an
/// identity, so that self-severing detection has something to match against.
pub fn status_json() -> Value {
    json!({
        "Version": TEST_CLI_VERSION,
        "BackendState": "Running",
        "Self": {
            "ID": "n1111111CNTRL",
            "PublicKey": "nodekey:1111111111111111111111111111111111111111111111111111111111111111",
            "HostName": "workstation",
            "DNSName": "workstation.example-tailnet.ts.net.",
            "OS": "macOS",
            "TailscaleIPs": ["100.64.0.1", "fd7a:115c:a1e0::1"],
            "Online": true
        }
    })
}

/// How the server should be built for one test.
pub struct Setup {
    entries: Vec<ToolEntry>,
    cli: Cli,
    /// A whole backend, when a test wants control of every answer.
    backend: Option<StubBackend>,
    /// Answers this test arranged, which take precedence over the probes.
    cli_rules: Vec<(Vec<String>, Reply)>,
    control_plane: Option<FakeControlPlane>,
    credentialled: bool,
    /// Variables the server should see. The suite answers nothing else, so no
    /// test can pass or fail because of what is set on the machine.
    env: Vec<(String, String)>,
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}

impl Setup {
    /// The real tool table, the default preset, the read tier, a healthy fake
    /// node and a credential: what an operator gets by running the binary.
    pub fn new() -> Self {
        Self {
            entries: tailscale_mcp::tools::entries(),
            cli: Cli::default(),
            backend: None,
            cli_rules: Vec::new(),
            control_plane: None,
            credentialled: true,
            env: Vec::new(),
        }
    }

    /// Use a different tool table, for tests about the machinery rather than
    /// about the tools.
    #[must_use]
    pub fn entries(mut self, entries: Vec<ToolEntry>) -> Self {
        self.entries = entries;
        self
    }

    #[must_use]
    pub fn preset(mut self, preset: &str) -> Self {
        self.cli.preset = Some(preset.to_owned());
        self
    }

    #[must_use]
    pub fn toolsets(mut self, toolsets: &str) -> Self {
        self.cli.toolsets = Some(toolsets.to_owned());
        self
    }

    /// Permit up to and including this tier.
    #[must_use]
    pub fn tier(mut self, tier: Tier) -> Self {
        self.cli.allow_write = tier >= Tier::Write;
        self.cli.allow_destructive = tier >= Tier::Destructive;
        self
    }

    /// Replace the fake `tailscale` outright, startup probes included.
    #[must_use]
    pub fn backend(mut self, backend: StubBackend) -> Self {
        self.backend = Some(backend);
        self
    }

    /// Answer one command. Answers arranged here are matched before the
    /// startup probes, so a test can make `version` itself fail.
    #[must_use]
    pub fn cli_answers(mut self, argv: &[&str], reply: Reply) -> Self {
        self.cli_rules
            .push((argv.iter().map(|a| (*a).to_owned()).collect(), reply));
        self
    }

    /// Start a fake control plane and give it a rule.
    ///
    /// # Panics
    /// If no loopback socket can be bound, which no test can work without.
    pub async fn api_answers(
        mut self,
        method: &str,
        path: &str,
        response: tailscale_rest::fake::Response,
    ) -> Self {
        let fake = match self.control_plane.take() {
            Some(fake) => fake,
            None => FakeControlPlane::start()
                .await
                .expect("a loopback socket for the fake control plane"),
        };
        self.control_plane = Some(fake.on(method, path, response));
        self
    }

    /// Give the fake control plane a rule that answers once, so a sequence of
    /// answers to the same request can be arranged — a paginated listing being
    /// the case that needs it.
    ///
    /// # Panics
    /// If no loopback socket can be bound, which no test can work without.
    pub async fn api_answers_once(
        mut self,
        method: &str,
        path: &str,
        response: tailscale_rest::fake::Response,
    ) -> Self {
        let fake = match self.control_plane.take() {
            Some(fake) => fake,
            None => FakeControlPlane::start()
                .await
                .expect("a loopback socket for the fake control plane"),
        };
        self.control_plane = Some(fake.once(method, path, response));
        self
    }

    /// Set an environment variable for this server.
    #[must_use]
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    /// Build with no `tailscale` binary on the machine.
    #[must_use]
    pub fn without_cli(mut self) -> Self {
        self.cli.no_local = true;
        self
    }

    /// Build with the tailnet surface switched off, credential or not.
    #[must_use]
    pub fn without_tailnet(mut self) -> Self {
        self.cli.no_tailnet = true;
        self
    }

    /// Build with no control-plane credential.
    #[must_use]
    pub fn without_credentials(mut self) -> Self {
        self.credentialled = false;
        self
    }

    /// Build the server and connect a client to it.
    ///
    /// # Panics
    /// If the configuration or the server does not build, which is the test
    /// itself being wrong rather than a behaviour worth reporting.
    pub async fn start(self) -> Harness {
        // Whatever the test asked for, and then the fake's address. The
        // test's own entry comes first so a test that sets the base URL
        // itself is not overruled by a fake it also arranged.
        let mut env = self.env;
        if let Some(fake) = self.control_plane.as_ref() {
            env.push((
                tailscale_mcp::config::API_BASE_URL_ENV.to_owned(),
                fake.base_url().to_owned(),
            ));
        }
        let config = Config::resolve_with(self.cli, |key| {
            env.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        })
        .expect("the configuration resolves");
        let backend = Arc::new(self.backend.unwrap_or_else(|| {
            let mut backend = StubBackend::failure(1, "the test did not say what this should do");
            for (argv, reply) in self.cli_rules {
                backend = backend.on(argv, reply);
            }
            probes(backend)
        }));
        let local_available = !config.is_disabled(tailscale_mcp::meta::Surface::Local);
        let backends = Backends {
            local: Arc::clone(&backend) as Arc<dyn LocalBackend>,
            local_available,
            credentials: self
                .credentialled
                .then(|| Credentials::ApiKey(Secret::new(TEST_API_KEY))),
        };

        let startup = server::build(&config, self.entries, backends)
            .await
            .expect("the server builds");
        let notes = startup.notes.clone();
        let context = Arc::clone(startup.server.context());

        let (client_side, server_side) = tokio::io::duplex(64 * 1024);
        let (server_read, server_write) = tokio::io::split(server_side);
        let serving = tokio::spawn(async move {
            let service = startup
                .server
                .serve((server_read, server_write))
                .await
                .expect("the server accepts the connection");
            let _ = service.waiting().await;
        });

        let (client_read, client_write) = tokio::io::split(client_side);
        let client = ().serve((client_read, client_write)).await.expect("the client connects");

        Harness {
            client,
            backend,
            control_plane: self.control_plane,
            context,
            notes,
            serving,
        }
    }
}

/// A running server, its fakes, and a client attached to it.
pub struct Harness {
    client: RunningService<rmcp::RoleClient, ()>,
    pub backend: Arc<StubBackend>,
    control_plane: Option<FakeControlPlane>,
    /// What the handlers were given, for the parts of a session that are not
    /// reachable through a tool call.
    pub context: Arc<tailscale_mcp::context::ToolContext>,
    /// What the operator would have seen on standard error.
    pub notes: Vec<String>,
    serving: tokio::task::JoinHandle<()>,
}

impl Harness {
    /// The fake control plane, for asserting on what reached it.
    ///
    /// # Panics
    /// If the test did not arrange one with [`Setup::api_answers`].
    pub fn control_plane(&self) -> &FakeControlPlane {
        self.control_plane
            .as_ref()
            .expect("this test arranged no control-plane answers")
    }

    /// What the server said about itself during the handshake.
    ///
    /// # Panics
    /// If the handshake did not complete.
    pub fn info(&self) -> Arc<rmcp::model::ServerPeerInfo> {
        self.client.peer_info().expect("the handshake completed")
    }

    /// The instructions sent with the handshake.
    ///
    /// # Panics
    /// If none were sent.
    pub fn instructions(&self) -> String {
        self.info()
            .instructions
            .clone()
            .expect("instructions are sent")
    }

    /// Every tool on offer.
    ///
    /// # Panics
    /// If the listing fails.
    pub async fn tools(&self) -> Vec<Tool> {
        self.client
            .list_all_tools()
            .await
            .expect("tools are listed")
    }

    /// Every resource on offer, and every template.
    ///
    /// # Panics
    /// If a listing fails.
    pub async fn resources(&self) -> Vec<rmcp::model::Resource> {
        self.client
            .list_all_resources()
            .await
            .expect("resources are listed")
    }

    pub async fn resource_templates(&self) -> Vec<rmcp::model::ResourceTemplate> {
        self.client
            .list_all_resource_templates()
            .await
            .expect("templates are listed")
    }

    /// Read one resource, or the protocol error saying why not.
    pub async fn read_resource(
        &self,
        uri: &str,
    ) -> Result<rmcp::model::ReadResourceResult, String> {
        self.client
            .read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
            .await
            .map_err(|error| error.to_string())
    }

    /// Every prompt on offer.
    ///
    /// # Panics
    /// If the listing fails.
    pub async fn prompts(&self) -> Vec<rmcp::model::Prompt> {
        self.client
            .list_all_prompts()
            .await
            .expect("prompts are listed")
    }

    /// Expand one prompt, with or without its argument.
    ///
    /// # Panics
    /// If the prompt could not be got at all.
    pub async fn prompt(&self, name: &str, arguments: Value) -> rmcp::model::GetPromptResult {
        let mut request = rmcp::model::GetPromptRequestParams::new(name.to_owned());
        if let Some(object) = arguments.as_object()
            && !object.is_empty()
        {
            request = request.with_arguments(object.clone());
        }
        self.client
            .get_prompt(request)
            .await
            .unwrap_or_else(|e| panic!("`{name}` could not be got: {e}"))
    }

    /// The names of every tool on offer.
    pub async fn tool_names(&self) -> Vec<String> {
        self.tools()
            .await
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// One tool's description, when it is on offer.
    pub async fn tool(&self, name: &str) -> Option<Tool> {
        self.tools().await.into_iter().find(|t| t.name == name)
    }

    /// Call a tool. A refusal is a result, so this returns one either way.
    ///
    /// # Panics
    /// If the call could not be made at all, which means the session broke.
    pub async fn call(&self, name: &str, args: Value) -> CallToolResult {
        let mut request = CallToolRequestParams::new(name.to_owned());
        if let Some(object) = args.as_object()
            && !object.is_empty()
        {
            request = request.with_arguments(object.clone());
        }
        self.client
            .call_tool(request)
            .await
            .unwrap_or_else(|e| panic!("`{name}` could not be called: {e}"))
    }

    /// Call a tool and assert it worked, returning what it answered.
    ///
    /// # Panics
    /// If the call was refused or failed.
    pub async fn call_ok(&self, name: &str, args: Value) -> Value {
        let result = self.call(name, args).await;
        assert_eq!(
            result.is_error,
            Some(false),
            "`{name}` was expected to succeed, and answered {:#?}",
            result.structured_content
        );
        result
            .structured_content
            .unwrap_or_else(|| panic!("`{name}` answered nothing"))
    }

    /// Call a tool and assert it failed, returning the error it answered with.
    ///
    /// # Panics
    /// If the call succeeded.
    pub async fn call_err(&self, name: &str, args: Value) -> Value {
        let result = self.call(name, args).await;
        assert_eq!(
            result.is_error,
            Some(true),
            "`{name}` was expected to fail, and answered {:#?}",
            result.structured_content
        );
        result
            .structured_content
            .unwrap_or_else(|| panic!("`{name}` failed without saying why"))
    }

    /// Every argument list the fake `tailscale` was called with.
    pub fn cli_calls(&self) -> Vec<Vec<String>> {
        self.backend.argv()
    }

    /// Close the session and stop the server.
    pub async fn shutdown(self) {
        let _ = self.client.cancel().await;
        self.serving.abort();
    }
}

/// A fake `tailscale` that answers the startup probes like a healthy node.
pub fn healthy_node() -> StubBackend {
    probes(StubBackend::failure(
        1,
        "the test did not say what this command should do",
    ))
}

/// Add the two answers the server asks for at startup, as a fallback behind
/// whatever the test arranged.
fn probes(backend: StubBackend) -> StubBackend {
    backend
        .on(["version"], Reply::ok(format!("{TEST_CLI_VERSION}\n")))
        .on(["status", "--json"], Reply::ok(status_json().to_string()))
}

/// Read a recorded response from `tests/fixtures`.
///
/// # Panics
/// If the fixture is missing, which is the test being wrong.
pub fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture `{}` could not be read: {e}", path.display()))
}

/// Read a recorded JSON response from `tests/fixtures`.
///
/// # Panics
/// If the fixture is missing or is not JSON.
pub fn fixture_json(name: &str) -> Value {
    let raw = fixture(name);
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("fixture `{name}` is not JSON: {e}"))
}
