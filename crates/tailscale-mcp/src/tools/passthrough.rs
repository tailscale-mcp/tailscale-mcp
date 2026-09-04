//! The one tool that runs a command this server has no tool for.
//!
//! Every other tool on the local surface is a fixed argument list with typed
//! parameters. This one takes the argument list from the caller, which is a
//! different kind of promise and so is off in every preset, `full` included: a
//! session reaches it by naming `local-passthrough`, and naming it is the
//! statement that the caller accepts commands this server has not vetted.
//!
//! ## How a command is judged
//!
//! The tool's row carries [`Tier::Read`], which is a floor and not the truth —
//! see [`ToolMeta::varying_tier`](crate::meta::ToolMeta::varying_tier). What
//! decides the tier is the command, and there are three answers:
//!
//! - [`COVERED`] names the commands a typed tool already runs, each at that
//!   tool's own tier and confirmation. `tailscale down` is destructive here
//!   because `tailscale_down` is destructive there, and needs the same
//!   `confirm: true`. A test drives every typed tool and pins this table to
//!   what they actually run, so the two cannot drift apart quietly.
//! - The two exclusion lists — this module's own and
//!   [`local_debug::EXCLUDED`], which [`excluded`] chains together — name the
//!   commands this server never runs, each with the reason the caller is
//!   shown. Closing a toolset is not a way in and neither is this.
//! - Anything else is unknown, and unknown counts as destructive. A command
//!   nobody has judged is judged at the top.
//!
//! ## Reading the command out of the arguments
//!
//! This server does not parse the client's flags and should not have to, so it
//! reads the command twice and keeps whichever reading is stricter. One reading
//! is the leading run of words that are not flags; the other is every word that
//! is not a flag. Each is matched deepest-first, up to `MAX_DEPTH` words, so
//! that `lock remove` is not read as `lock`.
//!
//! Neither reading is sound alone, and each fails in the opposite direction.
//! `tailscale serve --bg reset` runs `serve reset`, which the leading reading
//! calls `serve`: a write-tier session would wipe the serve configuration with
//! no confirmation. `tailscale funnel --set-path status 8080` runs `funnel`,
//! which the every-word reading calls `funnel status`: a destructive command
//! read as a reader. Working out which applies is the flag parsing this
//! refuses to do, and taking the stricter of the two is wrong only ever in the
//! direction of a tier or a confirmation the caller did not need. Case and
//! surrounding spaces are dropped before matching, because the client's own
//! subcommand lookup ignores case at every depth: `["DEBUG", "PREFS"]` runs
//! `debug prefs`.
//!
//! What no reading rescues is a path that stops partway into something
//! excluded. `["debug", "--file=get"]` has no word after `debug`, and letting
//! it through as unknown would run a `debug`-anything at the destructive tier,
//! so it is refused outright rather than guessed at. That is also why
//! `tailscale_debug_file_list` exists as a typed tool: its subcommand is a flag
//! on the parent, which is useful and is unreadable here.
//!
//! Two commands are deliberately absent from every table. `debug reload-config`
//! is in neither list and so is unknown-destructive, which is what DECISIONS
//! Q44 asks for: runnable, but never on this server's recommendation.
//! `completion` has no typed tool to inherit from, and inventing a tier for it
//! would be the first row nothing checks.
//!
//! ## What the caller gets
//!
//! An argument list, never a string: nothing here is parsed by a shell, so a
//! semicolon or a quote in an argument is a semicolon or a quote in an
//! argument. The command line reaches the log and any error redacted, because
//! unlike every typed tool's arguments these were not built here.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_cli::Invocation;

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};
use crate::meta::Tier;
use crate::tools::common::{Excluded, bounded_wait, printed, report};
use crate::tools::local_debug;

crate::tools! {
    /// Run a `tailscale` subcommand that no other tool covers, given as a list
    /// of arguments. Prefer a typed tool wherever one exists: they validate
    /// what they are given and answer with structured data rather than the
    /// text a person would read. What this is allowed to run depends on the
    /// command — one a typed tool covers takes that tool's tier and
    /// confirmation, an unrecognised one counts as destructive, and a command
    /// this server never runs is refused with the reason why.
    tailscale_run => RunParams, run,
        toolset: LocalPassthrough, tier: Read, varying: true;
}

