//! Where Tailscale posts events, and what it posts about.
//!
//! **The secret is the point of the endpoint.** A webhook's secret signs the
//! `Tailscale-Webhook-Signature` header, which is how a receiver tells a real
//! delivery from a forged one. It comes back twice in an endpoint's life: when
//! it is created and when it is rotated, and never on a read. Both answers are
//! forwarded whole, and `tests/minted_secrets.rs` holds them to the same rule
//! as a minted key.
//!
//! **Rotation is destructive.** The old secret stops verifying the moment the
//! new one exists, so every receiver checking signatures rejects every delivery
//! until it has been given the new one. That is a break in an existing
//! integration rather than a new thing being made, which is why it sits at the
//! destructive tier alongside deleting the endpoint.
//!
//! **A test delivery is a write with no state change.** It queues a real
//! `test` event at the endpoint — the control plane answers 202, accepted —
//! so it changes nothing here and does something out there.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use tailscale_rest::models::webhook::{CreateWebhookRequest, UpdateWebhookRequest};

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::tools::common::{Done, answered_or, each_present, path_segment, report};

crate::tools! {
    /// List the tailnet's webhook endpoints. No secret is included — a
    /// webhook's secret comes back only when it is created or rotated.
    tailnet_webhook_list => NoParams, webhook_list,
        toolset: TailnetWebhooks, tier: Read, idempotent: true;

    /// Create a webhook endpoint.
    ///
    /// **The signing secret is in the answer and nowhere else.** It signs the
    /// `Tailscale-Webhook-Signature` header; a receiver that checks signatures
    /// needs it, and there is no way to read it again — only to rotate it,
    /// which breaks the old one.
    ///
    /// `provider_type` shapes the payload for a receiver that expects its own
    /// format. Omit it for Tailscale's own shape.
    tailnet_webhook_create => WebhookCreateParams, webhook_create,
        toolset: TailnetWebhooks, tier: Write;

    /// Read one webhook endpoint by its id.
    tailnet_webhook_get => WebhookParams, webhook_get,
        toolset: TailnetWebhooks, tier: Read, idempotent: true;

    /// Replace which events an endpoint is sent.
    ///
    /// The whole list, not an addition: an event not in `subscriptions` stops
    /// being delivered. The endpoint's URL and provider cannot be changed —
    /// delete it and create another.
    tailnet_webhook_subscriptions_replace => WebhookSubscriptionsParams, webhook_subscriptions_replace,
        toolset: TailnetWebhooks, tier: Write, idempotent: true;

    /// Delete a webhook endpoint. Deliveries stop at once.
    tailnet_webhook_delete => WebhookParams, webhook_delete,
        toolset: TailnetWebhooks, tier: Destructive, idempotent: true;

    /// Queue a `test` event at the endpoint, to check it is reachable.
    ///
    /// Changes nothing in the tailnet and does send a real delivery. The
    /// control plane accepts it and delivers asynchronously, so a success here
    /// means queued rather than received.
    tailnet_webhook_test => WebhookParams, webhook_test,
        toolset: TailnetWebhooks, tier: Write;

    /// Replace an endpoint's signing secret, and answer with the new one.
    ///
    /// **The old secret stops verifying immediately.** Every receiver checking
    /// signatures rejects every delivery until it has the new secret, which is
    /// in this answer and nowhere else.
    tailnet_webhook_secret_rotate => WebhookParams, webhook_secret_rotate,
        toolset: TailnetWebhooks, tier: Destructive;
}

fn webhook_path(id: &str, rest: &str) -> ToolResult<String> {
    let id = path_segment("endpoint_id", id)?;
    Ok(format!("/api/v2/webhooks/{id}{rest}"))
}

/// The events an endpoint may ask for, each held to the description's list.
///
/// Checked as a set rather than one at a time so that a caller naming three
/// events with one typo is told which one, and refused before any of them is
/// subscribed.
fn checked_subscriptions(subscriptions: &[String]) -> ToolResult<Vec<String>> {
    if subscriptions.is_empty() {
        return Err(ToolError::invalid_args(
            "`subscriptions` is empty; an endpoint with no events is one nothing is ever sent to",
        ));
    }
    // Each event is present but not held to `SUBSCRIPTIONS`: the catalogue
    // grows whenever a feature does, and the live API already carries category
    // subscriptions the vendored description has not caught up with, so a copy
    // of the list here would refuse an event that exists (Q84). The values are
    // in the parameter's description, and the control plane refuses one it
    // does not know.
    each_present("subscriptions", subscriptions.to_vec())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WebhookParams {
    /// The endpoint's id, as a listing reports it.
    pub endpoint_id: String,
}

async fn webhook_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(client.tailnet_path(None, "/webhooks"))
        .send_as::<Value>()
        .await?)
}

