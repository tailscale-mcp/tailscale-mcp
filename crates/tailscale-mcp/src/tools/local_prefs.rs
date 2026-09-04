//! Changing what the local node is and where it is connected.
//!
//! Eight commands, and one idea running through them: the node this server
//! runs on is very often the node the caller is reaching us over. Every tool
//! here can end that conversation — `down` and `logout` by design, `up` and
//! `login` by re-authenticating or by restating a preference set that omits
//! something, `switch` by changing which account the connection belongs to. So
//! all but the two that only read are self-severing and refuse without
//! `confirm: true`, and the CLI's own risk acceptance is passed only on a call
//! that carried one (DECISIONS Q17).
//!
//! The division of labour between `tailscale_prefs_set` and `tailscale_up` is
//! the other thing to know. `up` applies a whole preference set: anything not
//! restated goes back to its default, which is why it is destructive and why
//! its description sends a caller who only wants to change one thing to
//! `tailscale_prefs_set`. `set` changes exactly the preferences it is given and
//! nothing else.

use std::time::Duration;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_cli::{Invocation, SecretFile};

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};

crate::tools! {
    /// Show the node's current preferences: one setting by name, or all of
    /// them. Reads only; use `tailscale_prefs_set` to change anything.
    tailscale_prefs_get => PrefsGetParams, prefs_get,
        toolset: LocalPrefs, tier: Read, idempotent: true, since: "1.90";

    /// Change only the preferences named here, leaving every other preference
    /// as it is. This is the tool to reach for on a node that is already
    /// connected: unlike `tailscale_up` it does not restate the whole
    /// preference set, so it cannot reset something by omitting it.
    tailscale_prefs_set => PrefsSetParams, prefs_set,
        toolset: LocalPrefs, tier: Write, idempotent: true;

    /// Connect this node to the tailnet, logging in if it is not already, and
    /// apply the preferences given here.
    ///
    /// This applies a *whole* preference set: anything not restated goes back
    /// to its default. On a node that is already connected, use
    /// `tailscale_prefs_set` instead unless you mean to reset the rest. Without
    /// an authentication key the client prints a login URL and waits, so the
    /// call is bounded by `timeout_seconds` and returns the URL when it has
    /// one.
    tailscale_up => UpParams, up,
        toolset: LocalPrefs, tier: Destructive, severing: true;

    /// Disconnect this node from the tailnet. The node stays logged in and can
    /// be reconnected with `tailscale_up`.
    tailscale_down => DownParams, down,
        toolset: LocalPrefs, tier: Destructive, severing: true;

    /// Log in, adding a new account profile if this node is already logged in
    /// to another. Prints a login URL and waits when no authentication key is
    /// given, bounded by `timeout_seconds`.
    tailscale_login => LoginParams, login,
        toolset: LocalPrefs, tier: Write, severing: true;

    /// Log out and expire this node's key. The node disconnects, and coming
    /// back needs a fresh login rather than a reconnect.
    tailscale_logout => LogoutParams, logout,
        toolset: LocalPrefs, tier: Destructive, severing: true;

    /// Switch this machine to one of the other accounts stored on it, which
    /// restarts the connection under that account's identity. Use
    /// `tailscale_switch_list` to see the accounts and their ids.
    tailscale_switch_profile => SwitchParams, switch_profile,
        toolset: LocalPrefs, tier: Write, severing: true;

    /// Forget one of the accounts stored on this machine. The account itself is
    /// untouched; it is logging in again that brings it back.
    tailscale_switch_remove => SwitchRemoveParams, switch_remove,
        toolset: LocalPrefs, tier: Destructive, confirm: true;
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// How long a connect or a login waits by default.
const DEFAULT_CONNECT_TIMEOUT: u64 = 60;
/// The longest either may be asked to wait. The CLI's own default is to wait
/// for ever, which an agent cannot recover from.
const MAX_CONNECT_TIMEOUT: u64 = 300;
/// What the CLI is given beyond the caller's own bound, so that the command
/// gets to report its timeout rather than being killed mid-sentence.
const GRACE: u64 = 5;

/// The risks the CLI asks about before it disconnects something: losing an SSH
/// session it is carrying, or taking down a macOS app connector. A tool that
/// requires `confirm: true` has already asked the caller that question, so the
/// answer is passed on rather than left for a prompt nobody can see.
const ACCEPTED_RISKS: &str = "--accept-risk=all";

// ---------------------------------------------------------------------------
// Preference parameters
// ---------------------------------------------------------------------------

/// The operating systems each platform-specific preference exists on.
const LINUX_ONLY: &[&str] = &["linux"];
const WINDOWS_ONLY: &[&str] = &["windows"];

/// How Tailscale manages the Linux firewall.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetfilterMode {
    /// Tailscale manages its own netfilter rules.
    On,
    /// Tailscale adds rules but does not divert traffic into them.
    Nodivert,
    /// Tailscale manages nothing; the rules are yours to write.
    Off,
}

impl NetfilterMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Nodivert => "nodivert",
            Self::Off => "off",
        }
    }
}

