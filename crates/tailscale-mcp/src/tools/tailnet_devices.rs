//! The devices in the tailnet, as the control plane sees them.
//!
//! The first tailnet toolset, so it settles a few things the rest follow.
//!
//! **Identifiers.** Every device endpoint takes a `device_id`, and the API
//! accepts either the node id (`n1234567CNTRL`, which the listing calls
//! `nodeId`) or the numeric `id`. Both are passed through as the caller wrote
//! them; nothing here converts one to the other, because either is valid and
//! guessing wrong would be a call to the wrong device.
//!
//! **Answers.** ADR-0004 has Tailscale's bodies come back in Tailscale's shape,
//! so a listing answers with `{"devices": [...]}` and a device with the device.
//! The one thing added is on the endpoints that answer with nothing: a caller
//! that asked for a deletion gets a small report saying what was deleted rather
//! than `null`, because `null` is indistinguishable from a bug.
//!
//! **Severing.** Six of these can cut this node off from the tailnet it is
//! being reached over — deleting it, expiring its key, de-authorising it,
//! moving its address, retagging it, or dropping the routes it carries. None
//! is marked self-severing here, because nothing yet recognises *this* node
//! among the arguments: ticket 21 builds that, and marking them before it
//! exists would put a claim in the annotations that the server cannot keep.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tailscale_rest::models::device::DEVICE_FIELDS;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, answered_or, one_of, path_segment, report};

