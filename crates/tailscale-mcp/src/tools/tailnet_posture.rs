//! Links to the device posture providers a tailnet trusts.
//!
//! A posture integration is a standing credential for someone else's service —
//! CrowdStrike Falcon, Intune, Jamf Pro and the rest — which Tailscale uses to
//! read what those services know about a device. So two things run through this
//! module that do not apply to the devices beside it.
//!
//! The first is the secret. Creating an integration means handing over a client
//! secret, and the control plane never sends one back. On the way through it is
//! held in [`Secret`], whose `Debug` redacts: these parameter structs derive
//! `Debug` like every other, and a derived `Debug` over a `String` is how a
//! credential reaches a log. Updating without naming one keeps the secret that
//! is already there, which is why the parameter is optional on an update and
//! required on a create.
//!
//! The second is that there is one integration per provider. Creating a second
//! for a provider that has one is refused by the control plane with a 409
//! rather than replacing it, so the tool description sends a caller to the
//! update instead of letting them find that out.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_rest::Secret;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, path_segment, report};

crate::tools! {
    /// List the posture integrations configured for the tailnet, with the
    /// status of each one's last sync. No secret is included.
    tailnet_posture_integration_list => ListParams, integration_list,
        toolset: TailnetPosture, tier: Read, idempotent: true;

    /// Read one posture integration by its id.
    tailnet_posture_integration_get => GetParams, integration_get,
        toolset: TailnetPosture, tier: Read, idempotent: true;

    /// Configure a link to a device posture provider.
    ///
    /// A tailnet may have only one integration per provider: if one already
    /// exists the control plane refuses this call, and
    /// `tailnet_posture_integration_update` is what changes the existing one.
    /// The client secret is sent to the control plane and never comes back.
    tailnet_posture_integration_create => CreateParams, integration_create,
        toolset: TailnetPosture, tier: Write;

    /// Change an existing posture integration. Anything not given is left as
    /// it is, including the client secret, and the provider cannot be changed.
    tailnet_posture_integration_update => UpdateParams, integration_update,
        toolset: TailnetPosture, tier: Write, idempotent: true;

    /// Remove a posture integration. Tailscale stops collecting posture from
    /// that provider, and any policy rule depending on those attributes stops
    /// matching.
    tailnet_posture_integration_delete => GetParams, integration_delete,
        toolset: TailnetPosture, tier: Destructive, idempotent: true;
}

fn integration_path(id: &str) -> ToolResult<String> {
    let id = path_segment("integration_id", id)?;
    Ok(format!("/api/v2/posture/integrations/{id}"))
}

/// The provider, present and sendable.
///
/// Not held to `POSTURE_PROVIDERS`. That list names six endpoint-security
/// products, which is a market rather than a specification (Q60), and the hint
/// this check used to carry — "if Tailscale has added a provider since this
/// server was built, the integration has to be created in the admin console" —
/// was an admission that the gate refused work the API would have done (Q84).
/// The values are in the parameter's description, and the control plane
/// refuses one it does not know.
fn checked_provider(provider: &str) -> ToolResult<String> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(ToolError::invalid_args(
            "`provider` is blank; name the posture provider this integration is for",
        ));
    }
    Ok(provider.to_owned())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListParams {}

async fn integration_list(ctx: &ToolContext, _params: ListParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(client.tailnet_path(None, "/posture/integrations"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetParams {
    /// The integration's id, as a listing reports it.
    pub integration_id: String,
}

async fn integration_get(ctx: &ToolContext, params: GetParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(integration_path(&params.integration_id)?)
        .send_as::<Value>()
        .await?)
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateParams {
    /// Which provider: one of `falcon`, `intune`, `jamfpro`, `kandji`,
    /// `kolide` or `sentinelone`.
    pub provider: String,
    /// The provider's client secret. Sent to the control plane and never
    /// returned by it, so keep a copy if you need one.
    pub client_secret: String,
    /// The provider's client identifier, where it issues one.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Which of the provider's clouds — `us-1` or `eu-1` for Falcon, a region
    /// for Intune. Leave unset for a provider that has one.
    #[serde(default)]
    pub cloud_id: Option<String>,
    /// Intune's directory (tenant) identifier. Not used by any other provider.
    #[serde(default)]
    pub tenant_id: Option<String>,
}

/// Written rather than derived, so that the secret is not printed.
///
/// `missing_debug_implementations` means these need a `Debug`, and the derived
/// one would print the credential a caller just handed us. Every other field is
/// shown, because a redacted struct nobody can read is its own problem.
impl std::fmt::Debug for CreateParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateParams")
            .field("provider", &self.provider)
            .field("client_secret", &"[redacted]")
            .field("client_id", &self.client_id)
            .field("cloud_id", &self.cloud_id)
            .field("tenant_id", &self.tenant_id)
            .finish()
    }
}

impl std::fmt::Debug for UpdateParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateParams")
            .field("integration_id", &self.integration_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[redacted]"),
            )
            .field("client_id", &self.client_id)
            .field("cloud_id", &self.cloud_id)
            .field("tenant_id", &self.tenant_id)
            .finish()
    }
}

