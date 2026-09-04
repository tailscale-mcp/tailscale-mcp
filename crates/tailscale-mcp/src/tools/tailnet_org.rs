//! Tailnets as things an organisation has several of.
//!
//! **Alpha upstream, and the descriptions say so.** All three endpoints carry
//! an Alpha badge in Tailscale's own description. That is not a reason to
//! leave them out — an organisation running the tailnet-creation API needs
//! them — but it is a reason for a caller to know, so each description states
//! it rather than leaving an agent to discover it from a shape that changed.
//!
//! **Deleting a tailnet is the largest thing this server can do.** It takes
//! every user, every device and every piece of configuration in that tailnet
//! with it, for everyone, and no tier alone should stand between an agent and
//! that. It is one of the four tailnet-scale operations `spec.md` puts behind
//! an explicit confirmation, alongside the three tailnet-lock ones.
//!
//! **The one endpoint here that paginates is the only one on this surface
//! that does.** Every other listing in the API answers whole, which is why
//! `tailnet_device_list` had to grow a window of this server's own (Q69) and
//! this one does not: it has real pagination, so it is followed rather than
//! sliced (Q82).

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, path_segment, report};

/// The largest page the description accepts, and its default.
const LARGEST_PAGE: u32 = 100;

/// How many pages a following listing will walk before handing the cursor
/// back. Ten pages is a thousand tailnets, which no organisation has; the
/// bound exists so a control plane that keeps answering with a cursor cannot
/// hold a tool call open indefinitely (Q82).
const PAGES_FOLLOWED: usize = 10;

crate::tools! {
    /// List every tailnet in an organisation, including the original one and
    /// any created through the tailnet-creation API.
    ///
    /// Alpha upstream. This is the one paginated endpoint on this surface: by
    /// default it follows the cursor and answers with every tailnet. Pass
    /// `cursor` to take one page at a time instead, using the `cursor` the
    /// previous answer carried.
    tailnet_organization_tailnet_list => OrganizationTailnetListParams, organization_tailnet_list,
        toolset: TailnetOrg, tier: Read, idempotent: true;

    /// Create a tailnet in an organisation.
    ///
    /// Alpha upstream. The answer carries OAuth client credentials for the new
    /// tailnet; they are shown once and cannot be read back, so keep them from
    /// this answer or the tailnet is unreachable except through the admin
    /// console.
    tailnet_organization_tailnet_create => OrganizationTailnetCreateParams, organization_tailnet_create,
        toolset: TailnetOrg, tier: Write, idempotent: false;

    /// Delete a tailnet, with all of its users, devices and configuration.
    ///
    /// Alpha upstream. Irreversible, and irreversible for everyone in that
    /// tailnet rather than only for this caller — which is why it needs an
    /// explicit `confirm: true` as well as the destructive tier. Requires an
    /// access token for the tailnet being deleted, or an OAuth client with the
    /// `all` scope from the tailnet that created it.
    tailnet_organization_tailnet_delete => OrganizationTailnetParams, organization_tailnet_delete,
        toolset: TailnetOrg, tier: Destructive, confirm: true;
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrganizationTailnetListParams {
    /// The organisation's id or name, as it appears in the admin console, or
    /// `-` for the organisation the credential belongs to.
    pub organization: String,
    /// Tailnets per page, 1 to 100. Defaults to 100, the maximum.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Take one page starting here, instead of following to the end. Use the
    /// `cursor` a previous answer carried.
    #[serde(default)]
    pub cursor: Option<String>,
}

/// `/api/v2/organizations/<organization>/tailnets`.
///
/// Built here rather than through `Client::tailnet_path`, which is for
/// tailnet-scoped routes: these two are scoped to an organisation, and the
/// session's default tailnet has nothing to do with them.
fn organization_path(organization: &str) -> ToolResult<String> {
    Ok(format!(
        "/api/v2/organizations/{}/tailnets",
        path_segment("organization", organization)?
    ))
}

/// The page size, held to what the description accepts.
///
/// Refused rather than clamped: a caller asking for 500 and silently getting
/// 100 would read the short page as the whole answer.
fn checked_limit(limit: Option<u32>) -> ToolResult<u32> {
    match limit {
        None => Ok(LARGEST_PAGE),
        Some(limit) if (1..=LARGEST_PAGE).contains(&limit) => Ok(limit),
        Some(limit) => Err(ToolError::invalid_args(format!(
            "`limit` is between 1 and {LARGEST_PAGE}; `{limit}` is outside that"
        ))),
    }
}

/// One page's tailnets, and the cursor that follows it if there is one.
fn page_of(answer: &Value) -> ToolResult<(Vec<Value>, Option<String>)> {
    let tailnets = answer
        .get("tailnets")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ToolError::new(
                crate::error::ErrorCode::ApiError,
                "the control plane answered a listing with no `tailnets` list, so there is \
                 nothing to follow",
            )
            .with_hint("Call again with a `cursor` to take the answer one page at a time.")
        })?
        .clone();
    let cursor = answer
        .get("cursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned);
    Ok((tailnets, cursor))
}

