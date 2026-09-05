//! The four subcommands, as a terminal gets them.
//!
//! These call the functions rather than spawning the binary, because a
//! `Report` carries both what was printed and whether it was bad news, which
//! is the whole contract: `main` turns the second into an exit code and does
//! nothing else. Spawning would test `print!`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use clap::Parser as _;
use clap::ValueEnum as _;
use serde_json::Value;
use tailscale_mcp::config::{Cli, Client, Config, ENV_VARS, NOT_IN_A_SNIPPET};
use tailscale_mcp::server::Backends;
use tailscale_mcp::subcommands;

/// A configuration from a command line, with nothing in the environment.
///
/// Nothing in the environment on purpose: three of the five subcommands must
/// work without a credential, and a test that inherited the developer's would
/// not notice if they stopped.
fn config(args: &[&str]) -> Config {
    let cli = Cli::try_parse_from(args).expect("the arguments parse");
    Config::resolve_with(cli, |_| None).expect("the configuration resolves")
}

/// Backends with nothing behind them.
///
/// Explicit, and never `Backends::discover`: that reads the process
/// environment, so a developer with `TAILSCALE_API_KEY` set would have these
/// tests reaching the real control plane — which is exactly what happened
/// before this existed.
fn nothing_behind_them() -> Backends {
    Backends {
        local: std::sync::Arc::new(tailscale_cli::stub::StubBackend::missing()),
        local_available: false,
        credentials: None,
    }
}

/// Every preset and tier combination, and what each offers.
///
/// `spec.md` fixes the total: "186 tools: 62 typed local tools, a 30-tool debug
/// toolset, one passthrough, and 93 tailnet tools". The rest of this table is
/// derived from the metadata table rather than agreed separately, and is pinned
/// here so that a change to any tool's toolset or tier is a change somebody has
/// to look at rather than a number that quietly moves (Q92).
const COUNTS: &[(&str, &str, usize)] = &[
    ("minimal", "read", 37),
    ("minimal", "write", 51),
    ("minimal", "destructive", 55),
    ("core", "read", 57),
    ("core", "write", 106),
    ("core", "destructive", 126),
    ("full", "read", 68),
    ("full", "write", 126),
    ("full", "destructive", 155),
];

#[test]
fn the_listing_counts_match_the_table_for_every_preset_and_tier() {
    for (preset, tier, expected) in COUNTS {
        let mut args = vec!["tailscale-mcp", "--preset", preset];
        match *tier {
            "write" => args.push("--allow-write"),
            "destructive" => args.push("--allow-destructive"),
            _ => {}
        }
        let report = subcommands::tools(&config(&args), true);
        assert!(report.ok);
        let parsed: Value = serde_json::from_str(&report.text).expect("JSON");
        let listed = parsed["tools"].as_array().expect("an array").len();
        assert_eq!(
            listed, *expected,
            "{preset} at the {tier} tier should offer {expected} tools, not {listed}"
        );
    }
}

#[test]
fn the_whole_table_is_the_one_the_spec_counted() {
    // 62 + 30 + 1 + 93. `full` is every typed toolset, so the debug toolset
    // and the passthrough have to be asked for by name — which is what makes
    // the difference between 155 and 186.
    let everything = config(&[
        "tailscale-mcp",
        "--preset",
        "full",
        "--toolsets",
        "+local-debug,+local-passthrough",
        "--allow-destructive",
    ]);
    let listing = subcommands::tools(&everything, true);
    let parsed: Value = serde_json::from_str(&listing.text).expect("JSON");
    assert_eq!(parsed["count"], 186);
    let tools = parsed["tools"].as_array().expect("an array");
    let local = tools.iter().filter(|t| t["surface"] == "local").count();
    assert_eq!(local, 93, "62 typed local tools, 30 debug, 1 passthrough");
    assert_eq!(
        tools.len() - local,
        93,
        "one per documented control-plane operation"
    );
}

