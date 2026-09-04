//! Publishing a local server: to the tailnet with `serve`, to the internet
//! with `funnel`.
//!
//! Two things shape this module. The first is that both commands run in the
//! *foreground* by default, holding the terminal until interrupted, which a
//! tool call can never do — so `--bg=true` is passed on every call that sets
//! or clears a handler, and no tool here can leave a process running. The
//! second is `--yes=true`, which answers the client's interactive prompt
//! before it is asked. A prompt nobody can see is a hang, and the question it
//! asks is one this server asks in its own terms: funnel publishes to the
//! public internet, so every funnel tool sits at the destructive tier and the
//! one that exposes something requires `confirm: true` (DECISIONS Q22).
//!
//! Serve and funnel write to one configuration, which is why `reset` appears
//! once rather than twice: `tailscale funnel reset` and `tailscale serve
//! reset` clear the same thing.
//!
//! Reading the configuration and writing it back is meant to be a no-op, so
//! `tailscale_serve_get_config` answers with the document the client printed
//! and `tailscale_serve_set_config` takes that document back inline. The CLI
//! insists on reading it from a file, so the document is written to a private
//! temporary file that exists only for the length of the call.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_cli::{Invocation, PrivateFile};

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};
use crate::tools::common::{find_url, flag, note, printed, push_list, push_text, report};

crate::tools! {
    /// Serve a local server, a file, a directory or a block of text to the
    /// tailnet. Reachable by other nodes on the tailnet only; use
    /// `tailscale_funnel_set` to publish to the internet. Runs in the
    /// background: the call returns as soon as the handler is in place.
    tailscale_serve_set => ServeSetParams, serve_set,
        toolset: LocalServe, tier: Write;

    /// Stop serving one endpoint, leaving every other handler in place. Names
    /// the same endpoint the handler was added on; reports `not_found` when
    /// there is no handler there.
    tailscale_serve_off => ServeOffParams, serve_off,
        toolset: LocalServe, tier: Write;

    /// Remove every serve and funnel handler on this node at once. There is
    /// one configuration behind both commands, so this clears both. Use
    /// `tailscale_serve_off` to remove a single endpoint.
    tailscale_serve_reset => NoParams, serve_reset,
        toolset: LocalServe, tier: Destructive, confirm: true;

    /// Stop this node accepting new connections for a service, while letting
    /// the connections it already has finish. Bring it back with
    /// `tailscale_serve_advertise`.
    tailscale_serve_drain => ServiceParams, serve_drain,
        toolset: LocalServe, tier: Write, since: "1.90";

    /// Remove every handler configured for one service on this node. Unlike
    /// `tailscale_serve_drain` this discards the configuration rather than
    /// stopping at the door, and `tailscale_serve_advertise` will not bring it
    /// back.
    tailscale_serve_clear => ServiceParams, serve_clear,
        toolset: LocalServe, tier: Destructive, confirm: true, since: "1.90";

    /// Offer this node to the tailnet as a host for a service, which is what
    /// undoes `tailscale_serve_drain`. Not needed after
    /// `tailscale_serve_set`, which advertises the service itself.
    tailscale_serve_advertise => ServiceParams, serve_advertise,
        toolset: LocalServe, tier: Write, since: "1.90";

    /// Read the service configuration this node is hosting, as a document that
    /// `tailscale_serve_set_config` takes back unchanged. Reads only.
    tailscale_serve_get_config => GetConfigParams, serve_get_config,
        toolset: LocalServe, tier: Read, idempotent: true, since: "1.90";

    /// Replace the service configuration on this node with the document given
    /// here, which overwrites every handler in scope. Writing back a document
    /// that `tailscale_serve_get_config` produced changes nothing.
    tailscale_serve_set_config => SetConfigParams, serve_set_config,
        toolset: LocalServe, tier: Write, idempotent: true, since: "1.90";

    /// Publish a local server to the **public internet** through Tailscale
    /// Funnel. Anyone on the internet who knows the name can reach it, with no
    /// tailnet membership and no authentication of any kind. Use
    /// `tailscale_serve_set` for a server that only the tailnet should reach.
    /// Funnel has to be enabled for the tailnet in the admin console first; on
    /// a tailnet where it is not, the client waits rather than failing, and
    /// this call comes back as a timeout carrying the URL that enables it.
    tailscale_funnel_set => FunnelSetParams, funnel_set,
        toolset: LocalServe, tier: Destructive, confirm: true;

    /// Stop publishing one endpoint to the internet. The handler stops being
    /// reachable from outside the tailnet; use `tailscale_serve_off` to remove
    /// it from the tailnet as well.
    tailscale_funnel_off => FunnelOffParams, funnel_off,
        toolset: LocalServe, tier: Destructive;
}

// ---------------------------------------------------------------------------
// The shape of every call
// ---------------------------------------------------------------------------