/// The deepest command path either table names, and so the most words worth
/// matching. Five paths are that deep, all of them under `configure`;
/// `every_path_fits_the_depth_the_matcher_searches` holds the rest shallower.
const MAX_DEPTH: usize = 3;

/// How long a passthrough command may run before this server gives up, and the
/// bound it uses when the caller names none.
const DEFAULT_RUN_TIMEOUT: u64 = 30;
const MAX_RUN_TIMEOUT: u64 = 300;

/// A command a typed tool already runs, and the terms it runs on there.
///
/// Neither field is this module's opinion. Both are read off the typed tool's
/// own row, and `the_covered_table_follows_the_tools_it_claims_to_follow`
/// re-derives them from the arguments those tools actually build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Covered {
    /// The command as it is written after `tailscale`, words separated by
    /// spaces.
    pub path: &'static str,
    /// The tier of the typed tool that runs it. Where two tools share a path,
    /// the higher of them: `switch` is both `tailscale_switch_list` and
    /// `tailscale_switch_profile`, and a caller writing `switch` unqualified
    /// may mean either.
    pub tier: Tier,
    /// Whether that tool requires a confirmation, for the same reason.
    pub confirm: bool,
}

/// Every command a typed tool covers, with the terms that tool runs it on.
///
/// Grouped as the modules are, so that a tool added there has an obvious home
/// here. Debug's twenty-nine come last, and its thirtieth tool is not among
/// them: `tailscale_debug_file_list` runs `debug --file=get`, a flag on the
/// parent command rather than a subcommand, which has no path to name.
pub const COVERED: &[Covered] = &[
    // -- local-prefs --------------------------------------------------------
    Covered {
        path: "up",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "down",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "set",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "get",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "login",
        tier: Tier::Write,
        confirm: true,
    },
    Covered {
        path: "logout",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "switch",
        tier: Tier::Write,
        confirm: true,
    },
    Covered {
        path: "switch remove",
        tier: Tier::Destructive,
        confirm: true,
    },
    // -- local-status -------------------------------------------------------
    Covered {
        path: "status",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "ip",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "netcheck",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "ping",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "whois",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "whoami",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "version",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "licenses",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "bugreport",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "appc-routes",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "routecheck",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "wait",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "dns status",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "dns query",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "exit-node list",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "exit-node suggest",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "metrics print",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "service list",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "syspolicy list",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "configure sysext status",
        tier: Tier::Read,
        confirm: false,
    },
    // -- local-serve --------------------------------------------------------
    Covered {
        path: "serve",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "serve status",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "serve reset",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "serve drain",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "serve clear",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "serve advertise",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "serve get-config",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "serve set-config",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "funnel",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "funnel status",
        tier: Tier::Read,
        confirm: false,
    },
    // -- local-files --------------------------------------------------------
    Covered {
        path: "file cp",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "file get",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "cert",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "metrics write",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "configure kubeconfig",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "syspolicy reload",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "drive list",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "drive share",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "drive rename",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "drive unshare",
        tier: Tier::Destructive,
        confirm: false,
    },
    // -- local-lock ---------------------------------------------------------
    Covered {
        path: "lock status",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "lock log",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "lock init",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "lock add",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "lock remove",
        tier: Tier::Destructive,
        confirm: false,
    },
    Covered {
        path: "lock sign",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "lock disable",
        tier: Tier::Destructive,
        confirm: true,
    },
    Covered {
        path: "lock disablement-kdf",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "lock local-disable",
        tier: Tier::Destructive,
        confirm: false,
    },
    Covered {
        path: "lock revoke-keys",
        tier: Tier::Destructive,
        confirm: true,
    },
    // -- local-debug --------------------------------------------------------
    Covered {
        path: "debug derp-map",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug netmap",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug hostinfo",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug control-knobs",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug daemon-goroutines",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug daemon-bus-graph",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug daemon-bus-queues",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug metrics",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug statedir",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug go-buildinfo",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug peer-relay-servers",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug peer-relay-sessions",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug stat",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug via",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug watch-ipn",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug peer-endpoint-changes",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug resolve",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug dial-types",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug derp",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug ts2021",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug portmap",
        tier: Tier::Read,
        confirm: false,
    },
    Covered {
        path: "debug component-logs",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug restun",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug rebind",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug rotate-disco-key",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug derp-unset-on-demand",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug pick-new-derp",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug force-prefer-derp",
        tier: Tier::Write,
        confirm: false,
    },
    Covered {
        path: "debug force-netmap-update",
        tier: Tier::Write,
        confirm: false,
    },
];

