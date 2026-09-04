//! Webhook endpoints, and the events they subscribe to.

use crate::Secret;
use crate::model;
use crate::models::KnownValues;

/// The destinations whose message format the description knows.
///
/// An endpoint with none of these gets Tailscale's own JSON shape, which the
/// API spells as the empty string rather than by omitting the field.
pub const PROVIDER_TYPES: &[&str] = &["slack", "mattermost", "googlechat", "discord"];

/// The events a webhook can subscribe to.
pub const SUBSCRIPTIONS: &[&str] = &[
    "nodeCreated",
    "nodeNeedsApproval",
    "nodeApproved",
    "nodeKeyExpiringInOneDay",
    "nodeKeyExpired",
    "nodeDeleted",
    "nodeSigned",
    "nodeNeedsSignature",
    "policyUpdate",
    "userCreated",
    "userNeedsApproval",
    "userSuspended",
    "userRestored",
    "userDeleted",
    "userApproved",
    "userRoleUpdated",
    "subnetIPForwardingNotEnabled",
    "exitNodeIPForwardingNotEnabled",
];

/// Four rows for two lists: the description declares `providerType` and
/// `subscriptions` both inline on `Webhook` and again as schemas of their own,
/// and the drift test wants every path it walks accounted for.
pub const KNOWN_VALUES: &[KnownValues] = &[
    ("Webhook.providerType", PROVIDER_TYPES),
    ("providerType", PROVIDER_TYPES),
    ("Webhook.subscriptions[]", SUBSCRIPTIONS),
    ("subscriptions[]", SUBSCRIPTIONS),
];

model! {
    /// An endpoint Tailscale posts events to.
    Webhook {
        endpoint_id: "endpointId" => String,
        /// Where the `POST` goes.
        endpoint_url: "endpointUrl" => String,
        /// One of [`PROVIDER_TYPES`], or the empty string for Tailscale's own
        /// message shape.
        provider_type: "providerType" => String,
        /// Blank where the endpoint was created by an OAuth client rather than
        /// by a person.
        creator_login_name: "creatorLoginName" => String,
        created: "created" => String,
        last_modified: "lastModified" => String,
        /// Any of [`SUBSCRIPTIONS`].
        subscriptions: "subscriptions" => Vec<String>,
        /// Signs the `Tailscale-Webhook-Signature` header, so a receiver can
        /// tell a real delivery from a forged one. Sent only when the endpoint
        /// is created and when the secret is rotated.
        secret: "secret" => Secret,
    }
}
