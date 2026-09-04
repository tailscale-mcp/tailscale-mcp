//! OAuth apps: the third-party integrations a tailnet authorises.
//!
//! Not to be confused with the OAuth *clients* in `tailnet_keys`. A client is
//! a credential this tailnet mints for its own automation and holds both
//! halves of; an app here is a registration for someone else's software to run
//! the authorization-code flow against this tailnet, and is identified by
//! redirect URIs rather than by a secret. They share a word and nothing else,
//! which is why the toolsets are separate and each set of descriptions says
//! which is which.
//!
//! **An update is a replacement.** `PUT` takes the same required fields as the
//! create — `name`, `redirect_uris` and `scopes` — and an app updated without
//! one of them is an app that loses it, so all three are required here too
//! rather than optional-and-omitted (Q80 is about fields the description marks
//! optional; these it marks required).

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, each_present, path_segment, report};

crate::tools! {
    /// List the OAuth apps registered on this tailnet, with their redirect
    /// URIs and scopes. Not the OAuth clients — those are `tailnet_key_list`.
    tailnet_oauth_app_list => NoParams, oauth_app_list,
        toolset: TailnetOauthApps, tier: Read, idempotent: true;

    /// Register an OAuth app.
    ///
    /// `name` is 3 to 50 characters of letters, digits, `.`, `-` and `_`.
    /// Every redirect URI must use `https`, except `localhost`, `127.0.0.1`
    /// and `::1`, which may use any scheme; a bare IP address host is refused
    /// by the control plane.
    tailnet_oauth_app_create => OauthAppWriteParams, oauth_app_create,
        toolset: TailnetOauthApps, tier: Write, idempotent: false;

    /// Read one OAuth app.
    tailnet_oauth_app_get => OauthAppParams, oauth_app_get,
        toolset: TailnetOauthApps, tier: Read, idempotent: true;

    /// Replace an OAuth app's registration.
    ///
    /// Every field is written, not merged: `name`, `redirect_uris` and
    /// `scopes` are required, and an omitted `description` or
    /// `allowed_node_attributes` clears what was there.
    tailnet_oauth_app_update => OauthAppUpdateParams, oauth_app_update,
        toolset: TailnetOauthApps, tier: Write, idempotent: true;

    /// Delete an OAuth app. Anything holding an authorisation from it stops
    /// working.
    tailnet_oauth_app_delete => OauthAppParams, oauth_app_delete,
        toolset: TailnetOauthApps, tier: Destructive, idempotent: true;
}

fn oauth_apps_path(client: &tailscale_rest::Client) -> String {
    client.tailnet_path(None, "/oauth-apps")
}

fn oauth_app_path(client: &tailscale_rest::Client, app_id: &str) -> ToolResult<String> {
    Ok(client.tailnet_path(
        None,
        &format!("/oauth-apps/{}", path_segment("app_id", app_id)?),
    ))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OauthAppParams {
    /// The app's id, as a listing reports it.
    pub app_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OauthAppWriteParams {
    /// 3 to 50 characters of letters, digits, `.`, `-` and `_`.
    pub name: String,
    /// At most 300 characters, shown to whoever is asked to authorise the app.
    #[serde(default)]
    pub description: Option<String>,
    /// Where the authorization-code flow may return to. At least one.
    pub redirect_uris: Vec<String>,
    /// The OAuth scopes the app may be granted, such as `auth_keys:create`.
    /// At least one.
    pub scopes: Vec<String>,
    /// Custom device attributes the app may set, such as `custom:myattribute`.
    #[serde(default)]
    pub allowed_node_attributes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OauthAppUpdateParams {
    /// The app's id, as a listing reports it.
    pub app_id: String,
    #[serde(flatten)]
    pub app: OauthAppWriteParams,
}

/// What a create or a replace sends.
///
/// The description gives both the same body, so both build this one. Written
/// out rather than passed through because these five fields are ours to name:
/// `redirectURIs` and `allowedNodeAttributes` are the wire spelling, and
/// `redirect_uris` and `allowed_node_attributes` are what a caller writes
/// (ADR-0004).
#[derive(Debug, Serialize)]
struct OauthAppBody {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(rename = "redirectURIs")]
    redirect_uris: Vec<String>,
    scopes: Vec<String>,
    #[serde(
        rename = "allowedNodeAttributes",
        skip_serializing_if = "Option::is_none"
    )]
    allowed_node_attributes: Option<Vec<String>>,
}

