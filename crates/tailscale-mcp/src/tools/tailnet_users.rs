//! The people who have access to the tailnet.
//!
//! Seven endpoints, five of which change somebody's standing: approving,
//! suspending, restoring, deleting, and changing a role. All five refuse to act
//! on the credential's own user — the control plane's rule, not this server's —
//! so an admin cannot lock themselves out through the API.
//!
//! **The two listing filters are documented strings with an extra value.**
//! `role` and `type` both accept everything the field itself accepts plus
//! `all`, which means do not filter. They are separate constants
//! ([`USER_ROLE_FILTERS`] against [`USER_ROLES`]) because a filter that
//! accepted `all` where the field does not would be a role nobody can be set
//! to, and the drift test holds each to its own place in the description.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tailscale_rest::models::user::{USER_ROLE_FILTERS, USER_ROLES, USER_TYPE_FILTERS};

use crate::context::ToolContext;
use crate::error::ToolResult;
use crate::tools::common::{Done, answered_or, one_of, path_segment};

crate::tools! {
    /// List the tailnet's users. Answers with `{"users": [...]}`.
    ///
    /// `role` and `type` narrow the listing at the control plane. Both accept
    /// `all`, which is the same as leaving them out.
    tailnet_user_list => UserListParams, user_list,
        toolset: TailnetUsers, tier: Read, idempotent: true;

    /// Read one user by their id.
    tailnet_user_get => UserParams, user_get,
        toolset: TailnetUsers, tier: Read, idempotent: true;

    /// Change a user's role.
    ///
    /// A credential owned by a user cannot change that user's own role, which
    /// the control plane refuses rather than this server.
    tailnet_user_role_set => UserRoleParams, user_role_set,
        toolset: TailnetUsers, tier: Write, idempotent: true;

    /// Approve a user waiting for approval.
    ///
    /// Does nothing if the tailnet does not require approval or the user is
    /// already approved, which the control plane treats as success.
    tailnet_user_approve => UserParams, user_approve,
        toolset: TailnetUsers, tier: Write, idempotent: true;

    /// Suspend a user: their devices stop connecting and they cannot sign in.
    /// Reversible with `tailnet_user_restore`.
    tailnet_user_suspend => UserParams, user_suspend,
        toolset: TailnetUsers, tier: Write, idempotent: true;

    /// Restore a suspended user, and their devices with them.
    tailnet_user_restore => UserParams, user_restore,
        toolset: TailnetUsers, tier: Write, idempotent: true;

    /// Delete a user from the tailnet, along with every device they own.
    ///
    /// Not reversible: the user has to be invited again and their devices
    /// re-registered.
    tailnet_user_delete => UserParams, user_delete,
        toolset: TailnetUsers, tier: Destructive, idempotent: true;
}

/// `/api/v2/users/<id><rest>`.
///
/// Not a tailnet path: a user is addressed globally, the same way a device is,
/// and only the listing goes through the tailnet.
fn user_path(id: &str, rest: &str) -> ToolResult<String> {
    let id = path_segment("user_id", id)?;
    Ok(format!("/api/v2/users/{id}{rest}"))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserListParams {
    /// `member` for the tailnet's own users, `shared` for users a device was
    /// shared with, `all` for both. Omit for all.
    #[serde(default)]
    pub user_type: Option<String>,
    /// One of the roles — `owner`, `admin`, `member` and the rest — or `all`.
    /// Omit for all.
    #[serde(default)]
    pub role: Option<String>,
}

async fn user_list(ctx: &ToolContext, params: UserListParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let user_type = params
        .user_type
        .as_deref()
        .map(|value| one_of("user_type", value, USER_TYPE_FILTERS))
        .transpose()?;
    let role = params
        .role
        .as_deref()
        .map(|value| one_of("role", value, USER_ROLE_FILTERS))
        .transpose()?;
    Ok(client
        .get(client.tailnet_path(None, "/users"))
        // `type` is the API's name for it and `user_type` is ours: `type` is a
        // Rust keyword, and a parameter a caller cannot spell in every client
        // is worse than one renamed on the way out (ADR-0004).
        .maybe_query("type", user_type)
        .maybe_query("role", role)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserParams {
    /// The user's id, as a listing reports it.
    pub user_id: String,
}

async fn user_get(ctx: &ToolContext, params: UserParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(user_path(&params.user_id, "")?)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserRoleParams {
    /// The user's id, as a listing reports it.
    pub user_id: String,
    /// `owner`, `member`, `admin`, `it-admin`, `network-admin`,
    /// `billing-admin` or `auditor`.
    pub role: String,
}

async fn user_role_set(ctx: &ToolContext, params: UserRoleParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = user_path(&params.user_id, "/role")?;
    let body = tailscale_rest::models::user::UserRole {
        // `all` is a filter, not a role: it belongs to the listing and would
        // be a role nobody can hold.
        role: Some(one_of("role", &params.role, USER_ROLES)?),
        unknown: Default::default(),
    };
    let answer = client.post(path).json(&body).send().await?;
    answered_or(
        answer,
        Done::new("role changed")
            .about("user_id", params.user_id)
            .about("role", params.role),
    )
}

/// The four standing changes, which differ only in the last path segment and
/// in what to say afterwards.
///
/// Written once rather than four times: each is a bodyless `POST` whose answer
/// is empty, so four copies would be four chances for one of them to drift.
async fn standing(
    ctx: &ToolContext,
    user_id: String,
    action: &'static str,
    done: &'static str,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = user_path(&user_id, &format!("/{action}"))?;
    let answer = client.post(path).send().await?;
    answered_or(answer, Done::new(done).about("user_id", user_id))
}

async fn user_approve(ctx: &ToolContext, params: UserParams) -> ToolResult<Value> {
    standing(ctx, params.user_id, "approve", "user approved").await
}

async fn user_suspend(ctx: &ToolContext, params: UserParams) -> ToolResult<Value> {
    standing(ctx, params.user_id, "suspend", "user suspended").await
}

async fn user_restore(ctx: &ToolContext, params: UserParams) -> ToolResult<Value> {
    standing(ctx, params.user_id, "restore", "user restored").await
}

async fn user_delete(ctx: &ToolContext, params: UserParams) -> ToolResult<Value> {
    standing(ctx, params.user_id, "delete", "user deleted").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_is_addressed_outside_the_tailnet_path() {
        assert_eq!(
            user_path("uid-example", "/suspend").expect("a valid id"),
            "/api/v2/users/uid-example/suspend"
        );
        assert!(user_path("..", "/delete").is_err());
    }

    #[test]
    fn the_filter_that_means_do_not_filter_is_not_a_role_anyone_can_hold() {
        // Two lists that differ by one value, and mixing them up would either
        // reject a legitimate filter or accept a role nobody can be set to.
        assert!(one_of("role", "all", USER_ROLE_FILTERS).is_ok());
        assert!(one_of("role", "all", USER_ROLES).is_err());
        assert!(one_of("role", "owner", USER_ROLES).is_ok());
        assert!(one_of("user_type", "shared", USER_TYPE_FILTERS).is_ok());
    }
}
