//! The things this binary does other than serve.
//!
//! An MCP server spends its life talking to a program, which makes it awkward
//! to ask a question of. These are the questions worth asking from a terminal:
//! *is my setup broken*, *what would this actually offer*, *what version is
//! this*, *what do I paste into my client*, and *would the control plane take
//! this policy*.
//!
//! Five jobs and three files, split by what makes each change: a new
//! credential kind changes the diagnosis, a new editor changes [`setup()`], and
//! a policy tool being renamed changes [`policy()`]. This module holds the three
//! that share the metadata table and the [`Report`].
//!
//! **What needs a credential.** [`diagnose`] and [`policy()`] do; the latter
//! also builds a server, because sending a policy the way a tool sends one
//! means using the client a session would have used. [`tools`], [`version`]
//! and [`setup()`] read compiled-in data and touch nothing — no credential, no
//! `tailscale` binary, no network — which is the criterion "no subcommand
//! requires credentials except the ones that check them".
//!
//! **Exit codes are the contract.** Diagnosis exits non-zero when a check
//! fails, because the shape it is meant for is a shell script or a container
//! health check, and a diagnosis that always succeeds is a diagnosis nobody
//! can automate.

pub mod policy;
pub mod setup;

use std::collections::BTreeSet;
use std::process::ExitCode;

use serde::Serialize;

use crate::config::Config;
use crate::gating::Gate;
use crate::meta::{Surface, ToolMeta};
use crate::server::Backends;
use crate::tools::common::pretty;
use crate::version::SUPPORTED_FLOOR;
use tailscale_rest::credentials::Credentials;

pub use policy::policy;
pub use setup::setup;

/// What a subcommand printed, and whether it was bad news.
///
/// Returned rather than exiting, so that a test can call these directly rather
/// than spawning the binary to find out what it said (spec: "Tests
/// preferentially live here").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub text: String,
    pub ok: bool,
}

impl Report {
    fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ok: true,
        }
    }

    /// Print it and become an exit code.
    #[allow(clippy::print_stdout, reason = "a subcommand's output is its point")]
    pub fn emit(&self) -> ExitCode {
        print!("{}", self.text);
        if self.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// How a check came out.
///
/// Three states and not two: a check the operator switched off did not pass,
/// it did not happen, and reporting it as a pass would be telling them their
/// credential is fine when nothing looked at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Passed,
    Skipped,
    Failed,
}

impl State {
    /// Whether this state should stop the exit code being zero.
    const fn is_failure(self) -> bool {
        matches!(self, Self::Failed)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Passed => "ok  ",
            Self::Skipped => "--  ",
            Self::Failed => "FAIL",
        }
    }
}

/// One thing that can be wrong with a setup.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// What was looked at.
    pub what: &'static str,
    /// How it came out.
    pub state: State,
    /// What was found, in one line.
    pub detail: String,
    /// What to do about it, when there is something to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

impl Check {
    fn passed(what: &'static str, detail: impl Into<String>) -> Self {
        Self {
            what,
            state: State::Passed,
            detail: detail.into(),
            remedy: None,
        }
    }

    /// A check the operator switched off, reported so that they can see it was
    /// not made.
    fn skipped(what: &'static str, detail: impl Into<String>) -> Self {
        Self {
            what,
            state: State::Skipped,
            detail: detail.into(),
            remedy: None,
        }
    }

    fn failed(what: &'static str, detail: impl Into<String>, remedy: impl Into<String>) -> Self {
        Self {
            what,
            state: State::Failed,
            detail: detail.into(),
            remedy: Some(remedy.into()),
        }
    }
}

