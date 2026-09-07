//! The vendored description is asked about after the work stops.
//!
//! `crates/tailscale-rest/tests/schema_drift.rs` is, in its own words, "a
//! tripwire on a refresh": it holds the models to
//! `docs/research/tailscale-openapi.yaml` and fires when somebody re-vendors
//! that file. It asks Tailscale nothing, and nothing in a push changes the
//! file, so it cannot notice the upstream moving — and a finished project never
//! re-vendors. `advisories.yml` makes the argument for the other check whose
//! answer changes while the tree does not; `schema-drift.yml` is that argument
//! applied here.
//!
//! Three things about it are worth holding mechanically. That the schedule is
//! still there, since a workflow whose trigger was deleted is a file that looks
//! like cover and is not — the same reason `advisories_are_asked_on_a_clock`
//! holds its sibling's. And that the two ends stay attached: the workflow has
//! to ask the address the vendored copy actually came from, and watch the file
//! the drift test actually reads. Either of those drifting would leave a green
//! weekly check that is asking about the wrong document, which is worse than no
//! check at all.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

/// A workflow's text.
fn workflow(name: &str) -> String {
    let path = repo::root().join(".github/workflows").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The value of an `env:` entry, wherever it sits.
fn env_value(text: &str, key: &str) -> String {
    let prefix = format!("{key}:");
    text.lines()
        .find_map(|line| line.trim().strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("`{key}` is set in the workflow"))
        .trim()
        .trim_matches('"')
        .to_owned()
}

/// The path `schema_drift.rs` reads, as the repository sees it.
///
/// The test names it relative to its own crate, which is two levels down.
fn the_description_the_drift_test_reads() -> String {
    let path = repo::root().join("crates/tailscale-rest/tests/schema_drift.rs");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .find_map(|line| line.trim().strip_prefix("const DESCRIPTION: &str = "))
        .expect("the drift test names the description it reads")
        .trim()
        .trim_end_matches(';')
        .trim_matches('"')
        .trim_start_matches("../../")
        .to_owned()
}

#[test]
fn the_schema_workflow_still_runs_on_a_schedule() {
    let text = workflow("schema-drift.yml");
    assert!(
        text.contains("schedule:") && text.contains("cron:"),
        "`schema-drift.yml` exists to ask on a clock; without a schedule it asks only when \
         somebody remembers to re-vendor, which is the thing it was written to replace"
    );
}

#[test]
fn the_workflow_asks_where_the_vendored_copy_came_from() {
    // Recorded in the table beside the file itself. Two places name this
    // address and they have to agree, or the weekly answer is about some other
    // document than the one in the tree.
    let asked = env_value(&workflow("schema-drift.yml"), "SCHEMA_URL");
    let path = repo::root().join("docs/research/README.md");
    let recorded =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(
        recorded.contains(&asked),
        "`schema-drift.yml` asks {asked}, which `docs/research/README.md` does not give as where \
         `tailscale-openapi.yaml` was fetched from"
    );
}

#[test]
fn the_workflow_watches_the_file_the_drift_test_reads() {
    let watched = env_value(&workflow("schema-drift.yml"), "VENDORED");
    let read = the_description_the_drift_test_reads();
    assert_eq!(
        watched, read,
        "`schema-drift.yml` watches {watched} and `schema_drift.rs` reads {read}; a refresh \
         prompted by one would not be checked by the other"
    );
}

#[test]
fn the_checks_catch_a_deleted_schedule_and_a_wandering_url() {
    // The failures above are only worth having if they fire, and none of them
    // is reachable from a passing tree — so each is exercised on text here.
    assert_eq!(
        env_value(
            "    env:\n      SCHEMA_URL: \"https://example.com/s\"\n",
            "SCHEMA_URL"
        ),
        "https://example.com/s"
    );
    assert_ne!(
        env_value("  VENDORED: docs/a.yaml", "VENDORED"),
        env_value("  VENDORED: docs/b.yaml", "VENDORED"),
        "two different files should compare unequal, which is the whole check"
    );
    let no_schedule = "on:\n  workflow_dispatch:\n";
    assert!(!(no_schedule.contains("schedule:") && no_schedule.contains("cron:")));
    assert!(!"https://example.com/other".contains("https://example.com/s\n"));
}
