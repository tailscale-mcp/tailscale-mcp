//! The release asks for the poke rather than waiting to be heard.
//!
//! `notify-tap.yml` triggers on `release: published`, and for two versions it
//! never fired: GitHub raises no workflow-triggering event for anything
//! `GITHUB_TOKEN` did — so that a workflow cannot set itself off — and
//! `release.yml` creates the release with `GITHUB_TOKEN`. 1.0.3 published to
//! every channel with the tap still serving 1.0.2, and the six-hour schedule
//! the poke exists to shorten was the only thing that would have healed it.
//!
//! It was invisible because the poke had only ever been exercised by hand, and
//! `workflow_dispatch` is one of the two events that rule excepts — so the
//! rehearsal took the one path that works. The release now dispatches it
//! explicitly, which is the same excepted path, asked for rather than awaited.
//!
//! What is held here is the wiring, since nothing else can hold it: the poke
//! is reachable only from a real release, and a release is the worst place to
//! find out.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

fn workflow(name: &str) -> String {
    let path = repo::root().join(".github/workflows").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_release_starts_the_tap_notifier_itself() {
    let release = workflow("release.yml");
    assert!(
        release.contains("gh workflow run notify-tap.yml"),
        "`release.yml` must start `notify-tap.yml` by name: a release it creates with \
         `GITHUB_TOKEN` raises no `release: published` for the notifier to hear"
    );
    assert!(
        release.contains("actions: write"),
        "starting another workflow needs `actions: write`, which `release.yml` does not \
         otherwise have"
    );
}

#[test]
fn the_notifier_can_still_be_started_that_way() {
    let notifier = workflow("notify-tap.yml");
    assert!(
        notifier.contains("workflow_dispatch:"),
        "`release.yml` dispatches `notify-tap.yml`, so it has to accept a dispatch; \
         without it the release's poke fails and the tap waits for the schedule"
    );
    assert!(
        notifier.contains("release:"),
        "the `release` trigger is kept as well — it costs nothing and covers a release \
         published by a person rather than by this workflow"
    );
}

#[test]
fn the_poke_is_sent_where_the_tap_can_act_on_it() {
    let release = workflow("release.yml");
    let created = release
        .find("gh release create")
        .expect("the release is created somewhere");
    let poked = release
        .find("gh workflow run notify-tap.yml")
        .expect("and the tap is told");
    assert!(
        created < poked,
        "the tap reads the formula and SHA256SUMS off the release, so it must be told \
         after the release exists and not before"
    );
}