/// `tools` honours the switches it accepts.
///
/// It takes `--no-local` and `--no-tailnet` and documents what they do, and it
/// deliberately ignores what this machine has — no CLI, no credential — so its
/// answer is the same everywhere. A switch is not that: it is the selection,
/// like the preset. Ignoring it reported 57 tools for a server that would have
/// served 29, which is the one number somebody runs this to find out.
#[test]
fn tools_leaves_out_a_surface_the_operator_switched_off() {
    let count = |args: &[&str]| -> (u64, Vec<String>) {
        let report = subcommands::tools(&config(args), true);
        assert!(report.ok);
        let parsed: Value = serde_json::from_str(&report.text).expect("JSON");
        let surfaces = parsed["tools"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|tool| tool["surface"].as_str().expect("a surface").to_owned())
            .collect();
        (parsed["count"].as_u64().expect("a count"), surfaces)
    };

    let (both, _) = count(&["tailscale-mcp"]);
    let (without_tailnet, local_only) = count(&["tailscale-mcp", "--no-tailnet"]);
    let (without_local, tailnet_only) = count(&["tailscale-mcp", "--no-local"]);

    assert!(
        without_tailnet < both && without_local < both,
        "switching a surface off should offer fewer tools, not the same {both}"
    );
    assert_eq!(
        without_tailnet + without_local,
        both,
        "the two surfaces partition the selection, so the halves should sum to it"
    );
    assert!(
        local_only.iter().all(|surface| surface == "local"),
        "--no-tailnet left a tailnet tool in the listing"
    );
    assert!(
        tailnet_only.iter().all(|surface| surface == "tailnet"),
        "--no-local left a local tool in the listing"
    );
}

#[test]
fn a_listing_says_which_toolset_and_surface_each_tool_belongs_to() {
    let report = subcommands::tools(&config(&["tailscale-mcp"]), true);
    let parsed: Value = serde_json::from_str(&report.text).expect("JSON");
    assert_eq!(parsed["preset"], "core");
    assert_eq!(parsed["tier"], "read");
    let first = &parsed["tools"][0];
    for field in ["name", "toolset", "surface", "tier", "summary"] {
        assert!(
            first[field].is_string(),
            "a row should say its {field}: {first}"
        );
    }
}

#[tokio::test]
async fn a_diagnosis_reports_every_check_even_when_the_first_one_fails() {
    // No CLI, no credential, no control plane: an operator running this wants
    // the whole list, not the first thing to go wrong.
    let broken = config(&["tailscale-mcp", "--cli-path", "/nonexistent/tailscale"]);
    let report = subcommands::diagnose(&broken, nothing_behind_them(), true).await;

    assert!(!report.ok, "and the exit code says so");
    let parsed: Value = serde_json::from_str(&report.text).expect("JSON");
    let checks = parsed["checks"].as_array().expect("an array");
    assert_eq!(checks.len(), 3, "each check is reported independently");
    assert!(
        checks.iter().all(|check| check["state"] == "failed"),
        "all three failed here: {}",
        report.text
    );
    assert!(
        checks
            .iter()
            .all(|check| check["remedy"].as_str().is_some_and(|r| !r.is_empty())),
        "a failed check without a remedy is a complaint, not a diagnosis"
    );
}

#[tokio::test]
async fn a_check_the_operator_switched_off_is_skipped_rather_than_passed() {
    let quiet = config(&["tailscale-mcp", "--no-local", "--no-tailnet"]);
    let report = subcommands::diagnose(&quiet, nothing_behind_them(), true).await;

    assert!(report.ok, "nothing failed, so nothing should stop a script");
    let parsed: Value = serde_json::from_str(&report.text).expect("JSON");
    for check in parsed["checks"].as_array().expect("an array") {
        assert_eq!(
            check["state"], "skipped",
            "telling an operator their credential is fine when nothing looked at it \
             would be worse than saying nothing"
        );
    }
}

