//! Audit logs, network flow logs, and streaming either of them somewhere else.

use serde_json::Value;

use crate::Secret;
use crate::model;
use crate::models::KnownValues;

/// The two log streams a tailnet produces.
pub const LOG_TYPES: &[&str] = &["configuration", "network"];

/// The systems logs can be streamed to.
pub const DESTINATION_TYPES: &[&str] = &[
    "splunk",
    "elastic",
    "panther",
    "cribl",
    "crowdstrike",
    "datadog",
    "axiom",
    "s3",
];

/// How a log stream is compressed. `none` is the default.
pub const COMPRESSION_FORMATS: &[&str] = &["zstd", "gzip", "none"];

/// How Tailscale authenticates to S3. `rolearn` is the recommended one.
pub const S3_AUTHENTICATION_TYPES: &[&str] = &["accesskey", "rolearn"];

/// What kind of log an audit record is. One member today, which is the whole
/// reason it is a string here (Q60).
pub const AUDIT_LOG_TYPES: &[&str] = &["CONFIG"];

/// What set a configuration change in motion.
pub const AUDIT_ORIGINS: &[&str] = &[
    "ADMIN_CONSOLE",
    "CONFIG_API",
    "CONTROL",
    "IDENTITY_PROVIDER",
    "NODE",
    "SUPPORT_REQUEST",
    "STRIPE",
    "SECURITY_NOTIFICATION",
    "LEGAL_NOTIFICATION",
    "BORDER0_API",
];

/// What kind of thing acted.
pub const AUDIT_ACTOR_TYPES: &[&str] = &[
    "USER",
    "NODE",
    "AUTOMATED_WORKER",
    "OAUTH_CLIENT",
    "SCIM",
    "MULLVAD",
    "LOGSTREAM",
    "SECRET_SCANNER",
    "PAM_CONNECTOR",
    "PAM_SERVICE_ACCOUNT",
];

/// What kind of thing was acted on.
pub const AUDIT_TARGET_TYPES: &[&str] = &[
    "TAILNET",
    "USER",
    "GROUP",
    "NODE",
    "API_KEY",
    "INVITE",
    "SHARE",
    "BILLING",
    "ADMIN_CONSOLE",
    "WEB_INTERFACE",
    "WEBHOOK_ENDPOINT",
    "FAILED_REQUEST",
];

/// Which property of the target changed. The longest of these lists and the
/// one most likely to grow, since every new setting adds a member.
pub const AUDIT_TARGET_PROPERTIES: &[&str] = &[
    "ACL",
    "ACL_TAGS",
    "ACCOUNT_EMAIL",
    "ADDRESS",
    "ALLOWED_IPS",
    "AUTO_APPROVED_ROUTES",
    "ATTRIBUTES",
    "BILLING_OWNER",
    "COLLECT_SERVICES",
    "COLLECT_POSTURE_IDENTITY",
    "MULLVAD_VPN",
    "DNS_CONFIG",
    "EMAIL",
    "EXIT_NODE",
    "FEATURE",
    "FILE_SHARING",
    "HTTPS",
    "KEY_EXPIRY_TIME",
    "KEY_EXPIRY",
    "LOG_EXIT_FLOWS",
    "LOGSTREAM_ENDPOINT",
    "MAGIC_DNS",
    "MACHINE_AUTH_NEEDED",
    "MACHINE_APPROVAL_NEEDED",
    "USER_APPROVAL_REQUIRED",
    "MACHINE_NAME",
    "MAX_KEY_DURATION",
    "NETWORK_FLOW_LOGGING",
    "GEOSTEERING",
    "NODE_SHARE",
    "TAILNET_INVITE",
    "PAYMENT_INFO",
    "POSTURE_IDENTITY",
    "POSTURE_INTEGRATION",
    "USER_ROLE",
    "SCIM",
    "SECURITY_EMAIL",
    "STRIPE_CUSTOMER_ID",
    "SUBSCRIPTION",
    "SUBSCRIBED_EVENTS",
    "SUPPORT_EMAIL",
    "SECRET",
    "TCD",
    "TKA",
    "AUTH_PROVIDER",
];

