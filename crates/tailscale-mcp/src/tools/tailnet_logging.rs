//! Reading the tailnet's two logs, and streaming either of them somewhere else.
//!
//! **Two logs, one shape of question.** Configuration audit logs record who
//! changed what; network flow logs record which node talked to which. Both are
//! read over a window that the API requires both ends of, and neither
//! paginates: a window is the only way to bound the answer, which is why
//! `start` and `end` are required here rather than defaulted to something this
//! server chose.
//!
//! **Streaming is per log type.** `configuration` and `network` each have
//! their own destination, their own status and their own delete, all under
//! `logType` in the path. The configuration a read returns is not the one a
//! write sends: `token`, `s3SecretAccessKey` and `gcsCredentials` are
//! write-only, so an endpoint read back is missing exactly the fields that
//! authenticate it. The tool descriptions say so, because a caller who reads
//! the configuration, edits it and writes it back would otherwise silently
//! erase the credential.
//!
//! **The AWS pair is for one destination.** Streaming to S3 with role-based
//! authentication needs an external id, which Tailscale mints and the operator
//! writes into an IAM trust policy; the validate call is how they check they
//! got it right before wiring the stream up. Both are on this surface because
//! they exist only to serve `destination_type: "s3"`.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tailscale_rest::models::logging::{AwsExternalIdRequest, AwsTrustPolicyRequest, LOG_TYPES};

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, each_present, path_segment, report};

crate::tools! {
    /// Read the configuration audit log over a window: who changed what, when
    /// and from where.
    ///
    /// `start` and `end` are RFC3339 timestamps and both are required — the
    /// endpoint does not paginate, so the window is what bounds the answer.
    /// The optional filters are ANDed across kinds and ORed within one: give
    /// two `event` values to see either.
    tailnet_audit_log_list => AuditLogParams, audit_log_list,
        toolset: TailnetLogging, tier: Read, idempotent: true;

    /// Read network flow logs over a window: which node reached which, over
    /// what protocol, and how much went each way.
    ///
    /// Requires network flow logging to be switched on for the tailnet —
    /// `tailnet_settings_get` reports whether it is. `start` and `end` are
    /// RFC3339 timestamps and both are required.
    tailnet_network_log_list => NetworkLogParams, network_log_list,
        toolset: TailnetLogging, tier: Read, idempotent: true;

    /// Read where a log type is being streamed to.
    ///
    /// The credential fields are write-only and never come back: `token`,
    /// `s3_secret_access_key` and `gcs_credentials` are absent from this
    /// answer even when they are configured. Do not read this, edit it and
    /// write it back — the write would clear them.
    tailnet_log_stream_get => LogStreamParams, log_stream_get,
        toolset: TailnetLogging, tier: Read, idempotent: true;

    /// Read whether a log stream is actually being delivered: last activity,
    /// last error, and the rates and counts behind them.
    tailnet_log_stream_status_get => LogStreamParams, log_stream_status_get,
        toolset: TailnetLogging, tier: Read, idempotent: true;

    /// Replace where a log type is streamed to.
    ///
    /// The whole endpoint, not a merge: a field this does not carry is gone. Send
    /// the credential — `token`, or `s3_secret_access_key`, or
    /// `gcs_credentials` — every time, because a read never returns it and a
    /// write without it removes it.
    tailnet_log_stream_replace => LogStreamReplaceParams, log_stream_replace,
        toolset: TailnetLogging, tier: Write, idempotent: true;

    /// Stop streaming a log type. The logs are still recorded and still
    /// readable here; only the delivery stops.
    tailnet_log_stream_delete => LogStreamParams, log_stream_delete,
        toolset: TailnetLogging, tier: Destructive, idempotent: true;

    /// Mint an AWS external id for role-based S3 log streaming.
    ///
    /// The answer carries the id and the Tailscale AWS account id that will
    /// assume the role; both go into the IAM trust policy. `reusable: true`
    /// returns the same id to later reusable calls until it is linked to an
    /// account, which is how a caller that may retry avoids stranding ids.
    tailnet_aws_external_id_create => AwsExternalIdParams, aws_external_id_create,
        toolset: TailnetLogging, tier: Write, idempotent: false;

    /// Check that an IAM role's trust policy actually lets Tailscale assume it
    /// with a given external id. Changes nothing; run it before configuring
    /// the stream.
    tailnet_aws_trust_policy_validate => AwsTrustPolicyParams, aws_trust_policy_validate,
        toolset: TailnetLogging, tier: Read, idempotent: true;
}

