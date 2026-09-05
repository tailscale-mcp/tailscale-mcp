//! A ticket may not be waiting for something that has already happened.
//!
//! The tracker under `.scratch/` is checked in, and a reader takes it at its
//! word. Ticket 31 said "waiting on 1.0.0" and, in its closing section, that
//! the trusted-publishing branch "is not merged" and that all six bootstrap
//! steps were "not done, and blocking" — through 1.0.1, 1.0.2, 1.0.3 and
//! 1.0.4, every one of which was published by the mechanism it described as
//! blocked, holding none of the secrets it described as still in place.
//!
//! The cost of that is not tidiness. The repair somebody reaches for when a
//! publish fails and the tracker says the token conversion never landed is to
//! put a token back — undoing the thing the ticket exists to have done. So a
//! status may name a version it is waiting for, and that version may not
//! already be tagged.
//!
//! Only the status line is read. What a ticket says in its body about a
//! release is prose about how the work went, and the tags are no judge of it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

use std::collections::BTreeSet;

/// Where the tracker lives, per `docs/agents/issue-tracker.md`.
const TRACKER: &str = ".scratch/tailscale-mcp-v1/issues";

/// A ticket's status line, as `(ticket, status)`.
fn statuses() -> Vec<(String, String)> {
    let dir = repo::root().join(TRACKER);
    let mut found: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("the issue directory")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).ok()?;
            let status = text
                .lines()
                .find_map(|line| line.strip_prefix("Status:"))?
                .trim()
                .to_owned();
            Some((name, status))
        })
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no tickets found under {TRACKER}");
    found
}

/// Every version that has shipped, read from the changelog.
///
/// The changelog rather than `git tag`, because the jobs that run this suite
/// check out shallow and without tags — `git tag` there lists nothing, and a
/// check that knows nothing passes everything. The changelog is generated from
/// those same tags at release time and is checked in, so it says the same
/// thing in a working tree, a shallow clone and an unpacked crate.
fn shipped() -> BTreeSet<String> {
    let changelog =
        std::fs::read_to_string(repo::root().join("CHANGELOG.md")).expect("the changelog");
    changelog
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .filter_map(|heading| heading.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// Anything in the status line that looks like a release: `1.0.0`, `v1.2.3`.
fn versions_named(status: &str) -> Vec<String> {
    status
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|word| {
            let parts: Vec<&str> = word.split('.').collect();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.parse::<u32>().is_ok())
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn no_ticket_waits_on_a_version_that_has_shipped() {
    let released = shipped();
    // A check that found no releases would pass by knowing nothing, which is
    // the one outcome indistinguishable from every ticket being fine.
    assert!(
        !released.is_empty(),
        "no released versions found in CHANGELOG.md, so this check cannot tell \
         a stale ticket from a fresh one"
    );
    for (ticket, status) in statuses() {
        for version in versions_named(&status) {
            assert!(
                !released.contains(&version),
                "{ticket} says `Status: {status}`, but {version} has shipped. \
                 Either the wait is over and the status is stale, or it is \
                 waiting on something else and should say so — a tracker that \
                 describes work as blocked after it shipped invites somebody \
                 to undo it."
            );
        }
    }
}

/// And the check can tell the two apart.
#[test]
fn the_check_reads_a_version_out_of_a_status_line() {
    assert_eq!(
        versions_named("in-progress — waiting on 1.0.0"),
        ["1.0.0"],
        "the shape this was written for"
    );
    assert_eq!(versions_named("in-progress — waiting on v2.1.0"), ["2.1.0"]);
    assert_eq!(
        versions_named("in-progress — bootstrap steps 1-5 done, step 6 outstanding"),
        Vec::<String>::new(),
        "step numbers and ranges are not versions"
    );
    assert_eq!(versions_named("done"), Vec::<String>::new());
    assert_eq!(
        versions_named("blocked by 30"),
        Vec::<String>::new(),
        "a ticket number is not a version"
    );
}