crate::tools! {
    /// List the devices in the tailnet. Answers with the control plane's own
    /// `{"devices": [...]}`, or with a `window` beside it when `limit` or
    /// `offset` narrowed the list here.
    ///
    /// The endpoint has no pagination, so a large tailnet answers whole and
    /// may exceed this server's result cap. `filters` is what asks the control
    /// plane for less, and so the only one of these that can rescue such a
    /// listing; `limit` and `offset` are applied to an answer that has already
    /// arrived. `fields: "all"` adds the costlier fields — posture identity,
    /// client connectivity — that `default` leaves out, and `fields:
    /// "default"` is the smaller answer.
    tailnet_device_list => DeviceListParams, device_list,
        toolset: TailnetDevices, tier: Read, idempotent: true;

    /// Read one device by its node id or its numeric id.
    tailnet_device_get => DeviceGetParams, device_get,
        toolset: TailnetDevices, tier: Read, idempotent: true;

    /// Remove a device from the tailnet permanently. The machine has to
    /// re-authenticate to come back, as a new device with a new address.
    ///
    /// Only devices belonging to this tailnet can be deleted; a device shared
    /// in from another tailnet is refused by the control plane.
    tailnet_device_delete => DeviceParams, device_delete,
        toolset: TailnetDevices, tier: Destructive, idempotent: true;

    /// Expire a device's node key, which disconnects it until someone
    /// re-authenticates the machine. The device itself is kept.
    tailnet_device_expire => DeviceParams, device_expire,
        toolset: TailnetDevices, tier: Destructive;

    /// Authorise a device, or revoke its authorisation.
    ///
    /// Only meaningful on a tailnet that requires device approval. Authorising
    /// is a write; revoking disconnects the device until it is authorised
    /// again, and needs the destructive tier.
    tailnet_device_authorize => DeviceAuthorizeParams, device_authorize,
        toolset: TailnetDevices, tier: Write, idempotent: true, varying: true;

    /// Rename a device. Its old MagicDNS names stop resolving, so anything
    /// addressing it by name has to be updated.
    tailnet_device_rename => DeviceRenameParams, device_rename,
        toolset: TailnetDevices, tier: Write, idempotent: true;

    /// Replace a device's tags. Tags must already be defined in the tailnet
    /// policy file, and the credential's own tags limit what it may assign.
    ///
    /// This replaces the whole set: tags not listed are removed, and removing
    /// a tag can remove the access a policy rule granted through it.
    tailnet_device_tags_set => DeviceTagsParams, device_tags_set,
        toolset: TailnetDevices, tier: Write, idempotent: true;

    /// Turn a device's key expiry off, so it stays connected without
    /// re-authenticating, or back on.
    tailnet_device_key_expiry_set => DeviceKeyExpiryParams, device_key_expiry_set,
        toolset: TailnetDevices, tier: Write, idempotent: true;

    /// Move a device to a different Tailscale IPv4 address.
    ///
    /// Existing connections to the old address break. The address has to come
    /// from the tailnet's own range.
    tailnet_device_ip_set => DeviceIpParams, device_ip_set,
        toolset: TailnetDevices, tier: Write, idempotent: true;

    /// Read the routes a device advertises and the subset that is enabled.
    tailnet_device_routes_get => DeviceParams, device_routes_get,
        toolset: TailnetDevices, tier: Read, idempotent: true;

    /// Replace the set of advertised routes that are enabled for a device.
    ///
    /// Only the enabled set can be set here; what a device advertises is the
    /// device's own configuration. Enabling a route the device does not
    /// advertise has no effect until it does.
    tailnet_device_routes_set => DeviceRoutesParams, device_routes_set,
        toolset: TailnetDevices, tier: Write, idempotent: true;

    /// Read the custom posture attributes set on a device, and when each
    /// expires.
    tailnet_device_attributes_get => DeviceParams, device_attributes_get,
        toolset: TailnetDevices, tier: Read, idempotent: true;

    /// Set one custom posture attribute on one device.
    ///
    /// The key must begin `custom:`. The value's type — string, number or
    /// boolean — is fixed by the first write and a later write of a different
    /// type is refused by the control plane.
    tailnet_device_attribute_set => AttributeSetParams, device_attribute_set,
        toolset: TailnetDevices, tier: Write, idempotent: true;

    /// Delete one custom posture attribute from one device. Only `custom:`
    /// attributes can be deleted; the ones Tailscale sets are read-only.
    tailnet_device_attribute_delete => AttributeParams, device_attribute_delete,
        toolset: TailnetDevices, tier: Destructive, idempotent: true;

    /// Set custom posture attributes on many devices in one call.
    ///
    /// A merge: a device the map does not mention is left alone, an attribute
    /// it does not mention is left alone, and an attribute given `null` is
    /// deleted. Each value is a string, a number, a boolean, `null`, or an
    /// object `{"value": ..., "expiry": "<RFC 3339>"}` to set an expiry too.
    tailnet_device_attributes_update => AttributesBatchParams, device_attributes_update,
        toolset: TailnetDevices, tier: Write, idempotent: true;
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// `/api/v2/device/<id>`, with whatever follows it.
///
/// Not a tailnet path: a device is addressed globally by its own id, and the
/// control plane resolves which tailnet it belongs to. Only the two
/// tailnet-wide endpoints — the listing and the batch attribute update — go
/// through [`tailscale_rest::Client::tailnet_path`].
fn device_path(device: &str, rest: &str) -> ToolResult<String> {
    let id = device_id("device_id", device)?;
    Ok(format!("/api/v2/device/{id}{rest}"))
}

/// A device identifier, checked and trimmed.
///
/// Wraps [`path_segment`] under a name that fits both its uses: most of these
/// identifiers become a path segment, and the batch attribute update's become
/// keys in a body. The check wanted is the same either way — an identifier and
/// not something that reshapes what it is put into — and `what` names the
/// parameter it came from, so a refusal points at the caller's own argument.
fn device_id(what: &str, device: &str) -> ToolResult<String> {
    path_segment(what, device)
}

/// The same, for the one endpoint that also names an attribute.
fn attribute_path(device: &str, key: &str) -> ToolResult<String> {
    let key = path_segment("attribute_key", key)?;
    if !key.starts_with("custom:") {
        return Err(ToolError::invalid_args(format!(
            "`attribute_key` has to begin with `custom:`; `{key}` is one of Tailscale's own \
             attributes, which are read-only"
        ))
        .with_hint("Prefix the name with `custom:`, as in `custom:diskEncrypted`."));
    }
    device_path(device, &format!("/attributes/{key}"))
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

/// One filter value, or several for the same field.
///
/// The API ANDs repeated parameters — `tags=tag:prod&tags=tag:subnet` selects
/// devices carrying both — so a map of one value per field could not express
/// half of what the endpoint offers. Either spelling is accepted because the
/// single value is much the commoner and `["tag:prod"]` is a worse way to
/// write it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Filter {
    One(String),
    Several(Vec<String>),
}

impl Filter {
    fn values(&self) -> &[String] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Several(values) => values,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceListParams {
    /// Which fields each device carries: `default` for the common ones, `all`
    /// to add posture identity and client connectivity. Omit for `default`.
    #[serde(default)]
    pub fields: Option<String>,
    /// Server-side filters, as field name to value, combined with AND — for
    /// example `{"isEphemeral": "true", "tags": ["tag:prod", "tag:subnet"]}`
    /// selects ephemeral devices carrying both tags. The field names are
    /// Tailscale's own, as they appear in a device.
    #[serde(default)]
    pub filters: Option<std::collections::BTreeMap<String, Filter>>,
    /// Return at most this many devices. Applied here rather than by the
    /// control plane, which has no pagination: the whole listing is fetched
    /// and this limits what is passed on. It cannot rescue a listing too large
    /// for the result cap, which is applied to the answer as it arrives.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Skip this many devices before `limit` applies.
    #[serde(default)]
    pub offset: Option<usize>,
}

/// What a windowed listing answers with.
///
/// Only a *windowed* one: without `limit` or `offset` the answer is the
/// control plane's own, forwarded whole (ADR-0004). The counts exist because
/// the API sends none, so without them a truncated list and a short tailnet
/// look identical (Q69).
#[derive(Debug, Serialize)]
struct DeviceListReport {
    devices: Vec<Value>,
    window: Window,
}

#[derive(Debug, Serialize)]
struct Window {
    /// How many the control plane sent, before the window.
    total: usize,
    /// How many are in `devices`.
    returned: usize,
    offset: usize,
    /// The limit asked for, so that an empty page is legible: `returned: 0`
    /// with a limit of zero is one thing and an offset past the end another.
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

async fn device_list(ctx: &ToolContext, params: DeviceListParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let fields = checked_fields(params.fields.as_deref())?;
    let mut request = client
        .get(client.tailnet_path(None, "/devices"))
        .maybe_query("fields", fields);
    for (name, filter) in params.filters.iter().flatten() {
        // The API's filters are `<field>=<value>` pairs alongside `fields`, so
        // a caller could shadow it by naming it here. That would be a filter
        // silently changing which fields came back, so it is refused.
        if name == "fields" {
            return Err(ToolError::invalid_args(
                "`fields` is its own parameter and cannot also be a filter",
            ));
        }
        for value in filter.values() {
            request = request.query(name, value);
        }
    }
    let answer = request.send_as::<Value>().await?;

    // Unwindowed, the answer is the control plane's, forwarded whole. Reading
    // `devices` out and rebuilding around it would drop any other key the
    // control plane sent and would report an empty tailnet for an answer whose
    // shape had changed, which is the opposite of what ADR-0004 asks for.
    match (params.limit, params.offset.unwrap_or(0)) {
        (None, 0) => Ok(answer),
        (limit, offset) => window(answer, limit, offset),
    }
}

/// Slice a listing, keeping its shape.
///
/// Refuses rather than guesses when the answer is not the shape the window
/// needs: a body with no `devices` array is one this build does not
/// understand, and windowing it would mean answering with an empty list.
fn window(answer: Value, limit: Option<usize>, offset: usize) -> ToolResult<Value> {
    let Some(all) = answer.get("devices").and_then(Value::as_array) else {
        return Err(ToolError::new(
            crate::error::ErrorCode::ApiError,
            "the control plane answered without a `devices` list, so there is nothing to window",
        )
        .with_hint("Call again without `limit` or `offset` to see the answer as it arrived."));
    };
    let total = all.len();
    let taken = all.iter().skip(offset);
    let devices: Vec<Value> = match limit {
        Some(limit) => taken.take(limit).cloned().collect(),
        None => taken.cloned().collect(),
    };
    report(DeviceListReport {
        window: Window {
            total,
            returned: devices.len(),
            offset,
            limit,
        },
        devices,
    })
}

/// `fields` is a documented string, so the value is checked against the list
/// beside the model rather than sent onward to be rejected.
fn checked_fields(fields: Option<&str>) -> ToolResult<Option<String>> {
    fields
        .map(|value| one_of("fields", value, DEVICE_FIELDS))
        .transpose()
}

// ---------------------------------------------------------------------------
// One device
// ---------------------------------------------------------------------------

/// A device, and nothing else.
///
/// Separate from [`DeviceGetParams`] rather than one type with an optional
/// `fields`, because a schema is a promise: a parameter a tool advertises and
/// never reads is a caller being told something works when it does nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceParams {
    /// The device's node id (`n1234567CNTRL`, the `nodeId` in a listing) or
    /// its numeric `id`. Either works.
    pub device_id: String,
}

/// A device, and which of its fields to read.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceGetParams {
    /// The device's node id (`n1234567CNTRL`, the `nodeId` in a listing) or
    /// its numeric `id`. Either works.
    pub device_id: String,
    /// Which fields to return: `default` for the common ones, `all` to add
    /// posture identity and client connectivity.
    #[serde(default)]
    pub fields: Option<String>,
}

async fn device_get(ctx: &ToolContext, params: DeviceGetParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let fields = checked_fields(params.fields.as_deref())?;
    Ok(client
        .get(device_path(&params.device_id, "")?)
        .maybe_query("fields", fields)
        .send_as::<Value>()
        .await?)
}

async fn device_delete(ctx: &ToolContext, params: DeviceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .delete(device_path(&params.device_id, "")?)
        .send()
        .await?;
    report(Done::new("deleted").about("device_id", params.device_id))
}

async fn device_expire(ctx: &ToolContext, params: DeviceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .post(device_path(&params.device_id, "/expire")?)
        .send()
        .await?;
    report(Done::new("key expired").about("device_id", params.device_id))
}

// ---------------------------------------------------------------------------
// Changing a device
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceAuthorizeParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// `true` to authorise, `false` to revoke an existing authorisation.
    pub authorized: bool,
}

