//! Against the real `tailscale` on this machine and the real control plane.
//!
//! **Off unless switched on**, and switched on by an environment variable
//! rather than a feature flag, so that turning them on is a thing somebody
//! does in a shell and never a thing a build inherits. `spec.md` puts them out
//! of continuous integration entirely: the machine there has no tailnet, and a
//! test suite that needs a credential to be green is a suite people learn to
//! ignore.
//!
//! | Variable | What it lets run |
//! |---|---|
//! | `TAILSCALE_MCP_E2E_LOCAL` | the local read paths, against this machine's `tailscale` |
//! | `TAILSCALE_MCP_E2E_TAILNET` | the tailnet read paths, against a real tailnet |
//! | `TAILSCALE_MCP_E2E_WRITE` | the one write path, which cleans up after itself |
//!
//! **Read-only unless the third is set**, which is separate from the other two
//! on purpose: somebody switching on the tailnet tests to check a credential
//! should not thereby be writing to their own tailnet.
//!
//! Everything here goes through the in-process client and a fully constructed
//! server, the same seam the rest of the suite uses. What differs is what is
//! underneath: real backends rather than fakes.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]

use rmcp::ServiceExt as _;
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::server::{self, Backends};

/// The gates, and what each is for.
const GATES: &[(&str, &str)] = &[
    ("TAILSCALE_MCP_E2E_LOCAL", "the local read paths"),
    ("TAILSCALE_MCP_E2E_TAILNET", "the tailnet read paths"),
    ("TAILSCALE_MCP_E2E_WRITE", "the one write path"),
];