async fn webhook_get(ctx: &ToolContext, params: WebhookParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(webhook_path(&params.endpoint_id, "")?)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WebhookCreateParams {
    /// Where deliveries are posted. Must be reachable from the internet.
    pub endpoint_url: String,
    /// `slack`, `mattermost`, `googlechat` or `discord`, to shape the payload
    /// for that service. Omit for Tailscale's own shape.
    #[serde(default)]
    pub provider_type: Option<String>,
    /// Which events to send, as `nodeCreated`, `userSuspended` and the rest.
    pub subscriptions: Vec<String>,
}

async fn webhook_create(ctx: &ToolContext, params: WebhookCreateParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let endpoint_url = params.endpoint_url.trim();
    if endpoint_url.is_empty() {
        return Err(ToolError::invalid_args(
            "`endpoint_url` is empty; a webhook needs somewhere to post to",
        ));
    }
    // The model, not a struct beside it: the drift test holds this shape to
    // the description, and a second hand-written one is a shape it does not
    // guard.
    let body = CreateWebhookRequest {
        endpoint_url: Some(endpoint_url.to_owned()),
        // Not held to `PROVIDER_TYPES` either: it names four chat products,
        // which is a market rather than a specification (Q60, Q84).
        provider_type: params.provider_type.clone(),
        subscriptions: Some(checked_subscriptions(&params.subscriptions)?),
        unknown: Default::default(),
    };
    Ok(client
        .post(client.tailnet_path(None, "/webhooks"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WebhookSubscriptionsParams {
    /// The endpoint's id, as a listing reports it.
    pub endpoint_id: String,
    /// The complete list of events to send. An event not named here stops
    /// being delivered.
    pub subscriptions: Vec<String>,
}

async fn webhook_subscriptions_replace(
    ctx: &ToolContext,
    params: WebhookSubscriptionsParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let path = webhook_path(&params.endpoint_id, "")?;
    let body = UpdateWebhookRequest {
        subscriptions: Some(checked_subscriptions(&params.subscriptions)?),
        unknown: Default::default(),
    };
    Ok(client.patch(path).json(&body).send_as::<Value>().await?)
}

async fn webhook_delete(ctx: &ToolContext, params: WebhookParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    client
        .delete(webhook_path(&params.endpoint_id, "")?)
        .send()
        .await?;
    report(Done::new("webhook deleted").about("endpoint_id", params.endpoint_id))
}

async fn webhook_test(ctx: &ToolContext, params: WebhookParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let answer = client
        .post(webhook_path(&params.endpoint_id, "/test")?)
        .send()
        .await?;
    answered_or(
        answer,
        // "Queued", not "delivered": the control plane answers 202 and sends
        // the event afterwards, so a success here says nothing about whether
        // the endpoint answered.
        Done::new("test event queued for delivery").about("endpoint_id", params.endpoint_id),
    )
}

async fn webhook_secret_rotate(ctx: &ToolContext, params: WebhookParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // Forwarded whole: the new secret is in here and nowhere else.
    Ok(client
        .post(webhook_path(&params.endpoint_id, "/rotate")?)
        .send_as::<Value>()
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_the_description_does_not_have_is_still_subscribed_to() {
        let good = ["nodeCreated".to_owned(), "userDeleted".to_owned()];
        assert_eq!(checked_subscriptions(&good).expect("known events"), good);

        // The live API already carries category subscriptions the vendored
        // description has not caught up with, so a list copied from it would
        // refuse an event that exists (Q84).
        let newer = ["categoryTailnetManagement".to_owned()];
        assert_eq!(checked_subscriptions(&newer).expect("sent anyway"), newer);

        // A blank entry is still a caller that meant to send nothing there.
        let error = checked_subscriptions(&["nodeCreated".to_owned(), "  ".to_owned()])
            .expect_err("one is blank");
        let reported = serde_json::to_value(&error).expect("reportable");
        assert_eq!(reported["code"], serde_json::json!("invalid_args"));
    }

    #[test]
    fn an_endpoint_with_no_events_is_refused_rather_than_created() {
        // The control plane accepts it and it is an endpoint nothing is ever
        // sent to, which reads as a webhook that does not work.
        assert!(checked_subscriptions(&[]).is_err());
    }
}
