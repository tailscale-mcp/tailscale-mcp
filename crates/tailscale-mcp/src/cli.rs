//! Running a local command and reading its failure.
//!
//! The interesting part is the reading. `tailscale` reports everything through
//! a non-zero exit code and a line of English on standard error, so this is
//! where that line becomes one of the error codes an agent can act on. Getting
//! it wrong is not cosmetic: `needs_operator` tells the caller to run one
//! command and try again, while `unsupported_version` tells it to stop asking.

use tailscale_cli::{ExecError, Invocation, Output};

use crate::context::{SelfIdentity, ToolContext};
use crate::error::{ToolError, ToolResult};
use crate::meta::ToolMeta;
use crate::version::{Version, satisfies};

/// Run a command, and turn anything other than a clean exit into a tool error.
pub async fn run(ctx: &ToolContext, meta: &ToolMeta, invocation: Invocation) -> ToolResult<Output> {
    let display = invocation.display();
    let output = ctx
        .local
        .run(invocation)
        .await
        .map_err(|e| exec_error(&display, e))?;
    if output.success() {
        return Ok(output);
    }
    Err(command_failure(ctx, meta, &display, &output))
}

/// Run a command and hand back its standard output as text, failures aside.
pub async fn run_text(
    ctx: &ToolContext,
    meta: &ToolMeta,
    invocation: Invocation,
) -> ToolResult<String> {
    let output = run(ctx, meta, invocation).await?;
    Ok(output.stdout_str().into_owned())
}

/// Something went wrong before the command produced a result.
fn exec_error(display: &str, error: ExecError) -> ToolError {
    match error {
        ExecError::BinaryNotFound { .. } | ExecError::BinaryNotExecutable { .. } => {
            ToolError::backend_unavailable("the local surface", &error.to_string())
        }
        ExecError::Timeout { timeout, .. } => ToolError::timeout(display, timeout.as_secs()),
        ExecError::Spawn { .. } => {
            ToolError::backend_unavailable("the local surface", &error.to_string())
        }
        ExecError::Io { .. } | ExecError::SecretFile(_) => {
            ToolError::new(crate::error::ErrorCode::CliFailed, error.to_string())
        }
    }
}

/// The command ran and refused. Work out what kind of refusal it was.
fn command_failure(
    ctx: &ToolContext,
    meta: &ToolMeta,
    display: &str,
    output: &Output,
) -> ToolError {
    let stderr = ctx.redactor.apply(&output.stderr);
    if is_unrecognised(&stderr) {
        return version_error(ctx, meta);
    }
    if needs_operator(&stderr) {
        return ToolError::needs_operator(&stderr);
    }
    if is_not_found(&stderr) {
        return ToolError::not_found(stderr.trim());
    }
    ToolError::cli_failed(display, output.exit_code, &stderr)
}

/// The binary does not know this subcommand or flag.
///
/// Matched on Go's `flag` package wording and the CLI's own dispatch, both of
/// which are stable across the releases this server supports.
fn is_unrecognised(stderr: &str) -> bool {
    const MARKERS: &[&str] = &[
        "flag provided but not defined",
        "unknown flag",
        "unknown subcommand",
        "unknown command",
        "is not a tailscale command",
        "unrecognized command",
    ];
    let lowered = stderr.to_ascii_lowercase();
    MARKERS.iter().any(|m| lowered.contains(m))
}

/// The command exists but this user may not run it.
fn needs_operator(stderr: &str) -> bool {
    const MARKERS: &[&str] = &[
        "access denied",
        "operator",
        "must be run as root",
        "permission denied",
        "you must be root",
    ];
    let lowered = stderr.to_ascii_lowercase();
    MARKERS.iter().any(|m| lowered.contains(m))
}

/// The command ran but its target is not there.
fn is_not_found(stderr: &str) -> bool {
    const MARKERS: &[&str] = &["no such", "not found", "does not exist", "unknown peer"];
    let lowered = stderr.to_ascii_lowercase();
    MARKERS.iter().any(|m| lowered.contains(m))
}