impl OauthAppBody {
    /// Checked on the way in: the three required lists are refused empty or
    /// blank here rather than sent to come back as a 400 naming nothing.
    fn build(app: OauthAppWriteParams) -> ToolResult<Self> {
        let redirect_uris = each_present("redirect_uris", app.redirect_uris)?;
        if redirect_uris.is_empty() {
            return Err(ToolError::invalid_args(
                "`redirect_uris` is empty; an OAuth app needs somewhere to return to",
            ));
        }
        let scopes = each_present("scopes", app.scopes)?;
        if scopes.is_empty() {
            return Err(ToolError::invalid_args(
                "`scopes` is empty; an OAuth app with no scope can do nothing",
            ));
        }
        Ok(Self {
            name: app.name,
            description: app.description,
            redirect_uris,
            scopes,
            allowed_node_attributes: app
                .allowed_node_attributes
                .map(|attributes| each_present("allowed_node_attributes", attributes))
                .transpose()?,
        })
    }
}

async fn oauth_app_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(oauth_apps_path(client))
        .send_as::<Value>()
        .await?)
}

async fn oauth_app_create(ctx: &ToolContext, params: OauthAppWriteParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let body = OauthAppBody::build(params)?;
    Ok(client
        .post(oauth_apps_path(client))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

async fn oauth_app_get(ctx: &ToolContext, params: OauthAppParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(oauth_app_path(client, &params.app_id)?)
        .send_as::<Value>()
        .await?)
}

async fn oauth_app_update(ctx: &ToolContext, params: OauthAppUpdateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = oauth_app_path(client, &params.app_id)?;
    let body = OauthAppBody::build(params.app)?;
    Ok(client.put(path).json(&body).send_as::<Value>().await?)
}

async fn oauth_app_delete(ctx: &ToolContext, params: OauthAppParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = oauth_app_path(client, &params.app_id)?;
    client.delete(path).send().await?;
    report(Done::new("oauth app deleted").about("app_id", params.app_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_body_is_sent_in_tailscales_spelling() {
        let body = OauthAppBody::build(OauthAppWriteParams {
            name: "my-oauth-app".to_owned(),
            description: None,
            redirect_uris: vec!["https://example.com/oauth/callback".to_owned()],
            scopes: vec!["auth_keys:create".to_owned()],
            allowed_node_attributes: None,
        })
        .expect("valid");
        assert_eq!(
            serde_json::to_value(&body).expect("serialisable"),
            json!({
                "name": "my-oauth-app",
                "redirectURIs": ["https://example.com/oauth/callback"],
                "scopes": ["auth_keys:create"],
            }),
            "an absent optional field is absent, not null"
        );
    }

    #[test]
    fn a_registration_that_could_not_work_is_refused_before_it_is_sent() {
        let bare = |redirect_uris: Vec<String>, scopes: Vec<String>| OauthAppWriteParams {
            name: "my-oauth-app".to_owned(),
            description: None,
            redirect_uris,
            scopes,
            allowed_node_attributes: None,
        };
        let uri = || vec!["https://example.com/cb".to_owned()];
        let scope = || vec!["auth_keys:create".to_owned()];

        assert!(OauthAppBody::build(bare(vec![], scope())).is_err());
        assert!(OauthAppBody::build(bare(uri(), vec![])).is_err());
        // A blank entry is a caller who meant to send nothing there.
        assert!(OauthAppBody::build(bare(vec![String::new()], scope())).is_err());
    }
}
