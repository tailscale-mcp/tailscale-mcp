//! Reading the state of the local node.
//!
//! Every read-only local command lives here, whichever part of the command tree
//! it comes from: `lock status` sits beside `status` because a toolset is a
//! group of things an operator is willing to let a model do, and reading the
//! tailnet-lock state is the same willingness as reading the peer list. The
//! grouping is by risk, not by the shape of the CLI (DECISIONS Q14).
//!
//! Two rules run through the module. Where the CLI prints a JSON document, that
//! document is forwarded unmodified — the control plane's schema is better
//! documentation than any re-modelling of it, and it changes on Tailscale's
//! schedule rather than ours. Where the CLI prints text, there is a parser, and
//! the parser has a test against a recorded sample.

use std::collections::BTreeMap;
use std::time::Duration;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tailscale_cli::Invocation;

use crate::cli;
use crate::context::ToolContext;
use crate::error::ToolResult;
use crate::meta::ToolMeta;
use crate::tools::common::{bounded_wait, document, flag, lines, note, object, report};
use crate::version::{SUPPORTED_FLOOR, Version};

crate::tools! {
    /// Report the state of this node and of every peer it can see: addresses,
    /// operating systems, connection paths, transfer counters and health
    /// warnings. The whole `tailscale status --json` document, unmodified.
    tailscale_status => StatusParams, status,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// List this node's Tailscale addresses, or resolve a peer or service name
    /// to its addresses.
    tailscale_ip => IpParams, ip,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Probe the local network and the DERP relays: whether UDP works, whether
    /// the NAT is symmetric, which relay is nearest and how far away each one
    /// is. Sends traffic, and takes a second or two.
    tailscale_netcheck => NoParams, netcheck,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Ping a peer at the Tailscale layer and report the path each reply took,
    /// which is how you tell a direct connection from a relayed one.
    tailscale_ping => PingParams, ping,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Identify the machine and user behind a Tailscale address.
    tailscale_whois => WhoisParams, whois,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Identify this node: its machine record and the user it is logged in as.
    tailscale_whoami => NoParams, whoami,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.90";

    /// Report the version of the `tailscale` binary this server drives, and
    /// whether it is new enough for everything this server models.
    tailscale_version => NoParams, version,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Print where the open-source licences of the components in this client
    /// build are published.
    tailscale_licenses => NoParams, licenses,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Emit a diagnostic marker to Tailscale's log service and return it, so
    /// that it can be quoted to Tailscale support. Uploads nothing beyond what
    /// the client already logs.
    tailscale_bugreport => BugreportParams, bugreport,
        toolset: LocalStatus, tier: Read, idempotent: false;

    /// List the routes this node has learnt as an app connector, and say
    /// whether it is acting as one at all.
    tailscale_appc_routes => AppcRoutesParams, appc_routes,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.90";

    /// Report the experimental reachability check: which advertised routes this
    /// node can reach and through which peer. Often has no report to give until
    /// a probe has been asked for.
    tailscale_routecheck => RoutecheckParams, routecheck,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.102";

    /// Wait until the node's network interface and addresses are ready, up to a
    /// bounded timeout. Returns immediately when they already are.
    tailscale_wait => WaitParams, wait,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.90";

    /// Report how DNS is resolving: whether MagicDNS is in use, which
    /// nameservers the tailnet supplies, and what the operating system's own
    /// resolver is configured with.
    tailscale_dns_status => DnsStatusParams, dns_status,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.72";

    /// Resolve a name through Tailscale's resolver and report the answer,
    /// including which resolver answered.
    tailscale_dns_query => DnsQueryParams, dns_query,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.72";

    /// List the exit nodes available to this node, both those in the tailnet
    /// and those from Mullvad, and say which one is selected.
    tailscale_exit_node_list => ExitNodeListParams, exit_node_list,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Ask the client which exit node it would pick, by latency and location.
    tailscale_exit_node_suggest => NoParams, exit_node_suggest,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.66";

    /// Report the client's user-facing metrics: routes advertised and approved,
    /// home relay region, and bytes and packets by path.
    tailscale_metrics_print => NoParams, metrics_print,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.78";

    /// List the Tailscale Services this node hosts.
    tailscale_service_list => NoParams, service_list,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.90";

    /// List the system policies in force on this machine — the settings an MDM
    /// profile or a local administrator has fixed — and where each came from.
    tailscale_syspolicy_list => NoParams, syspolicy_list,
        toolset: LocalStatus, tier: Read, idempotent: true, since: "1.72";

    /// Report whether tailnet lock is enabled and what this node's
    /// tailnet-lock key is.
    tailscale_lock_status => NoParams, lock_status,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// List the tailnet-lock updates this node knows about, newest first.
    tailscale_lock_log => LockLogParams, lock_log,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Report what this node is currently serving on the tailnet: which ports
    /// are listening and what each path forwards to.
    tailscale_serve_status => NoParams, serve_status,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Report what this node is currently exposing to the public internet
    /// through Funnel.
    tailscale_funnel_status => NoParams, funnel_status,
        toolset: LocalStatus, tier: Read, idempotent: true;

    /// Report the state of the macOS system extension the standalone client
    /// runs its networking in.
    tailscale_configure_sysext_status => NoParams, configure_sysext_status,
        toolset: LocalStatus, tier: Read, idempotent: true, platforms: ["macos"];

    /// List the Tailscale accounts stored on this machine and say which one is
    /// currently active.
    tailscale_switch_list => NoParams, switch_list,
        toolset: LocalStatus, tier: Read, idempotent: true;
}

// ---------------------------------------------------------------------------
// Shared shapes
// ---------------------------------------------------------------------------

/// A tool that takes nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

/// The serde default for a flag that is on unless the caller says otherwise.
const fn yes() -> bool {
    true
}

