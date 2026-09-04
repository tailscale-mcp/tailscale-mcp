//! Continuous integration asks for nothing a fork does not have.
//!
//! A pull request from a fork gets no repository secrets and a read-only
//! token. That is only a problem if a job wants one, and the moment one does,
//! every outside contribution starts failing for a reason the contributor
//! cannot fix and cannot see — quietly, because the maintainer's own runs keep
//! passing. So anything a fork's pull request can reach must read no secret,
//! name none of this server's own environment variables, and ask for no write
//! access.
//!
//! **What a fork can reach.** A workflow that runs on `pull_request`, and
//! anything a workflow can pull in, such as a composite action, which has no
//! triggers of its own to be judged by. A workflow that does not run on
//! `pull_request` — a release, driven by a tag — is out of this scope and may
//! hold what it needs; that is why the rule is about reachability rather than
//! about every file under `.github`.
//!
//! The one rule with no exception is `pull_request_target`, which runs a
//! fork's pull request with the base repository's secrets and token. Nothing
//! here has a use for it.
//!
//! It lives here, beside `fixtures_are_redacted`, for the same reason that one
//! does: it is a fact about the repository rather than about a crate, and this
//! is where the repository's mechanical checks are.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

use std::path::{Path, PathBuf};

/// The prefix on every environment variable this server reads: the settings,
/// the control-plane credentials and the end-to-end gates. None of them
/// belongs in a workflow a fork can reach, so the prefix is the whole rule and
/// a variable added later is covered without anybody remembering to add it
/// here.
const OUR_ENV_PREFIX: &str = "TAILSCALE_";

/// GitHub's only spelling for reading a repository secret.
const SECRET: &str = "secrets.";

/// The trigger that runs a fork's pull request against the base repository,
/// with its secrets and its token.
const PRIVILEGED_TRIGGER: &str = "pull_request_target";

/// Something in a file that a fork's pull request could not supply.
#[derive(Debug, PartialEq, Eq)]
struct Fault {
    what: &'static str,
    line: String,
}

/// One line's settings, with what YAML lets you dress them up in taken off:
/// the braces of a flow mapping, the quotes round a scalar, a trailing
/// comment, and the commas between them. `permissions: { contents: 'write' }
/// # for the release` and `contents: write` have to read the same to a check
/// that asks about write access, or the check is advice rather than a rule.
fn settings_in(line: &str) -> Vec<String> {
    let uncommented = line.split_once('#').map_or(line, |(before, _)| before);
    uncommented
        .split(',')
        .map(|part| {
            let bare: String = part
                .chars()
                .filter(|c| !matches!(c, '{' | '}' | '[' | ']' | '\'' | '"'))
                .collect();
            bare.trim().to_owned()
        })
        .filter(|part| !part.is_empty())
        .collect()
}

/// Every line of `text` that asks for a credential or for write access.
fn faults(text: &str) -> Vec<Fault> {
    let mut found = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // The credential rules read the raw line, comment and all: naming one
        // of these is as much a fault as setting it, and a check that read
        // past a comment could be hidden behind one.
        if trimmed.contains(SECRET) {
            found.push(Fault {
                what: "reads a repository secret",
                line: trimmed.to_owned(),
            });
        }
        if trimmed.contains(OUR_ENV_PREFIX) {
            found.push(Fault {
                what: "names one of this server's environment variables",
                line: trimmed.to_owned(),
            });
        }
        // Write access is anchored at the end of a setting, so this one reads
        // the settings rather than the line.
        for setting in settings_in(trimmed) {
            if setting.ends_with(": write") || setting.ends_with(": write-all") {
                found.push(Fault {
                    what: "asks for write access",
                    line: trimmed.to_owned(),
                });
                break;
            }
        }
    }
    found
}

/// Whether `text` declares the `pull_request` trigger, in either the block
/// form or the flow list. `pull_request_target` is a different trigger and
/// does not count as this one.
fn runs_on_a_pull_request(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("pull_request:")
            || settings_in(trimmed)
                .iter()
                .any(|part| part == "pull_request" || part == "on: pull_request")
    })
}

/// Whether `text` mentions the trigger that hands a fork the token.
fn uses_the_privileged_trigger(text: &str) -> bool {
    text.contains(PRIVILEGED_TRIGGER)
}

/// Whether `text` sets the token's permissions at the top level. Left unset, a
/// workflow inherits whatever the repository's default is, which is a setting
/// nobody can see from here.
fn declares_permissions(text: &str) -> bool {
    text.lines().any(|line| line.starts_with("permissions:"))
}

/// Every YAML file under `.github`: the workflows, and anything a workflow
/// could pull in.
fn github_yaml() -> Vec<PathBuf> {
    let directory = repo::root().join(".github");
    let mut found = Vec::new();
    let mut queue = vec![directory.clone()];
    while let Some(next) = queue.pop() {
        let entries = std::fs::read_dir(&next)
            .unwrap_or_else(|error| panic!("{} is not readable: {error}", next.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("yml" | "yaml")
            ) {
                found.push(path);
            }
        }
    }
    found.sort();
    assert!(
        !found.is_empty(),
        "{} holds nothing, so nothing runs on a pull request",
        directory.display()
    );
    found
}