/// Declare a parameter struct carrying the preferences that `set`, `up` and
/// `login` all understand, plus whatever else that command takes.
///
/// A macro rather than a shared struct behind `#[serde(flatten)]` because a
/// flattened schema is a composition rather than a plain object, and every one
/// of these tools is better served by a schema a client can read as a list of
/// named settings.
macro_rules! prefs_params {
    (
        $(#[doc = $doc:literal])*
        $name:ident { $( $(#[doc = $field_doc:literal])* $field:ident : $ty:ty ),* $(,)? }
    ) => {
        $(#[doc = $doc])*
        #[derive(Debug, Default, Deserialize, JsonSchema)]
        pub struct $name {
            /// Accept the DNS configuration the tailnet publishes, including
            /// MagicDNS.
            #[serde(default)]
            pub accept_dns: Option<bool>,
            /// Accept the subnet routes other nodes advertise.
            #[serde(default)]
            pub accept_routes: Option<bool>,
            /// Advertise this node as an app connector.
            #[serde(default)]
            pub advertise_connector: Option<bool>,
            /// Offer this node to the tailnet as an exit node. It still has to
            /// be approved in the admin console before anyone can use it.
            #[serde(default)]
            pub advertise_exit_node: Option<bool>,
            /// The subnet routes this node offers, as CIDR blocks. An empty
            /// list withdraws every route it was advertising.
            #[serde(default)]
            pub advertise_routes: Option<Vec<String>>,
            /// The exit node to send traffic through: an address, a base
            /// hostname, or `auto:any` to let the client choose. An empty
            /// string stops using one.
            #[serde(default)]
            pub exit_node: Option<String>,
            /// Keep reaching the local network directly while an exit node is
            /// in use.
            #[serde(default)]
            pub exit_node_allow_lan_access: Option<bool>,
            /// The name this node goes by on the tailnet, overriding the
            /// machine's own hostname.
            #[serde(default)]
            pub hostname: Option<String>,
            /// The Unix user allowed to run `tailscale` without elevation.
            #[serde(default)]
            pub operator: Option<String>,
            /// Allow device posture information to be collected from this node.
            #[serde(default)]
            pub report_posture: Option<bool>,
            /// Block all incoming connections from the tailnet.
            #[serde(default)]
            pub shields_up: Option<bool>,
            /// Run the Tailscale SSH server on this node.
            #[serde(default)]
            pub ssh: Option<bool>,
            /// Linux only. Source-NAT traffic arriving for an advertised
            /// subnet route.
            #[serde(default)]
            pub snat_subnet_routes: Option<bool>,
            /// Linux only. Drop forwarded traffic that does not belong to a
            /// known connection.
            #[serde(default)]
            pub stateful_filtering: Option<bool>,
            /// Linux only. How much of the firewall Tailscale manages.
            #[serde(default)]
            pub netfilter_mode: Option<NetfilterMode>,
            /// Windows only. Keep the tailnet connection up when no user is
            /// signed in.
            #[serde(default)]
            pub unattended: Option<bool>,
            $(
                $(#[doc = $field_doc])*
                #[serde(default)]
                pub $field: $ty,
            )*
        }

        impl $name {
            /// The flags for the preferences every one of these commands takes.
            fn shared_flags(&self) -> Vec<String> {
                let mut args = Vec::new();
                push_bool(&mut args, "accept-dns", self.accept_dns);
                push_bool(&mut args, "accept-routes", self.accept_routes);
                push_bool(&mut args, "advertise-connector", self.advertise_connector);
                push_bool(&mut args, "advertise-exit-node", self.advertise_exit_node);
                push_list(&mut args, "advertise-routes", self.advertise_routes.as_deref());
                push_text(&mut args, "exit-node", self.exit_node.as_deref());
                push_bool(
                    &mut args,
                    "exit-node-allow-lan-access",
                    self.exit_node_allow_lan_access,
                );
                push_text(&mut args, "hostname", self.hostname.as_deref());
                push_text(&mut args, "operator", self.operator.as_deref());
                push_bool(&mut args, "report-posture", self.report_posture);
                push_bool(&mut args, "shields-up", self.shields_up);
                push_bool(&mut args, "ssh", self.ssh);
                args
            }

            /// The flags for the preferences that exist on one platform only,
            /// refused here rather than by a binary that has never heard of
            /// them.
            fn platform_flags(&self) -> ToolResult<Vec<String>> {
                let mut args = Vec::new();
                if let Some(value) = self.snat_subnet_routes {
                    only_on("snat_subnet_routes", LINUX_ONLY)?;
                    args.push(flag("snat-subnet-routes", value));
                }
                if let Some(value) = self.stateful_filtering {
                    only_on("stateful_filtering", LINUX_ONLY)?;
                    args.push(flag("stateful-filtering", value));
                }
                if let Some(mode) = self.netfilter_mode {
                    only_on("netfilter_mode", LINUX_ONLY)?;
                    args.push(format!("--netfilter-mode={}", mode.as_str()));
                }
                if let Some(value) = self.unattended {
                    only_on("unattended", WINDOWS_ONLY)?;
                    args.push(flag("unattended", value));
                }
                Ok(args)
            }
        }
    };
}

prefs_params! {
    /// The preferences to change, and nothing else.
    PrefsSetParams {
        /// Apply Tailscale updates automatically as they are released.
        auto_update: Option<bool>,
        /// Be told when an update is available.
        update_check: Option<bool>,
        /// Serve the node's own web interface to the tailnet on port 5252.
        webclient: Option<bool>,
        /// A short name for this account profile on this machine.
        nickname: Option<String>,
        /// The UDP port this node listens on as a peer relay. `0` picks one at
        /// random, and an empty string turns the relay off.
        relay_server_port: Option<String>,
        /// Fixed `address:port` endpoints to advertise as a peer relay, for a
        /// node whose own view of its address is wrong.
        relay_server_static_endpoints: Option<Vec<String>>,
    }
}

prefs_params! {
    /// The whole preference set to connect with. Anything left out returns to
    /// its default.
    UpParams {
        /// ACL tags to claim for this node, with or without the `tag:` prefix.
        advertise_tags: Option<Vec<String>>,
        /// An authentication key, so the node can log in without a browser.
        /// Either the key itself or `file:<path>` to read it from a file. A key
        /// given here is written to a private temporary file and passed by
        /// reference: it never appears in the command line.
        auth_key: Option<String>,
        /// A control server other than Tailscale's own.
        login_server: Option<String>,
        /// Seconds to wait for the node to come up before giving up. Capped at
        /// 300; the client's own default is to wait for ever.
        timeout_seconds: Option<u64>,
        /// Log in again even though the node is already authenticated. Drops
        /// the connection while it happens.
        force_reauth: Option<bool>,
        /// Return every preference not named here to its default, rather than
        /// leaving the client to complain that they were not restated.
        reset: Option<bool>,
    }
}

prefs_params! {
    /// The identity to log in as, and the preferences to log in with.
    LoginParams {
        /// ACL tags to claim for this node, with or without the `tag:` prefix.
        advertise_tags: Option<Vec<String>>,
        /// An authentication key, so the node can log in without a browser.
        /// Either the key itself or `file:<path>` to read it from a file. A key
        /// given here is written to a private temporary file and passed by
        /// reference: it never appears in the command line.
        auth_key: Option<String>,
        /// A control server other than Tailscale's own.
        login_server: Option<String>,
        /// A short name for the account profile this login creates.
        nickname: Option<String>,
        /// Seconds to wait for the login to complete before giving up. Capped
        /// at 300; the client's own default is to wait for ever.
        timeout_seconds: Option<u64>,
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Render a boolean flag joined to its value, which is what Go's flag package
/// needs to tell it from a positional argument.
fn flag(name: &str, value: bool) -> String {
    format!("--{name}={value}")
}

fn push_bool(args: &mut Vec<String>, name: &str, value: Option<bool>) {
    if let Some(value) = value {
        args.push(flag(name, value));
    }
}

fn push_text(args: &mut Vec<String>, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        args.push(format!("--{name}={value}"));
    }
}

/// A list flag. The CLI takes these comma-separated, and an empty list is an
/// empty value, which is how a route set or an endpoint list is withdrawn.
fn push_list(args: &mut Vec<String>, name: &str, value: Option<&[String]>) {
    if let Some(values) = value {
        args.push(format!("--{name}={}", values.join(",")));
    }
}

/// Refuse a preference that does not exist on the machine we are on.
///
/// Before spawning, so that the answer names the setting and the platform
/// rather than repeating a Go flag-parsing error about a flag that will never
/// exist here.
fn only_on(setting: &str, platforms: &[&str]) -> ToolResult<()> {
    if platforms.contains(&std::env::consts::OS) {
        return Ok(());
    }
    Err(ToolError::new(
        ErrorCode::UnsupportedPlatform,
        format!(
            "`{setting}` is a {} preference, and this node runs {}",
            platforms.join(" or "),
            std::env::consts::OS
        ),
    ))
}

/// Bound a caller's wait, and give the CLI a little longer than we wait for it.
fn connect_timeouts(requested: Option<u64>) -> (u64, Duration) {
    let seconds = requested
        .unwrap_or(DEFAULT_CONNECT_TIMEOUT)
        .clamp(1, MAX_CONNECT_TIMEOUT);
    (seconds, Duration::from_secs(seconds + GRACE))
}

/// A secret on its way to the CLI, held open for as long as the call takes.
///
/// A value that is already a `file:` reference is passed through: it is a path,
/// not a secret, and re-copying it would gain nothing. Anything else is written
/// to a private temporary file, so that the key itself never reaches an
/// argument list that `ps` can read.
fn secret_argument(name: &str, value: &str) -> ToolResult<(String, Option<SecretFile>)> {
    if value.starts_with("file:") {
        return Ok((format!("--{name}={value}"), None));
    }
    let file = SecretFile::new(value).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("the {name} could not be written to a private file: {e}"),
        )
    })?;
    Ok((format!("--{name}={}", file.arg()), Some(file)))
}

/// Whatever the command said on standard error, redacted, when it said
/// anything. `up` and `login` talk to a person there.
fn note(ctx: &ToolContext, stderr: &str) -> Option<String> {
    let redacted = ctx.redactor.apply(stderr);
    let trimmed = redacted.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Turn a serialisable report into the value a tool answers with.
fn report(value: impl Serialize) -> ToolResult<Value> {
    serde_json::to_value(value).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("the report did not build: {e}"),
        )
    })
}

// ---------------------------------------------------------------------------
// prefs get
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PrefsGetParams {
    /// One preference by name, using the same names this server's parameters
    /// use with underscores or the CLI's own with dashes. Omit it for all of
    /// them.
    #[serde(default)]
    pub setting: Option<String>,
    /// Answer with the `tailscale set` flags that would reproduce the current
    /// preferences, instead of with the preference document. Useful for
    /// recording a node's configuration before changing it.
    #[serde(default)]
    pub as_set_flags: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PrefsReport {
    /// The preference that was asked about, or null for all of them.
    pub setting: Option<String>,
    /// The preference document, when the flag rendering was not asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Value>,
    /// The `tailscale set` flags that reproduce these preferences, when that
    /// rendering was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_flags: Option<String>,
}

async fn prefs_get(ctx: &ToolContext, params: PrefsGetParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_prefs_get;
    let mut args = vec!["get".to_owned()];
    args.push(flag(
        if params.as_set_flags {
            "set-flags"
        } else {
            "json"
        },
        true,
    ));
    // Flags first: Go stops parsing them at the first positional.
    if let Some(setting) = &params.setting {
        args.push(setting.replace('_', "-"));
    }
    let text = cli::run_text(ctx, meta, Invocation::read(args)).await?;
    if params.as_set_flags {
        return report(PrefsReport {
            setting: params.setting,
            preferences: None,
            set_flags: Some(text.trim().to_owned()),
        });
    }
    let preferences = serde_json::from_str::<Value>(text.trim()).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("`tailscale get` did not print JSON: {e}"),
        )
    })?;
    report(PrefsReport {
        setting: params.setting,
        preferences: Some(preferences),
        set_flags: None,
    })
}