/// Run in the background. The client's default is to hold the foreground until
/// it is interrupted, which no tool call can do.
const BACKGROUND: &str = "--bg=true";

/// Answer the client's own prompt. The question it asks — whether to expose
/// this to the internet, whether to take on a service — is the question the
/// `confirm` parameter asks in terms the caller can see, so leaving it for a
/// terminal nobody is watching would only hang the call (DECISIONS Q22).
const NO_PROMPT: &str = "--yes=true";

/// The word that turns a set into a clear, in the position the target occupies.
const OFF: &str = "off";

/// A tool that takes nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

/// Declare a parameter struct carrying the endpoint selectors that `serve` and
/// `funnel` share, plus whatever else that command takes.
///
/// A macro rather than one struct behind `#[serde(flatten)]`, for the reason
/// recorded in DECISIONS Q21: a flattened schema is a composition with no
/// top-level properties, and the confirmation gate injects into those.
macro_rules! endpoint_params {
    (
        $(#[doc = $doc:literal])*
        $name:ident { $( $(#[doc = $field_doc:literal])* $field:ident : $ty:ty ),* $(,)? }
    ) => {
        $(#[doc = $doc])*
        #[derive(Debug, Default, Deserialize, JsonSchema)]
        pub struct $name {
            /// Expose an HTTPS server on this port. The default when no
            /// endpoint is named at all is HTTPS on port 443.
            #[serde(default)]
            pub https: Option<u16>,
            /// Forward raw TCP on this port instead of speaking HTTP.
            #[serde(default)]
            pub tcp: Option<u16>,
            /// Forward TCP on this port, terminating TLS here rather than
            /// passing it through.
            #[serde(default)]
            pub tls_terminated_tcp: Option<u16>,
            /// Serve under this path rather than at the root, so that several
            /// servers can share one port.
            #[serde(default)]
            pub set_path: Option<String>,
            $(
                $(#[doc = $field_doc])*
                #[serde(default)]
                pub $field: $ty,
            )*
        }

        impl $name {
            /// The endpoints this call named, for [`one_endpoint`] to reduce
            /// to the one the client will accept.
            fn shared_endpoints(&self) -> Vec<(&'static str, u16)> {
                let mut chosen = Vec::new();
                if let Some(port) = self.https {
                    chosen.push(("https", port));
                }
                if let Some(port) = self.tcp {
                    chosen.push(("tcp", port));
                }
                if let Some(port) = self.tls_terminated_tcp {
                    chosen.push(("tls-terminated-tcp", port));
                }
                chosen
            }
        }
    };
}

endpoint_params! {
    /// What to serve to the tailnet, and where.
    ServeSetParams {
        /// What to serve: a port number such as `3000`, a URL such as
        /// `http://localhost:3000/api`, a path to a file or directory, a
        /// `text:` literal, or a `unix:` socket path. Required.
        target: String,
        /// Expose an HTTP server on this port. Plain HTTP inside the tailnet,
        /// with no certificate.
        http: Option<u16>,
        /// Serve for a Tailscale service under its own address, rather than
        /// for this node. A `svc:` name, or a bare name this server prefixes.
        service: Option<String>,
        /// Forward all traffic for the service to this machine, rather than
        /// proxying named ports. Services only.
        tun: Option<bool>,
        /// Application capabilities to pass through to the server behind this
        /// handler.
        accept_app_caps: Option<Vec<String>>,
        /// Speak the PROXY protocol to the server behind a TCP forwarder, so
        /// that it sees the client's address. Version `1` or `2`.
        proxy_protocol: Option<u8>,
    }
}

endpoint_params! {
    /// Which endpoint to stop serving.
    ServeOffParams {
        /// Stop serving plain HTTP on this port.
        http: Option<u16>,
        /// Stop serving for this Tailscale service rather than for this node.
        service: Option<String>,
    }
}

endpoint_params! {
    /// What to publish to the internet, and where.
    FunnelSetParams {
        /// What to publish: a port number such as `3000`, a URL such as
        /// `http://localhost:3000/api`, a path to a file or directory, a
        /// `text:` literal, or a `unix:` socket path. Required.
        target: String,
        /// Speak the PROXY protocol to the server behind a TCP forwarder, so
        /// that it sees the client's address. Version `1` or `2`.
        proxy_protocol: Option<u8>,
    }
}

endpoint_params! {
    /// Which endpoint to stop publishing.
    FunnelOffParams {}
}

/// One Tailscale service by name.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServiceParams {
    /// The service, as a `svc:` name such as `svc:web`. A bare name is given
    /// the prefix.
    pub service: String,
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Reduce the endpoints a call named to the single one the client accepts.
///
/// `None` means the caller named none and the client's own default — HTTPS on
/// 443 — applies. Two is refused here rather than by a client that would take
/// the last one silently.
fn one_endpoint(chosen: Vec<(&'static str, u16)>) -> ToolResult<Option<(&'static str, u16)>> {
    match chosen.len() {
        0 => Ok(None),
        1 => Ok(chosen.into_iter().next()),
        _ => Err(ToolError::invalid_args(format!(
            "name one endpoint, not {}: {}",
            chosen.len(),
            chosen
                .iter()
                .map(|(scheme, port)| format!("{scheme}:{port}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Add the endpoint flag, when there is one to add.
fn push_endpoint(args: &mut Vec<String>, endpoint: Option<(&str, u16)>) {
    if let Some((scheme, port)) = endpoint {
        args.push(format!("--{scheme}={port}"));
    }
}

/// How an endpoint is written in an answer.
fn describe(endpoint: Option<(&str, u16)>) -> Option<String> {
    endpoint.map(|(scheme, port)| format!("{scheme}:{port}"))
}

/// A service name in the form the client insists on.
///
/// The client rejects a bare name with a flag-parsing error rather than adding
/// the prefix itself, which is not an answer a caller can act on.
fn service_name(service: &str) -> String {
    let service = service.trim();
    if service.starts_with("svc:") {
        service.to_owned()
    } else {
        format!("svc:{service}")
    }
}

/// The PROXY protocol version, which the client takes as `1` or `2`.
fn proxy_protocol(version: Option<u8>) -> ToolResult<Option<String>> {
    match version {
        None => Ok(None),
        Some(v @ (1 | 2)) => Ok(Some(format!("--proxy-protocol={v}"))),
        Some(other) => Err(ToolError::invalid_args(format!(
            "the PROXY protocol has versions 1 and 2, not {other}"
        ))),
    }
}

/// Which services a configuration exchange covers.
///
/// Named rather than defaulted: `--all` on a write replaces the configuration
/// of every service this node hosts, which is not something to arrive at by
/// leaving a parameter out.
fn scope(service: Option<&str>, all: bool) -> ToolResult<(String, String)> {
    match (service, all) {
        (Some(_), true) => Err(ToolError::invalid_args(
            "name one service or set `all`, not both",
        )),
        (Some(service), false) => {
            let named = service_name(service);
            Ok((format!("--service={named}"), named))
        }
        (None, true) => Ok((flag("all", true), "all".to_owned())),
        (None, false) => Err(ToolError::invalid_args(
            "name the service this covers, or set `all` to true for every \
             service this node hosts",
        )),
    }
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// The answer to a call that added or removed a handler.
#[derive(Debug, Serialize, JsonSchema)]
pub struct HandlerReport {
    /// What is now being served, or `"off"` for a handler that was removed.
    pub target: String,
    /// The endpoint as `scheme:port`, when the call named one. Absent when the
    /// client's own default applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The path the handler was attached under, when it was not the root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The service this covers, when it was for a service rather than for this
    /// node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Whether this is reachable from the public internet. `false` means the
    /// tailnet and nothing outside it.
    pub public: bool,
    /// The address the handler is reachable at, when the client printed one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Everything the client printed, which for `serve` is a short report of
    /// what it set up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

/// The answer to a call that acted on a whole service or on the whole node.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScopeReport {
    /// What the call covered: a `svc:` name, or `"this node"`.
    pub scope: String,
    /// What was done to it.
    pub outcome: String,
    /// Anything the client said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A service configuration document, on its way out or on its way in.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConfigReport {
    /// What the document covers: a `svc:` name, or `"all"`.
    pub scope: String,
    /// The configuration itself. Pass it back to
    /// `tailscale_serve_set_config` unchanged and nothing changes.
    pub configuration: Value,
    /// Anything the client said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// serve
// ---------------------------------------------------------------------------

async fn serve_set(ctx: &ToolContext, params: ServeSetParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_serve_set;
    let mut chosen = params.shared_endpoints();
    if let Some(port) = params.http {
        chosen.push(("http", port));
    }
    let endpoint = one_endpoint(chosen)?;
    if params.tun == Some(true) && params.service.is_none() {
        return Err(ToolError::invalid_args(
            "`tun` forwards all traffic for a service, so it needs a `service`",
        ));
    }
    let service = params.service.as_deref().map(service_name);

    let mut args = vec![BACKGROUND.to_owned(), NO_PROMPT.to_owned()];
    push_endpoint(&mut args, endpoint);
    push_text(&mut args, "set-path", params.set_path.as_deref());
    push_text(&mut args, "service", service.as_deref());
    args.extend(proxy_protocol(params.proxy_protocol)?);
    push_list(
        &mut args,
        "accept-app-caps",
        params.accept_app_caps.as_deref(),
    );
    if let Some(tun) = params.tun {
        args.push(flag("tun", tun));
    }
    // Last, because Go stops reading flags at the first positional.
    args.push(params.target.clone());

    let output = cli::run(ctx, meta, Invocation::mutate(prefixed("serve", args))).await?;
    let stdout = output.stdout_str().into_owned();
    report(HandlerReport {
        target: params.target,
        endpoint: describe(endpoint),
        path: params.set_path,
        service,
        public: false,
        url: find_url(&stdout),
        printed: printed(ctx, &output),
    })
}

async fn serve_off(ctx: &ToolContext, params: ServeOffParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_serve_off;
    let mut chosen = params.shared_endpoints();
    if let Some(port) = params.http {
        chosen.push(("http", port));
    }
    let endpoint = one_endpoint(chosen)?;
    let service = params.service.as_deref().map(service_name);

    let mut args = vec![BACKGROUND.to_owned(), NO_PROMPT.to_owned()];
    push_endpoint(&mut args, endpoint);
    push_text(&mut args, "set-path", params.set_path.as_deref());
    push_text(&mut args, "service", service.as_deref());
    args.push(OFF.to_owned());

    let output = cli::run(ctx, meta, Invocation::mutate(prefixed("serve", args))).await?;
    report(HandlerReport {
        target: OFF.to_owned(),
        endpoint: describe(endpoint),
        path: params.set_path,
        service,
        public: false,
        url: None,
        printed: printed(ctx, &output),
    })
}

async fn serve_reset(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_serve_reset;
    let output = cli::run(ctx, meta, Invocation::mutate(["serve", "reset"])).await?;
    report(ScopeReport {
        scope: "this node".to_owned(),
        outcome: "every serve and funnel handler removed".to_owned(),
        note: note(ctx, &output.stderr),
    })
}

// ---------------------------------------------------------------------------
// services
// ---------------------------------------------------------------------------

/// The three service lifecycle commands, which differ only in the word.
async fn service_command(
    ctx: &ToolContext,
    meta: &crate::meta::ToolMeta,
    subcommand: &str,
    outcome: &str,
    params: ServiceParams,
) -> ToolResult<Value> {
    let service = service_name(&params.service);
    let output = cli::run(
        ctx,
        meta,
        Invocation::mutate(["serve", subcommand, &service]),
    )
    .await?;
    report(ScopeReport {
        scope: service,
        outcome: outcome.to_owned(),
        note: note(ctx, &output.stderr),
    })
}

async fn serve_drain(ctx: &ToolContext, params: ServiceParams) -> ToolResult<Value> {
    service_command(
        ctx,
        &metas::tailscale_serve_drain,
        "drain",
        "no longer accepting new connections; existing ones continue",
        params,
    )
    .await
}

async fn serve_clear(ctx: &ToolContext, params: ServiceParams) -> ToolResult<Value> {
    service_command(
        ctx,
        &metas::tailscale_serve_clear,
        "clear",
        "every handler for the service removed from this node",
        params,
    )
    .await
}

async fn serve_advertise(ctx: &ToolContext, params: ServiceParams) -> ToolResult<Value> {
    service_command(
        ctx,
        &metas::tailscale_serve_advertise,
        "advertise",
        "advertised to the tailnet as a host for the service",
        params,
    )
    .await
}

// ---------------------------------------------------------------------------
// the configuration exchange
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetConfigParams {
    /// One service by name. Give this or `all`, not both.
    #[serde(default)]
    pub service: Option<String>,
    /// Every service this node hosts.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetConfigParams {
    /// The configuration document, in the shape
    /// `tailscale_serve_get_config` answers with.
    pub configuration: Value,
    /// The one service this replaces the configuration of. Give this or `all`,
    /// not both.
    #[serde(default)]
    pub service: Option<String>,
    /// Replace the configuration of every service this node hosts.
    #[serde(default)]
    pub all: bool,
}

async fn serve_get_config(ctx: &ToolContext, params: GetConfigParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_serve_get_config;
    let (selector, scope) = scope(params.service.as_deref(), params.all)?;
    // The client's usage names a file, but it prints the document to standard
    // output and ignores the argument, so there is no file in this direction.
    let text = cli::run_text(
        ctx,
        meta,
        Invocation::read(["serve", "get-config", &selector]),
    )
    .await?;
    let configuration = parse_config(&text)?;
    report(ConfigReport {
        scope,
        configuration,
        note: None,
    })
}

async fn serve_set_config(ctx: &ToolContext, params: SetConfigParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_serve_set_config;
    let (selector, scope) = scope(params.service.as_deref(), params.all)?;
    // Held for the length of the call: dropping it removes the file and the
    // private directory it lives in.
    let file = config_file(&params.configuration)?;
    let output = cli::run(
        ctx,
        meta,
        Invocation::mutate(["serve", "set-config", &selector, &file.arg()]),
    )
    .await?;
    report(ConfigReport {
        scope,
        configuration: params.configuration,
        note: note(ctx, &output.stderr),
    })
}

/// The document the client printed.
fn parse_config(text: &str) -> ToolResult<Value> {
    serde_json::from_str(text.trim()).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("`tailscale serve get-config` did not print JSON: {e}"),
        )
    })
}

/// The document on its way to a client that will only read it from a file.
///
/// Written to a directory only this user may enter, and removed with the
/// returned value, so a configuration that names an internal host does not
/// outlive the call in a world-readable temporary directory.
fn config_file(configuration: &Value) -> ToolResult<PrivateFile> {
    let body = serde_json::to_vec_pretty(configuration).map_err(|e| {
        ToolError::invalid_args(format!("the configuration is not a JSON document: {e}"))
    })?;
    PrivateFile::written("serve-config.json", &body).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("the configuration could not be written to a private file: {e}"),
        )
    })
}

// ---------------------------------------------------------------------------
// funnel
// ---------------------------------------------------------------------------

async fn funnel_set(ctx: &ToolContext, params: FunnelSetParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_funnel_set;
    let endpoint = one_endpoint(params.shared_endpoints())?;

    let mut args = vec![BACKGROUND.to_owned(), NO_PROMPT.to_owned()];
    push_endpoint(&mut args, endpoint);
    push_text(&mut args, "set-path", params.set_path.as_deref());
    args.extend(proxy_protocol(params.proxy_protocol)?);
    args.push(params.target.clone());

    let output = cli::run(ctx, meta, Invocation::mutate(prefixed("funnel", args))).await?;
    let stdout = output.stdout_str().into_owned();
    report(HandlerReport {
        target: params.target,
        endpoint: describe(endpoint),
        path: params.set_path,
        service: None,
        public: true,
        url: find_url(&stdout),
        printed: printed(ctx, &output),
    })
}

async fn funnel_off(ctx: &ToolContext, params: FunnelOffParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_funnel_off;
    let endpoint = one_endpoint(params.shared_endpoints())?;

    let mut args = vec![BACKGROUND.to_owned(), NO_PROMPT.to_owned()];
    push_endpoint(&mut args, endpoint);
    push_text(&mut args, "set-path", params.set_path.as_deref());
    args.push(OFF.to_owned());

    let output = cli::run(ctx, meta, Invocation::mutate(prefixed("funnel", args))).await?;
    report(HandlerReport {
        target: OFF.to_owned(),
        endpoint: describe(endpoint),
        path: params.set_path,
        service: None,
        public: false,
        url: None,
        printed: printed(ctx, &output),
    })
}

/// The command name in front of the arguments built for it.
fn prefixed(command: &str, args: Vec<String>) -> Vec<String> {
    let mut all = Vec::with_capacity(args.len() + 1);
    all.push(command.to_owned());
    all.extend(args);
    all
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    use crate::meta::{Tier, Toolset};
    use crate::testing::{Reply, StubBackend, context};

    /// What the client prints when a handler goes up, with this tailnet's own
    /// names replaced by the documentation ones.
    const SERVE_OUTPUT: &str = "Available within your tailnet:\n\n\
        https://workstation.example-tailnet.ts.net/\n\
        |-- proxy http://127.0.0.1:3000\n\n\
        Serve started and running in the background.\n";

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

    /// The same for a call that should be refused, which is only interesting
    /// alongside the fact that nothing was run.
    async fn refused<F, P, Fut>(handler: F, params: P) -> ToolError
    where
        F: FnOnce(ToolContext, P) -> Fut,
        Fut: Future<Output = ToolResult<Value>>,
    {
        let backend = Arc::new(StubBackend::always(Reply::ok("")));
        let ctx = context(Arc::clone(&backend));
        let error = handler(ctx, params).await.expect_err("the handler refuses");
        assert!(
            backend.argv().is_empty(),
            "nothing should have run: {:?}",
            backend.argv()
        );
        error
    }

    /// The argument list of the single command a handler ran.
    fn only(argv: &[Vec<String>]) -> &[String] {
        assert_eq!(argv.len(), 1, "one command should have run: {argv:?}");
        &argv[0]
    }

    // -- serving to the tailnet ----------------------------------------------

    #[tokio::test]
    async fn serving_a_port_runs_in_the_background_and_answers_the_prompt() {
        let (_, argv) = against(
            Reply::ok(SERVE_OUTPUT),
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(only(&argv), ["serve", "--bg=true", "--yes=true", "3000"]);
    }

    #[tokio::test]
    async fn everything_that_can_hold_the_terminal_lets_it_go() {
        // The one property that makes these safe to call at all: no form of
        // any of them runs in the foreground, and none can stop on a prompt.
        let calls: Vec<Vec<String>> = vec![
            against(
                Reply::ok(""),
                |ctx, p| async move { serve_set(&ctx, p).await },
                ServeSetParams {
                    target: "3000".to_owned(),
                    ..ServeSetParams::default()
                },
            )
            .await
            .1,
            against(
                Reply::ok(""),
                |ctx, p| async move { serve_off(&ctx, p).await },
                ServeOffParams::default(),
            )
            .await
            .1,
            against(
                Reply::ok(""),
                |ctx, p| async move { funnel_set(&ctx, p).await },
                FunnelSetParams {
                    target: "3000".to_owned(),
                    ..FunnelSetParams::default()
                },
            )
            .await
            .1,
            against(
                Reply::ok(""),
                |ctx, p| async move { funnel_off(&ctx, p).await },
                FunnelOffParams::default(),
            )
            .await
            .1,
        ]
        .into_iter()
        .map(|argv| only(&argv).to_vec())
        .collect();

        for args in calls {
            assert!(
                args.contains(&BACKGROUND.to_owned()) && args.contains(&NO_PROMPT.to_owned()),
                "{args:?} could hold the foreground or stop on a prompt"
            );
        }
    }

    #[tokio::test]
    async fn the_target_comes_last_so_the_flags_are_read_as_flags() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "http://localhost:3000/api".to_owned(),
                https: Some(8443),
                set_path: Some("/api".to_owned()),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv).last().map(String::as_str),
            Some("http://localhost:3000/api")
        );
        assert_eq!(
            only(&argv),
            [
                "serve",
                "--bg=true",
                "--yes=true",
                "--https=8443",
                "--set-path=/api",
                "http://localhost:3000/api",
            ]
        );
    }

    #[tokio::test]
    async fn each_endpoint_is_passed_as_the_flag_the_client_knows() {
        for (params, expected) in [
            (
                ServeSetParams {
                    http: Some(80),
                    ..ServeSetParams::default()
                },
                "--http=80",
            ),
            (
                ServeSetParams {
                    https: Some(443),
                    ..ServeSetParams::default()
                },
                "--https=443",
            ),
            (
                ServeSetParams {
                    tcp: Some(2222),
                    ..ServeSetParams::default()
                },
                "--tcp=2222",
            ),
            (
                ServeSetParams {
                    tls_terminated_tcp: Some(2223),
                    ..ServeSetParams::default()
                },
                "--tls-terminated-tcp=2223",
            ),
        ] {
            let (answer, argv) = against(
                Reply::ok(""),
                |ctx, p| async move { serve_set(&ctx, p).await },
                ServeSetParams {
                    target: "3000".to_owned(),
                    ..params
                },
            )
            .await;
            assert!(
                only(&argv).contains(&expected.to_owned()),
                "{expected} missing from {:?}",
                only(&argv)
            );
            let (scheme, port) = expected.trim_start_matches("--").split_once('=').unwrap();
            assert_eq!(answer["endpoint"], json!(format!("{scheme}:{port}")));
        }
    }

    #[tokio::test]
    async fn naming_no_endpoint_leaves_the_client_its_own_default() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert!(
            !only(&argv).iter().any(|arg| arg.starts_with("--http")
                || arg.starts_with("--tcp")
                || arg.starts_with("--tls")),
            "no endpoint should have been chosen for the client: {:?}",
            only(&argv)
        );
        assert_eq!(answer.get("endpoint"), None);
    }

    #[tokio::test]
    async fn naming_two_endpoints_is_refused_before_anything_runs() {
        let error = refused(
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                http: Some(80),
                https: Some(443),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert!(error.message.contains("https:443") && error.message.contains("http:80"));
    }

    #[tokio::test]
    async fn forwarding_all_traffic_needs_a_service_to_forward_it_for() {
        let error = refused(
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                tun: Some(true),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn a_proxy_protocol_version_the_client_does_not_have_is_refused() {
        let error = refused(
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                proxy_protocol: Some(3),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert_eq!(
            proxy_protocol(Some(2)).unwrap(),
            Some("--proxy-protocol=2".to_owned())
        );
    }

    #[tokio::test]
    async fn a_service_call_carries_everything_a_service_needs() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                https: Some(443),
                service: Some("web".to_owned()),
                tun: Some(true),
                accept_app_caps: Some(vec!["example.com/cap/a".to_owned()]),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            [
                "serve",
                "--bg=true",
                "--yes=true",
                "--https=443",
                "--service=svc:web",
                "--accept-app-caps=example.com/cap/a",
                "--tun=true",
                "3000",
            ]
        );
        assert_eq!(answer["service"], json!("svc:web"));
    }

    #[test]
    fn a_bare_service_name_is_given_the_prefix_the_client_insists_on() {
        // The client rejects `web` with a flag-parsing error rather than
        // adding the prefix itself.
        assert_eq!(service_name("web"), "svc:web");
        assert_eq!(service_name("  web  "), "svc:web");
        assert_eq!(service_name("svc:web"), "svc:web");
    }

    #[tokio::test]
    async fn the_address_the_handler_answers_at_comes_back_with_it() {
        let (answer, _) = against(
            Reply::ok(SERVE_OUTPUT),
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(
            answer["url"],
            json!("https://workstation.example-tailnet.ts.net/")
        );
        assert!(
            answer["printed"]
                .as_str()
                .is_some_and(|p| p.contains("Serve started")),
            "the client's own report should come back: {answer}"
        );
    }

    // -- taking it down again -------------------------------------------------

    #[tokio::test]
    async fn turning_one_endpoint_off_leaves_every_other_handler_alone() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { serve_off(&ctx, p).await },
            ServeOffParams {
                https: Some(8443),
                set_path: Some("/api".to_owned()),
                ..ServeOffParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            [
                "serve",
                "--bg=true",
                "--yes=true",
                "--https=8443",
                "--set-path=/api",
                "off",
            ]
        );
        assert_eq!(answer["target"], json!("off"));
        assert_eq!(answer["endpoint"], json!("https:8443"));
    }

    #[tokio::test]
    async fn a_reset_names_no_endpoint_because_it_removes_them_all() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { serve_reset(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(only(&argv), ["serve", "reset"]);
        assert_eq!(answer["scope"], json!("this node"));
    }

    // -- the service lifecycle ------------------------------------------------

    #[tokio::test]
    async fn each_service_command_passes_the_service_and_nothing_else() {
        /// The three differ only in the word, so they are checked the same way.
        macro_rules! runs {
            ($handler:ident, $subcommand:literal) => {{
                let (answer, argv) = against(
                    Reply::ok(""),
                    |ctx, p| async move { $handler(&ctx, p).await },
                    ServiceParams {
                        service: "web".to_owned(),
                    },
                )
                .await;
                assert_eq!(only(&argv), ["serve", $subcommand, "svc:web"]);
                assert_eq!(answer["scope"], json!("svc:web"));
                assert!(answer["outcome"].as_str().is_some_and(|o| !o.is_empty()));
            }};
        }
        runs!(serve_drain, "drain");
        runs!(serve_clear, "clear");
        runs!(serve_advertise, "advertise");
    }

    // -- the configuration exchange -------------------------------------------

    #[tokio::test]
    async fn reading_the_configuration_asks_for_a_scope_and_gets_the_document() {
        let (answer, argv) = against(
            Reply::ok("{\"version\": \"0.0.1\"}\n"),
            |ctx, p| async move { serve_get_config(&ctx, p).await },
            GetConfigParams {
                service: None,
                all: true,
            },
        )
        .await;
        assert_eq!(only(&argv), ["serve", "get-config", "--all=true"]);
        assert_eq!(answer["scope"], json!("all"));
        assert_eq!(answer["configuration"], json!({"version": "0.0.1"}));
    }

    #[tokio::test]
    async fn one_service_is_read_by_the_name_the_client_uses() {
        let (answer, argv) = against(
            Reply::ok("{}"),
            |ctx, p| async move { serve_get_config(&ctx, p).await },
            GetConfigParams {
                service: Some("web".to_owned()),
                all: false,
            },
        )
        .await;
        assert_eq!(only(&argv), ["serve", "get-config", "--service=svc:web"]);
        assert_eq!(answer["scope"], json!("svc:web"));
    }

    #[tokio::test]
    async fn a_configuration_exchange_names_exactly_one_scope() {
        for (service, all) in [(None, false), (Some("web".to_owned()), true)] {
            let error = refused(
                |ctx, p| async move { serve_get_config(&ctx, p).await },
                GetConfigParams { service, all },
            )
            .await;
            assert_eq!(error.code, ErrorCode::InvalidArgs);
        }
    }

    #[tokio::test]
    async fn writing_the_configuration_hands_the_client_a_file_that_does_not_outlive_the_call() {
        let document = json!({"version": "0.0.1", "services": {}});
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { serve_set_config(&ctx, p).await },
            SetConfigParams {
                configuration: document.clone(),
                service: None,
                all: true,
            },
        )
        .await;
        let args = only(&argv);
        assert_eq!(&args[..3], ["serve", "set-config", "--all=true"]);
        let path = Path::new(&args[3]);
        assert!(
            !path.exists(),
            "the configuration file should be gone once the call is over: {}",
            path.display()
        );
        assert_eq!(answer["configuration"], document);
    }

    #[test]
    fn the_document_that_comes_out_is_the_document_that_goes_back_in() {
        // What "reading the configuration and writing it back unchanged is a
        // no-op" means on this side of the call: nothing is added, dropped or
        // reshaped between the two.
        let printed = "{\n  \"version\": \"0.0.1\",\n  \"services\": {}\n}\n";
        let read = parse_config(printed).expect("the client printed JSON");
        let file = config_file(&read).expect("the document is written");
        let written: Value =
            serde_json::from_slice(&file.read().expect("the file reads back")).unwrap();
        assert_eq!(written, read);
    }

    #[test]
    fn a_document_the_client_did_not_print_is_reported_rather_than_guessed_at() {
        let error = parse_config("Serve is not configured\n").expect_err("that is not JSON");
        assert_eq!(error.code, ErrorCode::CliFailed);
    }

    // -- the internet ---------------------------------------------------------

    #[tokio::test]
    async fn publishing_to_the_internet_says_so_in_the_answer() {
        let (public, _) = against(
            Reply::ok(SERVE_OUTPUT),
            |ctx, p| async move { funnel_set(&ctx, p).await },
            FunnelSetParams {
                target: "3000".to_owned(),
                ..FunnelSetParams::default()
            },
        )
        .await;
        assert_eq!(public["public"], json!(true));

        let (tailnet_only, _) = against(
            Reply::ok(SERVE_OUTPUT),
            |ctx, p| async move { serve_set(&ctx, p).await },
            ServeSetParams {
                target: "3000".to_owned(),
                ..ServeSetParams::default()
            },
        )
        .await;
        assert_eq!(tailnet_only["public"], json!(false));
    }

    #[tokio::test]
    async fn a_funnel_that_the_tailnet_has_not_enabled_hands_back_how_to_enable_it() {
        // The client does not fail here: it prints the enrolment URL and waits
        // for ever, so the bound on the call is what turns it into an answer.
        let backend = Arc::new(StubBackend::always(Reply::hung_after(
            "Funnel is not enabled on your tailnet.\n\
             To enable, visit: https://login.example.com/f/deadbeef\n",
        )));
        let ctx = context(Arc::clone(&backend));
        let error = funnel_set(
            &ctx,
            FunnelSetParams {
                target: "3000".to_owned(),
                ..FunnelSetParams::default()
            },
        )
        .await
        .expect_err("a client that never returns is not a success");
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(
            error
                .message
                .contains("https://login.example.com/f/deadbeef"),
            "the answer should carry what it was waiting on: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn stopping_a_funnel_takes_the_endpoint_off_the_internet() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { funnel_off(&ctx, p).await },
            FunnelOffParams {
                https: Some(8443),
                ..FunnelOffParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            ["funnel", "--bg=true", "--yes=true", "--https=8443", "off"]
        );
        assert_eq!(answer["public"], json!(false));
    }

    // -- the shape of the toolset ---------------------------------------------

    #[test]
    fn the_toolset_holds_the_ten_commands_that_publish_from_this_node() {
        let names: Vec<&str> = entries().iter().map(|e| e.meta.name).collect();
        assert_eq!(
            names,
            [
                "tailscale_serve_set",
                "tailscale_serve_off",
                "tailscale_serve_reset",
                "tailscale_serve_drain",
                "tailscale_serve_clear",
                "tailscale_serve_advertise",
                "tailscale_serve_get_config",
                "tailscale_serve_set_config",
                "tailscale_funnel_set",
                "tailscale_funnel_off",
            ]
        );
        for entry in entries() {
            assert_eq!(entry.meta.toolset, Toolset::LocalServe);
        }
    }

    #[test]
    fn funnel_is_out_of_reach_until_the_destructive_tier_is_allowed() {
        // The acceptance criterion the tier model exists for: publishing to
        // the internet is never something a write-tier session can do.
        for entry in entries() {
            if entry.meta.name.starts_with("tailscale_funnel_") {
                assert_eq!(
                    entry.meta.tier,
                    Tier::Destructive,
                    "`{}` publishes to or unpublishes from the internet",
                    entry.meta.name
                );
            }
        }
    }

    #[test]
    fn serving_to_the_tailnet_needs_no_more_than_the_write_tier() {
        // Everything a caller needs to expose a server on the tailnet and take
        // it down again, without granting the destructive tier. `reset` and
        // `clear` are the exceptions and are named here so that moving one of
        // the others is a deliberate act.
        for name in [
            "tailscale_serve_set",
            "tailscale_serve_off",
            "tailscale_serve_drain",
            "tailscale_serve_advertise",
            "tailscale_serve_get_config",
            "tailscale_serve_set_config",
        ] {
            let entry = entries()
                .into_iter()
                .find(|e| e.meta.name == name)
                .expect("the tool is declared");
            assert!(
                entry.meta.tier <= Tier::Write,
                "`{name}` should not need the destructive tier"
            );
        }
    }

    #[test]
    fn removing_everything_asks_first() {
        for name in [
            "tailscale_serve_reset",
            "tailscale_serve_clear",
            "tailscale_funnel_set",
        ] {
            let entry = entries()
                .into_iter()
                .find(|e| e.meta.name == name)
                .expect("the tool is declared");
            assert!(
                entry.meta.requires_confirmation,
                "`{name}` should not act without being asked twice"
            );
        }
    }
}