/// A document that is a list, under a field naming what the list holds.
///
/// `null` means the empty list: several of these commands print it when they
/// have nothing, and a caller should not have to tell the two apart.
async fn collection(
    ctx: &ToolContext,
    meta: &ToolMeta,
    invocation: Invocation,
    field: &str,
) -> ToolResult<Value> {
    let value = document(ctx, meta, invocation).await?;
    let items = match value {
        Value::Null => Value::Array(Vec::new()),
        Value::Array(items) => Value::Array(items),
        other => Value::Array(vec![other]),
    };
    Ok(json!({ field: items }))
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusParams {
    /// Include the peer list. Turning it off makes the answer far smaller on a
    /// large tailnet, at the cost of `Peer` coming back null.
    #[serde(default = "yes")]
    pub peers: bool,
    /// Include this node's own entry.
    #[serde(default = "yes")]
    pub include_self: bool,
    /// Only report peers with an active session.
    #[serde(default)]
    pub active: bool,
}

async fn status(ctx: &ToolContext, params: StatusParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_status,
        Invocation::read([
            "status".to_owned(),
            "--json=true".to_owned(),
            flag("peers", params.peers),
            flag("self", params.include_self),
            flag("active", params.active),
        ]),
    )
    .await
}

// ---------------------------------------------------------------------------
// ip
// ---------------------------------------------------------------------------

/// Which addresses to report.
#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AddressFamily {
    /// Both families, in the order the client prints them.
    #[default]
    Any,
    Ipv4,
    Ipv6,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpParams {
    /// A peer hostname, MagicDNS name, service name or address to resolve.
    /// Omit it to report this node's own addresses.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub family: AddressFamily,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AddressReport {
    /// What was asked about: the target, or this node.
    pub target: Option<String>,
    /// One entry per address the client printed, in its order.
    pub addresses: Vec<String>,
}

async fn ip(ctx: &ToolContext, params: IpParams) -> ToolResult<Value> {
    let mut args = vec!["ip".to_owned()];
    match params.family {
        AddressFamily::Any => {}
        AddressFamily::Ipv4 => args.push("--4=true".to_owned()),
        AddressFamily::Ipv6 => args.push("--6=true".to_owned()),
    }
    if let Some(target) = &params.target {
        args.push(target.clone());
    }
    let text = cli::run_text(ctx, &metas::tailscale_ip, Invocation::read(args)).await?;
    report(AddressReport {
        target: params.target,
        addresses: lines(&text).map(str::to_owned).collect(),
    })
}

// ---------------------------------------------------------------------------
// netcheck
// ---------------------------------------------------------------------------

async fn netcheck(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_netcheck,
        // `--format=json`, not `--json`: netcheck predates the flag the rest of
        // the CLI settled on.
        Invocation::read(["netcheck", "--format=json"]),
    )
    .await
}

// ---------------------------------------------------------------------------
// ping
// ---------------------------------------------------------------------------

/// The most pings one call will send. The CLI's own default is 10 and its
/// maximum is unbounded; a model that asks for a thousand should get a bounded
/// call rather than a session that stops answering.
const MAX_PING_COUNT: u32 = 20;
/// The longest one call will wait for a single reply.
const MAX_PING_TIMEOUT: u64 = 30;
/// What a caller gets by asking for nothing.
const DEFAULT_PING_COUNT: u32 = 5;
const DEFAULT_PING_TIMEOUT: u64 = 5;

const fn default_ping_count() -> u32 {
    DEFAULT_PING_COUNT
}
const fn default_ping_timeout() -> u64 {
    DEFAULT_PING_TIMEOUT
}

/// How to reach the peer.
#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PingMethod {
    /// Tailscale's own discovery ping, which reports the path.
    #[default]
    Disco,
    /// ICMP inside the tunnel.
    Icmp,
    /// Tailscale's in-tunnel message protocol.
    Tsmp,
    /// An HTTP request to the peer's peer API.
    Peerapi,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PingParams {
    /// A peer hostname, MagicDNS name or Tailscale address.
    pub target: String,
    /// How many pings to send. Capped at 20.
    #[serde(default = "default_ping_count")]
    pub count: u32,
    /// How long to wait for each reply, in seconds. Capped at 30.
    #[serde(default = "default_ping_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub method: PingMethod,
    /// Stop as soon as the path becomes direct, rather than sending every ping.
    #[serde(default = "yes")]
    pub until_direct: bool,
}

/// One reply, as the client printed it.
#[derive(Debug, Serialize, JsonSchema, PartialEq)]
pub struct PingReply {
    /// The peer that answered.
    pub peer: String,
    /// The address it answered from.
    pub address: String,
    /// The path the reply took: a relay region, or an address for a direct
    /// connection.
    pub via: String,
    /// Round-trip time in milliseconds, when the client reported one.
    pub latency_ms: Option<f64>,
}