#[test]
fn the_version_says_what_it_is_and_what_it_can_speak() {
    let report = subcommands::version();
    assert!(report.ok);
    assert!(report.text.contains(env!("CARGO_PKG_VERSION")));
    assert!(
        report
            .text
            .contains(&rmcp::model::ProtocolVersion::LATEST.to_string()),
        "the version it prefers: {}",
        report.text
    );
    assert!(
        rmcp::model::ProtocolVersion::KNOWN_VERSIONS
            .iter()
            .all(|known| report.text.contains(&known.to_string())),
        "and everything an older client could negotiate down to: {}",
        report.text
    );
}

/// And says each of them once.
///
/// The line names the preferred version and then lists what else it speaks, so
/// the list is the others. `KNOWN_VERSIONS` includes the preferred one, and
/// printing it unfiltered produced "MCP protocol 2025-11-25 (also speaks ...,
/// 2025-11-25, ...)" — a sentence claiming the server also speaks the version
/// it just said it speaks.
#[test]
fn the_version_names_each_protocol_once() {
    let report = subcommands::version();
    for known in rmcp::model::ProtocolVersion::KNOWN_VERSIONS {
        let version = known.to_string();
        assert_eq!(
            report.text.matches(&version).count(),
            1,
            "`{version}` appears more than once; the list after \"also speaks\" is \
             the versions other than the one already named: {}",
            report.text
        );
    }
}

/// The SDK version in `version`'s output is written by hand, because there is
/// no way to read a dependency's version at runtime. This is what stops it
/// going stale (Q97).
#[test]
fn the_sdk_version_printed_is_the_one_the_manifest_asks_for() {
    // The workspace manifest: the crate declares `rmcp.workspace = true`.
    let manifest = include_str!("../../../Cargo.toml");
    let declared = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("rmcp = "))
        .and_then(|rest| rest.split('"').nth(1))
        .expect("the manifest should name rmcp");
    assert!(
        subcommands::RMCP_VERSION.starts_with(declared.trim_start_matches(['^', '='])),
        "the manifest asks for rmcp {declared} and `version` prints {}; \
         one of them was not updated",
        subcommands::RMCP_VERSION
    );
}

#[test]
fn every_client_gets_the_shape_that_client_accepts() {
    // Not `SERVERS_KEY.iter()`: clap knows every variant, so a sixth client
    // added to the enum fails here until somebody says what shape it takes.
    for client in Client::value_variants() {
        let expected = SERVERS_KEY
            .iter()
            .find(|(known, _)| known == client)
            .map(|(_, key)| *key)
            .unwrap_or_else(|| panic!("{client:?} has no documented shape in this test"));

        let report = subcommands::setup(
            *client,
            &config(&["tailscale-mcp", "--preset", "full", "--allow-write"]),
        );
        assert!(report.ok);
        let object = snippet(&report);
        let entry = object[expected]["tailscale"].clone();
        assert!(
            entry.is_object(),
            "{client:?} keeps its servers under `{expected}`, and this snippet does not: {object}"
        );
        assert_eq!(entry["command"], env!("CARGO_PKG_NAME"));

        let env = entry["env"].as_object().expect("the settings it changed");
        for name in env.keys() {
            assert!(
                ENV_VARS.contains(&name.as_str()),
                "{name} is in the snippet but the server does not read it"
            );
            assert!(
                !NOT_IN_A_SNIPPET.contains(&name.as_str()),
                "{name} has no business in a client snippet"
            );
        }
        assert_eq!(env["TAILSCALE_MCP_PRESET"], "full");
        assert_eq!(env["TAILSCALE_MCP_ALLOW_WRITE"], "true");
    }
}

/// The key each client actually keeps its servers under.
///
/// Written out here rather than read from the code, because a test that asked
/// the code which key it used would agree with the code about a key neither of
/// them had checked against the client. These come from each client's own
/// documentation.
const SERVERS_KEY: &[(Client, &str)] = &[
    (Client::ClaudeCode, "mcpServers"),
    (Client::ClaudeDesktop, "mcpServers"),
    (Client::Cursor, "mcpServers"),
    (Client::Vscode, "servers"),
    (Client::Zed, "context_servers"),
];

