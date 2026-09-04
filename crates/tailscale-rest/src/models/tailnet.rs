//! The tailnet itself: its settings, its OAuth apps, and — for an
//! organization that has several — the tailnets in it.
//!
//! [`Error`] lives here too, for want of anywhere better. It is the shape
//! every failing call answers with, and `ApiError::describe` is what reads it
//! in practice; the model is here so the drift test covers it like any other
//! schema.

use crate::Secret;
use crate::model;
use crate::models::KnownValues;

/// Which roles may accept an invitation to another tailnet.
///
/// `none` is one of the values rather than the field being absent, so leaving
/// the setting off is itself a setting.
pub const ROLES_ALLOWED_TO_JOIN: &[&str] = &["none", "admin", "member"];

pub const KNOWN_VALUES: &[KnownValues] = &[(
    "TailnetSettings.usersRoleAllowedToJoinExternalTailnets",
    ROLES_ALLOWED_TO_JOIN,
)];

model! {
    /// What a failing call says went wrong.
    Error {
        message: "message" => String,
    }

    /// The tailnet-wide switches.
    ///
    /// Most are nullable in the description, where `null` means the tailnet's
    /// plan does not carry the feature — which is not the same answer as
    /// `false`, and is why they stay `Option` rather than defaulting.
    TailnetSettings {
        /// Stops the policy file being edited in the admin console, so that a
        /// GitOps or Terraform workflow is the only writer.
        acls_externally_managed_on: "aclsExternallyManagedOn" => bool,
        /// Where the admin console points a reader when the above is on.
        acls_external_link: "aclsExternalLink" => String,
        devices_approval_on: "devicesApprovalOn" => bool,
        devices_auto_updates_on: "devicesAutoUpdatesOn" => bool,
        /// How long a device's key lasts before it must reauthenticate.
        devices_key_duration_days: "devicesKeyDurationDays" => i64,
        users_approval_on: "usersApprovalOn" => bool,
        /// One of [`ROLES_ALLOWED_TO_JOIN`].
        users_role_allowed_to_join_external_tailnets:
            "usersRoleAllowedToJoinExternalTailnets" => String,
        network_flow_logging_on: "networkFlowLoggingOn" => bool,
        regional_routing_on: "regionalRoutingOn" => bool,
        /// Whether posture integrations may collect device identity.
        posture_identity_collection_on: "postureIdentityCollectionOn" => bool,
        /// Whether devices can be issued HTTPS certificates.
        https_enabled: "httpsEnabled" => bool,
    }

    /// An OAuth app, which is a third party a user can grant access to.
    ///
    /// Not to be confused with an OAuth client, which is a credential this
    /// server can hold; see [`crate::credentials::Credentials`].
    OAuthApp {
        id: "id" => String,
        /// 3 to 50 characters of `[A-Za-z0-9._-]`.
        name: "name" => String,
        /// At most 300 characters.
        description: "description" => String,
        /// Where the authorization code flow may return to. At least one is
        /// required and each must be `https`.
        redirect_uris: "redirectURIs" => Vec<String>,
        /// Must be non-empty.
        scopes: "scopes" => Vec<String>,
        /// The device attributes this app may set.
        allowed_node_attributes: "allowedNodeAttributes" => Vec<String>,
        /// Sent when the app is created and never again.
        client_secret: "clientSecret" => Secret,
        created: "created" => String,
        updated: "updated" => String,
    }

    /// Every OAuth app the tailnet has.
    OAuthAppList as "GET /tailnet/{tailnet}/oauth-apps 200" {
        oauth_apps: "oauthApps" => Vec<OAuthApp>,
    }

    /// What creating an OAuth app sends.
    ///
    /// The same five fields an update sends, and the description declares them
    /// through the same shared schemas, so one struct covers both.
    CreateOAuthAppRequest as "POST /tailnet/{tailnet}/oauth-apps body" {
        /// 3 to 50 characters of `[A-Za-z0-9._-]`. Required.
        name: "name" => String,
        /// At most 300 characters.
        description: "description" => String,
        /// Required, at least one, each `https` — or `http` on localhost.
        redirect_uris: "redirectURIs" => Vec<String>,
        /// Required and non-empty, as `auth_keys:create` and the like.
        scopes: "scopes" => Vec<String>,
        /// Device attributes this app may set, each beginning `custom:`.
        allowed_node_attributes: "allowedNodeAttributes" => Vec<String>,
    }

    /// What reconfiguring one sends, which is the same body. The secret is
    /// neither regenerated nor returned.
    UpdateOAuthAppRequest as "PUT /tailnet/{tailnet}/oauth-apps/{appId} body"
        is CreateOAuthAppRequest;

    /// One tailnet belonging to an organization.
    OrganizationTailnet {
        id: "id" => String,
        display_name: "displayName" => String,
        org_id: "orgId" => String,
        created_at: "createdAt" => String,
    }

    /// A page of an organization's tailnets.
    ListOrganizationTailnetsResponse {
        tailnets: "tailnets" => Vec<OrganizationTailnet>,
        /// Opaque, and the way to ask for the next page.
        cursor: "cursor" => String,
        /// Across every page, not this one.
        total_count: "totalCount" => i64,
    }

    /// What creating a tailnet asks for.
    CreateOrganizationTailnetRequest {
        display_name: "displayName" => String,
    }

    /// An OAuth client scoped to a newly created tailnet, so that the caller
    /// has a credential for it without a second round trip.
    TailnetOAuthClient {
        id: "id" => String,
        /// Sent once, in the answer that created the tailnet.
        secret: "secret" => Secret,
    }

    /// A newly created tailnet, or the one that already had the name.
    CreateOrganizationTailnetResponse {
        id: "id" => String,
        display_name: "displayName" => String,
        org_id: "orgId" => String,
        /// The suffix this tailnet's MagicDNS names are built on.
        dns_name: "dnsName" => String,
        created_at: "createdAt" => String,
        oauth_client: "oauthClient" => TailnetOAuthClient,
        /// `true` where the call matched an existing tailnet rather than
        /// making one, which is what makes creation safe to repeat.
        already_exists: "alreadyExists" => bool,
    }
}
