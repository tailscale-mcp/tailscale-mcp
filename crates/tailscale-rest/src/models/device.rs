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

    /// One share to create, as the request's array carries them.
    CreateDeviceInvite as "POST /device/{deviceId}/device-invites body[]" {
        /// Whether more than one person may accept this invite.
        multi_use: "multiUse" => bool,
        /// Whether the invited user may use the device as an exit node, where
        /// it advertises as one.
        allow_exit_node: "allowExitNode" => bool,
        /// Omit to create an invite nobody is mailed, whose `inviteUrl` is
        /// then shared by hand.
        email: "email" => String,
    }

    /// What accepting a share sends: the invite, as a URL or as its bare id.
    AcceptDeviceInvite as "POST /device-invites/-/accept body" {
        invite: "invite" => String,
    }

    /// What accepting a share answers with.
    ///
    /// Three flat objects rather than the [`Device`] and [`User`] models
    /// beside them: the description gives each its own small shape here, and a
    /// caller reading `device.ipv4` would not find it on a `Device`.
    AcceptedDeviceInvite as "POST /device-invites/-/accept 200" {
        device: "device" => SharedDevice,
        /// Whose device it is.
        sharer: "sharer" => SharePartner,
        /// Who now has it, which is the credential that made this call.
        accepted_by: "acceptedBy" => SharePartner,
    }

    /// The device a share hands over, as the acceptance describes it.
    SharedDevice as "POST /device-invites/-/accept 200.device" {
        id: "id" => String,
        os: "os" => String,
        name: "name" => String,
        fqdn: "fqdn" => String,
        ipv4: "ipv4" => String,
        ipv6: "ipv6" => String,
        /// Whether this share carries the device's exit node.
        include_exit_node: "includeExitNode" => bool,
    }

    /// One side of a share: whoever offered it, or whoever took it.
    SharePartner as "POST /device-invites/-/accept 200.sharer" {
        id: "id" => String,
        display_name: "displayName" => String,
        login_name: "loginName" => String,
        profile_pic_url: "profilePicURL" => String,
    }

    /// The other side, which the description gives the same shape.
    SharePartnerAcceptor as "POST /device-invites/-/accept 200.acceptedBy" is SharePartner;

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

    // -----------------------------------------------------------------------
    // What the endpoints send and answer with.
    //
    // The description spells these out where they are used rather than naming
    // them, so their paths are the routes rather than schema names (Q64). They
    // are here because the tools that build them are, and because the drift
    // test holds a route to its shape exactly as it holds a named schema.
    // -----------------------------------------------------------------------

    /// What a device listing answers with.
    ///
    /// One field, and it stays one field: the endpoint has no pagination and
    /// no total, so a caller windowing the list is windowing the whole of it.
    DeviceList as "GET /tailnet/{tailnet}/devices 200" {
        devices: "devices" => Vec<Device>,
    }

    /// Authorise a device, or revoke its authorisation with `false`.
    DeviceAuthorization as "POST /device/{deviceId}/authorized body" {
        authorized: "authorized" => bool,
    }

    /// Move a device to another address in the tailnet's range.
    DeviceAddress as "POST /device/{deviceId}/ip body" {
        ipv4: "ipv4" => String,
    }

    /// Turn a device's key expiry off, or back on.
    DeviceKeyExpiry as "POST /device/{deviceId}/key body" {
        key_expiry_disabled: "keyExpiryDisabled" => bool,
    }

    /// Rename a device, which retires its old MagicDNS names.
    DeviceName as "POST /device/{deviceId}/name body" {
        name: "name" => String,
    }

    /// Replace the routes a device is permitted to carry.
    ///
    /// Only the enabled set: what a device *advertises* it can route is the
    /// device's own business and cannot be set through the API.
    DeviceEnabledRoutes as "POST /device/{deviceId}/routes body" {
        routes: "routes" => Vec<String>,
    }

    /// Replace a device's tags.
    DeviceTags as "POST /device/{deviceId}/tags body" {
        tags: "tags" => Vec<String>,
    }

    /// Set one custom posture attribute on one device.
    PostureAttribute as "POST /device/{deviceId}/attributes/{attributeKey} body" {
        /// A string, a number or a boolean. The type is fixed by the first
        /// write and a later write of another type is refused.
        value: "value" => Value,
        /// When the control plane should forget it.
        expiry: "expiry" => String,
        comment: "comment" => String,
    }

    /// Set custom posture attributes on many devices at once.
    ///
    /// A JSON Merge Patch: a `null` value deletes the attribute, and a device
    /// the map does not mention is left alone.
    PostureAttributeBatch as "PATCH /tailnet/{tailnet}/device-attributes body" {
        nodes: "nodes" => BTreeMap<String, BTreeMap<String, Value>>,
        comment: "comment" => String,
    }

    /// The longer form a batched attribute may take, where a bare value would
    /// not carry an expiry.
    PostureAttributeValue
        as "PATCH /tailnet/{tailnet}/device-attributes body.nodes{}{}|anyOf[0]" {
        value: "value" => Value,
        expiry: "expiry" => String,
    }

    /// What a posture integration listing answers with.
    PostureIntegrationList as "GET /tailnet/{tailnet}/posture/integrations 200" {
        integrations: "integrations" => Vec<PostureIntegration>,
    }
}
