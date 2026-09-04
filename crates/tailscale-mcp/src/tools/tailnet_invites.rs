//! Sharing a device with someone outside the tailnet, and inviting someone in.
//!
//! Two families with the same shape — list, create, get, delete, resend, and
//! for devices an accept — kept in one toolset because a caller reaching for
//! one usually wants the other, and because everything below is true of both.
//!
//! **Six of these need a user-owned credential.** Creating, resending and
//! accepting either kind, and withdrawing a tailnet one, act as a person: the
//! invite records who sent it, and an
//! OAuth client or a federated identity is nobody. The control plane refuses
//! such a call, and its message does not say why — so these tools add the
//! reason as a hint when the refusal arrives (Q76). It is added on the way
//! back rather than checked on the way out because this server cannot tell
//! what kind of credential it holds: a token is a token.
//!
//! **An invite URL is a credential.** Anyone holding one can accept, not only
//! the addressee, which is why an invite created without an email exists at
//! all. They are returned as the control plane sends them and are never
//! logged, which is the same rule as a minted key.
//!
//! **Resending is rate limited to one a minute,** upstream. A 429 carries the
//! wait, which the error model already turns into `rate_limited`.
//!
//! **Four of these answer with a bare array**, where every other listing on
//! this surface arrives wrapped — `{"devices": …}`, `{"keys": …}`,
//! `{"users": …}`. A tool result's structured content is an object, so a bare
//! array cannot be forwarded at all, and these four are wrapped in
//! `{"invites": […]}`: the API's own envelope for every listing it does wrap
//! (Q78).

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_rest::models::user::INVITE_ROLES;

use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};
use crate::tools::common::{Done, one_of, path_segment, report};

crate::tools! {
    /// List the outstanding invitations to share one device.
    tailnet_device_invite_list => DeviceParams, device_invite_list,
        toolset: TailnetInvites, tier: Read, idempotent: true;

    /// Share a device with people outside the tailnet.
    ///
    /// Takes a list, so several people can be invited at once and each gets
    /// its own settings. Each answer carries an `inviteUrl`, which is a
    /// credential: anyone holding it can accept. Omitting `email` is how to
    /// create one nobody is mailed, to be passed on by hand.
    ///
    /// Needs a user-owned credential: the invite records who sent it.
    tailnet_device_invite_create => DeviceInviteCreateParams, device_invite_create,
        toolset: TailnetInvites, tier: Write;

    /// Read one device invitation by its id.
    tailnet_device_invite_get => InviteParams, device_invite_get,
        toolset: TailnetInvites, tier: Read, idempotent: true;

    /// Withdraw a device invitation. Anyone who already accepted keeps the
    /// share; this only stops it being accepted again.
    tailnet_device_invite_delete => InviteParams, device_invite_delete,
        toolset: TailnetInvites, tier: Destructive, idempotent: true;

    /// Send the invitation email again.
    ///
    /// Only for an invite that was created with an email address, and rate
    /// limited upstream to one a minute. Needs a user-owned credential.
    tailnet_device_invite_resend => InviteParams, device_invite_resend,
        toolset: TailnetInvites, tier: Write, idempotent: false;

    /// Accept a device invitation, taking the share for this credential's own
    /// user.
    ///
    /// Takes the invite URL or the bare id at the end of it. Needs a
    /// user-owned credential: a share belongs to a person.
    tailnet_device_invite_accept => AcceptParams, device_invite_accept,
        toolset: TailnetInvites, tier: Write;

    /// List the outstanding invitations to join the tailnet.
    tailnet_user_invite_list => NoParams, user_invite_list,
        toolset: TailnetInvites, tier: Read, idempotent: true;

    /// Invite people to join the tailnet, each with the role they will get.
    ///
    /// Takes a list, so several people can be invited at once. Each answer
    /// carries an `inviteUrl`, which is a credential: anyone holding it can
    /// accept. Omitting `email` is how to create one nobody is mailed.
    ///
    /// Needs a user-owned credential: the invite records who sent it.
    tailnet_user_invite_create => UserInviteCreateParams, user_invite_create,
        toolset: TailnetInvites, tier: Write;

    /// Read one tailnet invitation by its id.
    tailnet_user_invite_get => InviteParams, user_invite_get,
        toolset: TailnetInvites, tier: Read, idempotent: true;

    /// Withdraw a tailnet invitation. Needs a user-owned credential.
    tailnet_user_invite_delete => InviteParams, user_invite_delete,
        toolset: TailnetInvites, tier: Destructive, idempotent: true;

    /// Send the invitation email again.
    ///
    /// Only for an invite that was created with an email address, and rate
    /// limited upstream to one a minute. Needs a user-owned credential.
    tailnet_user_invite_resend => InviteParams, user_invite_resend,
        toolset: TailnetInvites, tier: Write, idempotent: false;
}