/// Check everything a broken setup is usually broken in.
///
/// Each check is reported whatever the others did: a missing `tailscale`
/// binary must not stop the credential from being checked, because an operator
/// running this wants the whole list, not the first thing to go wrong.
pub async fn diagnose(config: &Config, backends: Backends, json: bool) -> Report {
    let mut checks = Vec::new();

    checks.push(if backends.local_available {
        match crate::cli::probe_version(backends.local.as_ref()).await {
            Some(version) if version < SUPPORTED_FLOOR && !version.is_unstable() => Check::failed(
                "tailscale CLI",
                format!("found, reporting {version}"),
                format!(
                    "upgrade to {} or newer; older commands are still attempted, and \
                         report the version they need",
                    SUPPORTED_FLOOR
                ),
            ),
            Some(version) => Check::passed("tailscale CLI", format!("found, reporting {version}")),
            None => Check::failed(
                "tailscale CLI",
                "found, but its version could not be read",
                "run `tailscale version` and check it prints a version",
            ),
        }
    } else if config.is_disabled(Surface::Local) {
        Check::skipped("tailscale CLI", "not looked for: --no-local was given")
    } else {
        Check::failed(
            "tailscale CLI",
            "not found on the path",
            "install Tailscale, or point --cli-path at the binary",
        )
    });

    checks.push(
        match (&backends.credentials, config.is_disabled(Surface::Tailnet)) {
            (_, true) => Check::skipped(
                "control-plane credential",
                "not looked for: --no-tailnet was given",
            ),
            (Some(credentials), _) => Check::passed(
                "control-plane credential",
                // Named exhaustively, with no wildcard: a fourth kind of
                // credential should stop this compiling rather than be
                // reported as whichever one the wildcard happened to name.
                // The glossary reserves "API key"; it is an access token.
                match credentials {
                    Credentials::ApiKey(_) => "an API access token, from TAILSCALE_API_KEY",
                    Credentials::OauthClient { .. } => {
                        "an OAuth client, from TAILSCALE_OAUTH_CLIENT_ID and _SECRET"
                    }
                    Credentials::Federated { .. } => {
                        "a workload identity, from the platform's own token file"
                    }
                },
            ),
            (None, _) => Check::failed(
                "control-plane credential",
                "none found",
                "set TAILSCALE_API_KEY, or TAILSCALE_OAUTH_CLIENT_ID and \
             TAILSCALE_OAUTH_CLIENT_SECRET",
            ),
        },
    );

    checks.push(reachability(config, &backends).await);

    let ok = !checks.iter().any(|check| check.state.is_failure());
    if json {
        return Report {
            text: pretty(&serde_json::json!({"ok": ok, "checks": checks})),
            ok,
        };
    }

    let mut text = String::new();
    for check in &checks {
        text.push_str(&format!(
            "{} {:<26} {}\n",
            check.state.label(),
            check.what,
            check.detail
        ));
        if let Some(remedy) = &check.remedy {
            text.push_str(&format!("     {remedy}\n"));
        }
    }
    Report { text, ok }
}

/// Whether the control plane answers, which is the only check that leaves this
/// machine.
async fn reachability(config: &Config, backends: &Backends) -> Check {
    if config.is_disabled(Surface::Tailnet) {
        return Check::skipped("control plane", "not reached: --no-tailnet was given");
    }
    let Some(credentials) = backends.credentials.clone() else {
        return Check::failed(
            "control plane",
            "not reached: there is no credential to reach it with",
            "set a credential, then run this again",
        );
    };
    let mut settings = tailscale_rest::ClientConfig::new(credentials);
    settings.base_url = config.api_base_url.clone();
    settings.tailnet = config.tailnet.clone();
    settings.budget = tailscale_cli::DEFAULT_TIMEOUT;
    let client = match tailscale_rest::Client::new(settings) {
        Ok(client) => client,
        Err(error) => {
            return Check::failed(
                "control plane",
                format!("no client could be built: {error}"),
                "check TAILSCALE_MCP_API_BASE_URL",
            );
        }
    };
    // The cheapest authenticated read there is: it says the credential works,
    // the tailnet exists, and the network is there, in one round trip.
    match client
        .get(client.tailnet_path(None, "/devices"))
        .send_as::<serde_json::Value>()
        .await
    {
        Ok(_) => Check::passed("control plane", format!("{} answered", config.api_base_url)),
        Err(error) => Check::failed(
            "control plane",
            format!("{} refused: {error}", config.api_base_url),
            match error.status() {
                Some(401 | 403) => {
                    "the credential is not accepted; check it has not expired \
                                    and has the scopes the tools need"
                }
                Some(404) => "the tailnet was not found; check TAILSCALE_TAILNET",
                _ => "check network access to the control plane",
            },
        ),
    }
}

/// One row of the tool listing.
#[derive(Debug, Serialize)]
struct Listed {
    name: &'static str,
    toolset: &'static str,
    surface: &'static str,
    tier: &'static str,
    /// Whether `tier` is a floor rather than the whole truth, which for three
    /// rows it is. Written only where it is true, so that a row without the
    /// field means what it always meant.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    tier_is_a_floor: bool,
    summary: &'static str,
}