/// Report an unrecognised command as a version problem.
///
/// The minimum comes from the tool's own row when it has one. When it does not
/// — the command predates our floor and should have been there — the floor is
/// the honest answer: something older than anything we model is running.
fn version_error(ctx: &ToolContext, meta: &ToolMeta) -> ToolError {
    let needs = meta
        .min_version
        .map(str::to_owned)
        .unwrap_or_else(|| crate::version::SUPPORTED_FLOOR.to_string());
    let found = ctx
        .cli_version
        .map_or_else(|| "unknown".to_owned(), |v| v.to_string());
    ToolError::unsupported_version(meta.name, &needs, &found)
}

/// Whether a tool's stated minimum is met by the CLI we found.
///
/// Checked before spawning, so that a tool with a known minimum reports the
/// version code with its own number rather than whatever the binary says.
pub fn version_permits(ctx: &ToolContext, meta: &ToolMeta) -> ToolResult<()> {
    if satisfies(ctx.cli_version, meta.min_version) {
        Ok(())
    } else {
        Err(version_error(ctx, meta))
    }
}

/// Read the version out of `tailscale version`.
///
/// A failure here is not an error: the probe runs at startup, and a server that
/// refused to start because it could not read a version string would be worse
/// than one that runs without knowing it.
pub async fn probe_version(backend: &dyn tailscale_cli::LocalBackend) -> Option<Version> {
    let output = backend
        .run(Invocation::read(["version"]))
        .await
        .ok()
        .filter(Output::success)?;
    Version::parse_cli_output(&output.stdout_str())
}