/// Wrap a bare array in the envelope every other listing here arrives in.
///
/// Four endpoints answer with a JSON array at the top level. Structured
/// content in a tool result is an object, so there is nothing to forward
/// verbatim — the choice is which envelope, not whether (Q78). Anything that
/// is *not* an array goes through untouched, so a control plane that starts
/// wrapping these itself is followed rather than double-wrapped.
fn as_listing(answer: Value) -> Value {
    match answer {
        Value::Array(invites) => serde_json::json!({"invites": invites}),
        answered => answered,
    }
}

/// The reason a call that looks fine was refused.
///
/// Six of these endpoints accept only a credential owned by a person, and the
/// control plane's refusal does not say so — it is a 403, or a 400, with a
/// message about permissions. This server cannot check first, because a bearer
/// token does not say what minted it, so the requirement is added to the
/// refusal instead, where a caller will actually read it (Q76).
const NEEDS_A_PERSON: &str = "This endpoint accepts only a credential owned by a user, because an invitation records who \
     sent it. A token minted from an OAuth client or a federated identity is refused however \
     wide its scopes are.";

/// Attach that reason to a refusal that could be it.
///
/// Only to a refusal the control plane makes about permission: a 404 is a
/// missing invite and a 429 is the rate limit, and hanging an explanation
/// about credentials off either would send a caller the wrong way.
fn as_person(error: tailscale_rest::ApiError) -> ToolError {
    explain_credential(ToolError::from(error))
}

fn explain_credential(error: ToolError) -> ToolError {
    if matches!(error.code, ErrorCode::ApiError) && matches!(error.status, Some(400 | 401 | 403)) {
        return error.with_hint(NEEDS_A_PERSON);
    }
    error
}

fn device_invites_path(device: &str) -> ToolResult<String> {
    let id = path_segment("device_id", device)?;
    Ok(format!("/api/v2/device/{id}/device-invites"))
}

fn device_invite_path(id: &str, rest: &str) -> ToolResult<String> {
    let id = path_segment("invite_id", id)?;
    Ok(format!("/api/v2/device-invites/{id}{rest}"))
}

fn user_invite_path(id: &str, rest: &str) -> ToolResult<String> {
    let id = path_segment("invite_id", id)?;
    Ok(format!("/api/v2/user-invites/{id}{rest}"))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceParams {
    /// The device's node id or numeric id.
    pub device_id: String,
}

/// An invitation, of either kind.
///
/// One struct for both families: the id is the only argument either takes, and
/// which collection it names is decided by the tool, not by the caller.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct InviteParams {
    /// The invitation's id, as a listing reports it. Not the invite URL.
    pub invite_id: String,
}

// ---------------------------------------------------------------------------
// Device invitations
// ---------------------------------------------------------------------------