/// Both log readings need the same window, and the API requires both ends.
fn window(start: &str, end: &str) -> ToolResult<()> {
    for (name, value) in [("start", start), ("end", end)] {
        if value.trim().is_empty() {
            return Err(ToolError::invalid_args(format!(
                "`{name}` is blank; give an RFC3339 timestamp such as \
                 `2023-12-19T16:39:57-08:00`"
            )));
        }
    }
    Ok(())
}

/// The tailnet-relative path for a log type's stream, or its status.
fn stream_path(log_type: &str, rest: &str) -> ToolResult<String> {
    let log_type = path_segment("log_type", log_type)?;
    Ok(format!("/logging/{log_type}/stream{rest}"))
}

/// Name the two log types the description knows, on a refusal that looks like
/// a typo for one of them.
///
/// The list is not checked before the call (Q84): it is one of the enums Q60
/// found open, and a log type Tailscale adds should work here on the day it
/// exists rather than be refused by a copy of last year's list. What a caller
/// who wrote `config` needs is for the 404 to say so, which is what this does
/// — the same shape as the credential hint in `tailnet_invites`.
fn explain_log_type(error: ToolError, log_type: &str) -> ToolError {
    if error.status == Some(404) && !LOG_TYPES.contains(&log_type) {
        return error.with_hint(format!(
            "`{log_type}` is not one of the log types this description knows \
             ({}); check the spelling.",
            LOG_TYPES.join(", ")
        ));
    }
    error
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuditLogParams {
    /// RFC3339, inclusive: `2023-12-19T16:39:57-08:00`.
    pub start: String,
    /// RFC3339, inclusive.
    pub end: String,
    /// Only entries by these actors. A user id or a `~`-prefixed login name.
    #[serde(default)]
    pub actor: Option<Vec<String>>,
    /// Only entries about these targets.
    #[serde(default)]
    pub target: Option<Vec<String>>,
    /// Only these events, spelled as the log spells them:
    /// `NODE.UPDATE.ACL_TAGS`, `TAILNET.CREATE.TKA` and so on. The vocabulary
    /// is `<TARGET>.<ACTION>` or `<TARGET>.<ACTION>.<PROPERTY>`, and the
    /// control plane refuses one it does not know.
    #[serde(default)]
    pub event: Option<Vec<String>>,
}

async fn audit_log_list(ctx: &ToolContext, params: AuditLogParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    window(&params.start, &params.end)?;
    let mut request = client
        .get(client.tailnet_path(None, "/logging/configuration"))
        .query("start", &params.start)
        .query("end", &params.end);
    // Repeated rather than joined: the API reads one query parameter per
    // value, and a comma-joined list would be one actor whose name has commas
    // in it.
    for actor in each_present("actor", params.actor.unwrap_or_default())? {
        request = request.query("actor", actor);
    }
    for target in each_present("target", params.target.unwrap_or_default())? {
        request = request.query("target", target);
    }
    // Not checked against `AUDIT_EVENTS`: the catalogue grows whenever a
    // feature does (Q65), so a copy of it here would refuse an event that
    // exists (Q84). The values are in the parameter's description instead.
    for event in each_present("event", params.event.unwrap_or_default())? {
        request = request.query("event", event);
    }
    Ok(request.send_as::<Value>().await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NetworkLogParams {
    /// RFC3339, inclusive.
    pub start: String,
    /// RFC3339, inclusive.
    pub end: String,
}

async fn network_log_list(ctx: &ToolContext, params: NetworkLogParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    window(&params.start, &params.end)?;
    Ok(client
        .get(client.tailnet_path(None, "/logging/network"))
        .query("start", &params.start)
        .query("end", &params.end)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogStreamParams {
    /// `configuration` or `network`.
    pub log_type: String,
}

async fn log_stream_get(ctx: &ToolContext, params: LogStreamParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .get(client.tailnet_path(None, &stream_path(&params.log_type, "")?))
        .send_as::<Value>()
        .await
        .map_err(|error| explain_log_type(ToolError::from(error), &params.log_type))
}

async fn log_stream_status_get(ctx: &ToolContext, params: LogStreamParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .get(client.tailnet_path(None, &stream_path(&params.log_type, "/status")?))
        .send_as::<Value>()
        .await
        .map_err(|error| explain_log_type(ToolError::from(error), &params.log_type))
}

async fn log_stream_delete(ctx: &ToolContext, params: LogStreamParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = client.tailnet_path(None, &stream_path(&params.log_type, "")?);
    client.delete(path).send().await?;
    report(Done::new("log streaming disabled").about("log_type", params.log_type))
}

/// The endpoint to stream to.
///
/// Passed through as the caller wrote it rather than reassembled from named
/// parameters (ADR-0004). Nineteen fields, most conditional on
/// `destinationType`, and the conditions are the control plane's to enforce:
/// a struct here would have to encode which of them are required for S3 with
/// `rolearn` versus S3 with `accesskey` versus Splunk, and would be wrong the
/// day a destination is added.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogStreamReplaceParams {
    /// `configuration` or `network`.
    pub log_type: String,
    /// The whole endpoint, in Tailscale's own field names:
    /// `destinationType`, `url`, `user`, `token`, `uploadPeriodMinutes`,
    /// `compressionFormat`, and for S3 `s3Bucket`, `s3Region`, `s3KeyPrefix`,
    /// `s3AuthenticationType`, `s3AccessKeyId`, `s3SecretAccessKey`,
    /// `s3RoleArn` and `s3ExternalId`, or for GCS `gcsBucket`, `gcsKeyPrefix`,
    /// `gcsScopes` and `gcsCredentials`.
    pub configuration: Value,
}

async fn log_stream_replace(
    ctx: &ToolContext,
    params: LogStreamReplaceParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = client.tailnet_path(None, &stream_path(&params.log_type, "")?);
    let configuration = on_the_wire(params.configuration)?;
    Ok(client
        .put(path)
        .json(&configuration)
        .send_as::<Value>()
        .await?)
}

/// The stream configuration as it goes on the wire.
///
/// Almost nothing happens here, deliberately. Which fields a destination
/// requires, whether a URL is reachable, whether `destinationType` is a
/// system Tailscale streams to — all of it is the control plane's, and it
/// says so better than a guess would. The description's own `destinationType`
/// enum is proof of why: it lists eight systems and no `gcs`, while
/// `gcsBucket` in the same document says it is "Required if the destinationType
/// is `gcs`". A list copied from it would refuse a configuration the API
/// accepts (Q84).
///
/// The one thing removed is `logType`: it is read-only in the description and
/// is in the path already, so a body carrying it says the same thing twice and
/// can say it differently.
fn on_the_wire(configuration: Value) -> ToolResult<Value> {
    let Value::Object(mut configuration) = configuration else {
        return Err(ToolError::invalid_args(
            "`configuration` is an object describing the endpoint, not a list or a string",
        ));
    };
    configuration.remove("logType");
    Ok(Value::Object(configuration))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AwsExternalIdParams {
    /// Ask for an id that later reusable calls get back too, until it is
    /// linked to an AWS account. Leave it off for a fresh id each time.
    #[serde(default)]
    pub reusable: Option<bool>,
}

async fn aws_external_id_create(
    ctx: &ToolContext,
    params: AwsExternalIdParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // The model rather than a map built beside it: the drift test holds this
    // shape to the description, and a hand-written body is a second shape it
    // does not guard.
    let body = AwsExternalIdRequest {
        reusable: params.reusable,
        unknown: Default::default(),
    };
    Ok(client
        .post(client.tailnet_path(None, "/aws-external-id"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AwsTrustPolicyParams {
    /// The external id to validate, as `tailnet_aws_external_id_create`
    /// returned it.
    pub external_id: String,
    /// The IAM role Tailscale should be able to assume, as an ARN:
    /// `arn:aws:iam::000000000000:role/tailscale-log-writer`.
    pub role_arn: String,
}

async fn aws_trust_policy_validate(
    ctx: &ToolContext,
    params: AwsTrustPolicyParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = client.tailnet_path(
        None,
        &format!(
            "/aws-external-id/{}/validate-aws-trust-policy",
            path_segment("external_id", &params.external_id)?
        ),
    );
    let body = AwsTrustPolicyRequest {
        role_arn: Some(params.role_arn.clone()),
        unknown: Default::default(),
    };
    let answer = client.post(path).json(&body).send_as::<Value>().await?;
    // A pass is an empty body, and `null` reads as a tool that lost its
    // answer rather than as a policy that checks out (Q67).
    crate::tools::common::answered_or(
        answer,
        Done::new("trust policy accepted").about("role_arn", params.role_arn),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_window_with_a_blank_end_is_refused_naming_the_end() {
        let error = window("2023-12-19T16:39:57-08:00", "  ").expect_err("blank");
        let reported = serde_json::to_value(&error).expect("reportable");
        let message = reported["message"].as_str().expect("a message");
        assert!(
            message.contains("`end`"),
            "should name the parameter: {message}"
        );
        assert!(
            message.contains("2023-12-19"),
            "and show the shape it wants: {message}"
        );
    }

    #[test]
    fn the_log_type_decides_the_path_and_only_a_path_break_is_refused() {
        assert_eq!(
            stream_path("network", "/status").expect("known"),
            "/logging/network/stream/status"
        );
        // A log type the description does not list still reaches the control
        // plane, which is the point (Q84); only a segment that would rewrite
        // the path is refused.
        assert_eq!(
            stream_path("posture", "").expect("sent anyway"),
            "/logging/posture/stream"
        );
        assert!(stream_path("../acl", "").is_err());
    }

    #[test]
    fn a_log_type_that_is_not_one_of_the_two_is_explained_on_the_way_back() {
        let refused =
            ToolError::new(crate::error::ErrorCode::ApiError, "not found").with_status(404);
        let explained = explain_log_type(refused.clone(), "config");
        assert!(
            explained
                .hint
                .as_deref()
                .is_some_and(|h| h.contains("configuration")),
            "a typo should be told what the two are: {explained:?}"
        );
        // A real log type that 404s is a missing stream, not a typo.
        assert_eq!(explain_log_type(refused, "network").hint, None);
    }

    #[test]
    fn a_destination_the_description_forgot_still_reaches_the_control_plane() {
        // `gcs` is required by `gcsBucket`'s own description and missing from
        // the `destinationType` enum, so a list copied from it would refuse a
        // configuration the API accepts (Q84).
        let sent = on_the_wire(json!({
            "destinationType": "gcs",
            "gcsBucket": "mycompany-mybucket",
            "somethingNew": true,
        }))
        .expect("passed through");
        assert_eq!(sent["destinationType"], json!("gcs"));
        assert_eq!(sent["somethingNew"], json!(true));
        assert!(on_the_wire(json!(["not", "a", "document"])).is_err());
    }

    #[test]
    fn the_log_type_is_not_repeated_into_the_body() {
        // It is read-only upstream and already in the path; sent in both
        // places it could disagree with itself.
        let sent = on_the_wire(json!({"logType": "network", "url": "https://example.com"}))
            .expect("valid");
        assert_eq!(sent.get("logType"), None);
        assert_eq!(sent["url"], json!("https://example.com"));
    }
}
