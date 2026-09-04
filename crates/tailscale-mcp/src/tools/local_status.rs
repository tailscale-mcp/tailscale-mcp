//! Reading the state of the local node.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_cli::Invocation;

use crate::cli;
use crate::context::ToolContext;
use crate::error::ToolResult;
use crate::version::{SUPPORTED_FLOOR, Version};

crate::tools! {
    /// Report the version of the `tailscale` binary this server drives, and
    /// whether it is new enough for everything this server models.
    tailscale_version => NoParams, version,
        toolset: LocalStatus, tier: Read, idempotent: true;
}

/// A tool that takes nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

/// What `tailscale_version` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct VersionReport {
    /// The release the binary reports, when it could be read.
    pub version: Option<String>,
    /// Everything the binary printed, which also carries the commit and the Go
    /// toolchain. Kept because it is what a bug report needs.
    pub raw: String,
    /// The oldest release this server is written against.
    pub supported_floor: String,
    /// Whether the binary is at or above that floor. An unstable build — an odd
    /// minor number — counts as newer than the stable release above it.
    pub meets_floor: bool,
}

async fn version(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let raw = cli::run_text(
        ctx,
        &metas::tailscale_version,
        Invocation::read(["version"]),
    )
    .await?;
    let version = Version::parse_cli_output(&raw);
    let report = VersionReport {
        version: version.map(|v| v.to_string()),
        raw: raw.trim_end().to_owned(),
        supported_floor: SUPPORTED_FLOOR.to_string(),
        meets_floor: version.is_none_or(|v| v >= SUPPORTED_FLOOR || v.is_unstable()),
    };
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::context::SelfIdentity;
    use crate::error::{ErrorCode, Redactor};
    use crate::testing::StubBackend;

    fn context(backend: StubBackend) -> ToolContext {
        ToolContext {
            local: Arc::new(backend),
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity::default(),
            cli_version: None,
        }
    }

    #[tokio::test]
    async fn the_version_is_reported_with_the_floor_it_is_judged_against() {
        let ctx = context(StubBackend::ok(
            "1.102.2\n  tailscale commit: 0123456789ab\n",
        ));
        let value = version(&ctx, NoParams {}).await.expect("succeeds");
        assert_eq!(value["version"], "1.102.2");
        assert_eq!(value["supported_floor"], SUPPORTED_FLOOR.to_string());
        assert_eq!(value["meets_floor"], true);
        assert!(
            value["raw"].as_str().is_some_and(|r| r.contains("commit")),
            "{value}"
        );
    }

    #[tokio::test]
    async fn an_old_binary_is_reported_as_below_the_floor() {
        let ctx = context(StubBackend::ok("1.72.0\n"));
        let value = version(&ctx, NoParams {}).await.expect("succeeds");
        assert_eq!(value["meets_floor"], false);
    }

    #[tokio::test]
    async fn an_unstable_build_counts_as_newer_than_the_stable_above_it() {
        let ctx = context(StubBackend::ok("1.77.0\n"));
        let value = version(&ctx, NoParams {}).await.expect("succeeds");
        assert_eq!(value["meets_floor"], true);
    }

    #[tokio::test]
    async fn a_missing_binary_is_a_backend_error_not_a_crash() {
        let ctx = context(StubBackend::missing());
        let err = version(&ctx, NoParams {}).await.expect_err("fails");
        assert_eq!(err.code, ErrorCode::BackendUnavailable);
    }

    #[tokio::test]
    async fn exactly_one_command_is_run_with_no_extra_arguments() {
        let backend = Arc::new(StubBackend::ok("1.102.2\n"));
        let ctx = ToolContext {
            local: Arc::clone(&backend) as Arc<dyn tailscale_cli::LocalBackend>,
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity::default(),
            cli_version: None,
        };
        version(&ctx, NoParams {}).await.expect("succeeds");
        assert_eq!(backend.argv(), [["version"]]);
    }
}