/// Every documented command this server will not run at all.
///
/// Three of the four grounds `CONTEXT.md` names are here: it runs in the
/// foreground with no end, it wires a stream to a terminal this server does not
/// have, or it changes the host outside Tailscale. The fourth — printing a
/// secret — belongs to `local_debug::EXCLUDED`, which holds the hidden half of
/// the same rule. [`excluded`] is the two of them together, and is how anything
/// outside this module reads either.
const EXCLUDED: &[Excluded] = &[
    Excluded {
        path: "ssh",
        reason: "it opens an interactive session on a peer, which needs a terminal \
                 this server does not have",
    },
    Excluded {
        path: "nc",
        reason: "it wires a socket to standard input and output, which a tool call \
                 has nothing to connect",
    },
    Excluded {
        path: "web",
        reason: "it runs a web server in the foreground until interrupted",
    },
    Excluded {
        path: "systray",
        reason: "it runs a desktop application in the foreground until quit",
    },
    Excluded {
        path: "update",
        reason: "it replaces the `tailscale` binary this server is talking to; \
                 update the host the way the host is normally updated",
    },
    Excluded {
        path: "configure sysext activate",
        reason: "it installs a system extension and reboots into it, which is a \
                 change to the host rather than to the tailnet",
    },
    Excluded {
        path: "configure sysext deactivate",
        reason: "it removes a system extension the node may be running on, which is \
                 a change to the host rather than to the tailnet",
    },
    Excluded {
        path: "configure mac-vpn install",
        reason: "it installs a system VPN profile, which is a change to the host \
                 rather than to the tailnet",
    },
    Excluded {
        path: "configure mac-vpn uninstall",
        reason: "it removes a system VPN profile, which is a change to the host \
                 rather than to the tailnet",
    },
];

/// Every command this server refuses, documented and hidden alike.
///
/// The two halves are separate constants because they are each other's
/// context: one is read beside the debug tools, the other beside this one. A
/// caller meets them as a single list, which is what this is for.
pub fn excluded() -> impl Iterator<Item = &'static Excluded> {
    EXCLUDED.iter().chain(local_debug::EXCLUDED)
}