async fn device_authorize(ctx: &ToolContext, params: DeviceAuthorizeParams) -> ToolResult<Value> {
    // The inventory classifies this endpoint WRITE and ticket 17 says
    // "de-authorisation are destructive". Both are right about different
    // calls, so the row carries the floor and the call decides (Q70) — the
    // same arrangement the passthrough uses, and what `varying: true` means.
    if !params.authorized && ctx.max_tier < crate::meta::Tier::Destructive {
        return Err(ToolError::not_permitted(
            "revoking a device's authorisation",
            "--allow-destructive",
        ));
    }
    let client = ctx.tailnet()?;
    client
        .post(device_path(&params.device_id, "/authorized")?)
        .json(&json!({"authorized": params.authorized}))
        .send()
        .await?;
    report(
        Done::new(if params.authorized {
            "authorized"
        } else {
            "authorization revoked"
        })
        .about("device_id", params.device_id),
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceRenameParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// The new name. The control plane derives the MagicDNS name from it.
    pub name: String,
}

async fn device_rename(ctx: &ToolContext, params: DeviceRenameParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .post(device_path(&params.device_id, "/name")?)
        .json(&json!({"name": params.name}))
        .send()
        .await?;
    report(Done::new("renamed").about("device_id", params.device_id))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceTagsParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// The complete set of tags, each `tag:` prefixed. An empty list removes
    /// every tag, which returns the device to its owner's identity.
    pub tags: Vec<String>,
}

async fn device_tags_set(ctx: &ToolContext, params: DeviceTagsParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .post(device_path(&params.device_id, "/tags")?)
        .json(&json!({"tags": params.tags}))
        .send()
        .await?;
    report(Done::new("tags set").about("device_id", params.device_id))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceKeyExpiryParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// `true` to stop the device's key expiring, `false` to let it expire on
    /// the tailnet's schedule again.
    pub key_expiry_disabled: bool,
}

async fn device_key_expiry_set(
    ctx: &ToolContext,
    params: DeviceKeyExpiryParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .post(device_path(&params.device_id, "/key")?)
        .json(&json!({"keyExpiryDisabled": params.key_expiry_disabled}))
        .send()
        .await?;
    report(
        Done::new(if params.key_expiry_disabled {
            "key expiry disabled"
        } else {
            "key expiry enabled"
        })
        .about("device_id", params.device_id),
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceIpParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// The new IPv4 address, from the tailnet's own range.
    pub ipv4: String,
}

async fn device_ip_set(ctx: &ToolContext, params: DeviceIpParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .post(device_path(&params.device_id, "/ip")?)
        .json(&json!({"ipv4": params.ipv4}))
        .send()
        .await?;
    report(Done::new("address set").about("device_id", params.device_id))
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

async fn device_routes_get(ctx: &ToolContext, params: DeviceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(device_path(&params.device_id, "/routes")?)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeviceRoutesParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// The complete set of enabled routes, as CIDR blocks. An empty list
    /// disables every route the device advertises.
    pub routes: Vec<String>,
}

async fn device_routes_set(ctx: &ToolContext, params: DeviceRoutesParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // This one answers with the routes it settled on, which is worth more to a
    // caller than a report that it worked.
    Ok(client
        .post(device_path(&params.device_id, "/routes")?)
        .json(&json!({"routes": params.routes}))
        .send_as::<Value>()
        .await?)
}

// ---------------------------------------------------------------------------
// Posture attributes
// ---------------------------------------------------------------------------

async fn device_attributes_get(ctx: &ToolContext, params: DeviceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(device_path(&params.device_id, "/attributes")?)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttributeParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// The attribute's name, which has to begin `custom:`.
    pub attribute_key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttributeSetParams {
    /// The device's node id or numeric id.
    pub device_id: String,
    /// The attribute's name, which has to begin `custom:`.
    pub attribute_key: String,
    /// The value: a string, a number or a boolean. The type is fixed by the
    /// first write to this key.
    pub value: Value,
    /// When the control plane should forget the attribute, as RFC 3339. Omit
    /// for one that does not expire.
    #[serde(default)]
    pub expiry: Option<String>,
    /// A note recorded with the change, which the audit log keeps.
    #[serde(default)]
    pub comment: Option<String>,
}

async fn device_attribute_set(ctx: &ToolContext, params: AttributeSetParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = attribute_path(&params.device_id, &params.attribute_key)?;
    let mut body = json!({"value": params.value});
    if let Some(expiry) = &params.expiry {
        body["expiry"] = json!(expiry);
    }
    if let Some(comment) = &params.comment {
        body["comment"] = json!(comment);
    }
    // The one write among these the description gives a body: a `200` holding
    // the device's attributes as they now stand. That is worth more to a
    // caller than a report that it worked, so it is forwarded — but the
    // control plane also answers this call with nothing at all, and `null` is
    // not an answer, so an empty body falls back to the report (Q67).
    let answer = client.post(path).json(&body).send().await?;
    answered_or(
        answer,
        Done::new("attribute set")
            .about("device_id", params.device_id)
            .about("attribute_key", params.attribute_key),
    )
}

async fn device_attribute_delete(ctx: &ToolContext, params: AttributeParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = attribute_path(&params.device_id, &params.attribute_key)?;
    client.delete(path).send().await?;
    report(
        Done::new("attribute deleted")
            .about("device_id", params.device_id)
            .about("attribute_key", params.attribute_key),
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttributesBatchParams {
    /// Device id to attribute map. Each attribute name has to begin `custom:`;
    /// each value is a string, number, boolean, `null` to delete it, or
    /// `{"value": ..., "expiry": "<RFC 3339>"}`.
    pub nodes: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Value>>,
    /// A note recorded with the change, which the audit log keeps.
    #[serde(default)]
    pub comment: Option<String>,
}

async fn device_attributes_update(
    ctx: &ToolContext,
    params: AttributesBatchParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    if params.nodes.is_empty() {
        return Err(ToolError::invalid_args(
            "`nodes` names no devices, so there is nothing to change",
        ));
    }
    // Checked before anything is sent, because the call is all-or-nothing: one
    // bad key among a hundred devices would fail the whole batch after it had
    // gone, and the refusal here names the device and the key. The checked
    // spelling is what is sent, so an id the caller padded is the id it meant.
    let mut nodes = std::collections::BTreeMap::new();
    let mut changed = 0usize;
    for (device, attributes) in &params.nodes {
        let device = device_id("nodes", device)?;
        for key in attributes.keys() {
            if !key.starts_with("custom:") {
                return Err(ToolError::invalid_args(format!(
                    "`{key}` on `{device}` does not begin with `custom:`; only custom attributes \
                     can be set"
                )));
            }
        }
        changed += attributes.len();
        nodes.insert(device, attributes);
    }
    let devices = nodes.len();
    let mut body = json!({"nodes": nodes});
    if let Some(comment) = &params.comment {
        body["comment"] = json!(comment);
    }
    client
        .patch(client.tailnet_path(None, "/device-attributes"))
        .json(&body)
        .send()
        .await?;
    report(
        Done::new("attributes updated")
            .about("devices", devices)
            .about("attributes", changed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_is_addressed_globally_rather_than_through_a_tailnet() {
        // The path a device endpoint takes has no tailnet in it: the id is
        // unique across tailnets and the control plane resolves the rest.
        assert_eq!(
            device_path("n1234567CNTRL", "/routes").expect("a valid id"),
            "/api/v2/device/n1234567CNTRL/routes"
        );
        assert_eq!(
            device_path("123456789", "").expect("the numeric form is valid too"),
            "/api/v2/device/123456789"
        );
    }

    #[test]
    fn an_identifier_that_would_rewrite_the_path_is_refused() {
        // The reason `path_segment` exists. Escaping these would send a call
        // that fails somewhere less legible; refusing says which argument.
        for bad in [
            "n123/../tailnet/-/devices",
            "n123?fields=all",
            "n123 456",
            "",
        ] {
            let error = device_path(bad, "").expect_err("{bad} should be refused");
            assert_eq!(
                serde_json::to_value(&error).expect("reportable")["code"],
                json!("invalid_args"),
                "{bad}"
            );
        }
    }

    #[test]
    fn an_attribute_this_server_may_not_set_is_refused_before_the_call() {
        let error = attribute_path("n1", "node:os").expect_err("not a custom attribute");
        let reported = serde_json::to_value(&error).expect("reportable");
        assert_eq!(reported["code"], json!("invalid_args"));
        assert!(
            reported["message"]
                .as_str()
                .is_some_and(|m| m.contains("custom:")),
            "{reported:#?}"
        );
        assert_eq!(
            attribute_path("n1", "custom:diskEncrypted").expect("a custom one"),
            "/api/v2/device/n1/attributes/custom:diskEncrypted"
        );
    }

    #[test]
    fn the_field_selection_is_checked_against_the_list_beside_the_model() {
        assert_eq!(checked_fields(None).expect("absent is fine"), None);
        assert_eq!(
            checked_fields(Some("all"))
                .expect("a known value")
                .as_deref(),
            Some("all")
        );
        let error = checked_fields(Some("everything")).expect_err("not a known value");
        assert!(
            serde_json::to_value(&error).expect("reportable")["message"]
                .as_str()
                .is_some_and(|m| m.contains("all") && m.contains("default")),
            "the refusal should quote the values: {error:?}"
        );
    }
}
