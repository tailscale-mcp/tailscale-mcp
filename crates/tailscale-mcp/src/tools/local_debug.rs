//! The debug toolset: what tailscaled will say about itself, and what it will
//! be told to do again.
//!
//! `tailscale debug` is hidden, and its own help calls it "not a stable
//! interface". That is the whole reason this toolset exists separately and is
//! in no preset: a caller reaches it by naming `local-debug`, which is a
//! statement that it accepts output that may change shape between releases.
//! [`crate::gating`] keeps it out of `full` for that reason, so "every toolset"
//! never silently means "this one too".
//!
//! The thirty tools are twenty-two readers and eight knobs. The readers ask the
//! daemon what it currently believes — its netmap, its DERP map, its goroutines,
//! its bus — and change nothing. The knobs make it do something over again: a
//! fresh STUN round, a new socket, a different home relay. A knob changes
//! transient runtime state and never a preference, so the two are separated by
//! tier as well as by name, and a session that added this toolset at the read
//! tier gets the readers only.
//!
//! Because a knob is not a configuration write, its invocation takes the shared
//! lane: the exclusive lane exists to keep two `set` calls apart, and forcing a
//! re-STUN is not that.
//!
//! ## What is not here
//!
//! [`EXCLUDED`] names the fourteen `debug` subcommands that never become tools,
//! with the reason for each, and is the list the passthrough refuses. They fall
//! into four groups, which are [`CONTEXT.md`]'s own grounds for excluding a
//! command: it prints a secret, it runs in the foreground indefinitely, it
//! reaches outside what this server is willing to drive, or its whole purpose
//! is to break the node.
//!
//! `debug prefs` is in that list, and it is the one exclusion the research
//! table did not predict. Its help says it prints preferences; what it actually
//! prints includes `PrivateNodeKey`, `OldPrivateNodeKey` and the tailnet-lock
//! private key, none of which the shape-based redaction knows. Every other
//! field it holds is already reported by a tool that does not carry the keys,
//! so excluding it costs a caller nothing (DECISIONS Q45). The same dump is
//! reachable through `debug watch-ipn --initial`, which is the one flag of that
//! command not offered here; its six `--initial-*` siblings each ask for one
//! narrow field and carry no key, so they are parameters like any other.
//!
//! The parent command's own flags are excluded on the same grounds:
//! `--cpu-profile` and `--mem-profile` write a binary profile to a path or to
//! standard output, and `--file=<name>` and `--file=delete:<name>` act on the
//! Taildrop inbox that `tailscale_file_get` already owns. Its remaining form,
//! `--file=get`, lists that inbox without downloading anything and is offered
//! here as `tailscale_debug_file_list`, because no other tool answers "what is
//! waiting for me" (DECISIONS Q46).
//!
//! `debug reload-config` is in neither list, which is itself the statement. It
//! is the ninth command that would otherwise be a knob, and it is the one that
//! is not one: a knob changes transient runtime state without changing any
//! preference, and reloading makes tailscaled re-read a configuration file that
//! this server did not write, cannot see and cannot describe. Its effect is
//! whatever that file now says, so no honest summary of it could be written and
//! no annotation could state its tier. It stays a legitimate operation for
//! whoever does own that file, so the passthrough may still run it
//! (DECISIONS Q44).
//!
//! ## Bounded forms
//!
//! Two of these commands would otherwise run until interrupted.
//! `tailscale_debug_watch_ipn` takes a required count, which is how the client
//! itself ends the stream, and a wall-clock bound in case the node is quiet
//! enough that the count is never reached. `tailscale_debug_portmap` bounds
//! itself through the `--duration` the client already has. A bound that expires
//! is reported as a timeout carrying what the command had printed, which is the
//! same answer a foreground `tailscale_funnel` gives.
//!
//! A third would, and is the one flag withheld from an otherwise whole command:
//! `debug metrics --watch` prints deltas until interrupted and has no count to
//! bound it, so `tailscale_debug_metrics` offers the one-shot dump only.
//!
//! [`CONTEXT.md`]: https://github.com/tailscale-mcp/tailscale-mcp/blob/main/CONTEXT.md

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tailscale_cli::Invocation;

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::meta::ToolMeta;
use crate::tools::common::{
    bounded_wait, document, note, object, printed, push_bool, push_text, real_path, report,
};

