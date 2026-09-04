//! Every tool, checked the same way.
//!
//! The point of a table-driven contract is that adding a tool is not enough:
//! the tool has to say what a successful call and a failed call look like, or
//! the suite fails. What is asserted here is what a client can see — the tier,
//! the toolset, the annotations, and one call of each kind through a real
//! session — so a tool cannot pass by being right on the inside.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tailscale_cli::stub::Reply;
use tailscale_mcp::gating::Preset;
use tailscale_mcp::meta::{Tier, ToolMeta};
use tailscale_rest::fake::Response;

use harness::Setup;

/// What one tool needs to be answered with for one call.
#[derive(Default)]
struct Arrangement {
    /// Answers the fake `tailscale` should give, matched on an argument prefix.
    cli: Vec<(Vec<&'static str>, Reply)>,
    /// Answers the fake control plane should give.
    api: Vec<(&'static str, &'static str, Response)>,
}

impl Arrangement {
    fn cli(mut self, argv: &[&'static str], reply: Reply) -> Self {
        self.cli.push((argv.to_vec(), reply));
        self
    }

    #[expect(
        dead_code,
        reason = "used by the tailnet tools, which land in ticket 17"
    )]
    fn api(mut self, method: &'static str, path: &'static str, response: Response) -> Self {
        self.api.push((method, path, response));
        self
    }
}