/// Whether a gate is open.
fn open(gate: &str) -> bool {
    std::env::var(gate).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Say why nothing ran, and return `false`.
///
/// Printed rather than silent, because a test that skips without saying so is
/// indistinguishable from a test that passed, and the criterion is that the
/// suite "skips these tests and reports why". `cargo test` shows this with
/// `--nocapture`; the summary test below reports it either way.
fn skipped(gate: &str, what: &str) -> bool {
    println!("skipped: {what} would need {gate} set; it is not");
    false
}

fn ready(gate: &str, what: &str) -> bool {
    open(gate) || skipped(gate, what)
}

/// A server over the real backends this machine has.
async fn real(args: &[&str]) -> (Harnessed, Vec<String>) {
    let cli = <Cli as clap::Parser>::try_parse_from(args).expect("the arguments parse");
    let config = Config::resolve(cli).expect("the configuration resolves");
    let backends = Backends::discover(&config);
    let startup = server::build(&config, tailscale_mcp::tools::entries(), backends)
        .await
        .expect("the server builds");
    let notes = startup.notes.clone();

    let (client_side, server_side) = tokio::io::duplex(1 << 20);
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
    (
        Harnessed {
            client: Some(client),
            serving,
        },
        notes,
    )
}

/// A tool call, with arguments only when there are any.
fn request(name: &str, args: &Value) -> CallToolRequestParams {
    let request = CallToolRequestParams::new(name.to_owned());
    match args.as_object() {
        Some(object) if !object.is_empty() => request.with_arguments(object.clone()),
        _ => request,
    }
}

struct Harnessed {
    client: Option<rmcp::service::RunningService<rmcp::RoleClient, ()>>,
    serving: tokio::task::JoinHandle<()>,
}

impl Harnessed {
    /// Call a tool and expect it to work, saying what it answered if not.
    async fn call(&self, name: &str, args: Value) -> Value {
        let answer = self
            .client
            .as_ref()
            .expect("a client")
            .call_tool(request(name, &args))
            .await
            .unwrap_or_else(|error| panic!("`{name}` should be callable: {error}"));
        assert!(
            answer.is_error != Some(true),
            "`{name}` failed against the real thing: {:?}",
            answer.content
        );
        answer.structured_content.unwrap_or(Value::Null)
    }

    async fn shutdown(mut self) {
        if let Some(client) = self.client.take() {
            let _ = client.cancel().await;
        }
        self.serving.abort();
    }
}

/// What is switched on, said out loud whatever happens.
///
/// This one always runs, so that a suite where every gated test quietly did
/// nothing still says so. Without it, "all tests passed" would mean two
/// different things and look identical.
#[test]
fn the_gates_report_what_is_switched_off() {
    let mut open_gates = Vec::new();
    for (gate, what) in GATES {
        if open(gate) {
            open_gates.push(*what);
        } else {
            println!("skipped: {what} would need {gate} set; it is not");
        }
    }
    println!(
        "end-to-end: {}",
        if open_gates.is_empty() {
            "nothing is switched on; every test below did nothing".to_owned()
        } else {
            format!("running {}", open_gates.join(", "))
        }
    );
}

/// The local read paths, against whatever `tailscale` is on this machine.
#[tokio::test]
async fn the_local_read_paths_work_against_this_machine() {
    if !ready("TAILSCALE_MCP_E2E_LOCAL", "the local read paths") {
        return;
    }
    let (harness, notes) = real(&["tailscale-mcp", "--no-tailnet", "--preset", "full"]).await;
    println!("{}", notes.join("\n"));

    let status = harness.call("tailscale_status", json!({})).await;
    assert!(
        status["Self"]["ID"].as_str().is_some(),
        "status should name this node: {status}"
    );

    let version = harness.call("tailscale_version", json!({})).await;
    assert!(
        version["version"].as_str().is_some_and(|v| !v.is_empty()),
        "the CLI should report a version: {version}"
    );

    // Reads that need no network and no daemon state beyond what is there.
    harness.call("tailscale_prefs_get", json!({})).await;
    harness.call("tailscale_ip", json!({})).await;
    harness.call("tailscale_dns_status", json!({})).await;

    harness.shutdown().await;
}

/// The tailnet read paths, against a real tailnet.
///
/// A read-only credential is enough for every one of these, which is what the
/// criterion asks: nothing here writes.
#[tokio::test]
async fn the_tailnet_read_paths_work_against_a_real_tailnet() {
    if !ready("TAILSCALE_MCP_E2E_TAILNET", "the tailnet read paths") {
        return;
    }
    let (harness, notes) = real(&["tailscale-mcp", "--no-local", "--preset", "full"]).await;
    println!("{}", notes.join("\n"));

    let devices = harness.call("tailnet_device_list", json!({})).await;
    let listed = devices["devices"].as_array().expect("a device list").len();
    assert!(listed > 0, "a real tailnet has at least this node in it");

    let policy = harness.call("tailnet_policy_get", json!({})).await;
    assert!(
        policy["policy"].is_string() || policy["policy"].is_object(),
        "the policy file, in whichever form: {policy}"
    );

    harness.call("tailnet_dns_get", json!({})).await;
    harness.call("tailnet_settings_get", json!({})).await;
    harness.call("tailnet_key_list", json!({})).await;

    // One device by the id the listing gave, which is the read that would
    // catch an identifier this server builds wrongly.
    let first = devices["devices"][0]["nodeId"]
        .as_str()
        .expect("a node id")
        .to_owned();
    let device = harness
        .call("tailnet_device_get", json!({"device_id": first}))
        .await;
    assert!(device["nodeId"].as_str().is_some(), "one device: {device}");

    harness.shutdown().await;
}

/// The one write path, which puts back what it found.
///
/// A device's attributes are the smallest thing that can be written and
/// removed without affecting anything: a custom posture attribute belongs to
/// nothing until a policy rule names one, and this names one nothing will.
#[tokio::test]
async fn a_write_cleans_up_after_itself() {
    if !ready("TAILSCALE_MCP_E2E_WRITE", "the one write path") {
        return;
    }
    assert!(
        open("TAILSCALE_MCP_E2E_TAILNET"),
        "the write gate is not a way round the tailnet gate; set both"
    );
    let (harness, _) = real(&[
        "tailscale-mcp",
        "--no-local",
        "--preset",
        "full",
        "--allow-destructive",
    ])
    .await;

    let devices = harness.call("tailnet_device_list", json!({})).await;
    let device = devices["devices"][0]["nodeId"]
        .as_str()
        .expect("a node id")
        .to_owned();
    let attribute = "custom:tailscaleMcpEndToEnd";

    harness
        .call(
            "tailnet_device_attribute_set",
            json!({"device_id": device, "key": attribute, "value": true}),
        )
        .await;
    let after = harness
        .call(
            "tailnet_device_attributes_get",
            json!({"device_id": device}),
        )
        .await;
    assert_eq!(
        after["attributes"][attribute],
        json!(true),
        "the attribute should be there before it is taken away: {after}"
    );

    harness
        .call(
            "tailnet_device_attribute_delete",
            json!({"device_id": device, "key": attribute}),
        )
        .await;
    let cleaned = harness
        .call(
            "tailnet_device_attributes_get",
            json!({"device_id": device}),
        )
        .await;
    assert!(
        cleaned["attributes"][attribute].is_null(),
        "and gone again afterwards, whatever the test did in between: {cleaned}"
    );

    harness.shutdown().await;
}