/// The object out of a snippet, with the comments for a person dropped.
fn snippet(report: &tailscale_mcp::subcommands::Report) -> Value {
    serde_json::from_str(
        &report
            .text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<String>(),
    )
    .unwrap_or_else(|error| panic!("a snippet should be JSON: {error}\n{}", report.text))
}

/// Every setting an operator can change reaches the snippet, or is on the list
/// of ones that deliberately do not.
///
/// The criterion is that the snippet "produces a working server", and one
/// missing `--cli-path` produces a server with no local surface on the very
/// machine that needed the flag.
#[test]
fn a_snippet_carries_every_setting_that_is_not_deliberately_left_out() {
    let changed = config(&[
        "tailscale-mcp",
        "--preset",
        "full",
        "--toolsets",
        "-tailnet-dns",
        "--allow-destructive",
        "--no-local",
        "--no-tailnet",
        "--cli-path",
        "/opt/tailscale",
        "--max-result-bytes",
        "4096",
        "--log",
        "debug",
    ]);
    let carried: Vec<&str> = changed
        .changed_settings()
        .iter()
        .map(|(key, _)| *key)
        .collect();

    for variable in ENV_VARS {
        // The two tier variables are alternatives, not a pair: destructive
        // already implies write, so a snippet naming both would be saying the
        // same thing twice.
        let covered = carried.contains(variable)
            || NOT_IN_A_SNIPPET.contains(variable)
            || (*variable == tailscale_mcp::config::ALLOW_WRITE_ENV
                && carried.contains(&tailscale_mcp::config::ALLOW_DESTRUCTIVE_ENV));
        assert!(
            covered,
            "{variable} is neither carried into a snippet nor listed as one that is not, so a \
             server started from the snippet would not be the server it was printed from"
        );
    }

    // And the write tier on its own does carry its own variable.
    let writing = config(&["tailscale-mcp", "--allow-write"]);
    assert!(
        writing
            .changed_settings()
            .iter()
            .any(|(key, _)| *key == tailscale_mcp::config::ALLOW_WRITE_ENV),
        "a session that may write should say so"
    );

    let env = snippet(&subcommands::setup(Client::ClaudeCode, &changed))["mcpServers"]["tailscale"]
        ["env"]
        .clone();
    assert_eq!(env["TAILSCALE_MCP_CLI_PATH"], "/opt/tailscale");
    assert_eq!(env["TAILSCALE_MCP_MAX_RESULT_BYTES"], "4096");
    // The bounded filter, not the raw `debug` that was typed: the snippet
    // reproduces the server that was running, and that is what it was running.
    assert_eq!(env["TAILSCALE_MCP_LOG"], "debug,rmcp=info");
}

#[test]
fn a_snippet_that_changed_nothing_carries_no_settings() {
    let report = subcommands::setup(Client::ClaudeCode, &config(&["tailscale-mcp"]));
    assert!(
        snippet(&report)["mcpServers"]["tailscale"]["env"].is_null(),
        "spelling out a default is how a snippet goes stale, and every value in \
         one would be wrong the day a default changed: {}",
        report.text
    );
    assert!(
        report.text.contains("TAILSCALE_API_KEY"),
        "but the credential it cannot supply should be named"
    );
}

#[test]
fn setup_writes_nothing() {
    // The criterion is "it writes nothing", and the honest way to check is to
    // watch a directory rather than to read the code and agree with it. The
    // working directory is watched rather than moved into: changing it is a
    // process-wide mutation, and the other tests in this binary run alongside.
    let watched = std::env::current_dir().expect("a working directory");
    let listing = || {
        let mut names: Vec<String> = std::fs::read_dir(&watched)
            .expect("readable")
            .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        names.sort();
        names
    };
    let before = listing();

    for client in Client::value_variants() {
        let report = subcommands::setup(*client, &config(&["tailscale-mcp"]));
        assert!(report.ok);
    }

    assert_eq!(
        listing(),
        before,
        "a client's configuration file has the operator's own edits in it, and \
         rewriting one is not this server's decision to make"
    );
}