/// What was attempted against the target.
pub const AUDIT_ACTIONS: &[&str] = &[
    "LOGIN",
    "LOGOUT",
    "CREATE",
    "UPDATE",
    "DELETE",
    "CANCEL",
    "REVOKE",
    "APPROVE",
    "SUSPEND",
    "RESTORE",
    "ENABLE",
    "DISABLE",
    "ACCEPT",
    "EXPIRED",
    "PUSH_USER",
    "PUSH_GROUP",
    "VERIFY",
    "JOIN_WAITLIST",
    "INVITE",
    "JOIN",
    "LEAVE",
    "RESEND",
    "MIGRATE_AUTH_PROVIDER",
];

/// The IP protocols a flow log names. A flow over anything else is reported by
/// number, so this list is a spelling aid rather than a set of possibilities.
pub const FLOW_PROTOCOLS: &[&str] = &[
    "ah",
    "dccp",
    "egp",
    "esp",
    "gre",
    "icmp",
    "igmp",
    "igp",
    "ipv4",
    "ipv6-icmp",
    "sctp",
    "tcp",
    "udp",
];

/// Every event the audit log can be filtered by.
///
/// A hundred and thirty-eight of them, and the list that a  parameter
/// quotes. Kept whole rather than summarised because the caller has to spell one
/// exactly, and a truncated list is worse than none.
pub const AUDIT_EVENTS: &[&str] = &[
    "ADMIN_CONSOLE.LOGIN", "ADMIN_CONSOLE.LOGOUT", "API_KEY.CREATE", "API_KEY.EXPIRED",
    "API_KEY.REVOKE", "BILLING.CANCEL.SUBSCRIPTION", "BILLING.CREATE.SUBSCRIPTION",
    "BILLING.UPDATE.ADDRESS", "BILLING.UPDATE.BILLING_OWNER", "BILLING.UPDATE.EMAIL",
    "BILLING.UPDATE.PAYMENT_INFO", "BILLING.UPDATE.STRIPE_CUSTOMER_ID",
    "BILLING.UPDATE.SUBSCRIPTION", "FAILED_REQUEST.UPDATE", "GROUP.PUSH_GROUP.ATTRIBUTES",
    "INVITE.ACCEPT.FEATURE", "INVITE.ACCEPT.NODE_SHARE", "INVITE.ACCEPT.TAILNET_INVITE",
    "INVITE.CREATE.FEATURE", "INVITE.CREATE.NODE_SHARE", "INVITE.CREATE.TAILNET_INVITE",
    "INVITE.DELETE.NODE_SHARE", "INVITE.DELETE.TAILNET_INVITE", "INVITE.RESEND.NODE_SHARE",
    "INVITE.RESEND.TAILNET_INVITE", "NODE.APPROVE", "NODE.CREATE", "NODE.CREATE.ATTRIBUTES",
    "NODE.DELETE", "NODE.DELETE.ATTRIBUTES", "NODE.DISABLE.KEY_EXPIRY",
    "NODE.DISCONNECT_NODE.CLIENT_LOG", "NODE.ENABLE.KEY_EXPIRY",
    "NODE.EXPIRED.KEY_EXPIRY_TIME", "NODE.LOGIN", "NODE.LOGOUT", "NODE.REVOKE",
    "NODE.UPDATE.ACL_TAGS", "NODE.UPDATE.ALLOWED_IPS", "NODE.UPDATE.ATTRIBUTES",
    "NODE.UPDATE.AUTO_APPROVED_ROUTES", "NODE.UPDATE.EXIT_NODE",
    "NODE.UPDATE.KEY_EXPIRY_TIME", "NODE.UPDATE.MACHINE_NAME",
    "NODE.UPDATE.POSTURE_IDENTITY", "NODE.UPDATE.TKA", "SHARE.CREATE", "SHARE.DELETE",
    "SHARE.UPDATE", "TAILNET.ACCEPT.FEATURE", "TAILNET.CREATE",
    "TAILNET.CREATE.LOGSTREAM_ENDPOINT", "TAILNET.CREATE.POSTURE_INTEGRATION",
    "TAILNET.CREATE.TKA", "TAILNET.DELETE.LOGSTREAM_ENDPOINT",
    "TAILNET.DELETE.POSTURE_INTEGRATION", "TAILNET.DELETE.TKA",
    "TAILNET.DISABLE.COLLECT_POSTURE_IDENTITY", "TAILNET.DISABLE.COLLECT_SERVICES",
    "TAILNET.DISABLE.FILE_SHARING", "TAILNET.DISABLE.GEOSTEERING", "TAILNET.DISABLE.HTTPS",
    "TAILNET.DISABLE.LOG_EXIT_FLOWS", "TAILNET.DISABLE.MACHINE_APPROVAL_NEEDED",
    "TAILNET.DISABLE.MAGIC_DNS", "TAILNET.DISABLE.MULLVAD_VPN",
    "TAILNET.DISABLE.NETWORK_FLOW_LOGGING", "TAILNET.DISABLE.SCIM", "TAILNET.DISABLE.TKA",
    "TAILNET.DISABLE.USER_APPROVAL_REQUIRED", "TAILNET.ENABLE.COLLECT_POSTURE_IDENTITY",
    "TAILNET.ENABLE.COLLECT_SERVICES", "TAILNET.ENABLE.FILE_SHARING",
    "TAILNET.ENABLE.GEOSTEERING", "TAILNET.ENABLE.HTTPS", "TAILNET.ENABLE.LOG_EXIT_FLOWS",
    "TAILNET.ENABLE.MACHINE_APPROVAL_NEEDED", "TAILNET.ENABLE.MAGIC_DNS",
    "TAILNET.ENABLE.MULLVAD_VPN", "TAILNET.ENABLE.NETWORK_FLOW_LOGGING",
    "TAILNET.ENABLE.SCIM", "TAILNET.ENABLE.TKA", "TAILNET.ENABLE.USER_APPROVAL_REQUIRED",
    "TAILNET.JOIN", "TAILNET.JOIN_WAITLIST.FEATURE", "TAILNET.LEAVE",
    "TAILNET.UPDATE.ACCOUNT_EMAIL", "TAILNET.UPDATE.ACL", "TAILNET.UPDATE.DNS_CONFIG",
    "TAILNET.UPDATE.LOGSTREAM_ENDPOINT", "TAILNET.UPDATE.MAX_KEY_DURATION",
    "TAILNET.UPDATE.POSTURE_INTEGRATION", "TAILNET.UPDATE.SECURITY_EMAIL",
    "TAILNET.UPDATE.SUPPORT_EMAIL", "TAILNET.UPDATE.TCD", "TAILNET.UPDATE.TKA",
    "TAILNET.VERIFY.ACCOUNT_EMAIL", "TAILNET.VERIFY.SECURITY_EMAIL",
    "TAILNET.VERIFY.SUPPORT_EMAIL", "USER.APPROVE", "USER.CREATE", "USER.DELETE",
    "USER.INVITE", "USER.PUSH_USER.ATTRIBUTES", "USER.RESEND.TAILNET_INVITE",
    "USER.RESTORE", "USER.RESTORE_GLOBAL", "USER.SUSPEND", "USER.SUSPEND_GLOBAL",
    "USER.UPDATE.USER_ROLE", "WEBHOOK_ENDPOINT.CREATE", "WEBHOOK_ENDPOINT.DELETE",
    "WEBHOOK_ENDPOINT.UPDATE.SECRET", "WEBHOOK_ENDPOINT.UPDATE.SUBSCRIBED_EVENTS",
    "WEB_INTERFACE.LOGIN", "WEB_INTERFACE.LOGOUT", "PAM_CONNECTOR.CREATE",
    "PAM_CONNECTOR.CREATE.ACCESS_TOKEN", "PAM_CONNECTOR.DELETE",
    "PAM_CONNECTOR.DISABLE.ACCESS_TOKEN", "PAM_CONNECTOR.UPDATE", "PAM_SERVICE.CREATE",
    "PAM_SERVICE.DELETE", "PAM_SERVICE.UPDATE", "PAM_SERVICE_ACCOUNT.CREATE",
    "PAM_SERVICE_ACCOUNT.CREATE.ACCESS_TOKEN", "PAM_SERVICE_ACCOUNT.DELETE",
    "PAM_SERVICE_ACCOUNT.UPDATE", "PAM_SETTINGS.CREATE.CUSTOM_DOMAIN",
    "PAM_SETTINGS.CREATE.NOTIFICATION", "PAM_SETTINGS.CREATE.RECORDING_STORAGE",
    "PAM_SETTINGS.DELETE.CUSTOM_DOMAIN", "PAM_SETTINGS.DELETE.NOTIFICATION",
    "PAM_SETTINGS.DELETE.RECORDING_STORAGE", "PAM_SETTINGS.UPDATE",
    "PAM_SETTINGS.UPDATE.CUSTOM_DOMAIN", "PAM_SETTINGS.UPDATE.NOTIFICATION",
    "PAM_SETTINGS.UPDATE.SETUP_WIZARD",
];

