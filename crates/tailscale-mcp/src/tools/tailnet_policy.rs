//! The tailnet policy file: reading it, replacing it, previewing it, testing it.
//!
//! Four tools, and three things that make this toolset unlike the rest.
//!
//! **The document is not JSON.** A policy file is HuJSON — JSON with comments
//! and trailing commas — and the comments are the part a human wrote. So it
//! travels as text under `application/hujson`, and the read answers with the
//! text rather than a parsed object. A caller that wants JSON asks for it, and
//! gets a document with the comments gone.
//!
//! **The read carries a version.** `spec.md` names this as the one documented
//! exception to answering with the control plane's body and nothing else: the
//! version is an `ETag` header, and a header cannot be forwarded in a body
//! that has no room for one. So the read answers `{etag, format, policy}` and
//! the write takes the `etag` back.
//!
//! **The write is guarded.** A policy file replace is the highest-impact call
//! on this surface: it can lock every user out of every device at once. The
//! control plane's `If-Match` is what makes that safe, and this server refuses
//! a write that carries neither the version it read nor an explicit statement
//! that it is overwriting the untouched default (Q73). A version that no
//! longer matches comes back as a conflict, because somebody else changed the
//! policy in between.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_rest::models::policy::PREVIEW_SUBJECTS;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{one_of, report};

crate::tools! {
    /// Read the tailnet policy file, with the version identifier a write has
    /// to quote back.
    ///
    /// Answers `{"etag": ..., "format": ..., "policy": ...}`. `format:
    /// "hujson"` — the default — gives the document as written, comments and
    /// all; `format: "json"` gives it parsed, with the comments gone.
    /// `details: true` asks instead for the control plane's own report on the
    /// document, with the policy base64-encoded beside its warnings and
    /// errors.
    tailnet_policy_get => PolicyGetParams, policy_get,
        toolset: TailnetPolicy, tier: Read, idempotent: true;

    /// Replace the whole tailnet policy file.
    ///
    /// The highest-impact call on this surface: the policy is what decides who
    /// may reach what, and this replaces all of it. Read it first with
    /// `tailnet_policy_get`, change what came back, and send it with the
    /// `etag` that read gave you — the write is refused without either that or
    /// `over_default: true`, which is only for a tailnet whose policy is still
    /// the untouched default. A stale `etag` is a conflict: somebody else
    /// changed the policy since you read it.
    ///
    /// Validate first with `tailnet_policy_validate`, which changes nothing.
    ///
    /// Not idempotent, unusually for a replace: the guard makes the second
    /// call fail. Once the write lands, the `etag` it was made with is stale
    /// and the policy is no longer the untouched default.
    tailnet_policy_set => PolicySetParams, policy_set,
        toolset: TailnetPolicy, tier: Destructive;

    /// Show which rules of a candidate policy would match a user or an
    /// address, without saving anything.
    ///
    /// `subject_type: "user"` with an email address in `preview_for`, or
    /// `subject_type: "ipport"` with something like `10.0.0.1:80`. Answers the
    /// matching rules and the line each is written on.
    tailnet_policy_preview => PolicyPreviewParams, policy_preview,
        toolset: TailnetPolicy, tier: Read, idempotent: true;

    /// Check a policy, or run access tests, without saving anything.
    ///
    /// Two things in one endpoint, told apart by what is sent: give `tests`
    /// and they run against the policy in force; give `policy` and that
    /// document is parsed, checked, and its own `tests` run. An empty answer
    /// is a pass, which this reports as `{"passed": true}` so that a pass is
    /// not an empty result.
    tailnet_policy_validate => PolicyValidateParams, policy_validate,
        toolset: TailnetPolicy, tier: Read, idempotent: true;
}

/// The two ways a policy document can be spelled.
///
/// HuJSON is the tailnet's own: it is what the admin console shows and what a
/// person wrote the comments in. JSON is the same rules with the comments
/// dropped, for a caller that would rather parse than read.
const FORMATS: &[&str] = &["hujson", "json"];

const HUJSON: &str = "application/hujson";
const JSON: &str = "application/json";

/// Which spelling a call is asking for.
///
/// A type rather than a string carried around, because the answer is decided
/// three times over — which `Accept` to send, whether to parse what comes
/// back, what to call the format in the report — and three separate matches on
/// a string would be three places for `"hujson"` to be spelled wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    HuJson,
    Json,
}