crate::tools! {
    // -----------------------------------------------------------------------
    // readers: what the daemon currently believes
    // -----------------------------------------------------------------------

    /// The DERP map the local node is using: every relay region the control
    /// plane has told it about, with the servers in each. Large.
    tailscale_debug_derp_map => NoParams, derp_map,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The current netmap: every peer the local node knows, with the keys,
    /// addresses and endpoints it holds for each. The largest document this
    /// server produces, and the one most likely to exceed the result cap.
    tailscale_debug_netmap => NoParams, netmap,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// What the local node reports about itself to the control plane: its OS,
    /// version, hardware and the features it has enabled.
    tailscale_debug_hostinfo => NoParams, hostinfo,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The control knobs the control plane has set on this node: the per-tailnet
    /// switches that change how the daemon behaves without a preference.
    tailscale_debug_control_knobs => NoParams, control_knobs,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// A stack dump of every goroutine in tailscaled, for diagnosing a daemon
    /// that is wedged. Tens of kilobytes of Go stack traces.
    tailscale_debug_daemon_goroutines => NoParams, daemon_goroutines,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The daemon's internal event bus as a graph: which components publish
    /// which events and which subscribe to them.
    tailscale_debug_daemon_bus_graph => DaemonBusGraphParams, daemon_bus_graph,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// How much is queued on each of the daemon's internal event bus queues,
    /// for finding a subscriber that has fallen behind.
    tailscale_debug_daemon_bus_queues => NoParams, daemon_bus_queues,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The daemon's own internal metrics, in Prometheus text format. Distinct
    /// from `tailscale_metrics_print`, which reports the node's client metrics.
    tailscale_debug_metrics => NoParams, metrics,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The directory tailscaled keeps its state in on the local filesystem.
    tailscale_debug_statedir => NoParams, statedir,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The Go build information of the `tailscale` binary: its module versions,
    /// build settings and the toolchain that produced it.
    tailscale_debug_go_buildinfo => NoParams, go_buildinfo,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The peers this node could relay through, for reaching a peer that no
    /// direct path and no DERP region can carry.
    tailscale_debug_peer_relay_servers => NoParams, peer_relay_servers,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The relay sessions currently running through this node on behalf of
    /// other peers, and whether it is configured to serve them at all.
    tailscale_debug_peer_relay_sessions => NoParams, peer_relay_sessions,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// List the files waiting in this node's Taildrop inbox without downloading
    /// any of them. Use `tailscale_file_get` to fetch what this reports.
    tailscale_debug_file_list => NoParams, file_list,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// The mode and size of files on the local filesystem, as tailscaled's own
    /// process sees them, for telling a missing file from an unreadable one.
    tailscale_debug_stat => StatParams, stat,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Convert between a site's IPv4 prefix and the IPv6 `via` route that
    /// carries it. Give `site_id` and `prefix` to go one way, or `route` to go
    /// back. Arithmetic on the values given: nothing is contacted.
    tailscale_debug_via => ViaParams, via,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Watch the local node's state notifications until `count` of them have
    /// arrived or the bound expires, whichever comes first. Reports the events
    /// as they were published. A quiet node reaches the bound instead, which is
    /// reported as a timeout carrying whatever had arrived.
    tailscale_debug_watch_ipn => WatchIpnParams, watch_ipn,
        toolset: LocalDebug, tier: Read;

    /// The history of endpoint changes the local node has recorded for one
    /// peer, for diagnosing a connection that keeps being rebuilt.
    tailscale_debug_peer_endpoint_changes => PeerParams, peer_endpoint_changes,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Resolve a hostname through the daemon's own resolver rather than the
    /// operating system's, to see what tailscaled would have got.
    tailscale_debug_resolve => ResolveParams, resolve,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Try every path type in turn to a host and port — direct, through DERP,
    /// through the operating system — and report which of them connected.
    tailscale_debug_dial_types => DialTypesParams, dial_types,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Test the local node's DERP configuration by exercising the relays it
    /// would use. A network probe: it makes connections but changes nothing.
    tailscale_debug_derp => NoParams, derp,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Test that the control plane is reachable over the Noise protocol the
    /// daemon uses, reporting each dial attempt and its result.
    tailscale_debug_ts2021 => Ts2021Params, ts2021,
        toolset: LocalDebug, tier: Read, idempotent: true;

    /// Probe the local network's gateway for port-mapping support — PMP, PCP
    /// and UPnP — and report what it offered. Runs for `seconds`.
    tailscale_debug_portmap => PortmapParams, portmap,
        toolset: LocalDebug, tier: Read, idempotent: true;

    // -----------------------------------------------------------------------
    // knobs: make the daemon do something over again
    // -----------------------------------------------------------------------

    /// Turn verbose logging on for one of the daemon's components for a while.
    /// A zero or negative duration turns it off again. The logs go to the
    /// daemon's own log, which this server does not read.
    tailscale_debug_component_logs => ComponentLogsParams, component_logs,
        toolset: LocalDebug, tier: Write;

    /// Force a fresh STUN round, so the node re-learns the addresses peers
    /// should reach it on. Useful after a network change the daemon missed.
    tailscale_debug_restun => NoParams, restun,
        toolset: LocalDebug, tier: Write, idempotent: true;

    /// Force the node to rebind its UDP sockets, so it starts using whatever
    /// the operating system's routing table now says.
    tailscale_debug_rebind => NoParams, rebind,
        toolset: LocalDebug, tier: Write, idempotent: true;

    /// Rotate this node's disco key, so that peers must re-establish their
    /// direct paths to it. Existing sessions are rebuilt, not dropped.
    tailscale_debug_rotate_disco_key => NoParams, rotate_disco_key,
        toolset: LocalDebug, tier: Write;

    /// Put DERP connections back to always-on after something set them to
    /// on-demand. The repair for a node that has become slow to be reached.
    tailscale_debug_derp_unset_on_demand => NoParams, derp_unset_on_demand,
        toolset: LocalDebug, tier: Write, idempotent: true;

    /// Move the node to a different home DERP region, chosen by the daemon,
    /// until the next restart. For testing whether a region is the problem.
    tailscale_debug_pick_new_derp => NoParams, pick_new_derp,
        toolset: LocalDebug, tier: Write;

    /// Pin the node's home DERP to one region until the next restart, or pass
    /// region 0 to un-pin it and let the daemon choose again.
    tailscale_debug_force_prefer_derp => ForcePreferDerpParams, force_prefer_derp,
        toolset: LocalDebug, tier: Write, idempotent: true;

    /// Push a full netmap update through the daemon without changing it, to
    /// exercise everything that reacts to one.
    tailscale_debug_force_netmap_update => NoParams, force_netmap_update,
        toolset: LocalDebug, tier: Write, idempotent: true;
}

/// A `debug` subcommand this server does not offer, and why.
///
/// The reason is not decoration. It is what the passthrough tells a caller that
/// asked for one of these, and what a later reader of this list has to argue
/// with before adding one back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Excluded {
    /// The subcommand as it is written after `tailscale`, words separated by
    /// spaces, so that a caller's argument list can be matched against it.
    pub path: &'static str,
    /// Why it is not offered, phrased to be read by whoever asked for it.
    pub reason: &'static str,
}

/// Every `debug` subcommand that never becomes a tool.
///
/// Fourteen of `debug`'s forty-four subcommands. Twenty-nine of the rest are
/// declared above and `reload-config` is deliberately in neither list, which
/// accounts for all forty-four; the thirtieth tool, `tailscale_debug_file_list`,
/// runs a flag on the parent command rather than a subcommand. The
/// passthrough refuses these, so a toolset that is off is not a way in.
pub const EXCLUDED: &[Excluded] = &[
    Excluded {
        path: "debug prefs",
        reason: "it prints the node's private keys along with its preferences; \
                 `tailscale_debug_netmap` and the preference tools report the rest",
    },
    Excluded {
        path: "debug local-creds",
        reason: "it prints the credential for reaching tailscaled's private HTTP \
                 interface, which this server never uses",
    },
    Excluded {
        path: "debug env",
        reason: "it prints the whole process environment, which is where secrets \
                 for other services live",
    },
    Excluded {
        path: "debug localapi",
        reason: "it calls tailscaled's private HTTP interface directly, which this \
                 server does not do at all",
    },
    Excluded {
        path: "debug daemon-logs",
        reason: "it streams the daemon log until interrupted and has no bounded form",
    },
    Excluded {
        path: "debug daemon-bus-events",
        reason: "it streams bus events until interrupted and has no bounded form",
    },
    Excluded {
        path: "debug capture",
        reason: "it streams a packet capture of tunnel traffic until interrupted, \
                 and launches a separate application when given no path",
    },
    Excluded {
        path: "debug test-risk",
        reason: "it exists to test the client's interactive risk prompt, which a \
                 tool call cannot answer",
    },
    Excluded {
        path: "debug set-expire",
        reason: "it expires the local node's key, cutting this server off from the \
                 tailnet; re-authenticate deliberately instead",
    },
    Excluded {
        path: "debug break-tcp-conns",
        reason: "its purpose is to break the daemon's connections",
    },
    Excluded {
        path: "debug break-derp-conns",
        reason: "its purpose is to break the daemon's relay connections",
    },
    Excluded {
        path: "debug derp-set-on-demand",
        reason: "it makes the node reachable only after a delay; \
                 `tailscale_debug_derp_unset_on_demand` undoes it if something else did it",
    },
    Excluded {
        path: "debug clear-netmap-cache",
        reason: "it deletes the cached netmaps the node falls back on when the \
                 control plane is unreachable",
    },
    Excluded {
        path: "debug dev-store-set",
        reason: "it writes directly into tailscaled's state store, behind every \
                 check the rest of the client makes",
    },
];