pub const KNOWN_VALUES: &[KnownValues] = &[
    ("LogType", LOG_TYPES),
    (
        "LogstreamEndpointConfiguration.destinationType",
        DESTINATION_TYPES,
    ),
    (
        "LogstreamEndpointConfiguration.compressionFormat",
        COMPRESSION_FORMATS,
    ),
    (
        "LogstreamEndpointConfiguration.s3AuthenticationType",
        S3_AUTHENTICATION_TYPES,
    ),
    ("ConfigurationAuditLog.type", AUDIT_LOG_TYPES),
    ("ConfigurationAuditLog.origin", AUDIT_ORIGINS),
    ("ConfigurationAuditLog.actor.type", AUDIT_ACTOR_TYPES),
    ("ConfigurationAuditLog.target.type", AUDIT_TARGET_TYPES),
    (
        "ConfigurationAuditLog.target.property",
        AUDIT_TARGET_PROPERTIES,
    ),
    ("ConfigurationAuditLog.action", AUDIT_ACTIONS),
    ("ConnectionCounts.proto", FLOW_PROTOCOLS),
    ("?event[]", AUDIT_EVENTS),
];

model! {
    /// One configuration change, as the audit log records it.
    ConfigurationAuditLog {
        event_time: "eventTime" => String,
        /// One of [`AUDIT_LOG_TYPES`].
        log_type: "type" => String,
        /// Set where the rate limiter held the record back, naming the time it
        /// was enqueued rather than the time it was written.
        deferred_at: "deferredAt" => String,
        /// Shared by every event that came out of one operation.
        event_group_id: "eventGroupID" => String,
        /// One of [`AUDIT_ORIGINS`].
        origin: "origin" => String,
        actor: "actor" => AuditActor,
        target: "target" => AuditTarget,
        /// One of [`AUDIT_ACTIONS`].
        action: "action" => String,
        /// `target.property` before the change; of whatever shape that
        /// property has.
        old: "old" => Value,
        /// `target.property` after the change.
        new: "new" => Value,
        /// A reason, where the caller gave one.
        action_details: "actionDetails" => String,
        /// Present where the change failed, and readable by the person who
        /// attempted it.
        error: "error" => String,
    }

    /// Who or what made the change.
    AuditActor as "ConfigurationAuditLog.actor" {
        /// A user ID or a node ID, depending on `type`.
        id: "id" => String,
        /// One of [`AUDIT_ACTOR_TYPES`].
        actor_type: "type" => String,
        /// As it was at the time, not as it is now.
        login_name: "loginName" => String,
        display_name: "displayName" => String,
        tags: "tags" => Vec<String>,
    }

    /// What the change was made to.
    AuditTarget as "ConfigurationAuditLog.target" {
        id: "id" => String,
        /// As it was at the time.
        name: "name" => String,
        /// One of [`AUDIT_TARGET_TYPES`].
        target_type: "type" => String,
        /// Only meaningful where `type` is `NODE`.
        is_ephemeral: "isEphemeral" => bool,
        /// One of [`AUDIT_TARGET_PROPERTIES`]; what `old` and `new` hold.
        property: "property" => String,
    }

    /// Traffic between two addresses over one protocol.
    ConnectionCounts {
        /// One of [`FLOW_PROTOCOLS`], or the protocol number for anything else.
        proto: "proto" => String,
        /// `addr:port`.
        src: "src" => String,
        /// `addr:port`.
        dst: "dst" => String,
        tx_pkts: "txPkts" => i64,
        tx_bytes: "txBytes" => i64,
        rx_pkts: "rxPkts" => i64,
        rx_bytes: "rxBytes" => i64,
    }

    /// One node's traffic over one interval, by the path it took.
    NetworkFlowLog {
        logged: "logged" => String,
        node_id: "nodeId" => String,
        start: "start" => String,
        end: "end" => String,
        /// Tailscale address to Tailscale address.
        virtual_traffic: "virtualTraffic" => Vec<ConnectionCounts>,
        /// Through a subnet router.
        subnet_traffic: "subnetTraffic" => Vec<ConnectionCounts>,
        /// Through an exit node.
        exit_traffic: "exitTraffic" => Vec<ConnectionCounts>,
        /// The underlying transport, which is what the other three ride on.
        physical_traffic: "physicalTraffic" => Vec<ConnectionCounts>,
    }

    /// Where a log stream goes and how it authenticates.
    ///
    /// Most of this is conditional on `destinationType`: the `s3*` fields
    /// apply to S3, the `gcs*` fields to GCS, and `url`, `user` and `token` to
    /// the vendors.
    LogstreamEndpointConfiguration {
        /// One of [`LOG_TYPES`].
        log_type: "logType" => String,
        /// One of [`DESTINATION_TYPES`].
        destination_type: "destinationType" => String,
        /// Often empty for S3, where the official endpoint is used.
        url: "url" => String,
        user: "user" => String,
        /// A wait between uploads. Logs that do not fit in one upload are sent
        /// in several regardless.
        upload_period_minutes: "uploadPeriodMinutes" => i64,
        /// One of [`COMPRESSION_FORMATS`], defaulting to `none`.
        compression_format: "compressionFormat" => String,
        token: "token" => Secret,
        s3_bucket: "s3Bucket" => String,
        s3_region: "s3Region" => String,
        s3_key_prefix: "s3KeyPrefix" => String,
        /// One of [`S3_AUTHENTICATION_TYPES`].
        s3_authentication_type: "s3AuthenticationType" => String,
        s3_access_key_id: "s3AccessKeyId" => String,
        s3_secret_access_key: "s3SecretAccessKey" => Secret,
        /// The role Tailscale assumes under `rolearn` authentication.
        s3_role_arn: "s3RoleArn" => String,
        /// What Tailscale presents to AWS under `rolearn` authentication; see
        /// [`AwsExternalId`].
        s3_external_id: "s3ExternalId" => String,
        gcs_bucket: "gcsBucket" => String,
        gcs_key_prefix: "gcsKeyPrefix" => String,
        gcs_scopes: "gcsScopes" => Vec<String>,
        /// Workload identity credentials, as GCS's own JSON document.
        gcs_credentials: "gcsCredentials" => Secret,
    }

    /// How a log stream proves to AWS that it is this tailnet.
    ///
    /// The pair goes in the AWS role's trust policy, which is what makes
    /// `rolearn` authentication work without a stored access key.
    AwsExternalId {
        external_id: "externalId" => String,
        tailscale_aws_account_id: "tailscaleAwsAccountId" => String,
    }

    /// Whether the endpoint is being reached, and how well.
    LogstreamEndpointPublishingStatus {
        last_activity: "lastActivity" => String,
        last_error: "lastError" => String,
        max_body_size: "maxBodySize" => i64,
        num_bytes_sent: "numBytesSent" => i64,
        num_entries_sent: "numEntriesSent" => i64,
        num_spoofed_entries: "numSpoofedEntries" => i64,
        num_total_requests: "numTotalRequests" => i64,
        num_failed_requests: "numFailedRequests" => i64,
        rate_bytes_sent: "rateBytesSent" => f64,
        rate_entries_sent: "rateEntriesSent" => f64,
        rate_total_requests: "rateTotalRequests" => f64,
        rate_failed_requests: "rateFailedRequests" => f64,
    }
}
