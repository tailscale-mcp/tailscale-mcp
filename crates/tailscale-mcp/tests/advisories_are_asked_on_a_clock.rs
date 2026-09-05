//! The advisory check keeps asking after the work stops.
//!
//! `cargo deny check advisories` is the one check here whose answer depends on
//! something outside the tree: the RUSTSEC database grows on its own, so a
//! dependency that was clean at a release can be the subject of an advisory a
//! month later with nothing in this repository having changed. `ci.yml` runs
//! on `push` and `pull_request` only, which means it stops asking exactly when
//! a finished project goes quiet — and the binaries are on five registries by
//! then.
//!
//! `advisories.yml` asks weekly instead. Two things about it are worth holding
//! mechanically: that the schedule is still there, since a workflow whose
//! trigger was deleted is a file that looks like cover and is not; and that it
//! pins the same `cargo-deny` as `ci.yml`, since two pins that drift make the
//! scheduled answer and the pushed answer come from different tools, and the
//! disagreement would read as a real finding.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

/// A workflow's text.
fn workflow(name: &str) -> String {
    let path = repo::root().join(".github/workflows").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The `CARGO_DENY_VERSION: "x.y.z"` a workflow pins, wherever it sits.
fn pinned_deny(text: &str) -> String {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("CARGO_DENY_VERSION:"))
        .expect("a workflow that installs cargo-deny pins the version")
        .trim()
        .trim_matches('"')
        .to_owned()
}

#[test]
fn the_advisory_schedule_pins_the_same_cargo_deny() {
    let scheduled = pinned_deny(&workflow("advisories.yml"));
    let pushed = pinned_deny(&workflow("ci.yml"));
    assert_eq!(
        scheduled, pushed,
        "`advisories.yml` pins cargo-deny {scheduled} and `ci.yml` pins {pushed}; \
         the weekly answer and the per-push answer would come from different tools"
    );
}

#[test]
fn the_advisory_workflow_still_runs_on_a_schedule() {
    let text = workflow("advisories.yml");
    assert!(
        text.contains("schedule:") && text.contains("cron:"),
        "`advisories.yml` exists to ask on a clock; without a schedule it asks \
         only when somebody remembers, which is what it was written to replace"
    );
    assert!(
        text.contains("check advisories"),
        "`advisories.yml` should run the advisories check and not something else"
    );
}

#[test]
fn the_checks_catch_a_drifted_pin_and_a_deleted_schedule() {
    // The failures above are only worth having if they fire, and neither is
    // reachable from a passing tree — so both are exercised on text here.
    assert_eq!(
        pinned_deny("      CARGO_DENY_VERSION: \"1.2.3\"\n"),
        "1.2.3"
    );
    assert_ne!(
        pinned_deny("  CARGO_DENY_VERSION: \"0.20.2\""),
        pinned_deny("  CARGO_DENY_VERSION: \"0.21.0\""),
        "two different pins should compare unequal, which is the whole check"
    );
    let no_schedule = "on:\n  workflow_dispatch:\n";
    assert!(!(no_schedule.contains("schedule:") && no_schedule.contains("cron:")));
}
