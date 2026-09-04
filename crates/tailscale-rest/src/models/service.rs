//! Services: a name and a pair of addresses that several devices can host.

use crate::model;
use crate::models::KnownValues;

/// How far a device hosting a service has got through approval.
///
/// The two `approved:` values differ only in how the approval happened, which
/// is why this is one string and not a boolean and a flag.
pub const APPROVAL_LEVELS: &[&str] = &["not-approved", "approved:auto", "approved:manual"];

pub const KNOWN_VALUES: &[KnownValues] = &[("ServiceHostInfo.approvalLevel", APPROVAL_LEVELS)];

model! {
    /// A service, as the tailnet sees it.
    VipServiceInfo as "VIPServiceInfo" {
        /// Unique within the tailnet.
        name: "name" => String,
        /// A label for the admin console and for clients with access. At most
        /// 64 characters.
        display_name: "displayName" => String,
        /// The IPv4 first, then the IPv6.
        addrs: "addrs" => Vec<String>,
        comment: "comment" => String,
        /// `protocol:port` pairs, where `tcp` is the only protocol so far.
        /// `do-not-validate` skips the check.
        ports: "ports" => Vec<String>,
        tags: "tags" => Vec<String>,
    }

    /// The body a `PUT` sends, which is the same six fields.
    ///
    /// The description declares it separately to re-describe `addrs`, which on
    /// the way in is a request rather than a report: unset or one IPv4 for a
    /// new service, and for an existing one an IPv4 that may be changed
    /// alongside an IPv6 that may not.
    VipServiceInfoPut as "VIPServiceInfoPut" is VipServiceInfo;

    /// One device hosting a service.
    ServiceHostInfo {
        stable_node_id: "stableNodeID" => String,
        /// One of [`APPROVAL_LEVELS`].
        approval_level: "approvalLevel" => String,
        configured: "configured" => String,
    }

    /// Whether a device may host a service, and how it got there.
    VipServiceApproval as "VIPServiceApproval" {
        approved: "approved" => bool,
        /// `true` where an auto-approver did it rather than a person.
        auto_approved: "autoApproved" => bool,
    }
}