/// What a preset and tier would offer, without starting anything.
///
/// Read from the metadata table, which `spec.md` names as "the single source
/// for the tool-listing subcommand, the contract tests and the README's tool
/// table" — so this counts what the server would offer rather than what
/// anybody wrote down.
pub fn tools(config: &Config, json: bool) -> Report {
    let metas: Vec<ToolMeta> = crate::tools::entries()
        .iter()
        .map(|entry| entry.meta)
        .collect();
    // Nothing *discovered* is unavailable: this asks what the selection
    // offers, not what this machine happens to have, so that the answer is the
    // same everywhere. A surface the operator switched off is not that — it is
    // part of the selection, like the preset and the toolsets, and this
    // subcommand accepts `--no-local` and `--no-tailnet` and documents what
    // they do. Leaving them out reported 57 tools where the server it is
    // describing would have served 29, which `server::build` warns about in
    // as many words: the check is not only in `discover` so that a caller
    // assembling the surfaces itself cannot bypass the flag.
    let switched_off: BTreeSet<Surface> = [Surface::Local, Surface::Tailnet]
        .into_iter()
        .filter(|surface| config.is_disabled(*surface))
        .collect();
    let gate = Gate::unchecked(config.toolsets.clone(), config.max_tier, switched_off);
    let mut shown: Vec<&ToolMeta> = metas.iter().filter(|meta| gate.permits(meta)).collect();
    shown.sort_by_key(|meta| (meta.toolset.as_str(), meta.name));

    if json {
        let rows: Vec<Listed> = shown.iter().map(|meta| row(meta)).collect();
        return Report::ok(pretty(&serde_json::json!({
            "preset": config.preset.as_str(),
            "tier": config.max_tier.as_str(),
            // The same list the `tools` array and `count` beside it are drawn
            // from. Reporting the selection here put nine tailnet toolsets in
            // a document whose every tool was local.
            "toolsets": gate.offered_toolsets().map(|toolset| toolset.as_str()).collect::<Vec<_>>(),
            "count": rows.len(),
            "tools": rows,
        })));
    }

    let mut text = format!(
        "preset {}, tier {}, {} of {} tools\n\n",
        config.preset.as_str(),
        config.max_tier.as_str(),
        shown.len(),
        metas.len()
    );
    // The tier column shows the row's tier, and for three rows that is a floor
    // rather than the whole truth. `docs/tools.md` has a notes column and says
    // so there; this has neither the column nor the room, and a summary cut to
    // its first sentence drops the very clause that would have said it. So the
    // marker carries it, and appears only in a listing that has one to explain.
    if shown.iter().any(|meta| meta.varying_tier) {
        text.push_str(
            "`+` marks a tier that is a floor: some arguments to that tool need \
             a higher one.\n\n",
        );
    }
    let widest = shown.iter().map(|m| m.name.len()).max().unwrap_or(0);
    let mut toolset = None;
    for meta in &shown {
        if toolset != Some(meta.toolset) {
            toolset = Some(meta.toolset);
            text.push_str(&format!("{}\n", meta.toolset.as_str()));
        }
        let tier = if meta.varying_tier {
            format!("{}+", meta.tier.as_str())
        } else {
            meta.tier.as_str().to_owned()
        };
        text.push_str(&format!(
            "  {:<widest$}  {:<12}  {}\n",
            meta.name,
            tier,
            first_sentence(meta.summary)
        ));
    }
    Report::ok(text)
}

/// Enough of a summary for a table: the first sentence, and not much of it.
///
/// The whole summary is what a client is given and is as long as it needs to
/// be; a terminal column is not the place for it.
fn first_sentence(summary: &str) -> String {
    const WIDEST: usize = 72;
    let sentence = summary
        .split_once(". ")
        .map_or(summary, |(first, _)| first)
        .trim_end_matches('.');
    if sentence.chars().count() <= WIDEST {
        return sentence.to_owned();
    }
    let cut = sentence
        .char_indices()
        .take(WIDEST - 1)
        .last()
        .map_or(0, |(at, _)| at);
    let kept = sentence[..cut].trim_end();
    format!(
        "{}…",
        kept.rsplit_once(' ').map_or(kept, |(before, _)| before)
    )
}

fn row(meta: &ToolMeta) -> Listed {
    Listed {
        name: meta.name,
        toolset: meta.toolset.as_str(),
        surface: meta.toolset.surface().as_str(),
        tier: meta.tier.as_str(),
        tier_is_a_floor: meta.varying_tier,
        summary: meta.summary,
    }
}

/// This server's version, and what protocol versions it can speak.
pub fn version() -> Report {
    let latest = rmcp::model::ProtocolVersion::LATEST;
    // `KNOWN_VERSIONS` contains the newest one too, and it is named by itself
    // immediately before this list. "Also" is a claim about the others, so
    // repeating it there said the server speaks a version in addition to
    // itself.
    let others: Vec<String> = rmcp::model::ProtocolVersion::KNOWN_VERSIONS
        .iter()
        .filter(|known| **known != latest)
        .map(ToString::to_string)
        .collect();
    // A build where the newest is the only one is not a build that should
    // print an empty parenthesis.
    let rest = if others.is_empty() {
        String::new()
    } else {
        format!(" (also speaks {})", others.join(", "))
    };
    Report::ok(format!(
        "{} {}\nrmcp {RMCP_VERSION}\nMCP protocol {latest}{rest}\n",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ))
}

/// The version of rmcp this is built against.
///
/// Written here and held to the manifest by a test, because there is no way to
/// read a dependency's version at runtime: `DEP_*` variables reach only the
/// build script of a crate that declares `links`, which rmcp does not, so the
/// obvious `option_env!` would have been silently `None` for ever (Q97).
pub const RMCP_VERSION: &str = "3.2.0";