/// Read who this node is from `tailscale status --json`.
///
/// Only the handful of fields that name this node, because the point is to
/// recognise a control-plane operation aimed at ourselves (ticket 21) rather
/// than to model the status document — ticket 08 does that properly.
///
/// A failure gives an identity that matches nothing, which is the safe way
/// round: an operation we cannot prove is aimed at ourselves is treated as an
/// ordinary one, and the operator sees the same confirmation rules as anyone
/// managing another node.
pub async fn probe_identity(backend: &dyn tailscale_cli::LocalBackend) -> SelfIdentity {
    let Some(output) = backend
        .run(Invocation::read(["status", "--json"]))
        .await
        .ok()
        .filter(Output::success)
    else {
        return SelfIdentity::default();
    };
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&output.stdout_str()) else {
        return SelfIdentity::default();
    };
    let node = &document["Self"];
    SelfIdentity {
        device_id: node["ID"].as_str().map(str::to_owned),
        node_id: node["PublicKey"].as_str().map(str::to_owned),
        addresses: node["TailscaleIPs"]
            .as_array()
            .map(|ips| {
                ips.iter()
                    .filter_map(|ip| ip.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        dns_name: node["DNSName"].as_str().map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::error::{ErrorCode, Redactor};
    use crate::meta::{Tier, ToolMeta, Toolset};
    use crate::testing::StubBackend;

    fn meta(min_version: Option<&'static str>) -> ToolMeta {
        ToolMeta {
            name: "tailscale_service_list",
            toolset: Toolset::LocalStatus,
            tier: Tier::Read,
            summary: "",
            self_severing: false,
            requires_confirmation: false,
            idempotent: true,
            min_version,
        }
    }

    fn context(backend: StubBackend, cli_version: Option<Version>) -> ToolContext {
        ToolContext {
            local: Arc::new(backend),
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity::default(),
            cli_version,
        }
    }

    #[tokio::test]
    async fn a_clean_exit_is_not_an_error() {
        let ctx = context(StubBackend::ok("1.102.2\n"), None);
        let text = run_text(&ctx, &meta(None), Invocation::read(["version"]))
            .await
            .expect("should succeed");
        assert_eq!(text.trim(), "1.102.2");
    }

    #[tokio::test]
    async fn an_unknown_subcommand_reports_the_minimum_version() {
        let ctx = context(
            StubBackend::failure(1, "tailscale service: unknown subcommand \"list\"\n"),
            Some(Version::new(1, 78, 0)),
        );
        let err = run(
            &ctx,
            &meta(Some("1.94")),
            Invocation::read(["service", "list"]),
        )
        .await
        .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::UnsupportedVersion);
        assert!(err.message.contains("1.94"), "{}", err.message);
        assert!(err.message.contains("1.78.0"), "{}", err.message);
    }

    #[tokio::test]
    async fn an_unknown_flag_reports_the_minimum_version() {
        let ctx = context(
            StubBackend::failure(1, "flag provided but not defined: -report-posture\n"),
            None,
        );
        let err = run(&ctx, &meta(Some("1.58")), Invocation::read(["set"]))
            .await
            .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::UnsupportedVersion);
        assert!(err.message.contains("1.58"), "{}", err.message);
    }

    #[tokio::test]
    async fn a_tool_without_a_minimum_falls_back_to_the_floor() {
        let ctx = context(StubBackend::failure(1, "unknown subcommand\n"), None);
        let err = run(&ctx, &meta(None), Invocation::read(["nonsense"]))
            .await
            .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::UnsupportedVersion);
        assert!(
            err.message
                .contains(&crate::version::SUPPORTED_FLOOR.to_string()),
            "{}",
            err.message
        );
    }

    #[tokio::test]
    async fn a_known_minimum_is_checked_before_the_command_runs() {
        let ctx = context(StubBackend::ok(""), Some(Version::new(1, 78, 0)));
        let err = version_permits(&ctx, &meta(Some("1.94"))).expect_err("should refuse");
        assert_eq!(err.code, ErrorCode::UnsupportedVersion);
        version_permits(&ctx, &meta(Some("1.72"))).expect("older requirement is met");
        version_permits(&ctx, &meta(None)).expect("no requirement is always met");
    }

    #[tokio::test]
    async fn a_permission_refusal_is_reported_as_needing_an_operator() {
        let ctx = context(
            StubBackend::failure(
                1,
                "Access denied: this operation requires the operator to be set\n",
            ),
            None,
        );
        let err = run(&ctx, &meta(None), Invocation::read(["up"]))
            .await
            .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::NeedsOperator);
        assert!(
            err.hint.is_some(),
            "an operator error should say what to do"
        );
    }

    #[tokio::test]
    async fn a_missing_target_is_reported_as_not_found() {
        let ctx = context(StubBackend::failure(1, "no such peer: laptop\n"), None);
        let err = run(&ctx, &meta(None), Invocation::read(["ping", "laptop"]))
            .await
            .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn anything_else_is_a_plain_command_failure() {
        let ctx = context(StubBackend::failure(2, "something went wrong\n"), None);
        let err = run(&ctx, &meta(None), Invocation::read(["status"]))
            .await
            .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::CliFailed);
        assert_eq!(err.exit_code, Some(2));
        assert_eq!(err.stderr.as_deref(), Some("something went wrong"));
    }

    #[tokio::test]
    async fn a_missing_binary_disables_the_surface_rather_than_failing_the_command() {
        let ctx = context(StubBackend::missing(), None);
        let err = run(&ctx, &meta(None), Invocation::read(["status"]))
            .await
            .expect_err("should fail");
        assert_eq!(err.code, ErrorCode::BackendUnavailable);
    }

    #[tokio::test]
    async fn a_secret_in_the_error_stream_does_not_reach_the_caller() {
        let ctx = context(
            StubBackend::failure(1, "bad key tskey-auth-kAbCdEfGhIjK-secretpart\n"),
            None,
        );
        let err = run(&ctx, &meta(None), Invocation::read(["up"]))
            .await
            .expect_err("should fail");
        assert!(
            !err.stderr
                .as_deref()
                .unwrap_or_default()
                .contains("secretpart"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn the_version_probe_reads_the_first_line() {
        let backend = StubBackend::ok("1.102.2\n  go version: go1.24.1\n");
        assert_eq!(probe_version(&backend).await, Some(Version::new(1, 102, 2)));
    }

    #[tokio::test]
    async fn the_version_probe_gives_up_quietly() {
        assert_eq!(probe_version(&StubBackend::missing()).await, None);
        assert_eq!(probe_version(&StubBackend::failure(1, "no")).await, None);
        assert_eq!(probe_version(&StubBackend::ok("not a version")).await, None);
    }
}