impl Format {
    /// HuJSON is the default because it is the endpoint's: without an `Accept`
    /// the control plane answers HuJSON.
    fn parse(given: Option<&str>) -> ToolResult<Self> {
        match given {
            None => Ok(Self::HuJson),
            Some(value) => match one_of("format", value, FORMATS)?.as_str() {
                "json" => Ok(Self::Json),
                _ => Ok(Self::HuJson),
            },
        }
    }

    fn accept(self) -> &'static str {
        match self {
            Self::HuJson => HUJSON,
            Self::Json => JSON,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::HuJson => "hujson",
            Self::Json => "json",
        }
    }

    /// Read a body in this format. HuJSON is not JSON to parse, so it stays a
    /// string; JSON was asked for as JSON and is handed back parsed.
    fn read(self, text: String) -> ToolResult<Value> {
        match self {
            Self::HuJson => Ok(Value::String(text)),
            Self::Json => serde_json::from_str(&text).map_err(|source| {
                ToolError::new(
                    crate::error::ErrorCode::ApiError,
                    format!("the policy was asked for as JSON and did not parse as JSON: {source}"),
                )
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolicyGetParams {
    /// `hujson` for the document as written, comments included, or `json` for
    /// it parsed. Defaults to `hujson`.
    #[serde(default)]
    pub format: Option<String>,
    /// Ask instead for the control plane's report on the document: the policy
    /// base64-encoded, with the warnings and errors it found. Cannot be
    /// combined with `format`, which the control plane does not accept
    /// alongside it.
    #[serde(default)]
    pub details: Option<bool>,
}

/// What a read or a write answers with.
///
/// The version is why this is a report rather than the body: a caller needs it
/// to write, and it arrives as a header (Q75).
#[derive(Debug, Serialize)]
struct PolicyDocument {
    /// The version identifier, to be quoted back by a write. Absent if the
    /// control plane sent no `ETag`, which makes a guarded write impossible.
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    /// Which spelling `policy` is in, as `format` would be given back.
    format: &'static str,
    /// The document itself: a string for `hujson`, an object for `json`.
    policy: Value,
}

/// What `details: true` answers with instead.
///
/// A separate shape because it is a different thing — the control plane's
/// report *about* the policy, not the policy — and because a `format` field
/// saying `"details"` would be a value that `format` does not accept back.
#[derive(Debug, Serialize)]
struct PolicyReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    /// The control plane's own report: `acl` base64-encoded, beside the
    /// `warnings` and `errors` it found.
    details: Value,
}

async fn policy_get(ctx: &ToolContext, params: PolicyGetParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let acl = client.tailnet_path(None, "/acl");
    if params.details.unwrap_or(false) {
        if params.format.is_some() {
            return Err(ToolError::invalid_args(
                "`details` and `format` cannot be given together: the detailed report is always \
                 JSON, and the control plane refuses an `Accept` alongside `details`",
            ));
        }
        // Deliberately no `Accept`: the description says not to send one with
        // `details`, and the report is JSON whatever a caller would ask for.
        let answer = client
            .get(acl)
            .query("details", true)
            .send_answer::<Value>()
            .await?;
        return report(PolicyReport {
            etag: answer.etag,
            details: answer.value,
        });
    }

    // Text, not JSON, because HuJSON is not JSON to parse — and the same call
    // serves `json`, whose body happens to parse.
    let format = Format::parse(params.format.as_deref())?;
    let body = client
        .get(acl)
        .header("Accept", format.accept())
        .send_text()
        .await?;
    report(PolicyDocument {
        etag: body.etag,
        format: format.as_str(),
        policy: format.read(body.text)?,
    })
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolicySetParams {
    /// The complete replacement policy: a HuJSON document as a string, or a
    /// JSON object. Whichever it is decides what is sent, so a document with
    /// comments has to be a string or the comments are lost.
    pub policy: Value,
    /// The `etag` a `tailnet_policy_get` answered with. Required unless
    /// `over_default` is true.
    #[serde(default)]
    pub etag: Option<String>,
    /// Write only if the tailnet's policy is still the untouched default it
    /// was created with. The way to write the first policy, when there is no
    /// `etag` worth reading.
    #[serde(default)]
    pub over_default: Option<bool>,
}

/// What the control plane's `If-Match` is set to, or why it cannot be.
///
/// Refused here rather than sent without the header, because a `POST` with no
/// `If-Match` succeeds: the control plane treats an absent header as "replace
/// whatever is there", and the whole point of this guard is that a caller
/// working from a document it read cannot overwrite somebody else's change it
/// never saw (Q73).
fn if_match(etag: Option<&str>, over_default: bool) -> ToolResult<String> {
    let etag = etag.map(str::trim).filter(|e| !e.is_empty());
    match (etag, over_default) {
        (Some(_), true) => Err(ToolError::invalid_args(
            "`etag` and `over_default` say different things: one writes over the version you \
             read, the other only over the untouched default. Give one.",
        )),
        (Some(etag), false) => Ok(quoted(etag)),
        (None, true) => Ok(quoted("ts-default")),
        (None, false) => Err(ToolError::invalid_args(
            "a policy write needs `etag` — the version `tailnet_policy_get` answered with — or \
             `over_default: true` to write only over a tailnet's untouched default policy",
        )
        .with_hint(
            "Without one of these the write would overwrite whatever is there, including a \
             change somebody else made since you last read the policy.",
        )),
    }
}

/// `If-Match` values are quoted. An `ETag` arrives already quoted and is left
/// alone; `ts-default` and a caller who stripped the quotes are given them.
fn quoted(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return value.to_owned();
    }
    format!("\"{value}\"")
}

async fn policy_set(ctx: &ToolContext, params: PolicySetParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let guard = if_match(params.etag.as_deref(), params.over_default.unwrap_or(false))?;
    let request = with_policy(
        client
            .post(client.tailnet_path(None, "/acl"))
            .header("If-Match", guard),
        &params.policy,
    )?;
    // No `Accept`, so the answer is HuJSON whichever spelling the body was in
    // — the endpoint's own default, and the format the next write will want to
    // start from. The new `etag` is the reason to answer at all: without it a
    // caller that writes twice has to read in between.
    let body = request.send_text().await?;
    report(PolicyDocument {
        etag: body.etag,
        format: Format::HuJson.as_str(),
        policy: Value::String(body.text),
    })
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolicyPreviewParams {
    /// The candidate policy to preview: a HuJSON string or a JSON object.
    pub policy: Value,
    /// `user` to preview what a person may reach, or `ipport` for an address
    /// and port.
    pub subject_type: String,
    /// Who or what to preview for: an email address for `user`, something like
    /// `10.0.0.1:80` for `ipport`.
    pub preview_for: String,
}

async fn policy_preview(ctx: &ToolContext, params: PolicyPreviewParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let subject_type = one_of("subject_type", &params.subject_type, PREVIEW_SUBJECTS)?;
    let request = client
        .post(client.tailnet_path(None, "/acl/preview"))
        // The API's own spelling of both, which ADR-0004 keeps: our parameters
        // are snake_case and the query is Tailscale's.
        .query("type", &subject_type)
        .query("previewFor", &params.preview_for);
    Ok(with_policy(request, &params.policy)?
        .send_as::<Value>()
        .await?)
}

/// Attach a policy document to a request, in whichever spelling it came as.
fn with_policy<'a>(
    request: tailscale_rest::RequestBuilder<'a>,
    policy: &Value,
) -> ToolResult<tailscale_rest::RequestBuilder<'a>> {
    match policy {
        Value::String(text) => Ok(request.text(HUJSON, text.clone())),
        object @ Value::Object(_) => Ok(request.json(object)),
        _ => Err(ToolError::invalid_args(
            "`policy` is a HuJSON document as a string or a policy object; it is neither",
        )),
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolicyValidateParams {
    /// Access tests to run against the policy currently in force, as
    /// `[{"src": ..., "accept": [...], "deny": [...]}]`. Give this or
    /// `policy`, not both.
    #[serde(default)]
    pub tests: Option<Vec<Value>>,
    /// A hypothetical policy to check — a HuJSON string or a JSON object —
    /// which is parsed, validated, and has its own `tests` run. Give this or
    /// `tests`, not both.
    #[serde(default)]
    pub policy: Option<Value>,
}

/// What a validation answers with when it passed.
///
/// The control plane says nothing at all, which is the same body a failed call
/// with a lost answer would have. So a pass is stated (Q67).
#[derive(Debug, Serialize)]
struct Passed {
    passed: bool,
    checked: &'static str,
}

async fn policy_validate(ctx: &ToolContext, params: PolicyValidateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // One endpoint, two modes, told apart by the body's own JSON type: an
    // array is a list of tests, anything else is a policy. Sending both would
    // be one body that cannot be both, so it is refused here rather than
    // resolved by a rule a caller cannot see.
    let (request, checked) = match (params.tests, params.policy) {
        (Some(_), Some(_)) => {
            return Err(ToolError::invalid_args(
                "`tests` and `policy` are the endpoint's two modes and only one can be sent: \
                 `tests` runs against the policy in force, `policy` checks a document that is \
                 not saved",
            ));
        }
        (None, None) => {
            return Err(ToolError::invalid_args(
                "give `tests` to run access tests against the current policy, or `policy` to \
                 check a document",
            ));
        }
        (Some(tests), None) => (
            client
                .post(client.tailnet_path(None, "/acl/validate"))
                .json(&tests),
            "the tests against the current policy",
        ),
        (None, Some(policy)) => (
            with_policy(
                client.post(client.tailnet_path(None, "/acl/validate")),
                &policy,
            )?,
            "the policy given",
        ),
    };

    let answer = request.send_as::<Value>().await?;
    match answer {
        // "An empty response body implies passing validation or tests."
        Value::Null => report(Passed {
            passed: true,
            checked,
        }),
        found => Ok(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_write_without_a_guard_is_refused_before_it_is_sent() {
        let error = if_match(None, false).expect_err("no guard");
        let reported = serde_json::to_value(&error).expect("reportable");
        assert_eq!(reported["code"], json!("invalid_args"));
        let message = reported["message"].as_str().expect("a message");
        assert!(
            message.contains("etag") && message.contains("over_default"),
            "{message}"
        );
    }

    #[test]
    fn a_guard_is_quoted_however_it_arrives() {
        // An `ETag` header arrives quoted and is used as it came; `ts-default`
        // and a caller who trimmed the quotes are given them, because
        // `If-Match` compares the quoted form.
        assert_eq!(
            if_match(Some("\"e0b2816b418\""), false).expect("an etag"),
            "\"e0b2816b418\""
        );
        assert_eq!(
            if_match(Some("e0b2816b418"), false).expect("an etag"),
            "\"e0b2816b418\""
        );
        assert_eq!(if_match(None, true).expect("the default"), "\"ts-default\"");
    }

    #[test]
    fn the_two_guards_together_are_refused_rather_than_ranked() {
        // Picking one would be this server deciding which of two contradictory
        // instructions the caller meant.
        assert!(if_match(Some("\"abc\""), true).is_err());
        // A blank `etag` is no etag, not an empty version.
        assert!(if_match(Some("  "), false).is_err());
        assert_eq!(
            if_match(Some("   "), true).expect("the default"),
            "\"ts-default\""
        );
    }

    #[test]
    fn a_format_the_endpoint_does_not_have_is_refused_with_the_two_it_does() {
        assert_eq!(Format::parse(None).expect("the default"), Format::HuJson);
        assert_eq!(Format::parse(Some("json")).expect("json"), Format::Json);
        assert_eq!(Format::Json.accept(), JSON);
        assert_eq!(Format::HuJson.accept(), HUJSON);

        let error = Format::parse(Some("yaml")).expect_err("not a format");
        let reported = serde_json::to_value(&error).expect("reportable");
        for format in FORMATS {
            assert!(
                reported["message"]
                    .as_str()
                    .is_some_and(|m| m.contains(format)),
                "{reported:#?}"
            );
        }
    }

    #[test]
    fn a_hujson_document_is_never_parsed_and_a_json_one_always_is() {
        let commented = "{\n  // kept\n  \"acls\": [],\n}".to_owned();
        assert_eq!(
            Format::HuJson.read(commented.clone()).expect("text"),
            json!(commented),
            "parsing it would fail, and reformatting it would lose the comment"
        );
        assert_eq!(
            Format::Json
                .read("{\"acls\": []}".to_owned())
                .expect("json"),
            json!({"acls": []})
        );
        assert!(
            Format::Json.read(commented).is_err(),
            "JSON was asked for and something else arrived"
        );
    }
}