impl PingReply {
    /// Whether this reply came over a direct connection rather than a relay.
    fn is_direct(&self) -> bool {
        !self.via.starts_with("DERP")
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PingReport {
    pub target: String,
    /// Whether anything answered at all.
    pub reachable: bool,
    /// Whether the last reply came over a direct connection.
    pub direct: bool,
    pub replies: Vec<PingReply>,
    /// Everything the client printed, including lines that are not replies.
    pub raw: String,
}

/// Read one `pong from …` line.
fn parse_pong(line: &str) -> Option<PingReply> {
    let rest = line.strip_prefix("pong from ")?;
    let (peer, rest) = rest.split_once(" (")?;
    let (address, rest) = rest.split_once(") via ")?;
    let (via, latency) = rest.rsplit_once(" in ")?;
    Some(PingReply {
        peer: peer.to_owned(),
        address: address.to_owned(),
        via: via.to_owned(),
        latency_ms: latency
            .trim()
            .strip_suffix("ms")
            .and_then(|value| value.parse().ok()),
    })
}

async fn ping(ctx: &ToolContext, params: PingParams) -> ToolResult<Value> {
    let count = params.count.clamp(1, MAX_PING_COUNT);
    let per_ping = params.timeout_seconds.clamp(1, MAX_PING_TIMEOUT);
    let mut args = vec![
        "ping".to_owned(),
        format!("--c={count}"),
        format!("--timeout={per_ping}s"),
        flag("until-direct", params.until_direct),
    ];
    match params.method {
        PingMethod::Disco => {}
        PingMethod::Icmp => args.push("--icmp=true".to_owned()),
        PingMethod::Tsmp => args.push("--tsmp=true".to_owned()),
        PingMethod::Peerapi => args.push("--peerapi=true".to_owned()),
    }
    args.push(params.target.clone());

    // The command bounds itself, but only if it behaves. The wall-clock bound
    // is what makes "a bounded call always returns" true even when it does not.
    let budget = Duration::from_secs(u64::from(count) * per_ping + 5);
    let text = cli::run_text(
        ctx,
        &metas::tailscale_ping,
        Invocation::read(args).with_timeout(budget),
    )
    .await?;

    let replies: Vec<PingReply> = lines(&text).filter_map(parse_pong).collect();
    report(PingReport {
        target: params.target,
        reachable: !replies.is_empty(),
        direct: replies.last().is_some_and(PingReply::is_direct),
        replies,
        raw: text.trim_end().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// whois, whoami
// ---------------------------------------------------------------------------

/// Which transport the port in an address belongs to.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WhoisParams {
    /// A Tailscale address, optionally with a port (`100.64.0.2:22`).
    pub address: String,
    /// The transport the port belongs to, when one was given.
    #[serde(default)]
    pub proto: Option<Proto>,
}

async fn whois(ctx: &ToolContext, params: WhoisParams) -> ToolResult<Value> {
    let mut args = vec!["whois".to_owned(), "--json=true".to_owned()];
    if let Some(proto) = params.proto {
        args.push(format!("--proto={}", proto.as_str()));
    }
    args.push(params.address);
    object(ctx, &metas::tailscale_whois, Invocation::read(args)).await
}

async fn whoami(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_whoami,
        Invocation::read(["whoami", "--json=true"]),
    )
    .await
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

/// What `tailscale_version` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct VersionReport {
    /// The release the binary reports, when it could be read.
    pub version: Option<String>,
    /// Everything the binary printed, which also carries the commit and the Go
    /// toolchain. Kept because it is what a bug report needs.
    pub raw: String,
    /// The oldest release this server is written against.
    pub supported_floor: String,
    /// Whether the binary is at or above that floor. An unstable build — an odd
    /// minor number — counts as newer than the stable release above it.
    pub meets_floor: bool,
}

async fn version(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let raw = cli::run_text(
        ctx,
        &metas::tailscale_version,
        Invocation::read(["version"]),
    )
    .await?;
    let version = Version::parse_cli_output(&raw);
    report(VersionReport {
        version: version.map(|v| v.to_string()),
        raw: raw.trim_end().to_owned(),
        supported_floor: SUPPORTED_FLOOR.to_string(),
        meets_floor: version.is_none_or(|v| v >= SUPPORTED_FLOOR || v.is_unstable()),
    })
}

// ---------------------------------------------------------------------------
// licenses
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
pub struct LicenseReport {
    /// Where the licences for this build are published. Usually one URL.
    pub urls: Vec<String>,
    pub text: String,
}

async fn licenses(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let text = cli::run_text(
        ctx,
        &metas::tailscale_licenses,
        Invocation::read(["licenses"]),
    )
    .await?;
    report(LicenseReport {
        urls: text
            .split_whitespace()
            .filter(|word| word.starts_with("https://") || word.starts_with("http://"))
            .map(str::to_owned)
            .collect(),
        text: text.trim_end().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// bugreport
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BugreportParams {
    /// A note recorded alongside the marker, so that support can tell one
    /// report from another.
    #[serde(default)]
    pub note: Option<String>,
    /// Run extra local checks and print what they found.
    #[serde(default)]
    pub diagnose: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BugreportOutcome {
    /// The marker to quote to Tailscale support.
    pub marker: Option<String>,
    pub raw: String,
}

async fn bugreport(ctx: &ToolContext, params: BugreportParams) -> ToolResult<Value> {
    let mut args = vec!["bugreport".to_owned(), flag("diagnose", params.diagnose)];
    if let Some(note) = params.note {
        args.push(note);
    }
    let text = cli::run_text(ctx, &metas::tailscale_bugreport, Invocation::read(args)).await?;
    report(BugreportOutcome {
        // The marker is the last thing printed; `--diagnose` puts its findings
        // above it.
        marker: lines(&text).next_back().map(str::to_owned),
        raw: text.trim_end().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// appc-routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppcRoutesParams {
    /// Report the routes policy supplies as well as the ones learnt by name.
    #[serde(default)]
    pub all: bool,
    /// Report which domain each route was learnt from.
    #[serde(default)]
    pub map: bool,
    /// Report only how many routes there are.
    #[serde(default)]
    pub count_only: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AppcRoutesReport {
    /// Whether this node is acting as an app connector at all.
    pub app_connector: bool,
    /// The routes it has learnt, when it is one.
    pub routes: Vec<String>,
    pub raw: String,
}

async fn appc_routes(ctx: &ToolContext, params: AppcRoutesParams) -> ToolResult<Value> {
    let text = cli::run_text(
        ctx,
        &metas::tailscale_appc_routes,
        Invocation::read([
            "appc-routes".to_owned(),
            flag("all", params.all),
            flag("map", params.map),
            flag("n", params.count_only),
        ]),
    )
    .await?;
    // A node that is not a connector says so and exits cleanly, so the sentence
    // is the signal.
    let connector = !text.to_ascii_lowercase().contains("not a connector");
    report(AppcRoutesReport {
        app_connector: connector,
        routes: if connector {
            lines(&text)
                .filter(|line| line.contains('/'))
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        },
        raw: text.trim_end().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// routecheck
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RoutecheckParams {
    /// Run a fresh probe rather than reading the last report. Sends traffic.
    #[serde(default)]
    pub probe: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RoutecheckReport {
    /// Whether there was a report to read. There often is not until a probe has
    /// been asked for, which is a state rather than a failure.
    pub available: bool,
    /// The report as the client printed it, unmodified.
    pub report: Option<Value>,
    /// What the client said when it had no report.
    pub note: Option<String>,
}

async fn routecheck(ctx: &ToolContext, params: RoutecheckParams) -> ToolResult<Value> {
    let output = cli::run_tolerant(
        ctx,
        &metas::tailscale_routecheck,
        Invocation::read([
            "routecheck".to_owned(),
            "--format=json".to_owned(),
            flag("probe", params.probe),
        ]),
    )
    .await?;
    let document = serde_json::from_str::<Value>(output.stdout_str().trim()).ok();
    report(RoutecheckReport {
        available: document.is_some(),
        note: document
            .is_none()
            .then(|| note(ctx, &output.stderr))
            .flatten(),
        report: document,
    })
}

// ---------------------------------------------------------------------------
// wait
// ---------------------------------------------------------------------------

/// The longest one call will wait for the node to come up. The CLI's own
/// default is zero, which means forever.
const MAX_WAIT: u64 = 120;
const DEFAULT_WAIT: u64 = 30;

const fn default_wait() -> u64 {
    DEFAULT_WAIT
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WaitParams {
    /// How long to wait, in seconds. Capped at 120; there is no way to ask this
    /// tool to wait forever.
    #[serde(default = "default_wait")]
    pub timeout_seconds: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WaitReport {
    /// Whether the node's interface and addresses were ready in time.
    pub ready: bool,
    /// The bound that was actually applied, after capping.
    pub waited_up_to_seconds: u64,
    pub note: Option<String>,
}

async fn wait(ctx: &ToolContext, params: WaitParams) -> ToolResult<Value> {
    let (seconds, timeout) = bounded_wait(Some(params.timeout_seconds), MAX_WAIT, MAX_WAIT);
    let output = cli::run_tolerant(
        ctx,
        &metas::tailscale_wait,
        Invocation::read(["wait".to_owned(), format!("--timeout={seconds}s")])
            .with_timeout(timeout),
    )
    .await?;
    report(WaitReport {
        ready: output.success(),
        waited_up_to_seconds: seconds,
        note: note(ctx, &output.stderr),
    })
}

// ---------------------------------------------------------------------------
// dns
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DnsStatusParams {
    /// Include the forwarder's own debugging information.
    #[serde(default)]
    pub all: bool,
}

async fn dns_status(ctx: &ToolContext, params: DnsStatusParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_dns_status,
        Invocation::read([
            "dns".to_owned(),
            "status".to_owned(),
            "--json=true".to_owned(),
            flag("all", params.all),
        ]),
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DnsQueryParams {
    /// The name to resolve.
    pub name: String,
    /// The record type to ask for: `A`, `AAAA`, `CNAME`, `TXT` and so on.
    /// Defaults to whatever the client asks for when told nothing.
    #[serde(default)]
    pub record_type: Option<String>,
}

async fn dns_query(ctx: &ToolContext, params: DnsQueryParams) -> ToolResult<Value> {
    let mut args = vec![
        "dns".to_owned(),
        "query".to_owned(),
        "--json=true".to_owned(),
        params.name,
    ];
    if let Some(record_type) = params.record_type {
        args.push(record_type);
    }
    object(ctx, &metas::tailscale_dns_query, Invocation::read(args)).await
}

// ---------------------------------------------------------------------------
// exit nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExitNodeListParams {
    /// Only report exit nodes in this country.
    #[serde(default)]
    pub filter: Option<String>,
}

/// One row of the exit-node table.
#[derive(Debug, Serialize, JsonSchema, PartialEq)]
pub struct ExitNode {
    pub ip: String,
    pub hostname: String,
    /// `-` for a node in your own tailnet, which has no location.
    pub country: String,
    pub city: String,
    /// `selected` for the node in use, `-` otherwise.
    pub status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExitNodeListReport {
    /// Whether there were any exit nodes at all. None is an answer, and the
    /// client reports it by exiting non-zero.
    pub available: bool,
    pub exit_nodes: Vec<ExitNode>,
    pub note: Option<String>,
}

/// Read the exit-node table.
///
/// Five whitespace-separated columns with a header row; a hostname never
/// contains a space, so splitting on whitespace is sound.
fn parse_exit_nodes(text: &str) -> Vec<ExitNode> {
    lines(text)
        .filter(|line| !line.starts_with("IP "))
        .filter_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            let [ip, hostname, country, city, status] = columns[..] else {
                return None;
            };
            Some(ExitNode {
                ip: ip.to_owned(),
                hostname: hostname.to_owned(),
                country: country.to_owned(),
                city: city.to_owned(),
                status: status.to_owned(),
            })
        })
        .collect()
}

async fn exit_node_list(ctx: &ToolContext, params: ExitNodeListParams) -> ToolResult<Value> {
    let mut args = vec!["exit-node".to_owned(), "list".to_owned()];
    if let Some(filter) = params.filter {
        args.push(format!("--filter={filter}"));
    }
    let output = cli::run_tolerant(
        ctx,
        &metas::tailscale_exit_node_list,
        Invocation::read(args),
    )
    .await?;
    let exit_nodes = parse_exit_nodes(&output.stdout_str());
    report(ExitNodeListReport {
        available: !exit_nodes.is_empty(),
        note: note(ctx, &output.stderr),
        exit_nodes,
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExitNodeSuggestion {
    /// The node the client would pick, when it has one to offer.
    pub suggestion: Option<String>,
    pub raw: String,
}

async fn exit_node_suggest(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let output = cli::run_tolerant(
        ctx,
        &metas::tailscale_exit_node_suggest,
        Invocation::read(["exit-node", "suggest"]),
    )
    .await?;
    let text = output.stdout_str();
    report(ExitNodeSuggestion {
        suggestion: lines(&text).find_map(|line| {
            line.split_once("Suggested exit node:")
                .map(|(_, name)| name.trim().trim_end_matches('.').to_owned())
        }),
        raw: text.trim_end().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// metrics
// ---------------------------------------------------------------------------

/// One metric sample.
#[derive(Debug, Serialize, JsonSchema, PartialEq)]
pub struct Metric {
    pub name: String,
    /// The labels in the sample's braces, empty when it had none.
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MetricsReport {
    pub metrics: Vec<Metric>,
    /// The Prometheus text as it was printed, for anything that reads it
    /// directly.
    pub raw: String,
}

/// Read Prometheus exposition text.
///
/// Only the sample lines: `# TYPE` and `# HELP` are already dropped by
/// [`lines`], and the client emits no timestamps or exemplars.
fn parse_metrics(text: &str) -> Vec<Metric> {
    lines(text)
        .filter_map(|line| {
            let (key, value) = line.rsplit_once(char::is_whitespace)?;
            let value: f64 = value.trim().parse().ok()?;
            let (name, labels) = match key.trim().split_once('{') {
                Some((name, rest)) => (name, parse_labels(rest.trim_end_matches('}'))),
                None => (key.trim(), BTreeMap::new()),
            };
            Some(Metric {
                name: name.to_owned(),
                labels,
                value,
            })
        })
        .collect()
}

fn parse_labels(inner: &str) -> BTreeMap<String, String> {
    inner
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| {
            (
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            )
        })
        .collect()
}

async fn metrics_print(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let text = cli::run_text(
        ctx,
        &metas::tailscale_metrics_print,
        Invocation::read(["metrics", "print"]),
    )
    .await?;
    report(MetricsReport {
        metrics: parse_metrics(&text),
        raw: text.trim_end().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// services, policy, accounts
// ---------------------------------------------------------------------------

async fn service_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    collection(
        ctx,
        &metas::tailscale_service_list,
        Invocation::read(["service", "list", "--json=true"]),
        "services",
    )
    .await
}

async fn syspolicy_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_syspolicy_list,
        Invocation::read(["syspolicy", "list", "--json=true"]),
    )
    .await
}

async fn switch_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    collection(
        ctx,
        &metas::tailscale_switch_list,
        Invocation::read(["switch", "--list=true", "--json=true"]),
        "accounts",
    )
    .await
}

// ---------------------------------------------------------------------------
// tailnet lock
// ---------------------------------------------------------------------------

/// The client's own default, kept so that an unspecified call behaves the way
/// the command line does.
const DEFAULT_LOCK_LIMIT: u32 = 50;
const MAX_LOCK_LIMIT: u32 = 500;

const fn default_lock_limit() -> u32 {
    DEFAULT_LOCK_LIMIT
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LockLogParams {
    /// How many updates to report, newest first. Capped at 500.
    #[serde(default = "default_lock_limit")]
    pub limit: u32,
}

async fn lock_status(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_lock_status,
        Invocation::read(["lock", "status", "--json=true"]),
    )
    .await
}

async fn lock_log(ctx: &ToolContext, params: LockLogParams) -> ToolResult<Value> {
    let limit = params.limit.clamp(1, MAX_LOCK_LIMIT);
    collection(
        ctx,
        &metas::tailscale_lock_log,
        Invocation::read([
            "lock".to_owned(),
            "log".to_owned(),
            "--json=true".to_owned(),
            format!("--limit={limit}"),
        ]),
        "updates",
    )
    .await
}

// ---------------------------------------------------------------------------
// serve and funnel
// ---------------------------------------------------------------------------

async fn serve_status(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_serve_status,
        Invocation::read(["serve", "status", "--json=true"]),
    )
    .await
}

async fn funnel_status(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    object(
        ctx,
        &metas::tailscale_funnel_status,
        Invocation::read(["funnel", "status", "--json=true"]),
    )
    .await
}

// ---------------------------------------------------------------------------
// host configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
pub struct SysextReport {
    /// The state as the client words it, for example `OK (activated enabled)`.
    pub state: Option<String>,
    pub raw: String,
}

async fn configure_sysext_status(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let text = cli::run_text(
        ctx,
        &metas::tailscale_configure_sysext_status,
        Invocation::read(["configure", "sysext", "status"]),
    )
    .await?;
    report(SysextReport {
        state: lines(&text).find_map(|line| {
            line.split_once("System extension state:")
                .map(|(_, state)| state.trim().to_owned())
        }),
        raw: text.trim_end().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::context::{PathPolicy, SelfIdentity};
    use crate::error::{ErrorCode, Redactor};
    use crate::testing::{Reply, StubBackend};

    /// A recorded sample of what the real client prints.
    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../tests/fixtures/", $name))
        };
    }

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
        Fut: Future<Output = ToolResult<Value>>,
    {
        let backend = Arc::new(StubBackend::always(reply));
        let ctx = context(Arc::clone(&backend));
        let value = handler(ctx, params).await.expect("the handler succeeds");
        (value, backend.argv())
    }

    // -- what each tool runs --------------------------------------------------

    #[tokio::test]
    async fn status_asks_for_json_and_passes_its_switches_joined_to_their_values() {
        let (_, argv) = against(
            Reply::ok(fixture!("status.json")),
            |ctx, p| async move { status(&ctx, p).await },
            StatusParams {
                peers: false,
                include_self: true,
                active: true,
            },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "status",
                "--json=true",
                "--peers=false",
                "--self=true",
                "--active=true"
            ]]
        );
    }

    #[tokio::test]
    async fn the_status_document_is_forwarded_exactly_as_the_client_printed_it() {
        let (value, _) = against(
            Reply::ok(fixture!("status.json")),
            |ctx, p| async move { status(&ctx, p).await },
            StatusParams {
                peers: true,
                include_self: true,
                active: false,
            },
        )
        .await;
        let expected: Value = serde_json::from_str(fixture!("status.json")).unwrap();
        assert_eq!(value, expected);
    }

    #[tokio::test]
    async fn a_stopped_node_reports_its_state_rather_than_failing() {
        // `status` exits non-zero when the backend is not running, but the
        // document it prints is exactly what the caller asked for.
        let backend = Arc::new(StubBackend::always(Reply::Ran(tailscale_cli::Output {
            exit_code: Some(1),
            stdout: fixture!("status-stopped.json").as_bytes().to_vec(),
            stderr: "Tailscale is stopped.\n".to_owned(),
        })));
        let ctx = context(backend);
        let value = status(
            &ctx,
            StatusParams {
                peers: true,
                include_self: true,
                active: false,
            },
        )
        .await
        .expect("a stopped node is an answer, not a failure");
        assert_eq!(value["BackendState"], "Stopped");
    }

    #[tokio::test]
    async fn a_refusal_with_nothing_to_parse_is_still_a_failure() {
        let ctx = context(Arc::new(StubBackend::failure(1, "something broke")));
        let err = status(
            &ctx,
            StatusParams {
                peers: true,
                include_self: true,
                active: false,
            },
        )
        .await
        .expect_err("no document and a refusal is a failure");
        assert_eq!(err.code, ErrorCode::CliFailed);
    }

    #[tokio::test]
    async fn ip_selects_a_family_with_a_joined_flag_and_puts_the_target_last() {
        let (value, argv) = against(
            Reply::ok(fixture!("ip.txt")),
            |ctx, p| async move { ip(&ctx, p).await },
            IpParams {
                target: Some("laptop".to_owned()),
                family: AddressFamily::Ipv4,
            },
        )
        .await;
        assert_eq!(argv, [["ip", "--4=true", "laptop"]]);
        assert_eq!(value["addresses"][0], "100.64.0.1");
        assert_eq!(value["addresses"][1], "fd7a:115c:a1e0::1");
        assert_eq!(value["target"], "laptop");
    }

    #[tokio::test]
    async fn netcheck_uses_the_format_flag_it_predates_the_json_one_with() {
        let (value, argv) = against(
            Reply::ok(fixture!("netcheck.json")),
            |ctx, p| async move { netcheck(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(argv, [["netcheck", "--format=json"]]);
        assert_eq!(value["PreferredDERP"], 12);
    }

    #[tokio::test]
    async fn netcheck_ignores_the_log_lines_the_client_writes_to_standard_error() {
        let backend = Arc::new(StubBackend::always(Reply::Ran(tailscale_cli::Output {
            exit_code: Some(0),
            stdout: fixture!("netcheck.json").as_bytes().to_vec(),
            stderr: "2026/09/04 12:00:00 portmap: probing\n# Warning: unstable\n".to_owned(),
        })));
        let ctx = context(backend);
        let value = netcheck(&ctx, NoParams {}).await.expect("succeeds");
        assert_eq!(value["UDP"], true);
    }

    #[tokio::test]
    async fn whois_puts_its_flags_before_the_address() {
        let (value, argv) = against(
            Reply::ok(fixture!("whois.json")),
            |ctx, p| async move { whois(&ctx, p).await },
            WhoisParams {
                address: "100.64.0.2:22".to_owned(),
                proto: Some(Proto::Tcp),
            },
        )
        .await;
        assert_eq!(
            argv,
            [["whois", "--json=true", "--proto=tcp", "100.64.0.2:22"]]
        );
        assert_eq!(value["Node"]["ComputedName"], "laptop");
    }

    #[tokio::test]
    async fn dns_query_puts_the_record_type_after_the_name() {
        let (value, argv) = against(
            Reply::ok(fixture!("dns-query.json")),
            |ctx, p| async move { dns_query(&ctx, p).await },
            DnsQueryParams {
                name: "laptop.example-tailnet.ts.net".to_owned(),
                record_type: Some("A".to_owned()),
            },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "dns",
                "query",
                "--json=true",
                "laptop.example-tailnet.ts.net",
                "A"
            ]]
        );
        assert_eq!(value["ResponseCode"], "NOERROR");
    }

    #[tokio::test]
    async fn a_list_document_is_named_by_what_it_holds() {
        let (value, argv) = against(
            Reply::ok(fixture!("service-list.json")),
            |ctx, p| async move { service_list(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(argv, [["service", "list", "--json=true"]]);
        assert_eq!(value["services"][0]["Name"], "svc:web");
    }

    #[tokio::test]
    async fn an_empty_list_and_a_null_list_are_the_same_answer() {
        for printed in ["[]", "null"] {
            let (value, _) = against(
                Reply::ok(printed),
                |ctx, p| async move { switch_list(&ctx, p).await },
                NoParams {},
            )
            .await;
            assert_eq!(value["accounts"], json!([]), "printed {printed}");
        }
    }

    #[tokio::test]
    async fn the_lock_log_limit_is_capped_and_joined_to_its_flag() {
        let (_, argv) = against(
            Reply::ok(fixture!("lock-log.json")),
            |ctx, p| async move { lock_log(&ctx, p).await },
            LockLogParams { limit: 100_000 },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "lock",
                "log",
                "--json=true",
                &format!("--limit={MAX_LOCK_LIMIT}")
            ]]
        );
    }

    /// A caller that says nothing about the limit gets the client's own
    /// default, which is the number the schema advertises.
    #[tokio::test]
    async fn the_lock_log_limit_defaults_to_the_one_the_client_uses() {
        let params: LockLogParams =
            serde_json::from_value(json!({})).expect("an empty call parses");
        assert_eq!(params.limit, DEFAULT_LOCK_LIMIT);

        let (_, argv) = against(
            Reply::ok(fixture!("lock-log.json")),
            |ctx, p| async move { lock_log(&ctx, p).await },
            params,
        )
        .await;
        assert_eq!(
            argv,
            [[
                "lock",
                "log",
                "--json=true",
                &format!("--limit={DEFAULT_LOCK_LIMIT}")
            ]]
        );
    }

    // -- parsers --------------------------------------------------------------

    #[test]
    fn a_relayed_reply_and_a_direct_reply_are_told_apart() {
        let replies: Vec<PingReply> = lines(fixture!("ping.txt")).filter_map(parse_pong).collect();
        assert_eq!(
            replies,
            [
                PingReply {
                    peer: "laptop".to_owned(),
                    address: "100.64.0.2".to_owned(),
                    via: "DERP(sfo)".to_owned(),
                    latency_ms: Some(41.0),
                },
                PingReply {
                    peer: "laptop".to_owned(),
                    address: "100.64.0.2".to_owned(),
                    via: "203.0.113.9:41641".to_owned(),
                    latency_ms: Some(12.0),
                },
            ]
        );
        assert!(!replies[0].is_direct());
        assert!(replies[1].is_direct());
    }

    #[test]
    fn a_line_that_is_not_a_reply_is_not_read_as_one() {
        for line in [
            "no matching peer",
            "pong from laptop",
            "direct connection established",
            "",
        ] {
            assert!(parse_pong(line).is_none(), "{line}");
        }
    }

    #[tokio::test]
    async fn ping_honours_its_default_and_its_cap() {
        let (value, argv) = against(
            Reply::ok(fixture!("ping.txt")),
            |ctx, p| async move { ping(&ctx, p).await },
            PingParams {
                target: "laptop".to_owned(),
                count: default_ping_count(),
                timeout_seconds: default_ping_timeout(),
                method: PingMethod::Disco,
                until_direct: true,
            },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "ping",
                &format!("--c={DEFAULT_PING_COUNT}"),
                &format!("--timeout={DEFAULT_PING_TIMEOUT}s"),
                "--until-direct=true",
                "laptop"
            ]]
        );
        assert_eq!(value["reachable"], true);
        assert_eq!(value["direct"], true);
        assert_eq!(value["replies"].as_array().map(Vec::len), Some(2));

        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { ping(&ctx, p).await },
            PingParams {
                target: "laptop".to_owned(),
                count: 10_000,
                timeout_seconds: 10_000,
                method: PingMethod::Icmp,
                until_direct: false,
            },
        )
        .await;
        assert_eq!(
            argv,
            [[
                "ping",
                &format!("--c={MAX_PING_COUNT}"),
                &format!("--timeout={MAX_PING_TIMEOUT}s"),
                "--until-direct=false",
                "--icmp=true",
                "laptop"
            ]]
        );
    }

    #[tokio::test]
    async fn a_bounded_ping_bounds_the_process_too() {
        let backend = Arc::new(StubBackend::always(Reply::ok("")));
        let ctx = context(Arc::clone(&backend));
        ping(
            &ctx,
            PingParams {
                target: "laptop".to_owned(),
                count: 4,
                timeout_seconds: 3,
                method: PingMethod::Disco,
                until_direct: true,
            },
        )
        .await
        .expect("succeeds");
        let timeout = backend.calls()[0].timeout;
        assert_eq!(timeout, Duration::from_secs(4 * 3 + 5));
    }

    #[test]
    fn the_exit_node_table_is_read_without_its_header_or_its_footnotes() {
        let nodes = parse_exit_nodes(fixture!("exit-node-list.txt"));
        assert_eq!(
            nodes,
            [
                ExitNode {
                    ip: "100.64.0.2".to_owned(),
                    hostname: "laptop.example-tailnet.ts.net".to_owned(),
                    country: "-".to_owned(),
                    city: "-".to_owned(),
                    status: "-".to_owned(),
                },
                ExitNode {
                    ip: "100.64.0.3".to_owned(),
                    hostname: "se-sto-wg-001.example-tailnet.ts.net".to_owned(),
                    country: "Sweden".to_owned(),
                    city: "Stockholm".to_owned(),
                    status: "selected".to_owned(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn no_exit_nodes_is_an_answer_rather_than_a_failure() {
        let ctx = context(Arc::new(StubBackend::failure(1, "no exit nodes found\n")));
        let value = exit_node_list(&ctx, ExitNodeListParams { filter: None })
            .await
            .expect("an empty tailnet is an answer");
        assert_eq!(value["available"], false);
        assert_eq!(value["exit_nodes"], json!([]));
        assert_eq!(value["note"], "no exit nodes found");
    }

    #[tokio::test]
    async fn a_suggestion_is_lifted_out_of_the_sentence_it_arrives_in() {
        let (value, argv) = against(
            Reply::ok(fixture!("exit-node-suggest.txt")),
            |ctx, p| async move { exit_node_suggest(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(argv, [["exit-node", "suggest"]]);
        assert_eq!(value["suggestion"], "laptop.example-tailnet.ts.net");

        let (value, _) = against(
            Reply::ok("No exit node suggestion is available.\n"),
            |ctx, p| async move { exit_node_suggest(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(value["suggestion"], Value::Null);
    }

    #[test]
    fn prometheus_samples_are_read_with_their_labels() {
        let metrics = parse_metrics(fixture!("metrics.txt"));
        assert_eq!(
            metrics[0],
            Metric {
                name: "tailscaled_advertised_routes".to_owned(),
                labels: BTreeMap::new(),
                value: 0.0,
            }
        );
        let inbound: Vec<&Metric> = metrics
            .iter()
            .filter(|m| m.name == "tailscaled_inbound_bytes_total")
            .collect();
        assert_eq!(inbound.len(), 2);
        assert_eq!(inbound[0].labels["path"], "derp");
        assert_eq!(inbound[0].value, 4096.0);
        assert_eq!(inbound[1].labels["path"], "direct_ipv4");
        // The `# TYPE` and `# HELP` lines are not samples.
        assert!(
            metrics.iter().all(|m| !m.name.starts_with('#')),
            "{metrics:?}"
        );
    }

    #[tokio::test]
    async fn a_node_that_is_not_an_app_connector_says_so_and_reports_no_routes() {
        let (value, argv) = against(
            Reply::ok("not a connector\n"),
            |ctx, p| async move { appc_routes(&ctx, p).await },
            AppcRoutesParams {
                all: false,
                map: false,
                count_only: false,
            },
        )
        .await;
        assert_eq!(
            argv,
            [["appc-routes", "--all=false", "--map=false", "--n=false"]]
        );
        assert_eq!(value["app_connector"], false);
        assert_eq!(value["routes"], json!([]));
    }

    #[tokio::test]
    async fn an_app_connector_reports_the_routes_it_has_learnt() {
        let (value, _) = against(
            Reply::ok(fixture!("appc-routes.txt")),
            |ctx, p| async move { appc_routes(&ctx, p).await },
            AppcRoutesParams {
                all: true,
                map: false,
                count_only: false,
            },
        )
        .await;
        assert_eq!(value["app_connector"], true);
        assert_eq!(value["routes"], json!(["192.0.2.0/24", "198.51.100.0/24"]));
    }

    #[tokio::test]
    async fn a_routecheck_with_no_report_yet_is_a_state_not_a_failure() {
        let ctx = context(Arc::new(StubBackend::failure(
            1,
            "routecheck: report pending\n",
        )));
        let value = routecheck(&ctx, RoutecheckParams { probe: false })
            .await
            .expect("a pending report is an answer");
        assert_eq!(value["available"], false);
        assert_eq!(value["report"], Value::Null);
        assert_eq!(value["note"], "routecheck: report pending");
    }

    #[tokio::test]
    async fn a_routecheck_report_is_forwarded_whole() {
        let (value, argv) = against(
            Reply::ok(fixture!("routecheck.json")),
            |ctx, p| async move { routecheck(&ctx, p).await },
            RoutecheckParams { probe: true },
        )
        .await;
        assert_eq!(argv, [["routecheck", "--format=json", "--probe=true"]]);
        assert_eq!(value["available"], true);
        let expected: Value = serde_json::from_str(fixture!("routecheck.json")).unwrap();
        assert_eq!(value["report"], expected);
    }

    #[tokio::test]
    async fn waiting_is_bounded_and_a_timeout_is_reported_rather_than_raised() {
        let (value, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { wait(&ctx, p).await },
            WaitParams {
                timeout_seconds: 10_000,
            },
        )
        .await;
        assert_eq!(argv, [["wait", &format!("--timeout={MAX_WAIT}s")]]);
        assert_eq!(value["ready"], true);
        assert_eq!(value["waited_up_to_seconds"], MAX_WAIT);

        let ctx = context(Arc::new(StubBackend::failure(1, "timeout\n")));
        let value = wait(&ctx, WaitParams { timeout_seconds: 5 })
            .await
            .expect("a timeout is an answer");
        assert_eq!(value["ready"], false);
        assert_eq!(value["note"], "timeout");
    }

    #[tokio::test]
    async fn the_bug_report_marker_is_the_last_thing_printed() {
        let (value, argv) = against(
            Reply::ok(fixture!("bugreport.txt")),
            |ctx, p| async move { bugreport(&ctx, p).await },
            BugreportParams {
                note: Some("slow handshake".to_owned()),
                diagnose: true,
            },
        )
        .await;
        assert_eq!(argv, [["bugreport", "--diagnose=true", "slow handshake"]]);
        assert_eq!(value["marker"], fixture!("bugreport.txt").trim());
    }

    #[tokio::test]
    async fn the_licence_url_is_lifted_out_of_the_prose_around_it() {
        let (value, _) = against(
            Reply::ok(fixture!("licenses.txt")),
            |ctx, p| async move { licenses(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(
            value["urls"],
            json!(["https://tailscale.com/licenses/apple"])
        );
    }

    #[tokio::test]
    async fn the_system_extension_state_is_lifted_out_of_its_sentence() {
        let (value, argv) = against(
            Reply::ok(fixture!("sysext-status.txt")),
            |ctx, p| async move { configure_sysext_status(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(argv, [["configure", "sysext", "status"]]);
        assert_eq!(value["state"], "OK (activated enabled)");
    }

    // -- the table ------------------------------------------------------------

    #[test]
    fn every_tool_here_is_a_read_in_the_status_toolset() {
        for entry in entries() {
            assert_eq!(
                entry.meta.tier,
                crate::meta::Tier::Read,
                "`{}` changes something and does not belong here",
                entry.meta.name
            );
            assert_eq!(
                entry.meta.toolset,
                crate::meta::Toolset::LocalStatus,
                "{}",
                entry.meta.name
            );
            assert!(
                !entry.meta.requires_confirmation,
                "`{}` reads, so nothing needs confirming",
                entry.meta.name
            );
        }
    }

    #[test]
    fn the_toolset_holds_every_read_only_local_command_it_claims_to() {
        // Ticket 08 counts 25. A tool arriving here without being counted, or a
        // tool moving out without the count moving, is a drift worth catching.
        assert_eq!(entries().len(), 25);
    }

    #[test]
    fn only_the_commands_that_do_not_exist_everywhere_are_restricted() {
        let restricted: Vec<&str> = entries()
            .iter()
            .filter(|e| e.meta.platforms.is_some())
            .map(|e| e.meta.name)
            .collect();
        assert_eq!(restricted, ["tailscale_configure_sysext_status"]);
    }
}