async fn device_invite_list(ctx: &ToolContext, params: DeviceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(as_listing(
        client
            .get(device_invites_path(&params.device_id)?)
            .send_as::<Value>()
            .await?,
    ))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceInviteCreateParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// One entry per person to invite. An empty list is refused: it would be
    /// a call that creates nothing and answers as though it worked.
    pub invites: Vec<DeviceInviteRequest>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeviceInviteRequest {
    /// Whether more than one person may accept this invitation.
    #[serde(rename = "multiUse", default, skip_serializing_if = "Option::is_none")]
    pub multi_use: Option<bool>,
    /// Whether the invited user may route through this device as an exit
    /// node, where it advertises as one.
    #[serde(
        rename = "allowExitNode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_exit_node: Option<bool>,
    /// Who to mail it to. Omit for an invite nobody is mailed, whose
    /// `inviteUrl` is then shared by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

async fn device_invite_create(
    ctx: &ToolContext,
    params: DeviceInviteCreateParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    if params.invites.is_empty() {
        return Err(ToolError::invalid_args(
            "`invites` is empty; give at least one invitation to create",
        ));
    }
    client
        .post(device_invites_path(&params.device_id)?)
        .json(&params.invites)
        .send_as::<Value>()
        .await
        .map(as_listing)
        .map_err(as_person)
}

async fn device_invite_get(ctx: &ToolContext, params: InviteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(device_invite_path(&params.invite_id, "")?)
        .send_as::<Value>()
        .await?)
}

async fn device_invite_delete(ctx: &ToolContext, params: InviteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .delete(device_invite_path(&params.invite_id, "")?)
        .send()
        .await?;
    report(Done::new("invitation withdrawn").about("invite_id", params.invite_id))
}

async fn device_invite_resend(ctx: &ToolContext, params: InviteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = device_invite_path(&params.invite_id, "/resend")?;
    resend(client, path, params.invite_id).await
}

/// Both resends, which are the same call under two prefixes.
async fn resend(
    client: &tailscale_rest::Client,
    path: String,
    invite_id: String,
) -> ToolResult<Value> {
    let answer = client.post(path).send().await.map_err(as_person)?;
    crate::tools::common::answered_or(
        answer,
        Done::new("invitation sent again").about("invite_id", invite_id),
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AcceptParams {
    /// The invitation, either as the URL that was shared or as the bare id at
    /// the end of it.
    pub invite: String,
}

#[derive(Debug, Serialize)]
struct Accept {
    invite: String,
}

async fn device_invite_accept(ctx: &ToolContext, params: AcceptParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let invite = params.invite.trim();
    if invite.is_empty() {
        return Err(ToolError::invalid_args(
            "`invite` is empty; give the invitation URL or its id",
        ));
    }
    // Sent as it was given, URL or bare id: the description accepts both, and
    // trimming one into the other would be this server guessing at a format
    // it was told not to care about.
    let body = Accept {
        invite: invite.to_owned(),
    };
    client
        .post("/api/v2/device-invites/-/accept")
        .json(&body)
        .send_as::<Value>()
        .await
        .map_err(as_person)
}

// ---------------------------------------------------------------------------
// Tailnet invitations
// ---------------------------------------------------------------------------

async fn user_invite_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(as_listing(
        client
            .get(client.tailnet_path(None, "/user-invites"))
            .send_as::<Value>()
            .await?,
    ))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserInviteCreateParams {
    /// One entry per person to invite.
    pub invites: Vec<UserInviteRequest>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserInviteRequest {
    /// The role the invitee gets on accepting: `member`, `admin`, `it-admin`,
    /// `network-admin`, `billing-admin` or `auditor`. There is no `owner`
    /// here — a tailnet's owner is not something an invitation confers.
    /// Omit it and the control plane assigns `member` (Q80).
    #[serde(default)]
    pub role: Option<String>,
    /// Who to mail it to. Omit for an invite nobody is mailed, whose
    /// `inviteUrl` is then shared by hand.
    #[serde(default)]
    pub email: Option<String>,
}

async fn user_invite_create(
    ctx: &ToolContext,
    params: UserInviteCreateParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    if params.invites.is_empty() {
        return Err(ToolError::invalid_args(
            "`invites` is empty; give at least one invitation to create",
        ));
    }
    let body: Vec<Value> = params
        .invites
        .iter()
        .map(|invite| {
            let mut entry = serde_json::Map::new();
            if let Some(role) = &invite.role {
                entry.insert(
                    "role".to_owned(),
                    Value::String(one_of("role", role, INVITE_ROLES)?),
                );
            }
            if let Some(email) = &invite.email {
                entry.insert("email".to_owned(), Value::String(email.clone()));
            }
            Ok(Value::Object(entry))
        })
        .collect::<ToolResult<_>>()?;
    client
        .post(client.tailnet_path(None, "/user-invites"))
        .json(&body)
        .send_as::<Value>()
        .await
        .map(as_listing)
        .map_err(as_person)
}

async fn user_invite_get(ctx: &ToolContext, params: InviteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(user_invite_path(&params.invite_id, "")?)
        .send_as::<Value>()
        .await?)
}

async fn user_invite_delete(ctx: &ToolContext, params: InviteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .delete(user_invite_path(&params.invite_id, "")?)
        .send()
        .await
        .map_err(as_person)?;
    report(Done::new("invitation withdrawn").about("invite_id", params.invite_id))
}

async fn user_invite_resend(ctx: &ToolContext, params: InviteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = user_invite_path(&params.invite_id, "/resend")?;
    resend(client, path, params.invite_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_refusal_about_permission_gets_the_credential_explanation() {
        // A 404 is a missing invite and a 429 is the rate limit; hanging an
        // explanation about credentials off either would send a caller looking
        // in the wrong place.
        let refused = explain_credential(ToolError::api_error(403, "forbidden"));
        assert_eq!(refused.hint.as_deref(), Some(NEEDS_A_PERSON));

        assert_eq!(
            explain_credential(ToolError::new(ErrorCode::NotFound, "gone").with_status(404)).hint,
            None
        );
        assert!(
            explain_credential(ToolError::rate_limited(Some(60)))
                .hint
                .is_some()
        );
        assert_ne!(
            explain_credential(ToolError::rate_limited(Some(60)))
                .hint
                .as_deref(),
            Some(NEEDS_A_PERSON),
            "the rate limit keeps its own hint"
        );
    }

    #[test]
    fn a_bare_array_is_wrapped_and_anything_else_is_left_alone() {
        assert_eq!(
            as_listing(json!([{"id": "di-example"}])),
            json!({"invites": [{"id": "di-example"}]})
        );
        // If the control plane starts wrapping these itself, following it is
        // right and double-wrapping is not.
        assert_eq!(as_listing(json!({"invites": []})), json!({"invites": []}));
    }

    #[test]
    fn an_invitation_is_sent_in_tailscales_spelling_and_carries_only_what_was_given() {
        let invite = DeviceInviteRequest {
            multi_use: Some(true),
            allow_exit_node: None,
            email: Some("someone@example.com".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(&invite).expect("serialisable"),
            json!({"multiUse": true, "email": "someone@example.com"}),
            "an absent field is absent, not false"
        );
    }
}
