//! Tailscale services: a name and address that several devices stand behind.
//!
//! **The path has two spellings.** The vendored description documents
//! `/tailnet/{tailnet}/services`; Tailscale's own Go client calls
//! `/tailnet/{tailnet}/vip-services`. Both may be live and neither source
//! settles it, which the drift test has recorded since ticket 16. So these
//! tools send the documented spelling and, if the control plane answers that
//! it is not there, send the same call again at the other one (Q81). The
//! criterion is that service naming follow the path the live API serves rather
//! than only the published description, and asking is the only way to know.
//!
//! **A service name is `svc:<name>`,** unique across the tailnet — it cannot
//! collide with a machine name. The colon is in the identifier, not a
//! separator, and goes into the path as written.
//!
//! **The upsert replaces.** `PUT` creates a service that is not there and
//! overwrites one that is, discarding anything the body leaves out, which is
//! why it is `_replace` rather than `_update` (Q72).

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tailscale_rest::ApiError;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, answered_or, path_segment};

crate::tools! {
    /// List the tailnet's services. Answers with `{"vipServices": [...]}`.
    tailnet_service_list => NoParams, service_list,
        toolset: TailnetServices, tier: Read, idempotent: true;

    /// Read one service by its name, which is `svc:` followed by the name.
    tailnet_service_get => ServiceParams, service_get,
        toolset: TailnetServices, tier: Read, idempotent: true;

    /// Create a service, or replace one that already exists.
    ///
    /// One endpoint for both: a service that is not there is created, and one
    /// that is gets this document in place of its own — anything the document
    /// leaves out is discarded. Read `tailnet_service_get` first and send back
    /// what it answered with the one change made.
    ///
    /// On a create, the `name` in the document has to match `service_name`; on
    /// a replace, a different `name` renames the service.
    tailnet_service_replace => ServiceReplaceParams, service_replace,
        toolset: TailnetServices, tier: Write, idempotent: true;

    /// Delete a service. Anything addressing it by name stops resolving.
    tailnet_service_delete => ServiceParams, service_delete,
        toolset: TailnetServices, tier: Destructive, idempotent: true;

    /// List the devices standing behind a service, and whether each is
    /// approved to host it.
    tailnet_service_hosts_list => ServiceParams, service_hosts_list,
        toolset: TailnetServices, tier: Read, idempotent: true;

    /// Read whether one device may host a service, and whether an
    /// auto-approver decided it rather than a person.
    tailnet_service_approval_get => ServiceDeviceParams, service_approval_get,
        toolset: TailnetServices, tier: Read, idempotent: true;

    /// Approve one device to host a service, or withdraw that approval.
    ///
    /// Approving is a write. Withdrawing takes the device out of the service —
    /// traffic stops being sent to it — and needs the destructive tier.
    tailnet_service_approval_set => ServiceApprovalParams, service_approval_set,
        toolset: TailnetServices, tier: Write, idempotent: true, varying: true;
}

/// The spelling the vendored description documents.
const DOCUMENTED: &str = "/services";

/// The spelling Tailscale's own Go client uses for the same endpoints.
const GO_CLIENT: &str = "/vip-services";