// ---------------------------------------------------------------------------
// prefs set
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
pub struct PrefsSetReport {
    /// The flags that were applied, in the order they were passed. Exactly the
    /// preferences that were named: everything else was left alone.
    pub applied: Vec<String>,
    /// Anything the client said while applying them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

async fn prefs_set(ctx: &ToolContext, params: PrefsSetParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_prefs_set;
    let mut flags = params.shared_flags();
    flags.extend(params.platform_flags()?);
    push_bool(&mut flags, "auto-update", params.auto_update);
    push_bool(&mut flags, "update-check", params.update_check);
    push_bool(&mut flags, "webclient", params.webclient);
    push_text(&mut flags, "nickname", params.nickname.as_deref());
    push_text(
        &mut flags,
        "relay-server-port",
        params.relay_server_port.as_deref(),
    );
    push_list(
        &mut flags,
        "relay-server-static-endpoints",
        params.relay_server_static_endpoints.as_deref(),
    );
    if flags.is_empty() {
        return Err(ToolError::invalid_args(
            "name at least one preference to change; `tailscale_prefs_set` \
             changes only what it is given",
        ));
    }

    let mut args = vec!["set".to_owned()];
    args.extend(flags.iter().cloned());
    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    report(PrefsSetReport {
        applied: flags,
        note: note(ctx, &output.stderr),
    })
}

// ---------------------------------------------------------------------------
// up
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
pub struct ConnectReport {
    /// The `tailscale up --json` document when the client printed one; the
    /// backend state and, for an interactive login, the URL to visit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Value>,
    /// The URL a person has to open to finish an interactive login, when there
    /// is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_url: Option<String>,
    /// How long the call was allowed to wait.
    pub timeout_seconds: u64,
    /// Anything the client said while connecting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

async fn up(ctx: &ToolContext, params: UpParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_up;
    let (seconds, budget) = connect_timeouts(params.timeout_seconds);

    let mut args = vec!["up".to_owned(), flag("json", true)];
    args.extend(params.shared_flags());
    args.extend(params.platform_flags()?);
    push_list(
        &mut args,
        "advertise-tags",
        params.advertise_tags.as_deref(),
    );
    push_text(&mut args, "login-server", params.login_server.as_deref());
    push_bool(&mut args, "force-reauth", params.force_reauth);
    push_bool(&mut args, "reset", params.reset);
    args.push(format!("--timeout={seconds}s"));
    // The caller confirmed this call, which is the same question the client
    // asks interactively.
    args.push(ACCEPTED_RISKS.to_owned());

    // Held until the call returns, so the file outlives the child that reads it.
    let mut key = None;
    if let Some(value) = &params.auth_key {
        let (argument, file) = secret_argument("auth-key", value)?;
        args.push(argument);
        key = file;
    }

    let output = cli::run(ctx, meta, Invocation::mutate(args).with_timeout(budget)).await?;
    drop(key);
    let stdout = output.stdout_str();
    let state = serde_json::from_str::<Value>(stdout.trim()).ok();
    let login_url = state
        .as_ref()
        .and_then(|s| s["AuthURL"].as_str().map(str::to_owned))
        .filter(|url| !url.is_empty())
        .or_else(|| find_url(&stdout));
    report(ConnectReport {
        state,
        login_url,
        timeout_seconds: seconds,
        note: note(ctx, &output.stderr),
    })
}

/// The first URL in a block of text, which is how `login` and a non-JSON `up`
/// hand over an interactive login.
fn find_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("https://"))
        .map(|url| url.trim_end_matches(['.', ',']).to_owned())
}

