//! The three rows whose tier is a floor, and every place that has to say so.
//!
//! A row's tier is normally the whole truth: the gate reads it, the client is
//! annotated from it, and a session at that tier can make any call the tool
//! accepts. Three rows are not like that — the passthrough, whose risk is
//! whatever command it is handed, and the two Q70 rows, where one argument
//! authorises and the other revokes. Their tier is the floor, and the handler
//! makes the rest of the decision.
//!
//! That is fine as long as everything that reports a tier says which kind it
//! is reporting. Nothing did until this test existed: `docs/tools.md` carried
//! "tier depends on arguments" because its generator computes a notes column,
//! while the `tools` subcommand — the thing whose whole job is to answer "what
//! would this preset and tier offer" — printed `write` for a tool that will
//! refuse half its arguments at the write tier. The comment on `varying_tier`
//! meanwhile still said the passthrough was the only row that set it, two
//! rows after that stopped being true.
//!
//! So the list is pinned here rather than described in prose that nothing
//! checks. A fourth row is welcome; it just has to come with the edits.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use clap::Parser as _;
use serde_json::Value;
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::meta::Tier;
use tailscale_mcp::subcommands;

/// Every row whose tier is a floor, and nothing else.
const FLOORS: &[&str] = &[
    "tailnet_device_authorize",
    "tailnet_service_approval_set",
    "tailscale_run",
];

/// A selection wide enough to contain all of them, tier high enough to offer
/// them: the two hidden toolsets are in no preset, and the passthrough is in
/// one of those.
const EVERYTHING: &[&str] = &[
    "tailscale-mcp",
    "--preset",
    "full",
    "--toolsets",
    "+local-debug,+local-passthrough",
    "--allow-destructive",
];

fn config(args: &[&str]) -> Config {
    let cli = Cli::try_parse_from(args).expect("the arguments parse");
    Config::resolve_with(cli, |_| None).expect("the configuration resolves")
}

fn listing(args: &[&str]) -> Value {
    serde_json::from_str(&subcommands::tools(&config(args), true).text).expect("the listing parses")
}

#[test]
fn the_tier_is_a_floor_only_where_it_is_documented() {
    let mut found: Vec<&str> = tailscale_mcp::tools::entries()
        .iter()
        .filter(|entry| entry.meta.varying_tier)
        .map(|entry| entry.meta.name)
        .collect();
    found.sort_unstable();
    assert_eq!(
        found, FLOORS,
        "the rows whose tier is a floor have changed. That is allowed, but it \
         is written down in three other places that this test cannot edit: the \
         `varying_tier` comment in `meta.rs`, which names them; `docs/tools.md`, \
         which notes them (UPDATE_DOCS=1 regenerates it); and the summary of \
         the tool itself, which is where a model reads that some argument needs \
         a higher tier."
    );
}

/// A floor is annotated at the worst case it permits, not at itself.
///
/// The derivation in `meta.rs` does this, but a client only ever sees the
/// result, and the result is the thing that must not lie: a planning model
/// reading `readOnlyHint: true` on the passthrough would hand it anything.
#[test]
fn a_floor_is_annotated_at_the_worst_it_allows() {
    for entry in tailscale_mcp::tools::entries() {
        let meta = entry.meta;
        let annotations = meta.annotations();
        if meta.varying_tier {
            assert!(
                !annotations.read_only,
                "{} is a floor, so it cannot be annotated read-only",
                meta.name
            );
            assert!(
                annotations.destructive,
                "{} is a floor, so it is annotated destructive whatever its row says",
                meta.name
            );
        } else {
            assert_eq!(
                annotations.read_only,
                meta.tier == Tier::Read,
                "{} is not a floor, so its annotation follows its tier exactly",
                meta.name
            );
            assert_eq!(
                annotations.destructive,
                meta.tier == Tier::Destructive,
                "{} is not a floor, so its annotation follows its tier exactly",
                meta.name
            );
        }
    }
}

/// The JSON listing marks them, and marks nothing else.
#[test]
fn the_listing_says_which_tiers_are_floors() {
    let listing = listing(EVERYTHING);
    let tools = listing["tools"].as_array().expect("a tool array");
    let mut marked: Vec<&str> = tools
        .iter()
        .filter(|tool| tool["tier_is_a_floor"] == Value::Bool(true))
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    marked.sort_unstable();
    // The names and nothing else: a failure that printed the listing printed
    // 53 kilobytes of tool summaries, which is not a message anybody reads.
    assert_eq!(
        marked, FLOORS,
        "the JSON listing marks a different set of tools than the table does"
    );

    // Absent rather than `false` on the rest, so that a consumer diffing two
    // listings sees a field appear where something changed and nowhere else.
    let carried = tools
        .iter()
        .filter(|tool| tool.get("tier_is_a_floor").is_some())
        .count();
    assert_eq!(
        carried,
        FLOORS.len(),
        "only a row whose tier is a floor carries the field"
    );
}

/// And the table, which has no notes column, says it in the only place it can.
#[test]
fn the_table_explains_its_marker_where_it_uses_one() {
    let with = subcommands::tools(&config(EVERYTHING), false).text;
    assert!(
        with.contains("`+` marks a tier that is a floor"),
        "a listing containing a floor explains the marker: {with}"
    );
    for name in FLOORS {
        let row = with
            .lines()
            .find(|line| line.split_whitespace().next() == Some(name))
            .unwrap_or_else(|| panic!("{name} should be listed"));
        let tier = row.split_whitespace().nth(1).expect("a tier column");
        assert!(
            tier.ends_with('+'),
            "{name} carries a floor, so its tier column is marked: {row}"
        );
    }

    // `minimal` at the read tier reaches none of them, and a legend for a
    // marker that never appears is one more thing to wonder about.
    let without =
        subcommands::tools(&config(&["tailscale-mcp", "--preset", "minimal"]), false).text;
    assert!(
        !without.contains("marks a tier that is a floor"),
        "a listing with no floor in it does not explain the marker: {without}"
    );
}