/// Send a services call at the documented spelling, and at the other one if
/// the control plane says the first is not there.
///
/// The two sources disagree and neither can be checked from here, so this asks
/// (Q81). A 404 is safe to retry across all seven of these: none of them acts
/// before answering one, so a request that reached the wrong base path did
/// nothing. Where the service itself is genuinely missing, the second call
/// answers 404 as well and that is what the caller gets.
async fn either_spelling<F, Fut>(send: F) -> ToolResult<Value>
where
    F: Fn(&'static str) -> Fut,
    Fut: std::future::Future<Output = Result<Value, ApiError>>,
{
    match send(DOCUMENTED).await {
        Err(error) if error.status() == Some(404) => Ok(send(GO_CLIENT).await?),
        other => Ok(other?),
    }
}

/// The service name, checked once before either spelling is tried.
///
/// It carries a colon — `svc:example` — which `path_segment` allows because it
/// is part of the identifier rather than a separator. Checked out here rather
/// than inside the retry so the closure cannot fail: a name that is not a path
/// segment is a refusal, not something to ask the control plane about twice.
fn checked_name(service: &str) -> ToolResult<String> {
    path_segment("service_name", service)
}

/// `/api/v2/tailnet/<tailnet><base>/<service><rest>`, from a checked name.
fn service_path(client: &tailscale_rest::Client, base: &str, name: &str, rest: &str) -> String {
    client.tailnet_path(None, &format!("{base}/{name}{rest}"))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

async fn service_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    either_spelling(|base| client.get(client.tailnet_path(None, base)).send_as::<Value>()).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServiceParams {
    /// The service's name, as `svc:example`.
    pub service_name: String,
}

async fn service_get(ctx: &ToolContext, params: ServiceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let name = &checked_name(&params.service_name)?;
    either_spelling(|base| client.get(service_path(client, base, name, "")).send_as::<Value>()).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServiceReplaceParams {
    /// The service's name, as `svc:example`.
    pub service_name: String,
    /// The whole service document, in Tailscale's own shape: `name`,
    /// `displayName`, `addrs`, `comment`, `ports` — as `"tcp:80"`, or the
    /// single entry `"do-not-validate"` — and `tags`. Anything left out is
    /// discarded.
    pub service: Value,
}

async fn service_replace(ctx: &ToolContext, params: ServiceReplaceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    if !params.service.is_object() {
        return Err(ToolError::invalid_args(
            "`service` is the service document, an object with `name`, `displayName`, `addrs`, \
             `comment`, `ports` and `tags`",
        )
        .with_hint("Call `tailnet_service_get` and send back what it answered."));
    }
    let name = &checked_name(&params.service_name)?;
    let body = &params.service;
    either_spelling(|base| {
        client
            .put(service_path(client, base, name, ""))
            .json(body)
            .send_as::<Value>()
    })
    .await
}

async fn service_delete(ctx: &ToolContext, params: ServiceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let name = &checked_name(&params.service_name)?;
    let answer = either_spelling(|base| {
        client
            .delete(service_path(client, base, name, ""))
            .send_as::<Value>()
    })
    .await?;
    answered_or(
        answer,
        Done::new("service deleted").about("service_name", params.service_name.clone()),
    )
}

async fn service_hosts_list(ctx: &ToolContext, params: ServiceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let name = &checked_name(&params.service_name)?;
    either_spelling(|base| {
        client
            .get(service_path(client, base, name, "/devices"))
            .send_as::<Value>()
    })
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServiceDeviceParams {
    /// The service's name, as `svc:example`.
    pub service_name: String,
    /// The device's node id or numeric id.
    pub device_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServiceApprovalParams {
    /// The service's name, as `svc:example`.
    pub service_name: String,
    /// The device's node id or numeric id.
    pub device_id: String,
    /// Whether the device may host the service. `false` takes it out of the
    /// service.
    pub approved: bool,
}

/// `…/<service>/device/<device>/approved`, which both approval tools use.
fn approval_suffix(device: &str) -> ToolResult<String> {
    let device = path_segment("device_id", device)?;
    Ok(format!("/device/{device}/approved"))
}

async fn service_approval_get(ctx: &ToolContext, params: ServiceDeviceParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let name = &checked_name(&params.service_name)?;
    let suffix = approval_suffix(&params.device_id)?;
    either_spelling(|base| {
        client
            .get(service_path(client, base, name, &suffix))
            .send_as::<Value>()
    })
    .await
}

async fn service_approval_set(ctx: &ToolContext, params: ServiceApprovalParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // The danger is in the argument, not the tool: approving adds a host,
    // withdrawing removes a working one. The row carries the floor and the
    // call decides, as `tailnet_device_authorize` does (Q70).
    if !params.approved && ctx.max_tier < crate::meta::Tier::Destructive {
        return Err(ToolError::not_permitted(
            "withdrawing a device's approval to host a service",
            "--allow-destructive",
        ));
    }
    let name = &checked_name(&params.service_name)?;
    let suffix = approval_suffix(&params.device_id)?;
    let body = tailscale_rest::models::service::ServiceApprovalRequest {
        approved: Some(params.approved),
        unknown: Default::default(),
    };
    either_spelling(|base| {
        client
            .post(service_path(client, base, name, &suffix))
            .json(&body)
            .send_as::<Value>()
    })
    .await
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
    fn a_service_name_keeps_its_colon_and_a_dot_segment_is_still_refused() {
        // `svc:` is part of the identifier, so it goes into the path as
        // written. `..` is not an identifier at all.
        let client = client();
        let name = &checked_name("svc:example").expect("a name");
        assert_eq!(
            service_path(&client, DOCUMENTED, name, "/devices"),
            "/api/v2/tailnet/-/services/svc:example/devices"
        );
        assert_eq!(
            service_path(&client, GO_CLIENT, name, ""),
            "/api/v2/tailnet/-/vip-services/svc:example"
        );
        assert!(checked_name("..").is_err());
        assert!(approval_suffix("../..").is_err());
    }
}