/// Whether `path` is a workflow, rather than something a workflow pulls in.
fn is_a_workflow(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "workflows")
}

/// Every file, with its text.
fn read_all() -> Vec<(PathBuf, String)> {
    github_yaml()
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("a workflow is text");
            (path, text)
        })
        .collect()
}

#[test]
fn nothing_a_fork_can_reach_asks_for_a_secret_or_for_write_access() {
    let mut offences = Vec::new();
    for (path, text) in read_all() {
        // A workflow with its own triggers is judged by them; anything else
        // has none, so a pull-request workflow could pull it in.
        if is_a_workflow(&path) && !runs_on_a_pull_request(&text) {
            continue;
        }
        if is_a_workflow(&path) {
            assert!(
                declares_permissions(&text),
                "{} runs on a pull request and leaves the token's permissions \
                 to the repository's default",
                path.display()
            );
        }
        for fault in faults(&text) {
            offences.push(format!(
                "{}: {}: `{}`",
                path.display(),
                fault.what,
                fault.line
            ));
        }
    }
    assert!(
        offences.is_empty(),
        "a pull request from a fork could not run these:\n{}",
        offences.join("\n")
    );
}

#[test]
fn nothing_uses_the_trigger_that_hands_a_fork_the_token() {
    for (path, text) in read_all() {
        assert!(
            !uses_the_privileged_trigger(&text),
            "{} uses `{PRIVILEGED_TRIGGER}`, which runs a fork's pull request \
             with this repository's secrets",
            path.display()
        );
    }
}

#[test]
fn the_suite_runs_on_a_pull_request() {
    // Without this the rules above are a claim about nothing: a repository
    // with no pull-request workflow satisfies every one of them.
    let ran: Vec<PathBuf> = read_all()
        .into_iter()
        .filter(|(path, text)| {
            is_a_workflow(path) && runs_on_a_pull_request(text) && text.contains("cargo test")
        })
        .map(|(path, _)| path)
        .collect();
    assert!(
        !ran.is_empty(),
        "no workflow runs the suite on a pull request"
    );
}

#[test]
fn the_check_catches_what_a_workflow_needing_a_credential_looks_like() {
    // Each of these is the shape the mistake actually takes, so the check is
    // known to fire rather than assumed to.
    for (what, text) in [
        (
            "a repository secret in an environment",
            "        env:\n          TOKEN: ${{ secrets.CONTROL_PLANE_KEY }}\n",
        ),
        (
            "a control-plane credential",
            "        env:\n          TAILSCALE_API_KEY: whatever\n",
        ),
        (
            "an end-to-end gate turned on",
            "        env:\n          TAILSCALE_MCP_E2E_TAILNET: \"1\"\n",
        ),
        (
            "a token that can write",
            "permissions:\n  contents: write\n",
        ),
        (
            "the same, with a comment after it",
            "permissions:\n  contents: write # to upload the release assets\n",
        ),
        ("the same, quoted", "permissions:\n  contents: 'write'\n"),
        (
            "the same, as a flow mapping",
            "permissions: { contents: write, pages: write }\n",
        ),
        (
            "a token that can write everything",
            "permissions: write-all\n",
        ),
    ] {
        assert!(
            !faults(text).is_empty(),
            "{what} should have been caught: {text:?}"
        );
    }
}

#[test]
fn the_check_passes_what_a_workflow_needing_nothing_looks_like() {
    // The other half: a check that fired on everything would pass the tests
    // above and fail every honest workflow.
    for text in [
        "permissions:\n  contents: read\n",
        "        run: cargo test --workspace --all-targets --locked\n",
        "      - name: Write the changelog\n        run: git cliff > CHANGELOG.md\n",
        "        with:\n          key: cargo-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}\n",
        "  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n",
    ] {
        assert_eq!(faults(text), vec![], "should have passed: {text:?}");
    }
}

#[test]
fn the_triggers_are_read_the_way_github_reads_them() {
    assert!(!runs_on_a_pull_request(
        "on:\n  push:\n    branches: [main]\n"
    ));
    assert!(runs_on_a_pull_request("on:\n  push:\n  pull_request:\n"));
    assert!(runs_on_a_pull_request("on: [push, pull_request]\n"));
    // The privileged trigger is not the trigger this asks about, and is
    // refused outright by its own check.
    assert!(!runs_on_a_pull_request("on:\n  pull_request_target:\n"));
    assert!(uses_the_privileged_trigger("on:\n  pull_request_target:\n"));
    assert!(!uses_the_privileged_trigger("on:\n  pull_request:\n"));

    assert!(!declares_permissions("on:\n  pull_request:\n"));
    assert!(declares_permissions("permissions:\n  contents: read\n"));
    assert!(declares_permissions("permissions: read-all\n"));
    // Per-job permissions are not the top-level declaration this asks for.
    assert!(!declares_permissions("jobs:\n  test:\n    permissions:\n"));
}
