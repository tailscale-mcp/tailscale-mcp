//! Validating and deploying a policy file from a terminal.
//!
//! Everything here runs against the fake control plane and no MCP client,
//! which is ticket 25's third criterion. The other two are exit codes: quiet
//! and zero on success, loud and non-zero on failure, because the shape this
//! is meant for is a pipeline step.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write as _;

use clap::Parser as _;
use serde_json::json;
use tailscale_mcp::config::{Cli, Config, PolicyCommand};
use tailscale_mcp::server::Backends;
use tailscale_mcp::subcommands::{self, Report};
use tailscale_rest::fake::{FakeControlPlane, Response};

/// A policy file on disk, since that is what the subcommand takes.
fn written(document: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("a temporary file");
    file.write_all(document.as_bytes()).expect("to write it");
    file.flush().expect("to flush it");
    file
}

/// A configuration pointed at the fake, with a credential it will accept.
fn config(fake: &FakeControlPlane) -> Config {
    let cli = Cli::try_parse_from(["tailscale-mcp", "--no-local"]).expect("the arguments parse");
    Config::resolve_with(cli, |key| match key {
        "TAILSCALE_API_KEY" => Some("tskey-api-nExAmPlE-redacted".to_owned()),
        "TAILSCALE_MCP_API_BASE_URL" => Some(fake.base_url().to_owned()),
        _ => None,
    })
    .expect("the configuration resolves")
}

/// Backends carrying the fake's credential, rather than whatever this machine
/// happens to have in its environment.
fn backends() -> Backends {
    Backends {
        local: std::sync::Arc::new(tailscale_cli::stub::StubBackend::missing()),
        local_available: false,
        credentials: Some(tailscale_rest::credentials::Credentials::ApiKey(
            tailscale_rest::Secret::new("tskey-api-nExAmPlE-redacted"),
        )),
    }
}

async fn run(
    fake: &FakeControlPlane,
    action: fn(std::path::PathBuf) -> PolicyCommand,
    document: &str,
) -> Report {
    let file = written(document);
    let settings = config(fake);
    subcommands::policy(&settings, backends(), &action(file.path().to_path_buf())).await
}

fn check(file: std::path::PathBuf) -> PolicyCommand {
    PolicyCommand::Check { file }
}

fn deploy(file: std::path::PathBuf) -> PolicyCommand {
    PolicyCommand::Deploy { file }
}

const POLICY: &str = "{\n  // Who may reach what.\n  \"acls\": [],\n}";

#[tokio::test]
async fn a_policy_the_control_plane_accepts_is_checked_quietly() {
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane")
        .on(
            "POST",
            "/api/v2/tailnet/-/acl/validate",
            Response::json(json!({})),
        );

    let report = run(&fake, check, POLICY).await;
    assert!(report.ok, "a valid policy is not news");
    assert_eq!(
        report.text, "",
        "quiet on success: a pipeline log is read when something went wrong"
    );
}

#[tokio::test]
async fn a_malformed_policy_exits_non_zero_and_prints_what_the_control_plane_said() {
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane")
        .on(
            "POST",
            "/api/v2/tailnet/-/acl/validate",
            Response::status(
                400,
                json!({"message": "line 7: tag \"tag:nope\" is not defined"}),
            ),
        );

    let report = run(&fake, check, POLICY).await;
    assert!(!report.ok, "and the exit code carries that to the pipeline");
    assert!(
        report.text.contains("tag:nope"),
        "the upstream error is the useful part, and is passed through rather than \
         summarised: {}",
        report.text
    );
}

#[tokio::test]
async fn a_deployment_sends_the_version_it_read_a_moment_before() {
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane")
        .on(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", POLICY).with_header("ETag", "\"abc123\""),
        )
        .on("POST", "/api/v2/tailnet/-/acl", Response::json(json!({})));

    let report = run(&fake, deploy, POLICY).await;
    assert!(report.ok, "{}", report.text);
    assert_eq!(report.text, "");

    let sent = fake.recorded();
    let write = sent
        .iter()
        .find(|request| request.method == "POST")
        .expect("the write happened");
    assert_eq!(
        write.headers.get("if-match").map(String::as_str),
        Some("\"abc123\""),
        "the version identifier read immediately before, so that a write cannot \
         land on top of a change it never saw"
    );
    assert!(
        sent.iter().position(|r| r.method == "GET") < sent.iter().position(|r| r.method == "POST"),
        "and read before written, not carried in from an earlier step"
    );
}

#[tokio::test]
async fn a_version_mismatch_exits_non_zero_and_explains_the_conflict() {
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane")
        .on(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", POLICY).with_header("ETag", "\"abc123\""),
        )
        .on(
            "POST",
            "/api/v2/tailnet/-/acl",
            Response::status(412, json!({"message": "policy file has changed"})),
        );

    let report = run(&fake, deploy, POLICY).await;
    assert!(!report.ok);
    assert!(
        report.text.contains("somebody else wrote to this tailnet"),
        "a 412 is the one failure a person needs told what to do about: {}",
        report.text
    );
    assert!(
        report.text.contains("abc123"),
        "including which version it was guarding against: {}",
        report.text
    );
}

#[tokio::test]
async fn a_tailnet_with_no_policy_yet_is_written_over_its_default() {
    // No `ETag` came back, which is what an untouched tailnet looks like.
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane")
        .on(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", POLICY),
        )
        .on("POST", "/api/v2/tailnet/-/acl", Response::json(json!({})));

    let report = run(&fake, deploy, POLICY).await;
    assert!(report.ok, "{}", report.text);
    let sent = fake.recorded();
    let write = sent
        .iter()
        .find(|request| request.method == "POST")
        .expect("the write happened");
    assert_eq!(
        write.headers.get("if-match").map(String::as_str),
        Some("\"ts-default\""),
        "there is no version to guard against, so the guard becomes the control \
         plane's own name for an untouched policy (Q73)"
    );
}

#[tokio::test]
async fn a_file_that_is_not_there_is_a_failure_and_not_a_panic() {
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane");
    let settings = config(&fake);
    let report = subcommands::policy(
        &settings,
        backends(),
        &PolicyCommand::Check {
            file: std::path::PathBuf::from("/nonexistent/policy.hujson"),
        },
    )
    .await;

    assert!(!report.ok);
    assert!(
        report.text.contains("/nonexistent/policy.hujson"),
        "and says which file: {}",
        report.text
    );
    assert_eq!(
        fake.request_count(),
        0,
        "nothing was sent about a file that is not there"
    );
}

/// The advice about merging belongs to a conflict and to nothing else.
#[tokio::test]
async fn a_failure_that_is_not_a_conflict_gets_no_advice_about_merging() {
    let fake = FakeControlPlane::start()
        .await
        .expect("a fake control plane")
        .on(
            "GET",
            "/api/v2/tailnet/-/acl",
            Response::text("application/hujson", POLICY).with_header("ETag", "\"abc123\""),
        )
        .on(
            "POST",
            "/api/v2/tailnet/-/acl",
            Response::status(400, json!({"message": "tag \"tag:nope\" is not defined"})),
        );

    let report = run(&fake, deploy, POLICY).await;
    assert!(!report.ok);
    assert!(
        report.text.contains("tag:nope"),
        "the control plane's own message: {}",
        report.text
    );
    assert!(
        !report.text.contains("somebody else wrote"),
        "a malformed document already says what is wrong with it, and telling somebody to \
         merge would send them looking for a change nobody made: {}",
        report.text
    );
}