/// What a create or an update sends.
///
/// Built here rather than from the model because the two calls need different
/// halves of it: a create requires `provider` and `clientSecret`, an update
/// takes neither and treats every absent field as "leave it". Serialising a
/// model of all-optional fields would send the same body for both.
#[derive(Debug, Serialize)]
struct IntegrationBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(rename = "clientSecret", skip_serializing_if = "Option::is_none")]
    client_secret: Option<Secret>,
    #[serde(rename = "clientId", skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(rename = "cloudId", skip_serializing_if = "Option::is_none")]
    cloud_id: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    tenant_id: Option<String>,
}

impl IntegrationBody {
    /// Whether this would send a `PATCH` that changes nothing.
    ///
    /// Asked of the fields rather than of the serialised body: serialising can
    /// fail, and a check that sends the request when it cannot tell is a check
    /// that fails open. It also keeps the secret out of a `Value` built only
    /// to be counted.
    fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.client_secret.is_none()
            && self.client_id.is_none()
            && self.cloud_id.is_none()
            && self.tenant_id.is_none()
    }
}

async fn integration_create(ctx: &ToolContext, params: CreateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    if params.client_secret.trim().is_empty() {
        return Err(ToolError::invalid_args(
            "`client_secret` is empty; the provider's secret is what the integration \
             authenticates with",
        ));
    }
    let body = IntegrationBody {
        provider: Some(checked_provider(&params.provider)?),
        client_secret: Some(Secret::new(params.client_secret)),
        client_id: params.client_id,
        cloud_id: params.cloud_id,
        tenant_id: params.tenant_id,
    };
    Ok(client
        .post(client.tailnet_path(None, "/posture/integrations"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateParams {
    /// The integration's id, as a listing reports it.
    pub integration_id: String,
    /// A new client secret. Omit to keep the one already configured.
    #[serde(default)]
    pub client_secret: Option<String>,
    /// A new client identifier. Omit to keep the current one.
    #[serde(default)]
    pub client_id: Option<String>,
    /// A new cloud. Omit to keep the current one.
    #[serde(default)]
    pub cloud_id: Option<String>,
    /// A new directory (tenant) identifier. Omit to keep the current one.
    #[serde(default)]
    pub tenant_id: Option<String>,
}

async fn integration_update(ctx: &ToolContext, params: UpdateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let body = IntegrationBody {
        // The control plane ignores `provider` on an update, so sending one
        // would let a caller believe a provider had been changed when it had
        // not. Deleting and creating is the way to change it.
        provider: None,
        client_secret: params.client_secret.map(Secret::new),
        client_id: params.client_id,
        cloud_id: params.cloud_id,
        tenant_id: params.tenant_id,
    };
    if body.is_empty() {
        return Err(ToolError::invalid_args(
            "nothing to change: give at least one of `client_secret`, `client_id`, `cloud_id` \
             or `tenant_id`",
        ));
    }
    Ok(client
        .patch(integration_path(&params.integration_id)?)
        .json(&body)
        .send_as::<Value>()
        .await?)
}

async fn integration_delete(ctx: &ToolContext, params: GetParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .delete(integration_path(&params.integration_id)?)
        .send()
        .await?;
    // An empty body, as everywhere else on this surface, so the tool says what
    // it did rather than answering `null` (Q67).
    report(Done::new("deleted").about("integration_id", params.integration_id))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_integration_is_addressed_outside_the_tailnet_path() {
        // Two of the five are tailnet-scoped and three are not, which is the
        // description's shape rather than a choice made here.
        assert_eq!(
            integration_path("pi-abc123").expect("a valid id"),
            "/api/v2/posture/integrations/pi-abc123"
        );
        assert!(integration_path("../devices").is_err());
    }

    #[test]
    fn a_provider_the_description_does_not_know_still_reaches_the_control_plane() {
        // The list documents the parameter; it does not gate it. A provider
        // Tailscale adds after this build should work on the day it exists
        // rather than be refused by a copy of last year's list (Q84).
        assert_eq!(checked_provider("falcon").expect("a provider"), "falcon");
        assert_eq!(
            checked_provider("  something-new  ").expect("sent anyway"),
            "something-new",
            "and it is trimmed on the way, so a stray space is not a provider"
        );

        // Blank is still a caller that meant to send nothing.
        let error = checked_provider("   ").expect_err("no provider");
        let reported = serde_json::to_value(&error).expect("reportable");
        assert_eq!(reported["code"], json!("invalid_args"));
    }

    #[test]
    fn an_update_that_changes_nothing_is_refused_rather_than_sent() {
        // A PATCH with an empty body is a call the control plane accepts and
        // that changes nothing, which reads to a caller as success.
        let body = IntegrationBody {
            provider: None,
            client_secret: None,
            client_id: None,
            cloud_id: None,
            tenant_id: None,
        };
        assert_eq!(
            serde_json::to_value(&body).expect("it serialises"),
            json!({}),
            "every field elides when unset, which is what the refusal detects"
        );
    }

    #[test]
    fn an_update_sends_only_what_it_was_given() {
        let body = IntegrationBody {
            provider: None,
            client_secret: None,
            client_id: Some("id-1".to_owned()),
            cloud_id: None,
            tenant_id: None,
        };
        assert_eq!(
            serde_json::to_value(&body).expect("it serialises"),
            json!({"clientId": "id-1"}),
            "an absent field is absent, not null, so nothing else is cleared"
        );
    }
}