/// What this server knows about a command it was asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    /// A typed tool runs this, and these are its terms.
    Covered(&'static Covered),
    /// This server never runs it, and here is what to tell the caller.
    Excluded(&'static Excluded),
    /// Nobody has judged it, so it is judged at the top.
    Unknown,
}

/// The command a caller named, and what is known about it.
///
/// # Errors
///
/// When the arguments do not name a command this server can read: no bare word
/// at all, or bare words that stop partway into something excluded.
pub fn classify(args: &[String]) -> ToolResult<(String, Known)> {
    // Two readings, because this server does not parse the client's flags and
    // should not have to. The client does parse them: `tailscale serve --bg
    // reset` runs `serve reset`, so reading only the words before the first
    // flag would call it `serve` and let a write-tier session wipe the config
    // with no confirmation. Reading every bare word instead has the opposite
    // fault — `tailscale funnel --set-path status 8080` would read as `funnel
    // status`, a reader, when the flag's value only looks like a subcommand.
    //
    // Neither reading is right on its own, and working out which one applies is
    // the flag parsing this refuses to do. So both are taken and the stricter
    // wins. Being wrong then costs a caller a tier or a confirmation they did
    // not need, which is the direction to be wrong in.
    let leading = words(args.iter().take_while(|arg| !is_flag(arg)));
    let every = words(args.iter().filter(|arg| !is_flag(arg)));

    if every.first().is_none_or(String::is_empty) {
        return Err(ToolError::invalid_args(
            "the first argument has to be a subcommand rather than a flag or an \
             empty string, so that what runs can be read from the arguments",
        ));
    }

    let (path, known) = if leading.is_empty() {
        read(&every)
    } else {
        std::cmp::max_by_key(read(&leading), read(&every), |(_, known)| strictness(known))
    };

    // Unknown is ordinarily destructive-and-run. It is not that when a reading
    // stops partway into a command this server refuses: `["debug",
    // "--file=get"]` has no bare word after `debug`, and letting it through as
    // unknown would run a `debug`-anything on a session with the destructive
    // tier. Refusing the partial path is the only answer that does not depend
    // on parsing flags the way the client's own package would.
    if known == Known::Unknown {
        for reading in [&leading, &every] {
            if excluded().any(|e| is_proper_prefix(reading, e.path)) {
                return Err(ToolError::invalid_args(format!(
                    "`tailscale {}` names only part of a command, and this server \
                     decides what to allow from the words that are not flags. Write \
                     the whole subcommand, with its flags after it.",
                    reading.join(" ")
                )));
            }
        }
    }
    Ok((path, known))
}

/// Whether an argument is a flag rather than a word of the command.
fn is_flag(arg: &str) -> bool {
    arg.starts_with('-')
}

/// One reading's words, in the form the tables are written in.
///
/// Lowercased, because the client matches its own subcommands without regard to
/// case at every depth — `tailscale DEBUG PREFS` runs `debug prefs` — and a
/// table compared with `==` would otherwise be evaded by shouting. Trimmed for
/// the same reason in reverse: the client does not trim, so a padded word names
/// no command there, and reading it as one here only ever refuses more.
fn words<'a>(args: impl Iterator<Item = &'a String>) -> Vec<String> {
    args.map(|arg| arg.trim().to_ascii_lowercase()).collect()
}

/// The deepest command either table names at the start of `words`.
fn read(words: &[String]) -> (String, Known) {
    for depth in (1..=words.len().min(MAX_DEPTH)).rev() {
        let path = words[..depth].join(" ");
        if let Some(found) = excluded().find(|e| e.path == path) {
            return (path, Known::Excluded(found));
        }
        if let Some(found) = COVERED.iter().find(|c| c.path == path) {
            return (path, Known::Covered(found));
        }
    }
    (words.join(" "), Known::Unknown)
}

/// How strict a reading is, for choosing between two readings of one command.
///
/// Nothing is stricter than a command this server does not run at all, so that
/// ranks above every tier; below it, a higher tier is stricter than a lower one
/// and needing a confirmation is stricter than not.
fn strictness(known: &Known) -> (bool, Tier, bool) {
    match known {
        Known::Excluded(_) => (true, Tier::Destructive, true),
        Known::Covered(covered) => (false, covered.tier, covered.confirm),
        Known::Unknown => (false, Tier::Destructive, false),
    }
}

/// Whether `words` is the beginning of `path` without being all of it.
fn is_proper_prefix(words: &[String], path: &str) -> bool {
    let mut theirs = path.split(' ');
    !words.is_empty()
        && words
            .iter()
            .all(|word| theirs.next() == Some(word.as_str()))
        && theirs.next().is_some()
}

/// Run any `tailscale` subcommand.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunParams {
    /// The arguments to `tailscale`, one per element, subcommand first: `["dns",
    /// "status", "--all"]` and never `"dns status --all"`. Nothing here is
    /// parsed by a shell, so a value containing a space, a quote or a semicolon
    /// is one argument and not several.
    pub args: Vec<String>,
    /// Set to true to confirm the command, which some of them need for the same
    /// reason the tool covering them does. A refusal names the command and says
    /// whose terms it is borrowing; what those terms are for is on that tool.
    #[serde(default)]
    pub confirm: bool,
    /// How long to let the command run, in seconds. Clamped to 1..=300, and 30
    /// when it is not given, which is what every typed tool without a reason to
    /// wait longer uses.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// What a passthrough command did.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RunReport {
    /// The command as it ran, with anything secret-shaped removed.
    pub command: String,
    /// The tier it was judged at.
    pub tier: &'static str,
    /// Whether a typed tool covers it. When this is false the tier above is
    /// this server declining to guess rather than something it knows, and a
    /// typed tool is worth looking for.
    pub covered: bool,
    /// Everything the client printed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

