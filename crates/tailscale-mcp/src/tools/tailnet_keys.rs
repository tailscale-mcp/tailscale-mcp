//! Auth keys, API access tokens, OAuth clients and federated identities.
//!
//! One endpoint family covering four kinds of credential, which is why so much
//! here is conditional on `key_type`. An auth key registers devices and takes
//! `capabilities` and an expiry; an OAuth client and a federated identity take
//! `scopes` and take neither; an API access token can be listed, read and
//! revoked but never created here. The three lists of what `keyType` may be —
//! `KEY_TYPES` on the way out, [`CREATE_KEY_TYPES`] on a create,
//! [`UPDATE_KEY_TYPES`] on an update — are the description's own, and are what
//! the refusals quote.
//!
//! **The secret.** Creating a key mints one, and the answer that carries it is
//! the only one that ever will: no later read returns it. So the answer is
//! forwarded whole, with the secret in it, and the tool description says there
//! is no second chance. Nothing on this path logs a body, at any level, and
//! `tests/minted_secrets.rs` asserts that against a real subscriber rather
//! than against a reading of the code.
//!
//! **`capabilities` travels as Tailscale wrote it.** ADR-0004: a body that is
//! Tailscale's is accepted in Tailscale's shape. A caller pasting the
//! documented example gets exactly that example on the wire.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_rest::models::key::{CREATE_KEY_TYPES, UPDATE_KEY_TYPES};

use crate::context::ToolContext;
use crate::error::ToolResult;
use crate::tools::common::{Done, one_of, path_segment, report};

crate::tools! {
    /// List the tailnet's keys: auth keys, API access tokens, OAuth clients
    /// and federated identities. No secret is included — a key's secret is
    /// returned only by the call that created it.
    ///
    /// `all` decides how wide the listing is, and defaults to true, which is
    /// every key the credential's scopes let it see. Set it to false to see
    /// only the keys belonging to the credential's own user.
    tailnet_key_list => KeyListParams, key_list,
        toolset: TailnetKeys, tier: Read, idempotent: true;

    /// Read one key by its id. A revoked or expired key answers with
    /// `invalid: true` rather than a 404.
    tailnet_key_get => KeyParams, key_get,
        toolset: TailnetKeys, tier: Read, idempotent: true;

    /// Mint an auth key, an OAuth client or a federated identity.
    ///
    /// **The secret is in the answer and nowhere else.** There is no way to
    /// read it again; a caller that loses it has to create another key and
    /// revoke this one.
    ///
    /// `key_type: "auth"` takes `capabilities` and `expiry_seconds`;
    /// `"client"` and `"federated"` take `scopes` instead, and `tags` is
    /// required when the scopes include `devices:core` or `auth_keys`. An API
    /// access token cannot be created here.
    tailnet_key_create => KeyCreateParams, key_create,
        toolset: TailnetKeys, tier: Write;

    /// Reconfigure an OAuth client or a federated identity.
    ///
    /// Auth keys and API access tokens cannot be changed: revoke and mint
    /// another. The secret is neither regenerated nor returned.
    tailnet_key_update => KeyUpdateParams, key_update,
        toolset: TailnetKeys, tier: Write, idempotent: true;

    /// Revoke a key. Anything authenticating with it stops working at once,
    /// and devices registered with an auth key are unaffected.
    tailnet_key_delete => KeyParams, key_delete,
        toolset: TailnetKeys, tier: Destructive, idempotent: true;
}

fn key_path(client: &tailscale_rest::Client, id: &str) -> ToolResult<String> {
    let id = path_segment("key_id", id)?;
    Ok(client.tailnet_path(None, &format!("/keys/{id}")))
}

