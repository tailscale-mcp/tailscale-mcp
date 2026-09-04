//! Auth keys, API access tokens, OAuth clients and federated identities.
//!
//! The control plane calls all four a "key" and tells them apart by
//! [`Key::key_type`], so one model covers them and most fields apply to only
//! some of the four. Which is which is in [`KEY_TYPES`].

use std::collections::BTreeMap;

use crate::Secret;
use crate::model;
use crate::models::KnownValues;

/// The kinds of key the description knows.
///
/// `auth` registers machines, `client` is an OAuth client, `federated` is a
/// workload identity, and `api` is an access token — a user's own, or one
/// minted by either of the other two.
pub const KEY_TYPES: &[&str] = &["auth", "client", "api", "federated"];

/// What [`KEY_TYPES`] narrows to on the way in.
///
/// The description gives three different lists for one field: a response may
/// say `api`, a create may not ask for one, and an update may not change a key
/// into an `auth` key. A tool that quoted the response list on a create
/// parameter would be offering values the control plane rejects.
pub const CREATE_KEY_TYPES: &[&str] = &["auth", "client", "federated"];

/// What an update accepts, which is narrower still.
pub const UPDATE_KEY_TYPES: &[&str] = &["client", "federated"];

pub const KNOWN_VALUES: &[KnownValues] = &[("Key.keyType", KEY_TYPES),
    ("POST /tailnet/{tailnet}/keys body.keyType", CREATE_KEY_TYPES),
    ("PUT /tailnet/{tailnet}/keys/{keyId} body.keyType", UPDATE_KEY_TYPES),
];

model! {
    /// An auth key, an API access token, an OAuth client or a federated
    /// identity.
    Key {
        id: "id" => String,
        /// The secret itself, sent only in the answer that creates it. There
        /// is no second chance to read it.
        key: "key" => Secret,
        /// One of [`KEY_TYPES`].
        key_type: "keyType" => String,
        /// Auth keys only.
        expiry_seconds: "expirySeconds" => i64,
        created: "created" => String,
        updated: "updated" => String,
        expires: "expires" => String,
        revoked: "revoked" => String,
        capabilities: "capabilities" => KeyCapabilities,
        /// OAuth clients and federated identities: what the tokens they mint
        /// are allowed to do.
        scopes: "scopes" => Vec<String>,
        tags: "tags" => Vec<String>,
        description: "description" => String,
        invalid: "invalid" => bool,
        user_id: "userId" => String,
        /// Federated identities: the audience the JWT must claim.
        audience: "audience" => String,
        /// Federated identities: the issuer whose JWTs are accepted.
        issuer: "issuer" => String,
        /// Federated identities: the subject the JWT must claim.
        subject: "subject" => String,
        /// Federated identities: claims mapped to values, for narrowing which
        /// JWTs from the issuer are accepted.
        custom_claim_rules: "customClaimRules" => BTreeMap<String, String>,
    }

    /// What a key may do, by resource.
    KeyCapabilities {
        /// Populated for auth keys only.
        devices: "devices" => DeviceCapabilities,
    }

    /// A key's permissions over devices.
    DeviceCapabilities as "KeyCapabilities.devices" {
        create: "create" => CreateCapability,
    }

    /// What registering a device with this key produces.
    CreateCapability as "KeyCapabilities.devices.create" {
        /// A reusable key registers more than one device.
        reusable: "reusable" => bool,
        /// An ephemeral device is cleaned up when it goes away.
        ephemeral: "ephemeral" => bool,
        /// Devices registered with this key skip admin approval.
        preauthorized: "preauthorized" => bool,
        /// The tags every device registered with this key is given.
        tags: "tags" => Vec<String>,
    }
}