async fn organization_tailnet_list(
    ctx: &ToolContext,
    params: OrganizationTailnetListParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = organization_path(&params.organization)?;
    let limit = checked_limit(params.limit)?;

    // A cursor means the caller is paging by hand, and the page is theirs to
    // read: forwarded whole, cursor included, so the next call can continue.
    if let Some(cursor) = params.cursor {
        return Ok(client
            .get(path)
            .query("limit", limit)
            .query("cursor", cursor)
            .send_as::<Value>()
            .await?);
    }

    let mut gathered: Vec<Value> = Vec::new();
    let mut total: Option<Value> = None;
    let mut cursor: Option<String> = None;
    for _ in 0..PAGES_FOLLOWED {
        let answer = client
            .get(path.clone())
            .query("limit", limit)
            .maybe_query("cursor", cursor.as_deref())
            .send_as::<Value>()
            .await?;
        total = answer.get("totalCount").cloned().or(total);
        let (tailnets, next) = page_of(&answer)?;
        gathered.extend(tailnets);
        cursor = next;
        if cursor.is_none() {
            break;
        }
    }

    let mut answer = serde_json::Map::new();
    answer.insert("tailnets".to_owned(), Value::Array(gathered));
    if let Some(total) = total {
        answer.insert("totalCount".to_owned(), total);
    }
    // Present only when the walk stopped short, and then it says so: an
    // answer that quietly ended early would be read as the whole organisation.
    if let Some(cursor) = cursor {
        answer.insert("cursor".to_owned(), Value::String(cursor));
        answer.insert(
            "more".to_owned(),
            Value::String(format!(
                "stopped after {PAGES_FOLLOWED} pages; call again with this `cursor` for the rest"
            )),
        );
    }
    Ok(Value::Object(answer))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrganizationTailnetCreateParams {
    /// The organisation's id or name, or `-` for the credential's own.
    pub organization: String,
    /// A name for the tailnet: letters, digits, spaces, apostrophes and
    /// hyphens, unique within the organisation.
    pub display_name: String,
}

async fn organization_tailnet_create(
    ctx: &ToolContext,
    params: OrganizationTailnetCreateParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = organization_path(&params.organization)?;
    // Forwarded whole: the answer carries the new tailnet's OAuth client
    // credentials, which are shown once and never again.
    Ok(client
        .post(path)
        .json(&serde_json::json!({"displayName": params.display_name}))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrganizationTailnetParams {
    /// The tailnet to delete, by its id (`T123456CNTRL`) or its name.
    pub tailnet: String,
}

async fn organization_tailnet_delete(
    ctx: &ToolContext,
    params: OrganizationTailnetParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // Named explicitly rather than taken from the session's default tailnet:
    // deleting whatever `TAILSCALE_TAILNET` happens to say is exactly the
    // accident the confirmation exists to prevent.
    let path = format!("/api/v2/tailnet/{}", path_segment("tailnet", &params.tailnet)?);
    client.delete(path).send().await?;
    report(Done::new("tailnet deleted").about("tailnet", params.tailnet))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_page_size_the_api_would_reject_is_refused_here_rather_than_shortened() {
        assert_eq!(checked_limit(None).expect("default"), LARGEST_PAGE);
        assert_eq!(checked_limit(Some(25)).expect("in range"), 25);
        assert!(checked_limit(Some(0)).is_err());

        let error = checked_limit(Some(500)).expect_err("too large");
        let reported = serde_json::to_value(&error).expect("reportable");
        let message = reported["message"].as_str().expect("a message");
        assert!(
            message.contains("100") && message.contains("500"),
            "should name both the limit and what was asked: {message}"
        );
    }

    #[test]
    fn an_empty_cursor_ends_the_walk_rather_than_repeating_the_last_page() {
        let (tailnets, cursor) = page_of(&json!({"tailnets": [{"id": "T1"}], "cursor": ""}))
            .expect("a page");
        assert_eq!(tailnets.len(), 1);
        assert_eq!(cursor, None, "an empty cursor is not a cursor");

        let (_, cursor) = page_of(&json!({"tailnets": [], "cursor": "abc"})).expect("a page");
        assert_eq!(cursor.as_deref(), Some("abc"));
    }

    #[test]
    fn a_listing_with_no_tailnets_array_is_not_read_as_an_empty_organisation() {
        assert!(page_of(&json!({"totalCount": 3})).is_err());
    }
}