/// A `keyType` checked against the list that applies where it is going.
///
/// The description gives the field three different lists — a response may say
/// `api`, a create may not ask for one, an update may not turn a key into an
/// auth key — so the check has to take which call it is for. Quoting the wrong
/// list would be worse than not checking at all.
/// The key type, checked against the list this call accepts.
///
/// `None` in and `None` out: the description gives neither endpoint a required
/// list, so an unstated type is sent as nothing at all rather than as a default
/// this server invented (Q80).
fn checked_key_type(key_type: Option<&str>, allowed: &[&str]) -> ToolResult<Option<String>> {
    key_type
        .map(|key_type| one_of("key_type", key_type, allowed))
        .transpose()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeyListParams {
    /// Every key the credential may see, rather than only its own user's.
    /// Defaults to true.
    #[serde(default)]
    pub all: Option<bool>,
}

async fn key_list(ctx: &ToolContext, params: KeyListParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(client.tailnet_path(None, "/keys"))
        // Always sent, never elided: the description marks the parameter
        // required while its text calls it optional, and the two readings give
        // different listings (Q74).
        .query("all", params.all.unwrap_or(true))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeyParams {
    /// The key's id, as a listing reports it. Not the secret.
    pub key_id: String,
}

async fn key_get(ctx: &ToolContext, params: KeyParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(key_path(client, &params.key_id)?)
        .send_as::<Value>()
        .await?)
}

async fn key_delete(ctx: &ToolContext, params: KeyParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .delete(key_path(client, &params.key_id)?)
        .send()
        .await?;
    report(Done::new("key revoked").about("key_id", params.key_id))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeyCreateParams {
    /// `auth` for a device registration key, `client` for an OAuth client,
    /// `federated` for a federated identity. Omit it and the control plane
    /// creates an `auth` key (Q80).
    #[serde(default)]
    pub key_type: Option<String>,
    /// Up to 50 characters of letters, digits, hyphens and spaces.
    #[serde(default)]
    pub description: Option<String>,
    /// Auth keys: what registering with this key produces, in Tailscale's own
    /// shape — `{"devices": {"create": {"reusable": true, "ephemeral": false,
    /// "preauthorized": true, "tags": ["tag:example"]}}}`. Sent as written.
    #[serde(default)]
    pub capabilities: Option<Value>,
    /// Auth keys: how long the key stays usable, in seconds.
    #[serde(default)]
    pub expiry_seconds: Option<i64>,
    /// OAuth clients and federated identities: what tokens minted from this
    /// credential may do, as `auth_keys` or `devices:core:read`.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    /// The tags this credential may act as. Required when `scopes` includes
    /// `devices:core` or `auth_keys`.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Federated identities: the issuer whose JWTs are accepted.
    #[serde(default)]
    pub issuer: Option<String>,
    /// Federated identities: the subject a JWT must claim.
    #[serde(default)]
    pub subject: Option<String>,
    /// Federated identities: the audience a JWT must claim.
    #[serde(default)]
    pub audience: Option<String>,
    /// Federated identities: claims mapped to the values they must have.
    #[serde(default)]
    pub custom_claim_rules: Option<std::collections::BTreeMap<String, String>>,
}

/// What a create or an update sends.
///
/// Built here rather than serialised from a model because a create and an
/// update take different halves of it, and because `capabilities` is passed
/// through as the caller wrote it (ADR-0004) rather than round-tripped through
/// a struct that would drop anything the description has not caught up with.
#[derive(Debug, Serialize)]
struct KeyBody {
    #[serde(rename = "keyType", skip_serializing_if = "Option::is_none")]
    key_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Value>,
    #[serde(rename = "expirySeconds", skip_serializing_if = "Option::is_none")]
    expiry_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<String>,
    #[serde(rename = "customClaimRules", skip_serializing_if = "Option::is_none")]
    custom_claim_rules: Option<std::collections::BTreeMap<String, String>>,
}

async fn key_create(ctx: &ToolContext, params: KeyCreateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let key_type = checked_key_type(params.key_type.as_deref(), CREATE_KEY_TYPES)?;
    let body = KeyBody {
        key_type,
        description: params.description,
        capabilities: params.capabilities,
        expiry_seconds: params.expiry_seconds,
        scopes: params.scopes,
        tags: params.tags,
        issuer: params.issuer,
        subject: params.subject,
        audience: params.audience,
        custom_claim_rules: params.custom_claim_rules,
    };
    // Forwarded whole, secret and all: this answer is the only place the
    // secret ever appears.
    Ok(client
        .post(client.tailnet_path(None, "/keys"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeyUpdateParams {
    /// The key's id, as a listing reports it.
    pub key_id: String,
    /// `client` or `federated`. An auth key or an API access token cannot be
    /// reconfigured. Omitted, it is not sent, and the control plane decides
    /// whether an update without one is meaningful (Q80).
    #[serde(default)]
    pub key_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// The complete replacement list of scopes.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    /// The complete replacement list of tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub custom_claim_rules: Option<std::collections::BTreeMap<String, String>>,
}

async fn key_update(ctx: &ToolContext, params: KeyUpdateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = key_path(client, &params.key_id)?;
    let body = KeyBody {
        key_type: checked_key_type(params.key_type.as_deref(), UPDATE_KEY_TYPES)?,
        description: params.description,
        // An update carries neither: a key's capabilities and its expiry are
        // fixed when it is minted, and the description's update body has no
        // field for either.
        capabilities: None,
        expiry_seconds: None,
        scopes: params.scopes,
        tags: params.tags,
        issuer: params.issuer,
        subject: params.subject,
        audience: params.audience,
        custom_claim_rules: params.custom_claim_rules,
    };
    Ok(client.put(path).json(&body).send_as::<Value>().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_call_is_held_to_its_own_list_of_key_types() {
        // The whole reason `checked_key_type` takes the list: `api` is a real
        // key type that cannot be created, and `auth` is a real key type that
        // cannot be updated.
        assert!(checked_key_type(Some("auth"), CREATE_KEY_TYPES).is_ok());
        assert!(checked_key_type(Some("api"), CREATE_KEY_TYPES).is_err());
        assert!(checked_key_type(Some("auth"), UPDATE_KEY_TYPES).is_err());
        assert!(checked_key_type(Some("client"), UPDATE_KEY_TYPES).is_ok());

        // Unstated is not a value to check; it is a field that is not sent.
        assert_eq!(checked_key_type(None, UPDATE_KEY_TYPES).expect("no type"), None);

        let error = checked_key_type(Some("auth"), UPDATE_KEY_TYPES).expect_err("not updatable");
        let reported = serde_json::to_value(&error).expect("reportable");
        let message = reported["message"].as_str().expect("a message");
        assert!(
            message.contains("client") && message.contains("federated"),
            "the refusal quotes the list that applies here: {message}"
        );
        assert!(
            !message.contains("api"),
            "and not one that does not: {message}"
        );
    }

    #[test]
    fn a_capabilities_object_is_sent_exactly_as_it_was_given() {
        // The criterion in its smallest form: Tailscale's documented example,
        // through this server, unchanged.
        let documented = json!({
            "devices": {"create": {
                "reusable": false, "ephemeral": false,
                "preauthorized": false, "tags": ["tag:example"],
            }},
        });
        let body = KeyBody {
            key_type: Some("auth".to_owned()),
            description: Some("example".to_owned()),
            capabilities: Some(documented.clone()),
            expiry_seconds: Some(86400),
            scopes: None,
            tags: None,
            issuer: None,
            subject: None,
            audience: None,
            custom_claim_rules: None,
        };
        let sent = serde_json::to_value(&body).expect("it serialises");
        assert_eq!(sent["capabilities"], documented);
        assert_eq!(
            sent,
            json!({
                "keyType": "auth",
                "description": "example",
                "capabilities": documented,
                "expirySeconds": 86400,
            }),
            "and nothing else is sent: an absent field is absent, not null"
        );
    }
}
