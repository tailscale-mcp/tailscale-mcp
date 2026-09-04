//! One of the two supporting seams: process execution against a stub binary.
//!
//! Everything here is behaviour a faked backend would skip.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use tailscale_cli::{CliBackend, Concurrency, ExecError, Invocation, LocalBackend as _};

fn stub() -> CliBackend {
    CliBackend::at(env!("CARGO_BIN_EXE_tailscale-stub"))
}

/// The stub's own vocabulary, which the real CLI does not share.
fn stub_call(args: &[&str]) -> Invocation {
    Invocation::read(args.iter().copied())
}

#[tokio::test]
async fn arguments_reach_the_child_exactly_as_given() {
    let out = stub()
        .run(stub_call(&[
            "echo-args",
            "--json",
            "a b c",
            "quote\"and'quote",
            "semi;colon && rm -rf /",
        ]))
        .await
        .expect("the stub runs");

    assert!(out.success(), "{out:?}");
    let lines: Vec<String> = out.stdout_str().lines().map(str::to_owned).collect();
    assert_eq!(
        lines,
        [
            "--json",
            "a b c",
            "quote\"and'quote",
            "semi;colon && rm -rf /"
        ],
        "no shell was involved, so nothing was split or expanded"
    );
}

#[tokio::test]
async fn the_child_gets_an_allow_list_not_our_environment() {
    let out = stub()
        .run(stub_call(&["dump-env"]))
        .await
        .expect("the stub runs");
    let env = out.stdout_str().into_owned();

    // Cargo fills this process's environment with dozens of `CARGO_*`
    // variables. None of them is on the allow-list, so none may reach the
    // child. The same mechanism is what keeps `TAILSCALE_API_KEY` and the
    // `TS_DEBUG_*` knobs out of it.
    assert!(
        !env.contains("CARGO_PKG_NAME="),
        "the parent environment leaked through:\n{env}"
    );
    assert!(
        !env.contains("CARGO_MANIFEST_DIR="),
        "the parent environment leaked through:\n{env}"
    );

    assert!(
        env.contains("LC_ALL=C"),
        "output should be locale-stable:\n{env}"
    );
    assert!(
        env.contains("PATH="),
        "the child still needs a path:\n{env}"
    );
}

#[tokio::test]
async fn a_failure_carries_its_exit_code_and_its_stderr() {
    let out = stub()
        .run(stub_call(&["fail", "7", "needs to be run as root"]))
        .await
        .expect("the stub runs");

    assert!(!out.success());
    assert_eq!(out.exit_code, Some(7));
    assert_eq!(out.stderr, "needs to be run as root");
    assert!(out.stdout.is_empty());
}

#[tokio::test]
async fn a_document_can_be_handed_over_on_standard_input() {
    let out = stub()
        .run(stub_call(&["cat"]).with_stdin("// a policy file\n{}\n"))
        .await
        .expect("the stub runs");

    assert!(out.success());
    assert_eq!(out.stdout_str(), "// a policy file\n{}\n");
}

#[tokio::test]
async fn standard_input_is_closed_when_there_is_nothing_to_send() {
    // `cat` with no input must see end-of-file immediately rather than block
    // on a terminal that is not there.
    let out = tokio::time::timeout(Duration::from_secs(5), stub().run(stub_call(&["cat"])))
        .await
        .expect("the call must not hang")
        .expect("the stub runs");

    assert!(out.success());
    assert!(out.stdout.is_empty());
}

#[tokio::test]
async fn a_call_that_overruns_is_cut_off() {
    let started = Instant::now();
    let err = stub()
        .run(stub_call(&["sleep", "60000"]).with_timeout(Duration::from_millis(200)))
        .await
        .expect_err("the call must time out");

    assert!(matches!(err, ExecError::Timeout { .. }), "{err:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the timeout should fire promptly, took {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_timed_out_child_is_asked_to_stop_before_it_is_killed() {
    let marker = tempfile::NamedTempFile::new().expect("a temporary file");
    let marker_path = marker.path().to_path_buf();
    drop(marker); // We want the path, not the file: the stub creates it.

    let err = stub()
        .run(
            stub_call(&["ignore-term", &marker_path.display().to_string()])
                .with_timeout(Duration::from_millis(300)),
        )
        .await
        .expect_err("the call must time out");
    assert!(matches!(err, ExecError::Timeout { .. }), "{err:?}");

    assert!(
        marker_path.exists(),
        "the child was killed without being asked to stop first"
    );
    let _ = std::fs::remove_file(&marker_path);
}

#[tokio::test]
async fn reads_overlap_each_other() {
    let backend = stub();
    let started = Instant::now();
    let (a, b) = tokio::join!(
        backend.run(stub_call(&["sleep", "400"])),
        backend.run(stub_call(&["sleep", "400"])),
    );
    a.expect("the stub runs");
    b.expect("the stub runs");

    assert!(
        started.elapsed() < Duration::from_millis(700),
        "two reads should have overlapped, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_mutation_waits_for_another_mutation() {
    let backend = stub();
    let mutate = |ms: &'static str| {
        let mut inv = stub_call(&["sleep", ms]);
        inv.concurrency = Concurrency::Exclusive;
        inv
    };

    let started = Instant::now();
    let (a, b) = tokio::join!(backend.run(mutate("400")), backend.run(mutate("400")));
    a.expect("the stub runs");
    b.expect("the stub runs");

    assert!(
        started.elapsed() >= Duration::from_millis(750),
        "two mutations should have queued, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_read_waits_for_a_mutation_in_flight() {
    let backend = stub();
    let mut write = stub_call(&["sleep", "400"]);
    write.concurrency = Concurrency::Exclusive;

    let started = Instant::now();
    let (a, b) = tokio::join!(
        backend.run(write),
        backend.run(stub_call(&["sleep", "400"]))
    );
    a.expect("the stub runs");
    b.expect("the stub runs");

    assert!(
        started.elapsed() >= Duration::from_millis(750),
        "a read must not overlap a mutation, took {:?}",
        started.elapsed()
    );
}

#[test]
fn the_binary_override_is_honoured() {
    let backend = CliBackend::discover_with(Some(std::ffi::OsStr::new(env!(
        "CARGO_BIN_EXE_tailscale-stub"
    ))))
    .expect("the stub is executable");
    assert_eq!(
        backend.binary(),
        std::path::Path::new(env!("CARGO_BIN_EXE_tailscale-stub"))
    );
}

#[tokio::test]
async fn a_missing_binary_is_reported_rather_than_hidden() {
    let backend = CliBackend::at("/definitely/not/here/tailscale");
    let err = backend
        .run(stub_call(&["status"]))
        .await
        .expect_err("there is no such binary");
    assert!(matches!(err, ExecError::Spawn { .. }), "{err:?}");
}
