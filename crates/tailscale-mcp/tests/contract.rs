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
        // The debug toolset. Every payload here is a documentation value: the
        // real commands print this node's keys, addresses and peers, and none
        // of that belongs in a repo.
        contract!(
            "tailscale_debug_derp_map",
            ok: {} on ["debug", "derp-map"] => Reply::ok(r#"{"Regions":{}}"#),
            err: {} on ["debug", "derp-map"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_netmap",
            ok: {} on ["debug", "netmap"] => Reply::ok(r#"{"Peers":[]}"#),
            err: {} on ["debug", "netmap"] =>
                Reply::failed(1, "netmap is not available: not logged in"), "cli_failed"
        ),
        contract!(
            "tailscale_debug_hostinfo",
            ok: {} on ["debug", "hostinfo"] => Reply::ok(r#"{"OS":"macOS","Hostname":"example"}"#),
            err: {} on ["debug", "hostinfo"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_control_knobs",
            ok: {} on ["debug", "control-knobs"] => Reply::ok(r#"{"DisableUPnP":false}"#),
            err: {} on ["debug", "control-knobs"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"control-knobs\""), "unsupported_version"
        ),
        contract!(
            "tailscale_debug_daemon_goroutines",
            ok: {} on ["debug", "daemon-goroutines"] =>
                Reply::ok("goroutine 1 [running]:\nmain.main()\n"),
            err: {} on ["debug", "daemon-goroutines"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_daemon_bus_graph",
            ok: {} on ["debug", "daemon-bus-graph", "--format=json"] =>
                Reply::ok(r#"{"nodes":[],"edges":[]}"#),
            err: {"format": "dot"} on ["debug", "daemon-bus-graph", "--format=dot"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_daemon_bus_queues",
            ok: {} on ["debug", "daemon-bus-queues"] => Reply::ok(r#"{"queues":[]}"#),
            err: {} on ["debug", "daemon-bus-queues"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_metrics",
            ok: {} on ["debug", "metrics"] =>
                Reply::ok("# TYPE tailscaled_inbound_packets_total counter\ntailscaled_inbound_packets_total 0\n"),
            err: {} on ["debug", "metrics"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_statedir",
            ok: {} on ["debug", "statedir"] => Reply::ok("/var/lib/tailscale\n"),
            err: {} on ["debug", "statedir"] =>
                Reply::failed(1, "no state directory is configured"), "cli_failed"
        ),
        contract!(
            "tailscale_debug_go_buildinfo",
            ok: {} on ["debug", "go-buildinfo"] => Reply::ok(r#"{"GoVersion":"go1.24.0"}"#),
            err: {} on ["debug", "go-buildinfo"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"go-buildinfo\""), "unsupported_version"
        ),
        contract!(
            "tailscale_debug_peer_relay_servers",
            ok: {} on ["debug", "peer-relay-servers"] => Reply::ok("[]\n"),
            err: {} on ["debug", "peer-relay-servers"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"peer-relay-servers\""), "unsupported_version"
        ),
        contract!(
            "tailscale_debug_peer_relay_sessions",
            ok: {} on ["debug", "peer-relay-sessions"] =>
                Reply::ok("Server port: not configured\nSessions count: 0\n"),
            err: {} on ["debug", "peer-relay-sessions"] =>
                Reply::failed(1, "tailscale: unknown subcommand \"peer-relay-sessions\""), "unsupported_version"
        ),
        contract!(
            "tailscale_debug_file_list",
            ok: {} on ["debug", "--file=get"] => Reply::ok("null\n"),
            err: {} on ["debug", "--file=get"] =>
                Reply::failed(1, "Taildrop is not enabled on this node"), "cli_failed"
        ),
        contract!(
            "tailscale_debug_stat",
            ok: {"paths": ["/etc/hosts"]} on ["debug", "stat", "/etc/hosts"] =>
                Reply::ok("/etc/hosts: -rw-r--r--, 213\n"),
            err: {"paths": ["/etc/nope"]} on ["debug", "stat", "/etc/nope"] =>
                Reply::failed(1, "stat /etc/nope: no such file or directory"), "not_found"
        ),
        contract!(
            "tailscale_debug_via",
            ok: {"site_id": 7, "prefix": "10.1.0.0/16"} on ["debug", "via", "7", "10.1.0.0/16"] =>
                Reply::ok("fd7a:115c:a1e0:b1a:0:7:a01:0/112\n"),
            err: {"site_id": 7, "prefix": "10.1.0.0/16", "route": "fd7a::/112"}
                on ["debug", "via"] => Reply::ok(""), "invalid_args"
        ),
        contract!(
            "tailscale_debug_watch_ipn",
            ok: {"count": 1} on ["debug", "watch-ipn", "--count=1"] =>
                Reply::ok("{\"Version\":\"1.102.2\"}\n"),
            err: {"count": 0} on ["debug", "watch-ipn"] => Reply::ok(""), "invalid_args"
        ),
        contract!(
            "tailscale_debug_peer_endpoint_changes",
            ok: {"peer": "laptop"} on ["debug", "peer-endpoint-changes", "laptop"] =>
                Reply::ok(r#"{"changes":[]}"#),
            err: {"peer": "missing"} on ["debug", "peer-endpoint-changes", "missing"] =>
                Reply::failed(1, "error looking up IP of \"missing\": no such host"), "not_found"
        ),
        contract!(
            "tailscale_debug_resolve",
            ok: {"host": "example.com"} on ["debug", "resolve", "example.com"] =>
                Reply::ok("203.0.113.10\n"),
            err: {"host": "missing.invalid"} on ["debug", "resolve", "missing.invalid"] =>
                Reply::failed(1, "lookup missing.invalid: no such host"), "not_found"
        ),
        contract!(
            "tailscale_debug_dial_types",
            ok: {"host": "example.com", "port": 443} on ["debug", "dial-types", "example.com", "443"] =>
                Reply::ok("tcp dial to example.com:443 succeeded\n"),
            err: {"host": "example.com", "port": 443} on ["debug", "dial-types"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_derp",
            ok: {} on ["debug", "derp"] => Reply::ok("derp region 1: ok\n"),
            err: {} on ["debug", "derp"] =>
                Reply::failed(1, "no DERP map is available"), "cli_failed"
        ),
        contract!(
            "tailscale_debug_ts2021",
            ok: {} on ["debug", "ts2021"] => Reply::ok("did noise handshake\n"),
            err: {} on ["debug", "ts2021"] =>
                Reply::failed(1, "fetching keys: dial tcp: connection refused"), "cli_failed"
        ),
        contract!(
            "tailscale_debug_portmap",
            ok: {} on ["debug", "portmap", "--duration=5s"] =>
                Reply::ok("portmapper: no port mapping services were found\n"),
            err: {"gateway_addr": "192.0.2.1"} on ["debug", "portmap"] =>
                Reply::ok(""), "invalid_args"
        ),
        // The knobs. Each needs the write tier as well as the toolset.
        contract!(
            "tailscale_debug_component_logs",
            ok: {"component": "magicsock"}
                on ["debug", "component-logs", "--for=3600s", "magicsock"] => Reply::ok(""),
            err: {"component": "nonsense"} on ["debug", "component-logs"] =>
                Reply::failed(1, "unknown component \"nonsense\""), "cli_failed"
        ),
        contract!(
            "tailscale_debug_restun",
            ok: {} on ["debug", "restun"] => Reply::ok(""),
            err: {} on ["debug", "restun"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_rebind",
            ok: {} on ["debug", "rebind"] => Reply::ok(""),
            err: {} on ["debug", "rebind"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_rotate_disco_key",
            ok: {} on ["debug", "rotate-disco-key"] => Reply::ok(""),
            err: {} on ["debug", "rotate-disco-key"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_derp_unset_on_demand",
            ok: {} on ["debug", "derp-unset-on-demand"] => Reply::ok(""),
            err: {} on ["debug", "derp-unset-on-demand"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_pick_new_derp",
            ok: {} on ["debug", "pick-new-derp"] => Reply::ok("now using derp region 2\n"),
            err: {} on ["debug", "pick-new-derp"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            "tailscale_debug_force_prefer_derp",
            ok: {"region_id": 0} on ["debug", "force-prefer-derp", "0"] => Reply::ok(""),
            err: {"region_id": 99} on ["debug", "force-prefer-derp", "99"] =>
                Reply::failed(1, "region 99 is not in the DERP map"), "cli_failed"
        ),
        contract!(
            "tailscale_debug_force_netmap_update",
            ok: {} on ["debug", "force-netmap-update"] => Reply::ok(""),
            err: {} on ["debug", "force-netmap-update"] =>
                Reply::failed(1, "Access denied: cannot debug without operator access"), "needs_operator"
        ),
        contract!(
            // Its session runs at the tier of its row, which is the read floor,
            // so the failing call is a command the floor does not reach. That
            // is the passthrough's own contract rather than an accident of the
            // harness: what it may run is decided by the command.
            "tailscale_run",
            ok: {"args": ["version"]} on ["version"] => printed!("tailscale-version.txt"),
            err: {"args": ["down"]} on ["down"] => Reply::ok(""), "not_permitted"
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
        // A tool whose row is a floor rather than its whole tier is annotated
        // at its worst case: a client reading `read_only` cannot learn from
        // the annotation that this one depends on the arguments.
        let (read_only, destructive) = if meta.varying_tier {
            (false, true)
        } else {
            (meta.tier == Tier::Read, meta.tier == Tier::Destructive)
        };
        assert_eq!(
            annotations.read_only_hint,
            Some(read_only),
            "`{}` is at the {} tier",
            meta.name,
            meta.tier
        );
        assert_eq!(
            annotations.destructive_hint,
            Some(destructive),
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

/// The debug toolset is opt-in, and "every toolset" does not include it.
///
/// A caller reaching it has said `local-debug` out loud, which is the statement
/// that it accepts output the client itself calls unstable. A preset is the
/// opposite kind of choice — a name for a sensible default — so no preset, not
/// even the largest, may carry one of these.
#[tokio::test]
async fn no_preset_offers_a_debug_tool_at_any_tier() {
    let debug: BTreeSet<&str> = table()
        .iter()
        .filter(|m| m.toolset == tailscale_mcp::meta::Toolset::LocalDebug)
        .map(|m| m.name)
        .collect();
    assert_eq!(debug.len(), 30, "the debug toolset is thirty tools");

    for preset in [Preset::Minimal, Preset::Core, Preset::Full] {
        for tier in [Tier::Read, Tier::Write, Tier::Destructive] {
            let harness = Setup::new()
                .preset(preset.as_str())
                .tier(tier)
                .start()
                .await;
            let offered: BTreeSet<String> = harness.tool_names().await.into_iter().collect();
            let leaked: Vec<&&str> = debug
                .iter()
                .filter(|name| offered.contains(**name))
                .collect();
            assert!(
                leaked.is_empty(),
                "the {} preset at the {tier} tier offers {leaked:?}",
                preset.as_str()
            );
            harness.shutdown().await;
        }
    }
}

/// The toolset and the tier are separate keys to the same door.
///
/// Adding `local-debug` at the read tier buys the twenty-two readers and none
/// of the eight knobs: a reader asks the daemon what it believes, and a knob
/// makes it do something over again.
#[tokio::test]
async fn the_knobs_need_the_write_tier_as_well_as_the_toolset() {
    let readers: BTreeSet<&str> = table()
        .iter()
        .filter(|m| m.toolset == tailscale_mcp::meta::Toolset::LocalDebug && m.tier == Tier::Read)
        .map(|m| m.name)
        .collect();
    let knobs: BTreeSet<&str> = table()
        .iter()
        .filter(|m| m.toolset == tailscale_mcp::meta::Toolset::LocalDebug && m.tier == Tier::Write)
        .map(|m| m.name)
        .collect();
    assert_eq!((readers.len(), knobs.len()), (22, 8));

    let harness = Setup::new()
        .toolsets("local-debug")
        .tier(Tier::Read)
        .start()
        .await;
    let offered: BTreeSet<String> = harness.tool_names().await.into_iter().collect();
    for reader in &readers {
        assert!(offered.contains(*reader), "`{reader}` is a read-tier tool");
    }
    for knob in &knobs {
        assert!(!offered.contains(*knob), "`{knob}` needs the write tier");
        let error = harness.call_err(knob, json!({})).await;
        assert_eq!(error["code"], "not_permitted", "{knob}");
    }
    harness.shutdown().await;

    let harness = Setup::new()
        .toolsets("local-debug")
        .tier(Tier::Write)
        .start()
        .await;
    let offered: BTreeSet<String> = harness.tool_names().await.into_iter().collect();
    for knob in &knobs {
        assert!(
            offered.contains(*knob),
            "`{knob}` is on offer at the write tier"
        );
    }
    harness.shutdown().await;
}

/// The arguments a tool needs to get as far as spawning, read off its own
/// schema.
///
/// Every required property gets the emptiest value its type allows. The point
/// is never to make a meaningful call — the fake refuses most of these — but to
/// get past argument validation so that the client is actually reached and the
/// argument list is recorded.
fn minimal_arguments(tool: &rmcp::model::Tool) -> Value {
    let schema = &tool.input_schema;
    let properties = schema.get("properties").and_then(Value::as_object);
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut args = serde_json::Map::new();
    for name in required.iter().filter_map(Value::as_str) {
        let property = properties.and_then(|p| p.get(name));
        args.insert(name.to_owned(), emptiest_value(property));
    }
    Value::Object(args)
}

/// The emptiest value a schema will accept, by type.
fn emptiest_value(property: Option<&Value>) -> Value {
    // A nullable property is written `["string", "null"]`, so the first named
    // type is the one to satisfy.
    let kind = property
        .and_then(|p| p.get("type"))
        .map(|t| match t {
            Value::Array(types) => types
                .iter()
                .filter_map(Value::as_str)
                .find(|t| *t != "null")
                .unwrap_or("string")
                .to_owned(),
            other => other.as_str().unwrap_or("string").to_owned(),
        })
        .unwrap_or_else(|| "string".to_owned());

    match kind.as_str() {
        "integer" | "number" => json!(1),
        "boolean" => json!(false),
        "array" => json!([emptiest_value(
            property
                .and_then(|p| p.get("items"))
                .filter(|i| i.is_object())
        )]),
        _ => json!("x"),
    }
}

/// Every debug tool is called, and none of them runs an excluded command.
///
/// This is what the ticket's third criterion actually asks of the registry.
/// Its sibling below proves that no tool is *named* for an excluded path,
/// which is a weaker claim than it looks: `debug prefs` is excluded precisely
/// because a command's name does not tell you what it does, so a check on
/// names alone would not catch a tool that quietly ran one. This one drives
/// every debug tool through a real session and reads back the argument lists
/// the fake `tailscale` was given.
///
/// The other half of the criterion — the passthrough refusing the same list —
/// belongs to the passthrough, and is ticket 14.
#[tokio::test]
async fn no_debug_tool_runs_an_excluded_subcommand() {
    let harness = Setup::new()
        .toolsets("local-debug")
        .tier(Tier::Destructive)
        .start()
        .await;

    // `debug via` takes either a route or a site and a prefix, and refuses
    // before spawning when given a lone one, so the generator cannot reach it.
    let route = json!({"route": "fd7a:115c:a1e0:b1a:0:7:a01:0/112"});

    let mut called = 0;
    for tool in harness.tools().await {
        let name = tool.name.to_string();
        assert!(
            name.starts_with("tailscale_debug_"),
            "only debug tools should be on offer here, and `{name}` is not one"
        );
        let arguments = if name == "tailscale_debug_via" {
            route.clone()
        } else {
            minimal_arguments(&tool)
        };

        let before = harness.cli_calls().len();
        // Whether the call succeeded is beside the point; what it ran is not.
        let _ = harness.call(&name, arguments.clone()).await;
        let calls = harness.cli_calls();
        assert!(
            calls.len() > before,
            "`{name}` answered without reaching the client, so nothing was \
             proved about what it runs; it was called with {arguments}"
        );

        for argv in &calls[before..] {
            for excluded in tailscale_mcp::tools::local_debug::EXCLUDED {
                // Word by word, so that a future `debug prefsomething` is not
                // mistaken for `debug prefs`.
                let words: Vec<String> = excluded.path.split(' ').map(str::to_owned).collect();
                assert!(
                    !argv.starts_with(&words),
                    "`{name}` ran `{}`, which is the excluded `{}`",
                    argv.join(" "),
                    excluded.path
                );
            }
        }
        called += 1;
    }

    assert_eq!(called, 30, "every debug tool has to be exercised, not most");
    harness.shutdown().await;
}

/// No excluded `debug` subcommand is a tool name.
///
/// A weaker claim than its sibling above, and kept because it is the one a
/// caller can check: the name they would guess is not there, and calling it
/// says so plainly.
#[tokio::test]
async fn no_excluded_debug_subcommand_is_reachable_as_a_tool() {
    let harness = Setup::new()
        .toolsets("local-debug")
        .tier(Tier::Destructive)
        .start()
        .await;
    let offered: BTreeSet<String> = harness.tool_names().await.into_iter().collect();

    for excluded in tailscale_mcp::tools::local_debug::EXCLUDED {
        // `debug foo-bar` would have been `tailscale_debug_foo_bar`.
        let name = format!("tailscale_{}", excluded.path.replace([' ', '-'], "_"));
        assert!(
            !offered.contains(&name),
            "`{}` is excluded but `{name}` is on offer",
            excluded.path
        );
        // `not_found`, not `not_permitted`: a gated tool exists and a switch
        // would reach it, while these do not exist at all. Telling a caller to
        // enable something that would never appear would be the wrong answer.
        let error = harness.call_err(&name, json!({})).await;
        assert_eq!(error["code"], "not_found", "{name}");
    }
    harness.shutdown().await;
}

// ---------------------------------------------------------------------------
// the passthrough
// ---------------------------------------------------------------------------

/// A session with the passthrough on offer and nothing else.
async fn passthrough(tier: Tier, answers: &[(&[&str], Reply)]) -> harness::Harness {
    let mut setup = Setup::new().toolsets("local-passthrough").tier(tier);
    for (argv, reply) in answers {
        setup = setup.cli_answers(argv, reply.clone());
    }
    setup.start().await
}

#[tokio::test]
async fn the_passthrough_is_reached_by_naming_it_and_no_other_way() {
    // `full` means every toolset a preset is willing to imply, and this is not
    // one of them: the switch that turns it on is naming it.
    let broad = Setup::new()
        .preset("full")
        .tier(Tier::Destructive)
        .start()
        .await;
    assert!(
        !broad
            .tool_names()
            .await
            .contains(&"tailscale_run".to_owned()),
        "`full` offered the passthrough"
    );
    broad.shutdown().await;

    let named = Setup::new()
        .preset("full")
        .toolsets("+local-passthrough")
        .tier(Tier::Destructive)
        .start()
        .await;
    assert!(
        named
            .tool_names()
            .await
            .contains(&"tailscale_run".to_owned()),
        "adding the toolset did not offer the passthrough"
    );
    named.shutdown().await;
}

/// The read tier runs a read command and refuses a destructive one.
#[tokio::test]
async fn at_the_read_tier_the_command_decides_what_may_run() {
    let harness = passthrough(
        Tier::Read,
        &[(&["status"], Reply::ok(harness::fixture("status.json")))],
    )
    .await;

    let answer = harness
        .call_ok("tailscale_run", json!({"args": ["status", "--json"]}))
        .await;
    assert_eq!(answer["tier"], "read");
    assert_eq!(answer["covered"], true);

    let error = harness
        .call_err("tailscale_run", json!({"args": ["down"]}))
        .await;
    assert_eq!(error["code"], "not_permitted", "{error:#}");
    assert!(
        error["hint"]
            .as_str()
            .is_some_and(|h| h.contains("--allow-destructive")),
        "the refusal should name the switch that would allow it: {error:#}"
    );
    assert!(
        harness
            .cli_calls()
            .iter()
            .all(|argv| argv.first().map(String::as_str) != Some("down")),
        "`down` was refused and should not have run"
    );

    harness.shutdown().await;
}

/// A subcommand no tool covers is destructive, and is refused below that tier.
#[tokio::test]
async fn an_unknown_subcommand_is_judged_at_the_top() {
    for tier in [Tier::Read, Tier::Write] {
        let harness = passthrough(tier, &[]).await;
        let error = harness
            .call_err("tailscale_run", json!({"args": ["nonesuch"]}))
            .await;
        assert_eq!(
            error["code"], "not_permitted",
            "at the {tier} tier: {error:#}"
        );
        assert!(harness.cli_calls().iter().all(|argv| argv != &["nonesuch"]));
        harness.shutdown().await;
    }

    let harness = passthrough(Tier::Destructive, &[(&["nonesuch"], Reply::ok("it ran\n"))]).await;
    let answer = harness
        .call_ok("tailscale_run", json!({"args": ["nonesuch"]}))
        .await;
    assert_eq!(answer["tier"], "destructive");
    assert_eq!(
        answer["covered"], false,
        "an unknown command must say the tier was a refusal to guess: {answer:#}"
    );
    harness.shutdown().await;
}

/// Every excluded command, enumerated, is refused with the permission code.
#[tokio::test]
async fn every_excluded_command_is_refused() {
    let harness = passthrough(Tier::Destructive, &[]).await;

    let mut checked = 0;
    for excluded in tailscale_mcp::tools::passthrough::excluded() {
        let words: Vec<&str> = excluded.path.split(' ').collect();
        let before = harness.cli_calls().len();
        let error = harness
            .call_err("tailscale_run", json!({"args": words}))
            .await;
        assert_eq!(
            error["code"], "not_permitted",
            "`{}` was not refused: {error:#}",
            excluded.path
        );
        assert!(
            error["message"]
                .as_str()
                .is_some_and(|m| m.contains(excluded.reason)),
            "`{}` was refused without its reason: {error:#}",
            excluded.path
        );
        assert!(
            error["hint"].is_null(),
            "`{}` is refused by no switch, so nothing should suggest one: {error:#}",
            excluded.path
        );
        assert_eq!(
            harness.cli_calls().len(),
            before,
            "`{}` reached the client",
            excluded.path
        );
        checked += 1;
    }

    // Nine documented commands and the fourteen hidden ones ticket 13 named.
    assert_eq!(checked, 23, "the whole exclusion list has to be walked");

    // The one `debug` member in neither list stays runnable, which is what
    // makes the list above a list rather than a blanket refusal.
    let runnable = harness
        .call_err("tailscale_run", json!({"args": ["debug", "reload-config"]}))
        .await;
    assert_ne!(
        runnable["code"], "not_permitted",
        "`debug reload-config` is deliberately runnable (DECISIONS Q44): {runnable:#}"
    );

    harness.shutdown().await;
}

/// A command cannot be spelled past the judgement that applies to it.
///
/// Both of these were `/code-review` findings on this ticket, and both are one
/// mistake: reading the argument list as a single spelling when the client
/// reads it as another. What holds them closed is `classify` taking two
/// readings and keeping the stricter; these are the end-to-end proof that the
/// gate, the tables and the handler agree with it.
#[tokio::test]
async fn a_command_cannot_be_disguised_past_its_own_judgement() {
    // The client matches its own subcommands without regard to case, so this
    // is `debug prefs`, excluded for printing the node's private key.
    let harness = passthrough(Tier::Destructive, &[]).await;
    let before = harness.cli_calls().len();
    let error = harness
        .call_err("tailscale_run", json!({"args": ["DEBUG", "PREFS"]}))
        .await;
    assert_eq!(error["code"], "not_permitted", "{error:#}");
    assert_eq!(
        harness.cli_calls().len(),
        before,
        "a shouted `debug prefs` reached the client"
    );
    harness.shutdown().await;

    // `tailscale serve --bg reset` runs `serve reset`, which is destructive and
    // wants a confirmation. Read as `serve`, it is a write needing neither.
    let harness = passthrough(Tier::Write, &[]).await;
    let before = harness.cli_calls().len();
    let error = harness
        .call_err("tailscale_run", json!({"args": ["serve", "--bg", "reset"]}))
        .await;
    assert_eq!(error["code"], "not_permitted", "{error:#}");
    assert_eq!(
        harness.cli_calls().len(),
        before,
        "a write-tier session reset the serve configuration"
    );
    harness.shutdown().await;

    // And at the tier that does allow it, the confirmation is still owed.
    let harness = passthrough(Tier::Destructive, &[]).await;
    let before = harness.cli_calls().len();
    let error = harness
        .call_err("tailscale_run", json!({"args": ["serve", "--bg", "reset"]}))
        .await;
    assert_eq!(error["code"], "confirmation_required", "{error:#}");
    assert_eq!(
        harness.cli_calls().len(),
        before,
        "the serve configuration was reset without a confirmation"
    );
    harness.shutdown().await;
}

/// An argument full of shell syntax is one argument.
#[tokio::test]
async fn nothing_a_caller_writes_is_parsed_by_a_shell() {
    let awkward = "; rm -rf / & $(whoami) `id` | tee /tmp/x #'\"";
    let harness = passthrough(Tier::Read, &[(&["ping"], Reply::ok("pong\n"))]).await;

    harness
        .call_ok("tailscale_run", json!({"args": ["ping", awkward]}))
        .await;

    let ran = harness
        .cli_calls()
        .into_iter()
        .find(|argv| argv.first().map(String::as_str) == Some("ping"))
        .expect("`ping` ran");
    assert_eq!(
        ran,
        vec!["ping".to_owned(), awkward.to_owned()],
        "the argument was split, quoted or expanded on its way to the client"
    );

    harness.shutdown().await;
}

/// Arguments the schema-derived generator cannot satisfy, written out.
///
/// Each of these validates its input before spawning — a key has a prefix, a
/// selector is required, `set` refuses to change nothing — so the emptiest
/// value its type allows never reaches the client.
fn hand_written_arguments(name: &str) -> Option<Value> {
    Some(match name {
        "tailscale_debug_via" => json!({"route": "fd7a:115c:a1e0:b1a:0:7:a01:0/112"}),
        "tailscale_lock_add" => json!({"keys": [TLPUB]}),
        "tailscale_lock_remove" => json!({"keys": [TLPUB]}),
        "tailscale_lock_init" => json!({"trusted_keys": [TLPUB], "confirm": true}),
        "tailscale_lock_disable" => json!({"secret": DISABLEMENT_SECRET, "confirm": true}),
        "tailscale_lock_disablement_kdf" => json!({"secret": DISABLEMENT_HEX}),
        "tailscale_lock_revoke_keys" => json!({"keys": [TLPUB], "confirm": true}),
        "tailscale_lock_sign" => json!({"key": NODEKEY}),
        "tailscale_prefs_set" => json!({"nickname": "workstation"}),
        "tailscale_serve_get_config" => json!({"all": true}),
        "tailscale_serve_set_config" => json!({"all": true, "configuration": {}}),
        _ => return None,
    })
}

/// The passthrough's table of covered commands is the typed tools' own terms.
///
/// [`COVERED`](tailscale_mcp::tools::passthrough::COVERED) is eighty-seven
/// hand-written rows claiming to state, for each command, the tier and
/// confirmation of the tool that runs it. Nothing about a hand-written table
/// stays true on its own, so this drives every typed tool through a real
/// session, reads back the command each one actually ran, and re-derives the
/// table from that. Both directions are checked: no row is weaker than a tool
/// that runs its command, and no row is stronger than every tool that does.
#[tokio::test]
async fn the_covered_table_follows_the_tools_it_claims_to_follow() {
    use std::collections::BTreeMap;
    use tailscale_mcp::tools::passthrough::{COVERED, Known, classify};

    let harness = Setup::new()
        .preset("full")
        .toolsets("+local-debug")
        .tier(Tier::Destructive)
        .start()
        .await;

    // What the tools say, path by path: the highest tier any of them runs the
    // command at, and whether any of them requires a confirmation.
    let mut derived: BTreeMap<String, (Tier, bool)> = BTreeMap::new();

    for meta in table() {
        if meta.name == "tailscale_run" {
            // The tool doing the judging; it has no fixed command to judge.
            continue;
        }
        let tool = harness
            .tool(meta.name)
            .await
            .unwrap_or_else(|| panic!("`{}` was not offered", meta.name));
        let mut arguments =
            hand_written_arguments(meta.name).unwrap_or_else(|| minimal_arguments(&tool));
        if meta.requires_confirmation
            && let Some(object) = arguments.as_object_mut()
        {
            object.insert("confirm".to_owned(), json!(true));
        }

        let before = harness.cli_calls().len();
        let _ = harness.call(meta.name, arguments.clone()).await;
        let calls = harness.cli_calls();
        let ran = &calls[before..];
        assert_eq!(
            ran.len(),
            1,
            "`{}` ran {} commands, and this test reads one; it was called with \
             {arguments}",
            meta.name,
            ran.len()
        );
        let argv = &ran[0];

        if meta.name == "tailscale_debug_file_list" {
            // The one tool whose command is not a subcommand: `debug --file=get`
            // is a flag on the parent, so there is no path to put in the table.
            // The passthrough refuses it rather than reading `debug` and
            // guessing, which is why this tool exists.
            assert_eq!(argv, &["debug", "--file=get"]);
            let error = classify(argv).expect_err("a bare `debug` cannot be judged");
            assert_eq!(error.code, tailscale_mcp::error::ErrorCode::InvalidArgs);
            continue;
        }

        let (path, known) = classify(argv).unwrap_or_else(|e| {
            panic!(
                "`{}` ran `{}`, which the passthrough refuses to read: {e:?}",
                meta.name,
                argv.join(" ")
            )
        });
        let Known::Covered(row) = known else {
            panic!(
                "`{}` runs `tailscale {path}`, which is in no row of COVERED",
                meta.name
            );
        };
        assert!(
            row.tier >= meta.tier,
            "`tailscale {path}` is {} in COVERED but `{}` runs it at {}",
            row.tier,
            meta.name,
            meta.tier
        );
        assert!(
            row.confirm >= meta.requires_confirmation,
            "`{}` confirms and `tailscale {path}` does not",
            meta.name
        );

        let entry = derived.entry(path).or_insert((Tier::Read, false));
        entry.0 = entry.0.max(meta.tier);
        entry.1 |= meta.requires_confirmation;
    }

    for row in COVERED {
        let (tier, confirm) = *derived.get(row.path).unwrap_or_else(|| {
            panic!(
                "no tool runs `tailscale {}`, so its row states nobody's terms",
                row.path
            )
        });
        assert_eq!(
            (row.tier, row.confirm),
            (tier, confirm),
            "`tailscale {}` is stricter in COVERED than every tool that runs it",
            row.path
        );
    }

    harness.shutdown().await;
}