// ---------------------------------------------------------------------------
// down
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DownParams {
    /// Why the node is being disconnected, for tailnets whose policy asks for
    /// a reason.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DisconnectReport {
    /// What was done, so the answer reads the same whether or not the client
    /// had anything to say.
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

async fn down(ctx: &ToolContext, params: DownParams) -> ToolResult<Value> {
    let mut args = vec!["down".to_owned(), ACCEPTED_RISKS.to_owned()];
    push_text(&mut args, "reason", params.reason.as_deref());
    let output = cli::run(ctx, &metas::tailscale_down, Invocation::mutate(args)).await?;
    report(DisconnectReport {
        outcome: "disconnected",
        note: note(ctx, &output.stderr),
    })
}

// ---------------------------------------------------------------------------
// login
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
pub struct LoginReport {
    /// The URL a person has to open to finish the login, when the client
    /// printed one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_url: Option<String>,
    /// How long the call was allowed to wait.
    pub timeout_seconds: u64,
    /// What the client printed, which is where the login instructions are.
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

async fn login(ctx: &ToolContext, params: LoginParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_login;
    let (seconds, budget) = connect_timeouts(params.timeout_seconds);

    let mut args = vec!["login".to_owned()];
    args.extend(params.shared_flags());
    args.extend(params.platform_flags()?);
    push_list(
        &mut args,
        "advertise-tags",
        params.advertise_tags.as_deref(),
    );
    push_text(&mut args, "login-server", params.login_server.as_deref());
    push_text(&mut args, "nickname", params.nickname.as_deref());
    args.push(format!("--timeout={seconds}s"));

    let mut key = None;
    if let Some(value) = &params.auth_key {
        let (argument, file) = secret_argument("auth-key", value)?;
        args.push(argument);
        key = file;
    }

    let output = cli::run(ctx, meta, Invocation::mutate(args).with_timeout(budget)).await?;
    drop(key);
    let printed = ctx.redactor.apply(&output.stdout_str()).trim().to_owned();
    report(LoginReport {
        login_url: find_url(&printed),
        timeout_seconds: seconds,
        output: printed,
        note: note(ctx, &output.stderr),
    })
}

// ---------------------------------------------------------------------------
// logout
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogoutParams {
    /// Why the node is being logged out, for tailnets whose policy asks for a
    /// reason.
    #[serde(default)]
    pub reason: Option<String>,
}

async fn logout(ctx: &ToolContext, params: LogoutParams) -> ToolResult<Value> {
    let mut args = vec!["logout".to_owned()];
    push_text(&mut args, "reason", params.reason.as_deref());
    let output = cli::run(ctx, &metas::tailscale_logout, Invocation::mutate(args)).await?;
    report(DisconnectReport {
        outcome: "logged out",
        note: note(ctx, &output.stderr),
    })
}

// ---------------------------------------------------------------------------
// switch
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SwitchParams {
    /// Which account to switch to: the id from `tailscale_switch_list`, or the
    /// tailnet, account or display name.
    pub account: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SwitchReport {
    /// What was asked for, echoed so the answer says which account it was.
    pub account: String,
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

async fn switch_profile(ctx: &ToolContext, params: SwitchParams) -> ToolResult<Value> {
    let output = cli::run(
        ctx,
        &metas::tailscale_switch_profile,
        Invocation::mutate(["switch".to_owned(), params.account.clone()]),
    )
    .await?;
    report(SwitchReport {
        account: params.account,
        outcome: "switched",
        note: note(ctx, &output.stderr),
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SwitchRemoveParams {
    /// Which stored account to forget: the id from `tailscale_switch_list`, or
    /// the tailnet, account or display name.
    pub account: String,
}

async fn switch_remove(ctx: &ToolContext, params: SwitchRemoveParams) -> ToolResult<Value> {
    let output = cli::run(
        ctx,
        &metas::tailscale_switch_remove,
        Invocation::mutate([
            "switch".to_owned(),
            "remove".to_owned(),
            params.account.clone(),
        ]),
    )
    .await?;
    report(SwitchReport {
        account: params.account,
        outcome: "removed",
        note: note(ctx, &output.stderr),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::context::SelfIdentity;
    use crate::error::Redactor;
    use crate::meta::{Tier, Toolset};
    use crate::testing::{Reply, StubBackend};

    /// A recorded sample of what the real client prints.
    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../tests/fixtures/", $name))
        };
    }

    /// An authentication key of the right shape and no value, so that a test
    /// can look for it in an argument list without ever holding a real one.
    const FAKE_KEY: &str = "tskey-auth-example-notarealkey";

    fn context(backend: Arc<StubBackend>) -> ToolContext {
        ToolContext {
            local: backend as Arc<dyn tailscale_cli::LocalBackend>,
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity::default(),
            cli_version: None,
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

    /// The argument list of the single command a handler ran.
    fn only(argv: &[Vec<String>]) -> &[String] {
        assert_eq!(argv.len(), 1, "one command should have run: {argv:?}");
        &argv[0]
    }

    // -- the preference set ---------------------------------------------------

    #[tokio::test]
    async fn setting_one_preference_names_only_that_preference() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { prefs_set(&ctx, p).await },
            PrefsSetParams {
                hostname: Some("workstation".to_owned()),
                ..PrefsSetParams::default()
            },
        )
        .await;
        assert_eq!(only(&argv), ["set", "--hostname=workstation"]);
        assert_eq!(answer["applied"], json!(["--hostname=workstation"]));
    }

    #[tokio::test]
    async fn every_preference_that_was_given_is_passed_and_no_other() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { prefs_set(&ctx, p).await },
            PrefsSetParams {
                accept_routes: Some(true),
                advertise_routes: Some(vec![
                    "192.0.2.0/24".to_owned(),
                    "198.51.100.0/24".to_owned(),
                ]),
                exit_node: Some(String::new()),
                auto_update: Some(false),
                ..PrefsSetParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            [
                "set",
                "--accept-routes=true",
                "--advertise-routes=192.0.2.0/24,198.51.100.0/24",
                "--exit-node=",
                "--auto-update=false",
            ]
        );
    }

    #[tokio::test]
    async fn a_call_that_names_no_preference_is_refused_rather_than_run() {
        let backend = Arc::new(StubBackend::always(Reply::ok("")));
        let ctx = context(Arc::clone(&backend));
        let error = prefs_set(&ctx, PrefsSetParams::default())
            .await
            .expect_err("nothing to change is a mistake, not a no-op");
        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert!(backend.argv().is_empty(), "nothing should have run");
    }

    // -- platform-specific preferences ---------------------------------------

    #[tokio::test]
    async fn a_preference_from_another_operating_system_is_refused_before_anything_runs() {
        let backend = Arc::new(StubBackend::always(Reply::ok("")));
        let ctx = context(Arc::clone(&backend));
        let params = PrefsSetParams {
            netfilter_mode: Some(NetfilterMode::Nodivert),
            ..PrefsSetParams::default()
        };
        if cfg!(target_os = "linux") {
            // Where the preference exists, it is passed as the client spells it.
            prefs_set(&ctx, params).await.expect("linux has netfilter");
            assert!(
                only(&backend.argv()).contains(&"--netfilter-mode=nodivert".to_owned()),
                "{:?}",
                backend.argv()
            );
        } else {
            let error = prefs_set(&ctx, params)
                .await
                .expect_err("netfilter is a Linux preference");
            assert_eq!(error.code, ErrorCode::UnsupportedPlatform);
            assert!(
                error.message.contains("netfilter_mode")
                    && error.message.contains(std::env::consts::OS),
                "the answer should name the setting and the platform: {}",
                error.message
            );
            assert!(backend.argv().is_empty(), "nothing should have run");
        }
    }

    #[test]
    fn each_platform_preference_is_offered_only_where_it_exists() {
        for (setting, platforms) in [
            ("snat_subnet_routes", LINUX_ONLY),
            ("stateful_filtering", LINUX_ONLY),
            ("netfilter_mode", LINUX_ONLY),
            ("unattended", WINDOWS_ONLY),
        ] {
            let allowed = platforms.contains(&std::env::consts::OS);
            assert_eq!(
                only_on(setting, platforms).is_ok(),
                allowed,
                "{setting} on {}",
                std::env::consts::OS
            );
        }
    }

    // -- secrets --------------------------------------------------------------

    #[tokio::test]
    async fn an_authentication_key_reaches_the_client_by_reference_not_by_value() {
        let (_, argv) = against(
            Reply::ok(fixture!("up-running.json")),
            |ctx, p| async move { up(&ctx, p).await },
            UpParams {
                auth_key: Some(FAKE_KEY.to_owned()),
                ..UpParams::default()
            },
        )
        .await;
        let args = only(&argv);
        assert!(
            !args.iter().any(|a| a.contains(FAKE_KEY)),
            "the key must not be in the argument list: {args:?}"
        );
        assert!(
            args.iter()
                .any(|a| a.starts_with("--auth-key=file:") && a.ends_with(".key")),
            "the key should be passed by file reference: {args:?}"
        );
    }

    #[tokio::test]
    async fn a_key_that_is_already_a_file_reference_is_passed_through() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { login(&ctx, p).await },
            LoginParams {
                auth_key: Some("file:/run/secrets/tailscale.key".to_owned()),
                ..LoginParams::default()
            },
        )
        .await;
        assert!(
            only(&argv).contains(&"--auth-key=file:/run/secrets/tailscale.key".to_owned()),
            "{argv:?}"
        );
    }

    #[test]
    fn a_secret_argument_carries_a_path_and_the_file_carries_the_secret() {
        let (argument, file) = secret_argument("auth-key", FAKE_KEY).expect("a private file");
        let file = file.expect("a literal key needs a file");
        assert!(!argument.contains(FAKE_KEY), "{argument}");
        assert_eq!(
            std::fs::read_to_string(file.path()).expect("readable"),
            FAKE_KEY
        );
    }

    // -- bounded connects -----------------------------------------------------

    #[test]
    fn a_wait_is_bounded_whatever_was_asked_for() {
        assert_eq!(connect_timeouts(None).0, DEFAULT_CONNECT_TIMEOUT);
        assert_eq!(connect_timeouts(Some(5)).0, 5);
        assert_eq!(connect_timeouts(Some(0)).0, 1);
        assert_eq!(connect_timeouts(Some(9_999)).0, MAX_CONNECT_TIMEOUT);
        // The client is given longer than the caller waits, so that it reports
        // its own timeout rather than being killed part-way through.
        assert!(connect_timeouts(Some(60)).1 > Duration::from_secs(60));
    }

    #[tokio::test]
    async fn connecting_bounds_its_wait_and_accepts_the_risk_it_was_confirmed_for() {
        let (answer, argv) = against(
            Reply::ok(fixture!("up-running.json")),
            |ctx, p| async move { up(&ctx, p).await },
            UpParams {
                timeout_seconds: Some(9_999),
                ..UpParams::default()
            },
        )
        .await;
        let args = only(&argv);
        assert_eq!(args[0], "up");
        assert!(args.contains(&"--json=true".to_owned()), "{args:?}");
        assert!(args.contains(&"--timeout=300s".to_owned()), "{args:?}");
        assert!(args.contains(&ACCEPTED_RISKS.to_owned()), "{args:?}");
        assert_eq!(answer["timeout_seconds"], json!(MAX_CONNECT_TIMEOUT));
        assert_eq!(answer["state"]["BackendState"], "Running");
    }

    #[tokio::test]
    async fn reconnecting_passes_the_two_switches_that_make_it_destructive() {
        let (_, argv) = against(
            Reply::ok(fixture!("up-running.json")),
            |ctx, p| async move { up(&ctx, p).await },
            UpParams {
                force_reauth: Some(true),
                reset: Some(true),
                ..UpParams::default()
            },
        )
        .await;
        let args = only(&argv);
        assert!(args.contains(&"--force-reauth=true".to_owned()), "{args:?}");
        assert!(args.contains(&"--reset=true".to_owned()), "{args:?}");
    }

    // -- interactive logins ---------------------------------------------------

    #[tokio::test]
    async fn a_connect_that_needs_a_browser_hands_back_the_url() {
        let (answer, _) = against(
            Reply::ok(fixture!("up-needs-login.json")),
            |ctx, p| async move { up(&ctx, p).await },
            UpParams::default(),
        )
        .await;
        assert_eq!(
            answer["login_url"],
            "https://login.example.com/a/0123456789abcdef"
        );
        assert_eq!(answer["state"]["BackendState"], "NeedsLogin");
    }

    #[tokio::test]
    async fn a_login_hands_back_the_url_it_printed() {
        let (answer, argv) = against(
            Reply::ok(fixture!("login.txt")),
            |ctx, p| async move { login(&ctx, p).await },
            LoginParams {
                nickname: Some("work".to_owned()),
                advertise_tags: Some(vec!["tag:server".to_owned()]),
                ..LoginParams::default()
            },
        )
        .await;
        assert_eq!(
            answer["login_url"],
            "https://login.example.com/a/0123456789abcdef"
        );
        let args = only(&argv);
        assert!(
            args.contains(&"--advertise-tags=tag:server".to_owned()),
            "{args:?}"
        );
        assert!(args.contains(&"--nickname=work".to_owned()), "{args:?}");
        assert!(args.contains(&"--timeout=60s".to_owned()), "{args:?}");
        // `login` has no risk flag of its own, so none is invented for it.
        assert!(!args.contains(&ACCEPTED_RISKS.to_owned()), "{args:?}");
    }

    #[test]
    fn a_url_is_found_wherever_the_client_put_it_and_nowhere_else() {
        assert_eq!(
            find_url("To authenticate, visit:\n\n\thttps://login.example.com/a/00\n"),
            Some("https://login.example.com/a/00".to_owned())
        );
        assert_eq!(
            find_url("visit https://login.example.com/a/00, then come back."),
            Some("https://login.example.com/a/00".to_owned())
        );
        assert_eq!(find_url("Success.\n"), None);
    }

    // -- disconnecting --------------------------------------------------------

    #[tokio::test]
    async fn disconnecting_accepts_the_risk_and_carries_a_reason_when_policy_wants_one() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { down(&ctx, p).await },
            DownParams {
                reason: Some("maintenance window".to_owned()),
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            ["down", ACCEPTED_RISKS, "--reason=maintenance window"]
        );
        assert_eq!(answer["outcome"], "disconnected");
    }

    #[tokio::test]
    async fn logging_out_carries_a_reason_but_no_risk_flag_the_command_lacks() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { logout(&ctx, p).await },
            LogoutParams {
                reason: Some("decommissioned".to_owned()),
            },
        )
        .await;
        assert_eq!(only(&argv), ["logout", "--reason=decommissioned"]);
        assert_eq!(answer["outcome"], "logged out");
    }

    // -- profiles -------------------------------------------------------------

    #[tokio::test]
    async fn switching_and_forgetting_an_account_name_it_in_the_answer() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { switch_profile(&ctx, p).await },
            SwitchParams {
                account: "example-tailnet.ts.net".to_owned(),
            },
        )
        .await;
        assert_eq!(only(&argv), ["switch", "example-tailnet.ts.net"]);
        assert_eq!(answer["account"], "example-tailnet.ts.net");
        assert_eq!(answer["outcome"], "switched");

        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { switch_remove(&ctx, p).await },
            SwitchRemoveParams {
                account: "example-tailnet.ts.net".to_owned(),
            },
        )
        .await;
        assert_eq!(only(&argv), ["switch", "remove", "example-tailnet.ts.net"]);
        assert_eq!(answer["outcome"], "removed");
    }

    // -- reading preferences back --------------------------------------------

    #[tokio::test]
    async fn reading_preferences_asks_for_the_document() {
        let (answer, argv) = against(
            Reply::ok(fixture!("prefs.json")),
            |ctx, p| async move { prefs_get(&ctx, p).await },
            PrefsGetParams {
                setting: None,
                as_set_flags: false,
            },
        )
        .await;
        assert_eq!(only(&argv), ["get", "--json=true"]);
        assert_eq!(answer["preferences"]["WantRunning"], json!(true));
        assert!(answer["setting"].is_null());
        assert!(answer.get("set_flags").is_none());
    }

    #[tokio::test]
    async fn one_preference_is_asked_for_the_way_the_client_spells_it() {
        // Flags come before the positional, because Go stops parsing them at
        // the first one.
        let (answer, argv) = against(
            Reply::ok("{\"ShieldsUp\":false}"),
            |ctx, p| async move { prefs_get(&ctx, p).await },
            PrefsGetParams {
                setting: Some("shields_up".to_owned()),
                as_set_flags: false,
            },
        )
        .await;
        assert_eq!(only(&argv), ["get", "--json=true", "shields-up"]);
        assert_eq!(answer["setting"], "shields_up");
    }

    #[tokio::test]
    async fn preferences_can_be_read_back_as_the_flags_that_would_set_them() {
        let (answer, argv) = against(
            Reply::ok(fixture!("prefs-set-flags.txt")),
            |ctx, p| async move { prefs_get(&ctx, p).await },
            PrefsGetParams {
                setting: None,
                as_set_flags: true,
            },
        )
        .await;
        assert_eq!(only(&argv), ["get", "--set-flags=true"]);
        assert!(
            answer["set_flags"]
                .as_str()
                .expect("flags")
                .starts_with("--accept-dns=true"),
            "{answer:#}"
        );
        assert!(answer.get("preferences").is_none());
    }

    // -- the toolset itself ---------------------------------------------------

    #[test]
    fn the_toolset_holds_the_eight_commands_that_change_this_node() {
        let names: Vec<_> = entries().iter().map(|e| e.meta.name).collect();
        assert_eq!(
            names,
            [
                "tailscale_prefs_get",
                "tailscale_prefs_set",
                "tailscale_up",
                "tailscale_down",
                "tailscale_login",
                "tailscale_logout",
                "tailscale_switch_profile",
                "tailscale_switch_remove",
            ]
        );
    }

    #[test]
    fn everything_that_can_cut_the_connection_asks_first() {
        for entry in entries() {
            let meta = &entry.meta;
            assert_eq!(meta.toolset, Toolset::LocalPrefs, "{}", meta.name);
            let severing = matches!(
                meta.name,
                "tailscale_up"
                    | "tailscale_down"
                    | "tailscale_login"
                    | "tailscale_logout"
                    | "tailscale_switch_profile"
            );
            assert_eq!(
                meta.self_severing, severing,
                "`{}` is on the wrong side of the self-severing line",
                meta.name
            );
            // Reading is the only thing here that needs no confirmation, and
            // forgetting an account needs one without being self-severing.
            assert_eq!(
                meta.requires_confirmation,
                meta.tier != Tier::Read && meta.name != "tailscale_prefs_set",
                "`{}` confirms the wrong way round",
                meta.name
            );
        }
    }
}
