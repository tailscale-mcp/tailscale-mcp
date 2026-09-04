//! Devices, the routes they advertise, their posture, and shares of them.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::Secret;
use crate::model;
use crate::models::KnownValues;

/// The posture providers the description knows.
pub const POSTURE_PROVIDERS: &[&str] = &[
    "falcon",
    "intune",
    "jamfpro",
    "kandji",
    "kolide",
    "sentinelone",
];

/// What a device listing may ask for.
///
/// `default` is the subset the endpoint sends when the parameter is absent;
/// `all` adds the fields that cost the control plane something to gather.
pub const DEVICE_FIELDS: &[&str] = &["all", "default"];

pub const KNOWN_VALUES: &[KnownValues] = &[("PostureIntegration.provider", POSTURE_PROVIDERS),
    ("?fields", DEVICE_FIELDS),
];

model! {
    /// A machine in a tailnet.
    ///
    /// `nodeId` is the identifier to use; `id` is the older numeric one, still
    /// accepted wherever a device is named and still sent back here.
    Device {
        /// Both families: `100.x.y.z` and `fd7a:115c:…`.
        addresses: "addresses" => Vec<String>,
        id: "id" => String,
        node_id: "nodeId" => String,
        /// Who registered it. For an untagged device this is its owner; a
        /// tagged device is owned by its tags instead.
        user: "user" => String,
        /// The MagicDNS name.
        name: "name" => String,
        /// The machine name shown in the admin console.
        hostname: "hostname" => String,
        /// Empty for a device shared in from another tailnet.
        client_version: "clientVersion" => String,
        update_available: "updateAvailable" => bool,
        os: "os" => String,
        created: "created" => String,
        connected_to_control: "connectedToControl" => bool,
        /// Omitted for a device that has never been online, and for one that
        /// is online now.
        last_seen: "lastSeen" => String,
        key_expiry_disabled: "keyExpiryDisabled" => bool,
        expires: "expires" => String,
        authorized: "authorized" => bool,
        /// `true` for a device shared into this tailnet rather than a member
        /// of it.
        is_external: "isExternal" => bool,
        /// Several devices using one node key, which usually means node state
        /// was copied between machines.
        multiple_connections: "multipleConnections" => bool,
        /// A public key, not a secret, and of no use to any call here.
        machine_key: "machineKey" => String,
        /// A public key, and the one a locked tailnet signs to admit a node.
        node_key: "nodeKey" => String,
        blocks_incoming_connections: "blocksIncomingConnections" => bool,
        /// The advertised routes an admin has approved.
        enabled_routes: "enabledRoutes" => Vec<String>,
        /// The routes this device asks to expose, approved or not.
        advertised_routes: "advertisedRoutes" => Vec<String>,
        client_connectivity: "clientConnectivity" => ClientConnectivity,
        tags: "tags" => Vec<String>,
        /// Only populated where tailnet lock is on.
        tailnet_lock_error: "tailnetLockError" => String,
        /// Present whether or not tailnet lock is on: every node generates one.
        tailnet_lock_key: "tailnetLockKey" => String,
        ssh_enabled: "sshEnabled" => bool,
        posture_identity: "postureIdentity" => PostureIdentity,
        is_ephemeral: "isEphemeral" => bool,
        distro: "distro" => Distro,
    }

    /// What the device reports about the network it is on.
    ClientConnectivity as "Device.clientConnectivity" {
        /// The magicsock UDP `ip:port` endpoints, of either family.
        endpoints: "endpoints" => Vec<String>,
        /// `true` where the host's NAT mappings depend on the destination.
        mapping_varies_by_dest_ip: "mappingVariesByDestIP" => bool,
        /// Keyed by DERP server location.
        latency: "latency" => BTreeMap<String, DerpLatency>,
        client_supports: "clientSupports" => ClientSupports,
    }

    /// One DERP server's distance from the device.
    DerpLatency as "Device.clientConnectivity.latency{}" {
        /// `true` for the server this node prefers for incoming traffic.
        preferred: "preferred" => bool,
        latency_ms: "latencyMs" => f64,
    }

    /// NAT and address-family features the client reports.
    ///
    /// Every field is nullable in the description and `hairPinning` is
    /// documented as always null now, so absent and false are different
    /// answers here and the `Option` is carrying real information.
    ClientSupports as "Device.clientConnectivity.clientSupports" {
        /// No longer tracked; always null.
        hair_pinning: "hairPinning" => bool,
        /// Whether the OS supports IPv6, not whether IPv6 works here.
        ipv6: "ipv6" => bool,
        pcp: "pcp" => bool,
        pmp: "pmp" => bool,
        udp: "udp" => bool,
        upnp: "upnp" => bool,
    }

    /// Hardware identifiers, where the tailnet collects them.
    ///
    /// A device that has not opted in reports `{"disabled": true}` rather than
    /// nothing, which is why `disabled` is worth reading.
    PostureIdentity as "Device.postureIdentity" {
        serial_numbers: "serialNumbers" => Vec<String>,
        disabled: "disabled" => bool,
    }

    /// The operating system distribution, where the client can tell.
    Distro as "Device.distro" {
        name: "name" => String,
        version: "version" => String,
        code_name: "codeName" => String,
    }

    /// The routes a device advertises, and which of them are approved.
    DeviceRoutes {
        advertised_routes: "advertisedRoutes" => Vec<String>,
        enabled_routes: "enabledRoutes" => Vec<String>,
    }

    /// Posture attributes collected from a device.
    DevicePostureAttributes {
        /// Values are strings, numbers or booleans, so the map is untyped.
        attributes: "attributes" => BTreeMap<String, Value>,
        /// When each attribute stops counting, for those that expire.
        expiries: "expiries" => BTreeMap<String, String>,
    }

    /// An invitation sharing one device with a user outside its tailnet.
    DeviceInvite {
        id: "id" => String,
        created: "created" => String,
        tailnet_id: "tailnetId" => i64,
        device_id: "deviceId" => i64,
        sharer_id: "sharerId" => i64,
        multi_use: "multiUse" => bool,
        /// Whether the invited user may use the device as an exit node, where
        /// it advertises as one.
        allow_exit_node: "allowExitNode" => bool,
        /// Empty for an invite nobody was mailed, whose URL is shared by hand.
        email: "email" => String,
        last_email_sent_at: "lastEmailSentAt" => String,
        /// Anyone holding this link can accept, not only the addressee.
        invite_url: "inviteUrl" => String,
        accepted: "accepted" => bool,
        accepted_by: "acceptedBy" => InviteAcceptor,
    }

    /// Who accepted a share.
    InviteAcceptor as "DeviceInvite.acceptedBy" {
        id: "id" => String,
        login_name: "loginName" => String,
        profile_pic_url: "profilePicUrl" => String,
    }

    /// A configured link to a device posture provider.
    PostureIntegration {
        /// One of [`POSTURE_PROVIDERS`]. Required when creating, ignored when
        /// updating.
        provider: "provider" => String,
        /// Which of the provider's clouds: `us-1`, `eu-1` and so on for Falcon,
        /// a region for Intune, blank where the provider has one.
        cloud_id: "cloudId" => String,
        client_id: "clientId" => String,
        /// Intune's directory (tenant) ID; blank for every other provider.
        tenant_id: "tenantId" => String,
        /// Required when creating; omitted when updating leaves it as it was.
        client_secret: "clientSecret" => Secret,
        id: "id" => String,
        config_updated: "configUpdated" => String,
        status: "status" => PostureIntegrationStatus,
    }

    /// How the last sync with a posture provider went.
    PostureIntegrationStatus as "PostureIntegration.status" {
        last_sync: "lastSync" => String,
        error: "error" => String,
        provider_host_count: "providerHostCount" => i64,
        matched_count: "matchedCount" => i64,
        possible_matched_count: "possibleMatchedCount" => i64,
    }
}