/// One tool's contract: what it does when it works, and when it does not.
struct Contract {
    tool: &'static str,
    /// A call that should succeed, and what the world looks like when it does.
    success: (Value, Arrangement),
    /// A call that should fail, and the code it should fail with.
    failure: (Value, Arrangement, &'static str),
}

/// The contract for every tool in the table.
///
/// A tool with no row here fails [`every_tool_has_a_contract`], which is the
/// mechanism that keeps this list complete as the table grows.
fn contracts() -> Vec<Contract> {
    /// One row, written the way it reads: the tool, the call, the answer
    /// arranged for it, and the code the same tool gives when it goes wrong.
    macro_rules! contract {
        (
            $tool:literal,
            ok: $ok_args:tt on $ok_argv:expr => $ok_reply:expr,
            err: $err_args:tt on $err_argv:expr => $err_reply:expr, $code:literal
        ) => {
            Contract {
                tool: $tool,
                success: (
                    json!($ok_args),
                    Arrangement::default().cli(&$ok_argv, $ok_reply),
                ),
                failure: (
                    json!($err_args),
                    Arrangement::default().cli(&$err_argv, $err_reply),
                    $code,
                ),
            }
        };
    }

    /// A recorded answer from `tests/fixtures`.
    macro_rules! printed {
        ($name:literal) => {
            Reply::ok(harness::fixture($name))
        };
    }

    vec![
        contract!(
            "tailscale_status",
            ok: {} on ["status"] => printed!("status.json"),
            err: {} on ["status"] => Reply::Unavailable, "backend_unavailable"
        ),
        contract!(
            "tailscale_ip",
            ok: {} on ["ip"] => printed!("ip.txt"),
            err: {"target": "missing"} on ["ip"] =>
                Reply::failed(1, "no such host: missing"), "not_found"
        ),
        contract!(
            "tailscale_netcheck",
            ok: {} on ["netcheck"] => printed!("netcheck.json"),
            err: {} on ["netcheck"] =>
                Reply::failed(1, "netcheck: the probe could not be run"), "cli_failed"
        ),
        contract!(
            "tailscale_ping",
            ok: {"target": "laptop"} on ["ping"] => printed!("ping.txt"),
            err: {"target": "missing"} on ["ping"] =>
                Reply::failed(1, "ping \"missing\": unknown peer"), "not_found"
        ),
        contract!(
            "tailscale_whois",
            ok: {"address": "100.64.0.2"} on ["whois"] => printed!("whois.json"),
            err: {"address": "203.0.113.9"} on ["whois"] =>
                Reply::failed(1, "whois: 203.0.113.9 is outside the tailnet"), "cli_failed"
        ),
        contract!(
            "tailscale_whoami",
            ok: {} on ["whoami"] => printed!("whoami.json"),
            // A command added after our floor: an old binary refuses it by not
            // knowing it, and the caller is told which release added it.
            err: {} on ["whoami"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"whoami\""), "unsupported_version"
        ),
        contract!(
            "tailscale_version",
            ok: {} on ["version"] => printed!("tailscale-version.txt"),
            err: {} on ["version"] =>
                Reply::failed(1, "failed to connect to local tailscaled"), "cli_failed"
        ),
        contract!(
            "tailscale_licenses",
            ok: {} on ["licenses"] => printed!("licenses.txt"),
            err: {} on ["licenses"] =>
                Reply::failed(1, "licenses: this build carries no licence index"), "cli_failed"
        ),
        contract!(
            "tailscale_bugreport",
            ok: {"note": "slow handshake"} on ["bugreport"] => printed!("bugreport.txt"),
            err: {} on ["bugreport"] =>
                Reply::failed(1, "bugreport: the log service could not be reached"), "cli_failed"
        ),
        contract!(
            "tailscale_appc_routes",
            ok: {"all": true} on ["appc-routes"] => printed!("appc-routes.txt"),
            err: {} on ["appc-routes"] =>
                Reply::failed(1, "appc-routes: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_routecheck",
            ok: {} on ["routecheck"] => printed!("routecheck.json"),
            err: {} on ["routecheck"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"routecheck\""),
                "unsupported_version"
        ),
        contract!(
            "tailscale_wait",
            ok: {"timeout_seconds": 1} on ["wait"] => Reply::ok(""),
            err: {"timeout_seconds": 1} on ["wait"] => Reply::Unavailable, "backend_unavailable"
        ),
        contract!(
            "tailscale_dns_status",
            ok: {} on ["dns", "status"] => printed!("dns-status.json"),
            err: {} on ["dns", "status"] =>
                Reply::failed(1, "dns status: the resolver configuration could not be read"),
                "cli_failed"
        ),
        contract!(
            "tailscale_dns_query",
            ok: {"name": "laptop.example-tailnet.ts.net"} on ["dns", "query"] =>
                printed!("dns-query.json"),
            err: {"name": "absent.example-tailnet.ts.net"} on ["dns", "query"] =>
                Reply::failed(1, "dns query: the resolver answered NXDOMAIN"), "cli_failed"
        ),
        contract!(
            "tailscale_exit_node_list",
            ok: {} on ["exit-node", "list"] => printed!("exit-node-list.txt"),
            // Having no exit nodes is an answer, so the failure this tool can
            // still have is being refused outright.
            err: {} on ["exit-node", "list"] =>
                Reply::failed(1, "Access denied: this operation requires the operator"),
                "needs_operator"
        ),
        contract!(
            "tailscale_exit_node_suggest",
            ok: {} on ["exit-node", "suggest"] => printed!("exit-node-suggest.txt"),
            err: {} on ["exit-node", "suggest"] => Reply::Unavailable, "backend_unavailable"
        ),
        contract!(
            "tailscale_metrics_print",
            ok: {} on ["metrics", "print"] => printed!("metrics.txt"),
            err: {} on ["metrics", "print"] =>
                Reply::failed(1, "metrics: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_service_list",
            ok: {} on ["service", "list"] => printed!("service-list.json"),
            err: {} on ["service", "list"] =>
                Reply::failed(1, "tailscale service: unknown subcommand \"list\""),
                "unsupported_version"
        ),
        contract!(
            "tailscale_syspolicy_list",
            ok: {} on ["syspolicy", "list"] => printed!("syspolicy-list.json"),
            err: {} on ["syspolicy", "list"] =>
                Reply::failed(1, "syspolicy: the policy store could not be read"), "cli_failed"
        ),
        contract!(
            "tailscale_lock_status",
            ok: {} on ["lock", "status"] => printed!("lock-status.json"),
            err: {} on ["lock", "status"] =>
                Reply::failed(1, "lock: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_lock_log",
            ok: {} on ["lock", "log"] => printed!("lock-log.json"),
            err: {} on ["lock", "log"] =>
                Reply::failed(1, "lock log: the key authority is unreachable"), "cli_failed"
        ),
        contract!(
            "tailscale_serve_status",
            ok: {} on ["serve", "status"] => printed!("serve-status.json"),
            err: {} on ["serve", "status"] =>
                Reply::failed(1, "serve: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_funnel_status",
            ok: {} on ["funnel", "status"] => printed!("funnel-status.json"),
            err: {} on ["funnel", "status"] =>
                Reply::failed(1, "funnel: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_configure_sysext_status",
            ok: {} on ["configure", "sysext", "status"] => printed!("sysext-status.txt"),
            err: {} on ["configure", "sysext", "status"] =>
                Reply::failed(1, "configure sysext: the extension could not be queried"),
                "cli_failed"
        ),
        contract!(
            "tailscale_switch_list",
            ok: {} on ["switch"] => printed!("switch-list.json"),
            err: {} on ["switch"] =>
                Reply::failed(1, "switch: the profile store could not be read"), "cli_failed"
        ),
        contract!(
            "tailscale_prefs_get",
            ok: {} on ["get"] => printed!("prefs.json"),
            err: {} on ["get"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"get\""), "unsupported_version"
        ),
        contract!(
            "tailscale_prefs_set",
            ok: {"hostname": "workstation"} on ["set"] => Reply::ok(""),
            err: {"shields_up": true} on ["set"] =>
                Reply::failed(1, "set: shields-up is managed by policy"), "cli_failed"
        ),
        contract!(
            "tailscale_up",
            ok: {} on ["up"] => printed!("up-running.json"),
            err: {} on ["up"] => Reply::Unavailable, "backend_unavailable"
        ),
        contract!(
            "tailscale_down",
            ok: {} on ["down"] => Reply::ok(""),
            err: {} on ["down"] =>
                Reply::failed(1, "down: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_login",
            ok: {} on ["login"] => printed!("login.txt"),
            // Logging in changes the node's identity, which an unprivileged
            // caller is not allowed to do.
            err: {} on ["login"] =>
                Reply::failed(1, "Access denied: this operation requires the operator"),
                "needs_operator"
        ),
        contract!(
            "tailscale_logout",
            ok: {} on ["logout"] => Reply::ok(""),
            err: {} on ["logout"] =>
                Reply::failed(1, "logout: the profile could not be cleared"), "cli_failed"
        ),
        contract!(
            "tailscale_switch_profile",
            ok: {"account": "example-tailnet.ts.net"} on ["switch"] => Reply::ok(""),
            err: {"account": "nobody"} on ["switch"] =>
                Reply::failed(1, "switch: profile \"nobody\" not found"), "not_found"
        ),
        contract!(
            "tailscale_switch_remove",
            ok: {"account": "example-tailnet.ts.net"} on ["switch", "remove"] => Reply::ok(""),
            err: {"account": "nobody"} on ["switch", "remove"] =>
                Reply::failed(1, "switch remove: the profile store is read-only"), "cli_failed"
        ),
        contract!(
            "tailscale_serve_set",
            ok: {"target": "3000"} on ["serve"] => printed!("serve-set.txt"),
            err: {"target": "3000", "http": 80, "https": 443} on ["serve"] =>
                Reply::ok(""), "invalid_args"
        ),
        contract!(
            "tailscale_serve_off",
            ok: {"https": 8443} on ["serve"] => Reply::ok(""),
            err: {"https": 8443} on ["serve"] =>
                Reply::failed(1, "error: failed to remove web serve: handler does not exist"),
                "not_found"
        ),
        contract!(
            "tailscale_serve_reset",
            ok: {} on ["serve", "reset"] => Reply::ok(""),
            err: {} on ["serve", "reset"] =>
                Reply::failed(1, "reset: the daemon refused the request"), "cli_failed"
        ),
        contract!(
            "tailscale_serve_drain",
            ok: {"service": "svc:web"} on ["serve", "drain"] => Reply::ok(""),
            err: {"service": "svc:web"} on ["serve", "drain"] =>
                Reply::failed(1, "drain: no such service"), "not_found"
        ),
        contract!(
            "tailscale_serve_clear",
            ok: {"service": "svc:web"} on ["serve", "clear"] => Reply::ok(""),
            err: {"service": "svc:web"} on ["serve", "clear"] =>
                Reply::failed(1, "clear: the configuration could not be written"), "cli_failed"
        ),
        contract!(
            "tailscale_serve_advertise",
            ok: {"service": "svc:web"} on ["serve", "advertise"] => Reply::ok(""),
            err: {"service": "svc:web"} on ["serve", "advertise"] =>
                Reply::failed(1, "advertise: no such service"), "not_found"
        ),
        contract!(
            "tailscale_serve_get_config",
            ok: {"all": true} on ["serve", "get-config"] => printed!("serve-config.json"),
            err: {} on ["serve", "get-config"] => Reply::ok(""), "invalid_args"
        ),
        contract!(
            "tailscale_serve_set_config",
            ok: {"all": true, "configuration": {"version": "0.0.1"}} on ["serve", "set-config"] =>
                Reply::ok(""),
            err: {"all": true, "service": "svc:web", "configuration": {}}
                on ["serve", "set-config"] => Reply::ok(""), "invalid_args"
        ),
        contract!(
            "tailscale_funnel_set",
            ok: {"target": "3000"} on ["funnel"] => printed!("serve-set.txt"),
            err: {"target": "3000"} on ["funnel"] =>
                Reply::hung_after("Funnel is not enabled on your tailnet."), "timeout"
        ),
        contract!(
            "tailscale_funnel_off",
            ok: {"https": 8443} on ["funnel"] => Reply::ok(""),
            err: {"https": 8443} on ["funnel"] =>
                Reply::failed(1, "error: failed to remove funnel: handler does not exist"),
                "not_found"
        ),
        contract!(
            "tailscale_file_cp",
            ok: {"files": ["/tmp/notes.txt"], "target": "laptop"} on ["file", "cp"] =>
                Reply::ok("notes.txt: 4.1 kB\n"),
            err: {"files": ["/tmp/notes.txt"], "target": "missing"} on ["file", "cp"] =>
                Reply::failed(1, "error looking up IP of \"missing\": lookup missing: no such host"),
                "not_found"
        ),
        contract!(
            "tailscale_file_targets",
            ok: {} on ["file", "cp"] => printed!("file-targets.txt"),
            err: {} on ["file", "cp"] =>
                Reply::failed(1, "file cp: not logged in"), "cli_failed"
        ),
        contract!(
            "tailscale_file_get",
            ok: {"directory": "/tmp/inbox"} on ["file", "get"] => Reply::ok("notes.txt\n"),
            err: {"directory": "/tmp/inbox"} on ["file", "get"] =>
                Reply::failed(1, "\"/tmp/inbox\" is not a directory"), "cli_failed"
        ),
        contract!(
            "tailscale_cert",
            ok: {
                "domain": "workstation.example-tailnet.ts.net",
                "cert_file": "/tmp/node.crt",
                "key_file": "/tmp/node.key"
            } on ["cert"] => Reply::ok(""),
            err: {
                "domain": "workstation.example-tailnet.ts.net",
                "cert_file": "/tmp/node.crt",
                "key_file": "/tmp/node.key"
            } on ["cert"] =>
                Reply::failed(1, "500 Internal Server Error: invalid domain"), "cli_failed"
        ),
        contract!(
            "tailscale_metrics_write",
            ok: {"path": "/tmp/tailscaled.prom"} on ["metrics", "write"] => Reply::ok(""),
            err: {"path": "/tmp/tailscaled.prom"} on ["metrics", "write"] =>
                Reply::failed(1, "error writing metrics: read-only file system"), "cli_failed"
        ),
        contract!(
            "tailscale_configure_kubeconfig",
            ok: {"hostname": "cluster"} on ["configure", "kubeconfig"] => Reply::ok(""),
            err: {"hostname": "cluster"} on ["configure", "kubeconfig"] =>
                Reply::failed(1, "no such host: cluster"), "not_found"
        ),
        contract!(
            "tailscale_syspolicy_reload",
            ok: {} on ["syspolicy", "reload"] => printed!("syspolicy-reload.json"),
            err: {} on ["syspolicy", "reload"] =>
                Reply::failed(1, "syspolicy: the policy store could not be reloaded"),
                "cli_failed"
        ),
        contract!(
            "tailscale_drive_list",
            ok: {} on ["drive", "list"] => printed!("drive-list.txt"),
            // The macOS GUI packaging carries the subcommand and refuses it,
            // which is a fact about the build rather than about the call.
            err: {} on ["drive", "list"] => Reply::failed(
                1,
                "Taildrive CLI commands are not supported when using the macOS GUI app."
            ), "unsupported_platform"
        ),
        contract!(
            "tailscale_drive_share",
            ok: {"name": "docs", "path": "/srv/docs"} on ["drive", "share"] => Reply::ok(""),
            err: {"name": "docs", "path": "/srv/docs"} on ["drive", "share"] =>
                Reply::failed(1, "drive share: \"/srv/docs\" is not a directory"), "cli_failed"
        ),
        contract!(
            "tailscale_drive_rename",
            ok: {"name": "docs", "new_name": "handbook"} on ["drive", "rename"] => Reply::ok(""),
            err: {"name": "docs", "new_name": "handbook"} on ["drive", "rename"] =>
                Reply::failed(1, "share \"docs\" does not exist"), "not_found"
        ),
        contract!(
            "tailscale_drive_unshare",
            ok: {"name": "docs"} on ["drive", "unshare"] => Reply::ok(""),
            err: {"name": "docs"} on ["drive", "unshare"] =>
                Reply::failed(1, "share \"docs\" does not exist"), "not_found"
        ),
        // Tailnet lock. Every key here is a documentation value: a real
        // `tlpub:` key names a real signing node, and none belongs in a repo.
        contract!(
            "tailscale_lock_init",
            ok: {"trusted_keys": [TLPUB]} on ["lock", "init"] =>
                Reply::ok("disablement-secret:00112233445566778899aabbccddeeff\n"),
            err: {"trusted_keys": [TLPUB]} on ["lock", "init"] => Reply::failed(
                1,
                "the tailnet lock key of the current node must be one of the trusted keys during initialization"
            ), "cli_failed"
        ),
        contract!(
            "tailscale_lock_add",
            ok: {"keys": [TLPUB]} on ["lock", "add"] => Reply::ok(""),
            err: {"keys": [TLPUB]} on ["lock", "add"] =>
                Reply::failed(1, "tailnet lock is not enabled"), "cli_failed"
        ),
        contract!(
            "tailscale_lock_remove",
            ok: {"keys": [TLPUB]} on ["lock", "remove"] => Reply::ok(""),
            err: {"keys": [TLPUB]} on ["lock", "remove"] =>
                Reply::failed(1, "tailnet lock is not enabled"), "cli_failed"
        ),
        contract!(
            "tailscale_lock_sign",
            ok: {"key": NODEKEY} on ["lock", "sign"] => Reply::ok(""),
            err: {"key": NODEKEY} on ["lock", "sign"] => Reply::failed(
                1,
                "error: 500 Internal Server Error: signing failed: tailnet-lock is not active"
            ), "cli_failed"
        ),
        contract!(
            "tailscale_lock_disable",
            ok: {"secret": DISABLEMENT_SECRET} on ["lock", "disable"] => Reply::ok(""),
            err: {"secret": DISABLEMENT_SECRET} on ["lock", "disable"] => Reply::failed(
                1,
                "error: 400 Bad Request: tailnet-lock disable failed: tailnet-lock is not active"
            ), "cli_failed"
        ),
        contract!(
            "tailscale_lock_disablement_kdf",
            ok: {"secret": DISABLEMENT_HEX} on ["lock", "disablement-kdf"] =>
                Reply::ok("disablement:756fe19f200fbfc9ad431e75c7942b82\n"),
            err: {"secret": DISABLEMENT_HEX} on ["lock", "disablement-kdf"] =>
                Reply::failed(1, "encoding/hex: invalid byte: U+007A 'z'"), "cli_failed"
        ),
        contract!(
            "tailscale_lock_local_disable",
            ok: {} on ["lock", "local-disable"] => Reply::ok(""),
            err: {} on ["lock", "local-disable"] => Reply::failed(
                1,
                "error: 400 Bad Request: tailnet-lock local disable failed: tailnet-lock is not active"
            ), "cli_failed"
        ),
        contract!(
            "tailscale_lock_revoke_keys",
            ok: {"keys": [TLPUB]} on ["lock", "revoke-keys"] =>
                Reply::ok("run this on the next signing node\n"),
            err: {"keys": [TLPUB]} on ["lock", "revoke-keys"] => Reply::failed(
                1,
                "generation of recovery AUM failed: sending generate-recovery-aum: 500 Internal Server Error: tailnet-lock is not active"
            ), "cli_failed"
        ),
    ]
}

/// A tailnet-lock key that names nobody.
const TLPUB: &str = "tlpub:0000000000000000000000000000000000000000000000000000000000000000";
/// A node key that names nobody.
const NODEKEY: &str = "nodekey:0000000000000000000000000000000000000000000000000000000000000000";
/// A disablement secret in the two shapes the two commands want it in.
const DISABLEMENT_SECRET: &str = "disablement-secret:00112233445566778899aabbccddeeff";
const DISABLEMENT_HEX: &str = "00112233445566778899aabbccddeeff";

/// Build a session in which `meta`'s tool is on offer, arranged as the case says.
async fn session(meta: &ToolMeta, arrangement: &Arrangement) -> harness::Harness {
    let mut setup = Setup::new().toolsets(meta.toolset.as_str()).tier(meta.tier);
    for (argv, reply) in &arrangement.cli {
        setup = setup.cli_answers(argv, reply.clone());
    }
    for (method, path, response) in &arrangement.api {
        setup = setup.api_answers(method, path, response.clone()).await;
    }
    setup.start().await
}

/// The arguments a call is made with, plus the confirmation the tool requires.
fn arguments(meta: &ToolMeta, args: &Value) -> Value {
    let mut args = args.clone();
    if meta.requires_confirmation
        && let Some(object) = args.as_object_mut()
    {
        object.insert("confirm".to_owned(), json!(true));
    }
    args
}

fn table() -> Vec<ToolMeta> {
    tailscale_mcp::tools::entries()
        .into_iter()
        .map(|e| e.meta)
        .collect()
}

fn contract_for(name: &str) -> Contract {
    contracts()
        .into_iter()
        .find(|c| c.tool == name)
        .unwrap_or_else(|| panic!("no contract for `{name}`"))
}

#[test]
fn every_tool_has_a_contract() {
    let declared: BTreeSet<&str> = table().iter().map(|m| m.name).collect();
    let covered: BTreeSet<&str> = contracts().iter().map(|c| c.tool).collect();

    let uncovered: Vec<&&str> = declared.difference(&covered).collect();
    assert!(
        uncovered.is_empty(),
        "these tools have no contract row, so nothing checks what they do: {uncovered:?}"
    );

    let invented: Vec<&&str> = covered.difference(&declared).collect();
    assert!(
        invented.is_empty(),
        "these contract rows name tools that do not exist: {invented:?}"
    );
}

#[tokio::test]
async fn every_tool_is_named_for_the_surface_it_acts_on() {
    for meta in table() {
        assert!(
            meta.name.starts_with(meta.surface().prefix()),
            "`{}` belongs to the {} surface but is not named for it",
            meta.name,
            meta.surface().as_str()
        );
        assert_eq!(
            meta.toolset.surface(),
            meta.surface(),
            "`{}` is in a toolset from another surface",
            meta.name
        );
    }
}

#[tokio::test]
async fn every_tool_describes_itself_the_way_its_tier_says() {
    for meta in table() {
        let harness = session(&meta, &Arrangement::default()).await;
        let tool = harness
            .tool(meta.name)
            .await
            .unwrap_or_else(|| panic!("`{}` is not offered by its own toolset", meta.name));

        assert!(
            tool.description.as_deref().is_some_and(|d| !d.is_empty()),
            "`{}` has no description, so a model cannot choose it",
            meta.name
        );

        let annotations = tool
            .annotations
            .unwrap_or_else(|| panic!("`{}` is not annotated", meta.name));
        assert_eq!(
            annotations.read_only_hint,
            Some(meta.tier == Tier::Read),
            "`{}` is at the {} tier",
            meta.name,
            meta.tier
        );
        assert_eq!(
            annotations.destructive_hint,
            Some(meta.tier == Tier::Destructive),
            "`{}` is at the {} tier",
            meta.name,
            meta.tier
        );
        assert_eq!(
            annotations.idempotent_hint,
            Some(meta.idempotent),
            "`{}` declares idempotent: {}",
            meta.name,
            meta.idempotent
        );
        assert_eq!(annotations.open_world_hint, Some(true), "{}", meta.name);

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn every_tool_answers_its_success_case() {
    for meta in table() {
        let contract = contract_for(meta.name);
        let (args, arrangement) = contract.success;
        let harness = session(&meta, &arrangement).await;

        if !meta.runs_here() {
            // The table is the same on every platform, so a tool whose command
            // only exists elsewhere is still listed and still has a contract.
            // What it owes a caller here is the reason, not the answer.
            let error = harness.call_err(meta.name, arguments(&meta, &args)).await;
            assert_eq!(
                error["code"],
                "unsupported_platform",
                "`{}` does not exist on {} and should say so: {error:#}",
                meta.name,
                std::env::consts::OS
            );
            harness.shutdown().await;
            continue;
        }

        let answer = harness.call_ok(meta.name, arguments(&meta, &args)).await;
        assert!(
            answer.is_object(),
            "`{}` answered with something a client cannot destructure: {answer}",
            meta.name
        );

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn every_tool_answers_its_failure_case_with_the_code_it_promised() {
    for meta in table() {
        let contract = contract_for(meta.name);
        let (args, arrangement, code) = contract.failure;
        let code = if meta.runs_here() {
            code
        } else {
            "unsupported_platform"
        };
        let harness = session(&meta, &arrangement).await;

        let error = harness.call_err(meta.name, arguments(&meta, &args)).await;
        assert_eq!(
            error["code"], code,
            "`{}` failed with the wrong code: {error:#}",
            meta.name
        );
        assert!(
            error["message"].as_str().is_some_and(|m| !m.is_empty()),
            "`{}` failed without a message: {error:#}",
            meta.name
        );

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn every_tool_that_needs_confirming_refuses_without_it() {
    for meta in table().into_iter().filter(|m| m.requires_confirmation) {
        let contract = contract_for(meta.name);
        let (args, arrangement) = contract.success;
        let harness = session(&meta, &arrangement).await;

        // The same call that succeeds above, minus the confirmation.
        let error = harness.call_err(meta.name, args).await;
        assert_eq!(
            error["code"], "confirmation_required",
            "`{}` ran without being confirmed",
            meta.name
        );

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn each_preset_offers_more_than_the_one_below_it() {
    // The presets are meant to nest, so an operator moving up never loses a
    // tool they had. Checked against the real table, at every tier.
    for tier in [Tier::Read, Tier::Write, Tier::Destructive] {
        let mut previous: Option<(&str, BTreeSet<String>)> = None;
        for preset in Preset::ALL {
            let harness = Setup::new()
                .preset(preset.as_str())
                .tier(tier)
                .start()
                .await;
            let offered: BTreeSet<String> = harness.tool_names().await.into_iter().collect();

            if let Some((smaller, below)) = &previous {
                let lost: Vec<&String> = below.difference(&offered).collect();
                assert!(
                    lost.is_empty(),
                    "moving from {smaller} to {} at the {tier} tier loses {lost:?}",
                    preset.as_str()
                );
            }
            previous = Some((preset.as_str(), offered));
            harness.shutdown().await;
        }
    }
}

#[tokio::test]
async fn no_tool_is_reachable_from_a_session_that_did_not_ask_for_its_toolset() {
    // A session that selected some other toolset cannot see or call this tool,
    // even at the destructive tier. A zero-tool session cannot start, so the
    // session is built from every *other* toolset that has tools in it.
    let occupied: BTreeSet<&str> = table().iter().map(|m| m.toolset.as_str()).collect();

    for meta in table() {
        let elsewhere: Vec<&str> = occupied
            .iter()
            .copied()
            .filter(|t| *t != meta.toolset.as_str())
            .collect();
        if elsewhere.is_empty() {
            // Nothing else has tools yet, so there is no session to build.
            continue;
        }

        let harness = Setup::new()
            .toolsets(&elsewhere.join(","))
            .tier(Tier::Destructive)
            .start()
            .await;

        assert!(
            harness.tool(meta.name).await.is_none(),
            "`{}` is offered by a session that did not select {}",
            meta.name,
            meta.toolset.as_str()
        );
        let error = harness.call_err(meta.name, json!({})).await;
        assert_eq!(error["code"], "not_permitted", "{}", meta.name);

        harness.shutdown().await;
    }
}
