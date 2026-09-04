//! The tailnet's own settings, and the addresses its notices go to.
//!
//! Five tools over two unrelated resources, together because both are the
//! tailnet talking about itself rather than about its devices or its people.
//!
//! **A contact change is not immediate.** Setting an address mails a
//! verification link; until it is followed the old address keeps receiving,
//! and the new one sits in `fallbackEmail` with `needsVerification` true.
//! `tailnet_contact_verification_resend` mails the link again, and only works
//! while a verification is pending.
//!
//! **Settings are patched, not replaced.** The endpoint takes a partial
//! document and leaves out what is not mentioned, which is the one shape here
//! that does *not* need the replace-versus-merge distinction the DNS toolset
//! makes: there is no replacing form to confuse it with.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tailscale_rest::models::user::CONTACT_TYPES;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, answered_or, one_of};

crate::tools! {
    /// Read the addresses the tailnet's notices go to: `account`, `support`
    /// and `security`, each with its verification state.
    tailnet_contacts_get => NoParams, contacts_get,
        toolset: TailnetSettings, tier: Read, idempotent: true;

    /// Change one contact address.
    ///
    /// Not immediate: the new address is mailed a verification link and sits
    /// in `fallbackEmail` until it is followed, while the old one keeps
    /// receiving.
    tailnet_contact_update => ContactUpdateParams, contact_update,
        toolset: TailnetSettings, tier: Write, idempotent: true;

    /// Send the verification link again for a contact address waiting on one.
    ///
    /// Only works while a verification is pending; there is nothing to resend
    /// for an address already in use.
    tailnet_contact_verification_resend => ContactParams, contact_verification_resend,
        toolset: TailnetSettings, tier: Write, idempotent: true;

    /// Read the tailnet-wide settings: device and user approval, key
    /// durations, automatic updates, network flow logging and the rest.
    ///
    /// A setting reads as `null` where the tailnet's plan does not carry the
    /// feature, which is not the same answer as `false`.
    tailnet_settings_get => NoParams, settings_get,
        toolset: TailnetSettings, tier: Read, idempotent: true;

    /// Change tailnet-wide settings.
    ///
    /// A merge: a setting the document does not mention is left alone. The
    /// field names are Tailscale's own, as `tailnet_settings_get` reports
    /// them — `devicesApprovalOn`, `usersApprovalOn`,
    /// `devicesKeyDurationDays` and so on.
    tailnet_settings_update => SettingsUpdateParams, settings_update,
        toolset: TailnetSettings, tier: Write, idempotent: true;
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

async fn contacts_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(client.tailnet_path(None, "/contacts"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactParams {
    /// Which contact: `account`, `support` or `security`.
    pub contact_type: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContactUpdateParams {
    /// Which contact: `account`, `support` or `security`.
    pub contact_type: String,
    /// The new address. It receives a verification link and does not take
    /// over until that link is followed.
    pub email: String,
}

/// `/api/v2/tailnet/<tailnet>/contacts/<kind><rest>`.
///
/// The kind goes in unescaped because it has already been held to
/// [`CONTACT_TYPES`], which contains nothing a path would notice.
fn contact_path(client: &tailscale_rest::Client, kind: &str, rest: &str) -> ToolResult<String> {
    let kind = one_of("contact_type", kind, CONTACT_TYPES)?;
    Ok(client.tailnet_path(None, &format!("/contacts/{kind}{rest}")))
}

async fn contact_update(ctx: &ToolContext, params: ContactUpdateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = contact_path(client, &params.contact_type, "")?;
    let email = params.email.trim();
    if email.is_empty() {
        return Err(ToolError::invalid_args(
            "`email` is empty; a contact has to be an address",
        ));
    }
    let body = tailscale_rest::models::user::UpdateContact {
        email: Some(email.to_owned()),
        unknown: Default::default(),
    };
    let answer = client.patch(path).json(&body).send().await?;
    answered_or(
        answer,
        Done::new("verification sent to the new address")
            .about("contact_type", params.contact_type)
            .about("email", email),
    )
}

async fn contact_verification_resend(
    ctx: &ToolContext,
    params: ContactParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = contact_path(client, &params.contact_type, "/resend-verification-email")?;
    let answer = client.post(path).send().await?;
    answered_or(
        answer,
        Done::new("verification sent again").about("contact_type", params.contact_type),
    )
}

async fn settings_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(client.tailnet_path(None, "/settings"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SettingsUpdateParams {
    /// The settings to change, in Tailscale's own shape — for example
    /// `{"devicesApprovalOn": true, "devicesKeyDurationDays": 90}`. A setting
    /// not mentioned is left alone.
    pub settings: Value,
}

async fn settings_update(ctx: &ToolContext, params: SettingsUpdateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let Some(settings) = params.settings.as_object() else {
        return Err(ToolError::invalid_args(
            "`settings` is an object of settings to change, as `tailnet_settings_get` reports \
             them",
        ));
    };
    if settings.is_empty() {
        // A `PATCH` with an empty body succeeds and changes nothing, which
        // reads to a caller as the change having been made.
        return Err(ToolError::invalid_args(
            "`settings` is empty; give at least one setting to change",
        ));
    }
    let answer = client
        .patch(client.tailnet_path(None, "/settings"))
        .json(&params.settings)
        .send()
        .await?;
    answered_or(answer, Done::new("settings changed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> tailscale_rest::Client {
        tailscale_rest::Client::new(tailscale_rest::ClientConfig::new(
            tailscale_rest::credentials::Credentials::ApiKey(tailscale_rest::Secret::new(
                "tskey-api-nExAmPlE-redacted",
            )),
        ))
        .expect("a client with no network behind it")
    }

    #[test]
    fn a_contact_kind_that_is_not_one_of_the_three_never_reaches_a_path() {
        let client = client();
        assert_eq!(
            contact_path(&client, "security", "/resend-verification-email").expect("a kind"),
            "/api/v2/tailnet/-/contacts/security/resend-verification-email"
        );
        assert!(contact_path(&client, "..", "").is_err());
        assert!(contact_path(&client, "billing", "").is_err());
    }
}