/// A tool that takes nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

// ---------------------------------------------------------------------------
// reports
// ---------------------------------------------------------------------------

/// What a command that prints prose rather than a document had to say.
///
/// Most of `debug` predates any thought of being parsed and prints for a person
/// reading a terminal. Handing that text back whole is the honest answer: this
/// server does not know which line mattered, and inventing a structure over
/// output the client calls unstable would break on the release that reworded
/// it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TextReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

/// A single path this server was asked about.
#[derive(Debug, Serialize, JsonSchema)]
pub struct StatEntry {
    pub path: String,
    pub mode: String,
    pub size: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StatReport {
    pub entries: Vec<StatEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AddressReport {
    pub addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PathReport {
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
/// What `debug via` made of its arguments.
///
/// The command answers with one line and nothing else, so the line is the
/// report: there is no second thing it printed to hand back beside it.
pub struct ViaReport {
    pub converted: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BusGraphReport {
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WatchReport {
    /// The notifications that arrived, in the order they were published.
    pub events: Vec<Value>,
    /// How many were asked for. Fewer may have arrived only if the command
    /// ended early, which it does not do on its own.
    pub asked_for: u32,
    /// The bound the call was given, in seconds.
    pub seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What a knob did, which is never more than "the daemon was told".
///
/// None of these commands reports a new state, and asking for one afterwards
/// would be a second call with a different answer, so the report says what was
/// asked for and hands back anything the client printed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct KnobReport {
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

// ---------------------------------------------------------------------------
// parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DaemonBusGraphParams {
    /// `json` for a document, or `dot` for a Graphviz drawing returned as text.
    #[serde(default)]
    pub format: Option<BusGraphFormat>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BusGraphFormat {
    Json,
    Dot,
}

impl BusGraphFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Dot => "dot",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatParams {
    /// Paths on the local filesystem to report on. At least one.
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ViaParams {
    /// The site's numeric identifier. Give this with `prefix` to build a route.
    #[serde(default)]
    pub site_id: Option<u32>,
    /// The site's IPv4 prefix, such as `10.1.0.0/16`. Needs `site_id`.
    #[serde(default)]
    pub prefix: Option<String>,
    /// An IPv6 `via` route to take apart, reported as the site and prefix it
    /// stands for. Give this on its own.
    #[serde(default)]
    pub route: Option<String>,
}

/// The client's own default for how long to wait, and the longest this server
/// will hold a call open for one.
const DEFAULT_WATCH_SECONDS: u64 = 30;
const MAX_WATCH_SECONDS: u64 = 300;
const MAX_EVENTS: u32 = 100;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WatchIpnParams {
    /// How many notifications to wait for before returning. Required, because
    /// without one the command never ends. Capped at 100.
    pub count: u32,
    /// How long to wait for them, in seconds. Defaults to 30, capped at 300.
    /// Reaching this bound is reported as a timeout.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// Include the daemon's engine statistics in each notification.
    #[serde(default)]
    pub engine_updates: Option<bool>,
    /// Include the health actions the daemon wants taken.
    #[serde(default)]
    pub health_actions: Option<bool>,
    /// Include peer changes. On by default in the client.
    #[serde(default)]
    pub peer_changes: Option<bool>,
    /// Include incremental peer patches. On by default in the client.
    #[serde(default)]
    pub peer_patches: Option<bool>,
    /// Include each peer's WireGuard state.
    #[serde(default)]
    pub peer_wireguard_state: Option<bool>,
    /// Put the client version the daemon knows about in the first message.
    #[serde(default)]
    pub initial_client_version: Option<bool>,
    /// Put the current Taildrive shares in the first message.
    #[serde(default)]
    pub initial_drive_shares: Option<bool>,
    /// Put the current health state in the first message.
    #[serde(default)]
    pub initial_health: Option<bool>,
    /// Put the Taildrop files currently going out in the first message.
    #[serde(default)]
    pub initial_outgoing_files: Option<bool>,
    /// Put the current status in the first message.
    #[serde(default)]
    pub initial_status: Option<bool>,
    /// Put the exit node the daemon would suggest in the first message.
    #[serde(default)]
    pub initial_suggested_exit_node: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeerParams {
    /// The peer, by hostname or Tailscale IP address.
    pub peer: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveParams {
    /// The hostname to look up.
    pub host: String,
    /// Which addresses to ask for: `ip`, `ip4` or `ip6`. Defaults to `ip`.
    #[serde(default)]
    pub net: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DialTypesParams {
    /// The host or IP address to dial.
    pub host: String,
    /// The port to dial.
    pub port: u16,
    /// The network to dial: `tcp`, `udp` and so on. Defaults to `tcp`.
    #[serde(default)]
    pub network: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Ts2021Params {
    /// The control-plane host to test against. Defaults to the real one.
    #[serde(default)]
    pub host: Option<String>,
    /// The protocol version to speak. Defaults to the client's own.
    #[serde(default)]
    pub version: Option<u32>,
    /// An ACE server address to offer as a candidate path.
    #[serde(default)]
    pub ace: Option<String>,
    /// A file on the local filesystem holding a dial plan as JSON.
    #[serde(default)]
    pub dial_plan: Option<String>,
    /// Report every step of the exchange rather than the outcome.
    #[serde(default)]
    pub verbose: Option<bool>,
}

/// The client's own default probe length, and the longest this server will hold
/// a call open for one.
const DEFAULT_PORTMAP_SECONDS: u64 = 5;
const MAX_PORTMAP_SECONDS: u64 = 120;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PortmapParams {
    /// How long to probe for, in seconds. Defaults to 5, capped at 120.
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    /// Probe only one protocol: `pmp`, `pcp` or `upnp`. All three by default.
    #[serde(default)]
    pub r#type: Option<String>,
    /// Probe this gateway rather than the one the daemon found. Must be given
    /// together with `self_addr`.
    #[serde(default)]
    pub gateway_addr: Option<String>,
    /// The address to claim as this node's own. Must be given together with
    /// `gateway_addr`.
    #[serde(default)]
    pub self_addr: Option<String>,
    /// Report every HTTP request and response the probe made.
    #[serde(default)]
    pub log_http: Option<bool>,
}

/// The client's own default, which is what an unspecified call should do.
const DEFAULT_COMPONENT_LOG_SECONDS: i64 = 3600;

const fn default_component_log_seconds() -> i64 {
    DEFAULT_COMPONENT_LOG_SECONDS
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComponentLogsParams {
    /// Which component to log: `magicsock`, `sockstats` or `syspolicy`.
    pub component: String,
    /// For how long, in seconds. Zero or negative turns the logging off again.
    /// Defaults to an hour, which is the client's own default.
    #[serde(default = "default_component_log_seconds")]
    pub for_seconds: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForcePreferDerpParams {
    /// The DERP region to pin the node to, or 0 to un-pin it.
    pub region_id: i32,
}

// ---------------------------------------------------------------------------
// running
// ---------------------------------------------------------------------------

/// Run a command that prints for a person, and hand back what it printed.
async fn text(ctx: &ToolContext, meta: &ToolMeta, invocation: Invocation) -> ToolResult<Value> {
    let output = cli::run(ctx, meta, invocation).await?;
    report(TextReport {
        printed: printed(ctx, &output),
    })
}

/// Run a knob and report that the daemon was told.
async fn knob(
    ctx: &ToolContext,
    meta: &ToolMeta,
    invocation: Invocation,
    outcome: impl Into<String>,
) -> ToolResult<Value> {
    let output = cli::run(ctx, meta, invocation).await?;
    report(KnobReport {
        outcome: outcome.into(),
        printed: printed(ctx, &output),
    })
}

// ---------------------------------------------------------------------------
// readers
// ---------------------------------------------------------------------------

async fn derp_map(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_derp_map,
        Invocation::read(["debug", "derp-map"]),
    )
    .await
}

async fn netmap(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_netmap,
        Invocation::read(["debug", "netmap"]),
    )
    .await
}

async fn hostinfo(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    // The TPM line this prints goes to standard error, which `object` does not
    // parse, so the document survives it.
    object(
        ctx,
        &metas::tailscale_debug_hostinfo,
        Invocation::read(["debug", "hostinfo"]),
    )
    .await
}

async fn control_knobs(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_control_knobs,
        Invocation::read(["debug", "control-knobs"]),
    )
    .await
}

async fn daemon_goroutines(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    text(
        ctx,
        &metas::tailscale_debug_daemon_goroutines,
        Invocation::read(["debug", "daemon-goroutines"]),
    )
    .await
}

async fn daemon_bus_graph(ctx: &ToolContext, params: DaemonBusGraphParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_debug_daemon_bus_graph;
    let format = params.format.unwrap_or(BusGraphFormat::Json);
    let invocation = Invocation::read([
        "debug".to_owned(),
        "daemon-bus-graph".to_owned(),
        format!("--format={}", format.as_str()),
    ]);
    match format {
        BusGraphFormat::Json => {
            let graph = document(ctx, meta, invocation).await?;
            report(BusGraphReport {
                format: format.as_str().to_owned(),
                graph: Some(graph),
                printed: None,
            })
        }
        BusGraphFormat::Dot => {
            let output = cli::run(ctx, meta, invocation).await?;
            report(BusGraphReport {
                format: format.as_str().to_owned(),
                graph: None,
                printed: Some(output.stdout_str().trim_end().to_owned()),
            })
        }
    }
}

async fn daemon_bus_queues(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_daemon_bus_queues,
        Invocation::read(["debug", "daemon-bus-queues"]),
    )
    .await
}

async fn metrics(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    // Prometheus text, not JSON, and deliberately not parsed here: the daemon's
    // internal metric names are the least stable thing in a toolset that is
    // already unstable, and `tailscale_metrics_print` is the parsed one.
    text(
        ctx,
        &metas::tailscale_debug_metrics,
        Invocation::read(["debug", "metrics"]),
    )
    .await
}

async fn statedir(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let path = cli::run_text(
        ctx,
        &metas::tailscale_debug_statedir,
        Invocation::read(["debug", "statedir"]),
    )
    .await?;
    report(PathReport {
        path: path.trim().to_owned(),
    })
}

async fn go_buildinfo(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_go_buildinfo,
        Invocation::read(["debug", "go-buildinfo"]),
    )
    .await
}

async fn peer_relay_servers(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_peer_relay_servers,
        Invocation::read(["debug", "peer-relay-servers"]),
    )
    .await
}

async fn peer_relay_sessions(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    text(
        ctx,
        &metas::tailscale_debug_peer_relay_sessions,
        Invocation::read(["debug", "peer-relay-sessions"]),
    )
    .await
}

async fn file_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    // An empty inbox prints `null`, which is a document but not a list. The
    // caller asked what is waiting, and "nothing" is a list of nothing.
    let value = document(
        ctx,
        &metas::tailscale_debug_file_list,
        Invocation::read(["debug", "--file=get"]),
    )
    .await?;
    let files = if value.is_null() {
        Value::Array(Vec::new())
    } else {
        value
    };
    Ok(json!({ "files": files }))
}

async fn stat(ctx: &ToolContext, params: StatParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_debug_stat;
    if params.paths.is_empty() {
        return Err(ToolError::invalid_args(
            "`paths` needs at least one path to report on",
        ));
    }
    let mut args = vec!["debug".to_owned(), "stat".to_owned()];
    for path in &params.paths {
        args.push(real_path(ctx, "paths", path)?);
    }
    let output = cli::run(ctx, meta, Invocation::read(args)).await?;
    report(StatReport {
        entries: output.stdout_str().lines().filter_map(parse_stat).collect(),
        printed: printed(ctx, &output),
    })
}

/// One `path: mode, size` line.
///
/// Split from the right, because a path may hold a colon and a comma but the
/// mode and size that follow it may not.
fn parse_stat(line: &str) -> Option<StatEntry> {
    let (path, rest) = line.trim().rsplit_once(": ")?;
    let (mode, size) = rest.rsplit_once(", ")?;
    Some(StatEntry {
        path: path.to_owned(),
        mode: mode.to_owned(),
        size: size.to_owned(),
    })
}

async fn via(ctx: &ToolContext, params: ViaParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_debug_via;
    // One positional, two meanings, and the client tells them apart by how many
    // words follow. Refusing the mixtures here means the caller learns which
    // conversion it asked for rather than reading an argument-count error.
    let args = match (
        params.site_id,
        params.prefix.as_deref(),
        params.route.as_deref(),
    ) {
        (Some(site_id), Some(prefix), None) => {
            vec![
                "debug".to_owned(),
                "via".to_owned(),
                site_id.to_string(),
                prefix.to_owned(),
            ]
        }
        (None, None, Some(route)) => {
            vec!["debug".to_owned(), "via".to_owned(), route.to_owned()]
        }
        (None, None, None) => {
            return Err(ToolError::invalid_args(
                "give `site_id` and `prefix` to build a route, or `route` to take one apart",
            ));
        }
        _ => {
            return Err(ToolError::invalid_args(
                "`site_id` and `prefix` go together and neither goes with `route`",
            ));
        }
    };
    let text = cli::run_text(ctx, meta, Invocation::read(args)).await?;
    report(ViaReport {
        converted: ctx.redactor.apply(text.trim()).into_owned(),
    })
}

async fn watch_ipn(ctx: &ToolContext, params: WatchIpnParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_debug_watch_ipn;
    if params.count == 0 {
        return Err(ToolError::invalid_args(
            "`count` must be at least 1: the client reads zero as \"never stop\"",
        ));
    }
    let count = params.count.min(MAX_EVENTS);
    let (seconds, bound) = bounded_wait(
        params.timeout_seconds,
        DEFAULT_WATCH_SECONDS,
        MAX_WATCH_SECONDS,
    );

    let mut args = vec![
        "debug".to_owned(),
        "watch-ipn".to_owned(),
        format!("--count={count}"),
    ];
    push_bool(&mut args, "engine-updates", params.engine_updates);
    push_bool(&mut args, "health-actions", params.health_actions);
    push_bool(&mut args, "peer-changes", params.peer_changes);
    push_bool(&mut args, "peer-patches", params.peer_patches);
    push_bool(
        &mut args,
        "peer-wireguard-state",
        params.peer_wireguard_state,
    );
    push_bool(
        &mut args,
        "initial-client-version",
        params.initial_client_version,
    );
    push_bool(
        &mut args,
        "initial-drive-shares",
        params.initial_drive_shares,
    );
    push_bool(&mut args, "initial-health", params.initial_health);
    push_bool(
        &mut args,
        "initial-outgoing-files",
        params.initial_outgoing_files,
    );
    push_bool(&mut args, "initial-status", params.initial_status);
    push_bool(
        &mut args,
        "initial-suggested-exit-node",
        params.initial_suggested_exit_node,
    );

    let output = cli::run(ctx, meta, Invocation::read(args).with_timeout(bound)).await?;
    report(WatchReport {
        events: notifications(&output.stdout_str()),
        asked_for: count,
        seconds,
        note: note(ctx, &output.stderr),
    })
}

/// The notifications in a `watch-ipn` stream.
///
/// The command prints one pretty JSON object per notification, one after
/// another with nothing between them, so this is not a document and
/// `serde_json::from_str` refuses it. Reading the stream value by value is what
/// the format actually is.
fn notifications(stdout: &str) -> Vec<Value> {
    serde_json::Deserializer::from_str(stdout)
        .into_iter::<Value>()
        .map_while(Result::ok)
        .collect()
}

async fn peer_endpoint_changes(ctx: &ToolContext, params: PeerParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_debug_peer_endpoint_changes,
        Invocation::read(["debug", "peer-endpoint-changes", &params.peer]),
    )
    .await
}

async fn resolve(ctx: &ToolContext, params: ResolveParams) -> ToolResult<Value> {
    let mut args = vec![
        "debug".to_owned(),
        "resolve".to_owned(),
        params.host.clone(),
    ];
    push_text(&mut args, "net", params.net.as_deref());
    let output = cli::run(ctx, &metas::tailscale_debug_resolve, Invocation::read(args)).await?;
    report(AddressReport {
        addresses: output
            .stdout_str()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
        printed: printed(ctx, &output),
    })
}

async fn dial_types(ctx: &ToolContext, params: DialTypesParams) -> ToolResult<Value> {
    let mut args = vec![
        "debug".to_owned(),
        "dial-types".to_owned(),
        params.host.clone(),
        params.port.to_string(),
    ];
    push_text(&mut args, "network", params.network.as_deref());
    text(
        ctx,
        &metas::tailscale_debug_dial_types,
        Invocation::read(args),
    )
    .await
}

async fn derp(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    text(
        ctx,
        &metas::tailscale_debug_derp,
        Invocation::read(["debug", "derp"]),
    )
    .await
}

async fn ts2021(ctx: &ToolContext, params: Ts2021Params) -> ToolResult<Value> {
    let mut args = vec!["debug".to_owned(), "ts2021".to_owned()];
    push_text(&mut args, "host", params.host.as_deref());
    push_text(&mut args, "ace", params.ace.as_deref());
    if let Some(path) = params.dial_plan.as_deref() {
        args.push(format!(
            "--dial-plan={}",
            real_path(ctx, "dial_plan", path)?
        ));
    }
    if let Some(version) = params.version {
        args.push(format!("--version={version}"));
    }
    push_bool(&mut args, "verbose", params.verbose);
    text(ctx, &metas::tailscale_debug_ts2021, Invocation::read(args)).await
}

async fn portmap(ctx: &ToolContext, params: PortmapParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_debug_portmap;
    // The client's own note: neither override means anything without the other,
    // and passing one alone probes a network that does not exist.
    if params.gateway_addr.is_some() != params.self_addr.is_some() {
        return Err(ToolError::invalid_args(
            "`gateway_addr` and `self_addr` override the probe's view of the network \
             together, so give both or neither",
        ));
    }
    let (seconds, bound) = bounded_wait(
        params.duration_seconds,
        DEFAULT_PORTMAP_SECONDS,
        MAX_PORTMAP_SECONDS,
    );
    let mut args = vec![
        "debug".to_owned(),
        "portmap".to_owned(),
        format!("--duration={seconds}s"),
    ];
    push_text(&mut args, "type", params.r#type.as_deref());
    push_text(&mut args, "gateway-addr", params.gateway_addr.as_deref());
    push_text(&mut args, "self-addr", params.self_addr.as_deref());
    push_bool(&mut args, "log-http", params.log_http);
    text(ctx, meta, Invocation::read(args).with_timeout(bound)).await
}

// ---------------------------------------------------------------------------
// knobs
// ---------------------------------------------------------------------------

async fn component_logs(ctx: &ToolContext, params: ComponentLogsParams) -> ToolResult<Value> {
    let seconds = params.for_seconds;
    let outcome = if seconds > 0 {
        format!(
            "verbose logging for `{}` is on for the next {seconds}s",
            params.component
        )
    } else {
        format!("verbose logging for `{}` is off", params.component)
    };
    knob(
        ctx,
        &metas::tailscale_debug_component_logs,
        Invocation::mutate_shared([
            "debug".to_owned(),
            "component-logs".to_owned(),
            format!("--for={seconds}s"),
            params.component.clone(),
        ]),
        outcome,
    )
    .await
}

async fn restun(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    knob(
        ctx,
        &metas::tailscale_debug_restun,
        Invocation::mutate_shared(["debug", "restun"]),
        "the node was told to re-learn its own addresses",
    )
    .await
}

async fn rebind(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    knob(
        ctx,
        &metas::tailscale_debug_rebind,
        Invocation::mutate_shared(["debug", "rebind"]),
        "the node was told to rebind its sockets",
    )
    .await
}

async fn rotate_disco_key(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    knob(
        ctx,
        &metas::tailscale_debug_rotate_disco_key,
        Invocation::mutate_shared(["debug", "rotate-disco-key"]),
        "the node's disco key was rotated; peers will rebuild their direct paths",
    )
    .await
}

async fn derp_unset_on_demand(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    knob(
        ctx,
        &metas::tailscale_debug_derp_unset_on_demand,
        Invocation::mutate_shared(["debug", "derp-unset-on-demand"]),
        "relay connections are back to always-on",
    )
    .await
}

async fn pick_new_derp(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    knob(
        ctx,
        &metas::tailscale_debug_pick_new_derp,
        Invocation::mutate_shared(["debug", "pick-new-derp"]),
        "the node was moved to another home relay region until it restarts",
    )
    .await
}

async fn force_prefer_derp(ctx: &ToolContext, params: ForcePreferDerpParams) -> ToolResult<Value> {
    let outcome = if params.region_id == 0 {
        "the node is free to choose its own home relay region again".to_owned()
    } else {
        format!(
            "the node prefers relay region {} until it restarts",
            params.region_id
        )
    };
    knob(
        ctx,
        &metas::tailscale_debug_force_prefer_derp,
        Invocation::mutate_shared([
            "debug".to_owned(),
            "force-prefer-derp".to_owned(),
            params.region_id.to_string(),
        ]),
        outcome,
    )
    .await
}

async fn force_netmap_update(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    knob(
        ctx,
        &metas::tailscale_debug_force_netmap_update,
        Invocation::mutate_shared(["debug", "force-netmap-update"]),
        "a full netmap update was pushed through the daemon",
    )
    .await
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use super::*;
    use crate::context::{PathPolicy, SelfIdentity};
    use crate::error::{ErrorCode, Redactor};
    use crate::meta::{Tier, Toolset};
    use crate::testing::{Reply, StubBackend};

    fn context(backend: Arc<StubBackend>) -> ToolContext {
        ToolContext {
            local: backend as Arc<dyn tailscale_cli::LocalBackend>,
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity::default(),
            cli_version: None,
            paths: PathPolicy::default(),
        }
    }

    /// Run a handler against a scripted client and report both what it answered
    /// and what it ran.
    async fn against<F, P, Fut>(reply: Reply, handler: F, params: P) -> (Value, Vec<Vec<String>>)
    where
        F: FnOnce(ToolContext, P) -> Fut,
        Fut: std::future::Future<Output = ToolResult<Value>>,
    {
        let backend = Arc::new(StubBackend::always(reply));
        let ctx = context(Arc::clone(&backend));
        let value = handler(ctx, params).await.expect("the handler succeeds");
        (value, backend.argv())
    }

    /// Run a handler that is expected to refuse, and report the refusal.
    async fn refusal<F, P, Fut>(reply: Reply, handler: F, params: P) -> ToolError
    where
        F: FnOnce(ToolContext, P) -> Fut,
        Fut: std::future::Future<Output = ToolResult<Value>>,
    {
        let backend = Arc::new(StubBackend::always(reply));
        let ctx = context(backend);
        handler(ctx, params).await.expect_err("the handler refuses")
    }

    fn watching(count: u32, timeout_seconds: Option<u64>) -> WatchIpnParams {
        WatchIpnParams {
            count,
            timeout_seconds,
            engine_updates: None,
            health_actions: None,
            peer_changes: None,
            peer_patches: None,
            peer_wireguard_state: None,
            initial_client_version: None,
            initial_drive_shares: None,
            initial_health: None,
            initial_outgoing_files: None,
            initial_status: None,
            initial_suggested_exit_node: None,
        }
    }

    #[test]
    fn the_toolset_is_twenty_two_readers_and_eight_knobs() {
        let all = entries();
        assert_eq!(all.len(), 30, "the debug toolset is thirty tools");

        let readers = all.iter().filter(|e| e.meta.tier == Tier::Read).count();
        let knobs = all.iter().filter(|e| e.meta.tier == Tier::Write).count();
        assert_eq!(readers, 22, "twenty-two readers");
        assert_eq!(knobs, 8, "eight knobs");
        assert_eq!(
            readers + knobs,
            all.len(),
            "a debug tool is a reader or a knob"
        );
    }

    #[test]
    fn every_tool_is_in_the_debug_toolset() {
        for entry in entries() {
            assert_eq!(
                entry.meta.toolset,
                Toolset::LocalDebug,
                "`{}` is declared elsewhere",
                entry.meta.name
            );
        }
    }

    #[test]
    fn nothing_here_needs_a_confirmation() {
        // A confirmation is for an operation that cannot be undone. Every knob
        // here is undone by the daemon's next restart, so asking for one would
        // spend the caller's attention on the wrong tools.
        for entry in entries() {
            assert!(
                !entry.meta.requires_confirmation,
                "`{}` asks for a confirmation it does not need",
                entry.meta.name
            );
        }
    }

    #[test]
    fn no_excluded_command_is_also_declared_as_a_tool() {
        let declared: BTreeSet<String> = entries()
            .iter()
            .map(|e| {
                e.meta
                    .name
                    .replace("tailscale_debug_", "debug ")
                    .replace('_', "-")
            })
            .collect();
        for excluded in EXCLUDED {
            assert!(
                !declared.contains(excluded.path),
                "`{}` is both excluded and offered",
                excluded.path
            );
        }
    }

    #[test]
    fn the_excluded_list_is_complete_and_says_why() {
        assert_eq!(
            EXCLUDED.len(),
            14,
            "fourteen of the forty-four are excluded"
        );
        let mut paths = BTreeSet::new();
        for excluded in EXCLUDED {
            assert!(
                excluded.path.starts_with("debug "),
                "`{}` is not a debug subcommand",
                excluded.path
            );
            assert!(
                excluded.reason.len() > 20,
                "`{}` is excluded without saying why",
                excluded.path
            );
            assert!(
                paths.insert(excluded.path),
                "`{}` is excluded twice",
                excluded.path
            );
        }
    }

    #[test]
    fn a_stat_line_splits_from_the_right() {
        let entry = parse_stat("/etc/hosts: -rw-r--r--, 213").expect("a well-formed line");
        assert_eq!(entry.path, "/etc/hosts");
        assert_eq!(entry.mode, "-rw-r--r--");
        assert_eq!(entry.size, "213");

        // A path may hold both separators; the mode and size that follow may not.
        let odd = parse_stat("/tmp/a: b, c/file: -rw-------, 0").expect("a path with separators");
        assert_eq!(odd.path, "/tmp/a: b, c/file");
        assert_eq!(odd.size, "0");

        assert!(parse_stat("something else entirely").is_none());
    }

    #[test]
    fn concatenated_notifications_are_read_one_at_a_time() {
        // What `watch-ipn` actually prints: whole objects, pretty, with nothing
        // between them. A document parser refuses the second one.
        let stream = "{\n\t\"Version\": \"1.102.2\"\n}\n{\n\t\"Version\": \"1.102.2\"\n}\n";
        assert!(serde_json::from_str::<Value>(stream).is_err());

        let events = notifications(stream);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["Version"], "1.102.2");
    }

    #[test]
    fn a_half_written_notification_is_dropped_rather_than_losing_the_rest() {
        let stream = "{\"a\": 1}\n{\"b\": 2}\n{\"c\":";
        let events = notifications(stream);
        assert_eq!(
            events.len(),
            2,
            "the whole objects survive the truncated one"
        );
    }

    #[test]
    fn an_empty_stream_is_no_events_rather_than_an_error() {
        assert!(notifications("").is_empty());
    }

    // -- the event watcher always returns ------------------------------------

    #[tokio::test]
    async fn the_watcher_honours_its_count_and_its_cap() {
        let (value, argv) = against(
            Reply::ok("{\"Version\":\"1.102.2\"}\n"),
            |ctx, p| async move { watch_ipn(&ctx, p).await },
            watching(3, None),
        )
        .await;
        assert_eq!(argv, [["debug", "watch-ipn", "--count=3"]]);
        assert_eq!(value["asked_for"], 3);
        assert_eq!(value["seconds"], DEFAULT_WATCH_SECONDS);
        assert_eq!(value["events"].as_array().map(Vec::len), Some(1));

        let (value, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { watch_ipn(&ctx, p).await },
            watching(10_000, Some(10_000)),
        )
        .await;
        assert_eq!(
            argv,
            [["debug", "watch-ipn", &format!("--count={MAX_EVENTS}")]]
        );
        assert_eq!(value["asked_for"], MAX_EVENTS);
        assert_eq!(value["seconds"], MAX_WATCH_SECONDS);
    }

    #[tokio::test]
    async fn a_count_of_zero_is_refused_rather_than_run() {
        // Zero is the client's spelling of "never stop", and a tool call that
        // never stops is the one thing a bounded form exists to prevent.
        let error = refusal(
            Reply::ok(""),
            |ctx, p| async move { watch_ipn(&ctx, p).await },
            watching(0, None),
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn a_quiet_node_reaches_the_bound_and_the_call_still_answers() {
        // The client has no timeout of its own here, so the bound is this
        // server's. Reaching it is reported, not waited through.
        let error = refusal(
            Reply::TimedOut {
                printed: "{\"Version\":\"1.102.2\"}".to_owned(),
            },
            |ctx, p| async move { watch_ipn(&ctx, p).await },
            watching(5, Some(1)),
        )
        .await;
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(
            error.message.contains("1.102.2"),
            "what did arrive is handed back: {}",
            error.message
        );
    }

    // -- what the rest of them run -------------------------------------------

    #[tokio::test]
    async fn the_bus_graph_reports_a_document_or_a_drawing_but_never_both() {
        let (value, argv) = against(
            Reply::ok(r#"{"nodes":[]}"#),
            |ctx, p| async move { daemon_bus_graph(&ctx, p).await },
            DaemonBusGraphParams { format: None },
        )
        .await;
        assert_eq!(argv, [["debug", "daemon-bus-graph", "--format=json"]]);
        assert_eq!(value["format"], "json");
        assert!(value["graph"].is_object());
        assert!(value.get("printed").is_none());

        let (value, argv) = against(
            Reply::ok("digraph {}\n"),
            |ctx, p| async move { daemon_bus_graph(&ctx, p).await },
            DaemonBusGraphParams {
                format: Some(BusGraphFormat::Dot),
            },
        )
        .await;
        assert_eq!(argv, [["debug", "daemon-bus-graph", "--format=dot"]]);
        assert_eq!(value["format"], "dot");
        assert_eq!(value["printed"], "digraph {}");
        assert!(value.get("graph").is_none());
    }

    #[tokio::test]
    async fn an_empty_inbox_is_a_list_of_nothing_rather_than_a_null() {
        let (value, argv) = against(
            Reply::ok("null\n"),
            |ctx, p| async move { file_list(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(argv, [["debug", "--file=get"]]);
        assert_eq!(value["files"], json!([]));
    }

    #[tokio::test]
    async fn the_two_conversions_via_offers_are_told_apart_before_anything_runs() {
        let (value, argv) = against(
            Reply::ok("fd7a:115c:a1e0:b1a:0:7:a01:0/112\n"),
            |ctx, p| async move { via(&ctx, p).await },
            ViaParams {
                site_id: Some(7),
                prefix: Some("10.1.0.0/16".to_owned()),
                route: None,
            },
        )
        .await;
        assert_eq!(argv, [["debug", "via", "7", "10.1.0.0/16"]]);
        assert_eq!(value["converted"], "fd7a:115c:a1e0:b1a:0:7:a01:0/112");

        let (_, argv) = against(
            Reply::ok("site 7 (0x7), 10.1.0.0/16\n"),
            |ctx, p| async move { via(&ctx, p).await },
            ViaParams {
                site_id: None,
                prefix: None,
                route: Some("fd7a:115c:a1e0:b1a:0:7:a01:0/112".to_owned()),
            },
        )
        .await;
        assert_eq!(argv, [["debug", "via", "fd7a:115c:a1e0:b1a:0:7:a01:0/112"]]);

        for mixture in [
            ViaParams {
                site_id: Some(7),
                prefix: None,
                route: None,
            },
            ViaParams {
                site_id: Some(7),
                prefix: Some("10.1.0.0/16".to_owned()),
                route: Some("fd7a::/112".to_owned()),
            },
            ViaParams {
                site_id: None,
                prefix: None,
                route: None,
            },
        ] {
            let error = refusal(
                Reply::ok(""),
                |ctx, p| async move { via(&ctx, p).await },
                mixture,
            )
            .await;
            assert_eq!(error.code, ErrorCode::InvalidArgs);
        }
    }

    #[tokio::test]
    async fn the_portmap_overrides_are_refused_one_at_a_time() {
        // The client says each override "must also pass" the other, so a lone
        // one describes a network that does not exist.
        for lonely in [
            PortmapParams {
                duration_seconds: None,
                r#type: None,
                gateway_addr: Some("192.0.2.1".to_owned()),
                self_addr: None,
                log_http: None,
            },
            PortmapParams {
                duration_seconds: None,
                r#type: None,
                gateway_addr: None,
                self_addr: Some("192.0.2.2".to_owned()),
                log_http: None,
            },
        ] {
            let error = refusal(
                Reply::ok(""),
                |ctx, p| async move { portmap(&ctx, p).await },
                lonely,
            )
            .await;
            assert_eq!(error.code, ErrorCode::InvalidArgs);
        }

        let (value, argv) = against(
            Reply::ok("portmapper: no port mapping services were found\n"),
            |ctx, p| async move { portmap(&ctx, p).await },
            PortmapParams {
                duration_seconds: Some(10_000),
                r#type: Some("upnp".to_owned()),
                gateway_addr: Some("192.0.2.1".to_owned()),
                self_addr: Some("192.0.2.2".to_owned()),
                log_http: Some(false),
            },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "debug",
                "portmap",
                &format!("--duration={MAX_PORTMAP_SECONDS}s"),
                "--type=upnp",
                "--gateway-addr=192.0.2.1",
                "--self-addr=192.0.2.2",
                "--log-http=false"
            ]]
        );
        assert!(
            value["printed"]
                .as_str()
                .is_some_and(|p| p.contains("portmapper"))
        );
    }

    #[test]
    fn the_flags_that_are_rust_keywords_still_reach_the_schema_under_their_own_names() {
        // `--type` is `debug portmap`'s flag and ADR-0004 says a parameter
        // carries the flag's own name, which here collides with a keyword. A
        // raw identifier keeps the name; this asserts that serde and schemars
        // both drop the `r#`, because a silent `r#type` in the schema would be
        // a parameter no caller could guess and no client could send.
        let schema =
            rmcp::handler::server::tool::schema_for_input::<PortmapParams>().expect("a schema");
        let properties = schema["properties"]
            .as_object()
            .expect("the schema lists properties");
        assert!(properties.contains_key("type"), "got {properties:#?}");
        assert!(!properties.keys().any(|k| k.starts_with("r#")));

        let parsed: PortmapParams = serde_json::from_value(json!({"type": "upnp"}))
            .expect("`type` is the name on the wire too");
        assert_eq!(parsed.r#type.as_deref(), Some("upnp"));
    }

    #[tokio::test]
    async fn turning_component_logging_off_says_so_rather_than_saying_how_long_for() {
        let (value, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { component_logs(&ctx, p).await },
            ComponentLogsParams {
                component: "magicsock".to_owned(),
                for_seconds: default_component_log_seconds(),
            },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "debug",
                "component-logs",
                &format!("--for={DEFAULT_COMPONENT_LOG_SECONDS}s"),
                "magicsock"
            ]]
        );
        assert!(
            value["outcome"]
                .as_str()
                .is_some_and(|o| o.contains("is on"))
        );

        let (value, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { component_logs(&ctx, p).await },
            ComponentLogsParams {
                component: "magicsock".to_owned(),
                for_seconds: 0,
            },
        )
        .await;
        assert_eq!(argv, [["debug", "component-logs", "--for=0s", "magicsock"]]);
        assert_eq!(value["outcome"], "verbose logging for `magicsock` is off");
    }

    #[tokio::test]
    async fn pinning_a_relay_region_and_letting_it_go_read_differently() {
        let (value, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { force_prefer_derp(&ctx, p).await },
            ForcePreferDerpParams { region_id: 0 },
        )
        .await;
        assert_eq!(argv, [["debug", "force-prefer-derp", "0"]]);
        assert!(
            value["outcome"]
                .as_str()
                .is_some_and(|o| o.contains("free to choose"))
        );

        let (value, _) = against(
            Reply::ok(""),
            |ctx, p| async move { force_prefer_derp(&ctx, p).await },
            ForcePreferDerpParams { region_id: 2 },
        )
        .await;
        assert!(
            value["outcome"]
                .as_str()
                .is_some_and(|o| o.contains("region 2"))
        );
    }

    #[tokio::test]
    async fn a_knob_that_prints_nothing_still_reports_what_it_did() {
        let (value, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { restun(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(argv, [["debug", "restun"]]);
        assert!(
            value["outcome"]
                .as_str()
                .is_some_and(|o| o.contains("re-learn"))
        );
        assert!(
            value.get("printed").is_none(),
            "nothing printed, nothing reported"
        );
    }

    #[tokio::test]
    async fn stat_refuses_an_empty_list_and_a_stream() {
        let error = refusal(
            Reply::ok(""),
            |ctx, p| async move { stat(&ctx, p).await },
            StatParams { paths: Vec::new() },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);

        // `-` is the client's spelling of standard output, which a tool call
        // does not have.
        let error = refusal(
            Reply::ok(""),
            |ctx, p| async move { stat(&ctx, p).await },
            StatParams {
                paths: vec!["-".to_owned()],
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }
}