async fn run(ctx: &ToolContext, params: RunParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_run;
    let (path, known) = classify(&params.args)?;

    let (tier, confirm) = match known {
        Known::Excluded(excluded) => {
            // No hint: there is no switch that turns this on, which is the
            // whole difference between this refusal and the gate's.
            return Err(ToolError::new(
                ErrorCode::NotPermitted,
                format!(
                    "`tailscale {}` is never run by this server: {}",
                    excluded.path, excluded.reason
                ),
            ));
        }
        Known::Covered(covered) => (covered.tier, covered.confirm),
        Known::Unknown => (Tier::Destructive, false),
    };

    // `Tier::Read` outranks nothing and has no flag, so the two halves are one
    // condition: a tier above the session's is a tier with a switch to name.
    if tier > ctx.max_tier
        && let Some(flag) = tier.flag()
    {
        return Err(ToolError::not_permitted(&format!("tailscale {path}"), flag));
    }
    // Only a covered row asks for a confirmation; an unknown command is
    // destructive without one, because there is no tool whose terms to borrow.
    if confirm && !params.confirm {
        return Err(ToolError::confirmation_required(
            &format!("tailscale {path}"),
            "runs at the same terms as the tool that covers it",
        ));
    }

    let (_, bound) = bounded_wait(params.timeout_seconds, DEFAULT_RUN_TIMEOUT, MAX_RUN_TIMEOUT);
    // Anything that is not a read takes the exclusive lane. A typed tool knows
    // whether its own write can share; this one does not, and queueing behind
    // another mutation costs a wait where guessing wrong costs a race.
    let invocation = if tier == Tier::Read {
        Invocation::read(params.args.clone())
    } else {
        Invocation::mutate(params.args.clone())
    }
    .with_timeout(bound);

    let command = cli::displayed(ctx, &invocation);
    tracing::info!(command = %command, tier = tier.as_str(), "passthrough");
    let output = cli::run(ctx, meta, invocation).await?;
    report(RunReport {
        command,
        tier: tier.as_str(),
        covered: matches!(known, Known::Covered(_)),
        printed: printed(ctx, &output),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Reply, StubBackend, context};
    use std::sync::Arc;

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    fn class(args: &[&str]) -> Known {
        classify(&words(args))
            .expect("the arguments name a command")
            .1
    }

    #[test]
    fn every_path_fits_the_depth_the_matcher_searches() {
        for path in COVERED
            .iter()
            .map(|c| c.path)
            .chain(excluded().map(|e| e.path))
        {
            assert!(
                path.split(' ').count() <= MAX_DEPTH,
                "`{path}` is deeper than the matcher looks"
            );
        }
    }

    #[test]
    fn no_path_is_named_twice() {
        let mut seen: Vec<&str> = COVERED
            .iter()
            .map(|c| c.path)
            .chain(excluded().map(|e| e.path))
            .collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(before, seen.len(), "a command is judged in two places");
    }

    #[test]
    fn every_path_is_written_the_way_the_matcher_compares_it() {
        // The matcher lowercases what it is given and compares with `==`, so a
        // row written in mixed case would sit in the table matching nothing.
        for path in COVERED
            .iter()
            .map(|c| c.path)
            .chain(excluded().map(|e| e.path))
        {
            assert_eq!(path, path.to_ascii_lowercase(), "`{path}` cannot match");
            assert_eq!(path.trim(), path, "`{path}` cannot match");
        }
    }

    #[test]
    fn the_deepest_match_wins() {
        // `lock` alone is in no table, so a shallower match would make
        // `lock remove` unknown rather than destructive-by-its-own-row.
        assert_eq!(
            class(&["lock", "remove", "tlpub:0000"]),
            Known::Covered(&Covered {
                path: "lock remove",
                tier: Tier::Destructive,
                confirm: false,
            })
        );
        assert!(matches!(class(&["lock"]), Known::Unknown));
    }

    #[test]
    fn a_flag_after_the_subcommand_does_not_hide_it() {
        let Known::Covered(covered) = class(&["serve", "--https=443", "off"]) else {
            panic!("`serve` is covered whatever follows it");
        };
        assert_eq!(covered.tier, Tier::Write);
    }

    #[test]
    fn a_flag_before_an_excluded_subcommand_does_not_hide_it() {
        // Reading only the words before the first flag would call this `debug`,
        // which is in no table, and run it as an unknown-destructive command.
        let Known::Excluded(excluded) = class(&["debug", "-v", "prefs"]) else {
            panic!("`debug prefs` is excluded whatever precedes it");
        };
        assert_eq!(excluded.path, "debug prefs");
    }

    #[test]
    fn a_partial_path_into_an_excluded_command_is_refused_rather_than_guessed() {
        // Neither reading gets past `debug`, so there is nothing to judge and
        // letting it through as unknown would run it at the destructive tier.
        let error = classify(&words(&["debug", "--file=get"]))
            .expect_err("`debug` on its own cannot be judged");
        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert!(
            error.message.contains("only part of a command"),
            "{error:?}"
        );
    }

    #[test]
    fn shouting_does_not_evade_the_exclusion_list() {
        // The client matches its own subcommands without regard to case, so
        // `tailscale DEBUG PREFS` runs `debug prefs` and prints private keys.
        let Known::Excluded(excluded) = class(&["DEBUG", "PREFS"]) else {
            panic!("case is not part of a command's name");
        };
        assert_eq!(excluded.path, "debug prefs");
        assert!(matches!(class(&["Status"]), Known::Covered(_)));
    }

    #[test]
    fn a_flag_between_the_words_does_not_hide_a_deeper_subcommand() {
        // `tailscale serve --bg reset` runs `serve reset`. Reading `serve`
        // alone would let a write-tier session wipe the config unconfirmed.
        let (path, known) =
            classify(&words(&["serve", "--bg", "reset"])).expect("`serve reset` is readable");
        assert_eq!(path, "serve reset");
        let Known::Covered(covered) = known else {
            panic!("`serve reset` has a row");
        };
        assert_eq!((covered.tier, covered.confirm), (Tier::Destructive, true));
    }

    #[test]
    fn a_flag_value_that_only_looks_like_a_subcommand_keeps_the_stricter_terms() {
        // The other direction: `status` here is the value of `--set-path`, and
        // reading it as `funnel status` would drop a destructive command to a
        // reader. Neither reading can be trusted alone, so the stricter wins.
        let (path, known) = classify(&words(&["funnel", "--set-path", "status", "8080"]))
            .expect("`funnel` is readable");
        assert_eq!(path, "funnel");
        let Known::Covered(covered) = known else {
            panic!("`funnel` has a row");
        };
        assert_eq!((covered.tier, covered.confirm), (Tier::Destructive, true));
    }

    #[test]
    fn a_blank_first_argument_is_refused_rather_than_run_as_a_nameless_command() {
        // `["", "down"]` would otherwise read as the unknown command `" down"`
        // and run at the destructive tier as `tailscale "" down`.
        for args in [vec![""], vec!["", "down"], vec!["   "]] {
            let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
            let error = classify(&args).expect_err("a blank subcommand cannot be read");
            assert_eq!(error.code, ErrorCode::InvalidArgs);
        }
        // A blank word after the subcommand is one of its arguments, and the
        // subcommand is still read.
        let args: Vec<String> = ["ping", ""].into_iter().map(str::to_owned).collect();
        let (path, _) = classify(&args).expect("`ping` is still readable");
        assert_eq!(path, "ping");
    }

    #[test]
    fn a_leading_flag_does_not_hide_the_subcommand_behind_it() {
        // The leading-words reading is empty here, so only the other one has
        // anything to say, and it says `status`.
        let (path, known) =
            classify(&words(&["--socket=/tmp/x", "status"])).expect("`status` is still named");
        assert_eq!(path, "status");
        assert!(matches!(known, Known::Covered(_)));
    }

    #[test]
    fn an_unjudged_command_is_destructive() {
        assert!(matches!(class(&["nonesuch"]), Known::Unknown));
        assert_eq!(
            strictness(&class(&["nonesuch"])),
            (false, Tier::Destructive, false)
        );
    }

    #[test]
    fn reload_config_stays_runnable() {
        // DECISIONS Q44: in neither table, so unknown and destructive rather
        // than refused.
        assert!(matches!(class(&["debug", "reload-config"]), Known::Unknown));
    }

    /// Run the handler against a scripted client and report what it ran.
    async fn against(reply: Reply, params: RunParams) -> (ToolResult<Value>, Vec<Vec<String>>) {
        let backend = Arc::new(StubBackend::always(reply));
        let ctx = context(backend.clone());
        let answer = run(&ctx, params).await;
        (answer, backend.argv())
    }

    fn params(args: &[&str]) -> RunParams {
        RunParams {
            args: words(args),
            confirm: false,
            timeout_seconds: None,
        }
    }

    #[tokio::test]
    async fn an_argument_with_shell_characters_arrives_as_one_argument() {
        let awkward = "a b; rm -rf /$(echo hi) 'quoted' \"also\"|&";
        let (answer, argv) = against(
            Reply::ok("ok\n"),
            RunParams {
                args: words(&["ping", awkward]),
                ..params(&[])
            },
        )
        .await;
        answer.expect("a read runs");
        assert_eq!(argv, vec![vec!["ping".to_owned(), awkward.to_owned()]]);
    }

    #[tokio::test]
    async fn a_covered_command_needing_confirmation_refuses_without_one() {
        let (answer, argv) = against(Reply::ok(""), params(&["down"])).await;
        let error = answer.expect_err("`down` confirms");
        assert_eq!(error.code, ErrorCode::ConfirmationRequired);
        assert!(argv.is_empty(), "nothing should have run");
    }

    #[tokio::test]
    async fn an_excluded_command_is_refused_with_its_reason_and_no_hint() {
        let (answer, argv) = against(Reply::ok(""), params(&["debug", "prefs"])).await;
        let error = answer.expect_err("`debug prefs` is excluded");
        assert_eq!(error.code, ErrorCode::NotPermitted);
        assert!(error.message.contains("private keys"), "{error:?}");
        assert!(
            error.hint.is_none(),
            "no switch turns this on, so nothing should suggest one"
        );
        assert!(argv.is_empty(), "nothing should have run");
    }

    #[tokio::test]
    async fn the_report_says_how_the_command_was_judged() {
        let (answer, _) = against(Reply::ok("100.64.0.1\n"), params(&["ip"])).await;
        let value = answer.expect("a read runs");
        assert_eq!(value["tier"], "read");
        assert_eq!(value["covered"], true);
        assert_eq!(value["command"], "tailscale ip");
        assert_eq!(value["printed"], "100.64.0.1");
    }

    #[tokio::test]
    async fn a_secret_on_the_argument_list_is_kept_out_of_the_report() {
        // The one place a caller's own arguments become text this server
        // shows: every typed tool builds its own, and none of them can carry
        // a key the way this can.
        let key = "tskey-auth-kFakeExample-0123456789";
        let (answer, argv) = against(
            Reply::ok("done\n"),
            RunParams {
                args: words(&["up", &format!("--auth-key={key}")]),
                confirm: true,
                timeout_seconds: None,
            },
        )
        .await;
        let value = answer.expect("`up` runs once confirmed");
        let command = value["command"].as_str().expect("a command line");
        assert!(
            !command.contains(key),
            "the key reached the report: {command}"
        );

        // Redaction is what is shown and never what is run: the client still
        // has to receive the real key.
        assert_eq!(argv, vec![words(&["up", &format!("--auth-key={key}")])]);
    }
}
