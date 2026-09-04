//! The people in a tailnet, invitations to join it, and who to email about it.

use crate::model;
use crate::models::KnownValues;

/// The roles a user can hold.
///
/// `owner` is not in [`INVITE_ROLES`]: a tailnet has one, and it is
/// transferred rather than invited.
pub const USER_ROLES: &[&str] = &[
    "owner",
    "member",
    "admin",
    "it-admin",
    "network-admin",
    "billing-admin",
    "auditor",
];

/// The roles an invitation can offer.
pub const INVITE_ROLES: &[&str] = &[
    "member",
    "admin",
    "it-admin",
    "network-admin",
    "billing-admin",
    "auditor",
];

/// Whether a user belongs to this tailnet or is shared into it.
pub const USER_TYPES: &[&str] = &["member", "shared"];

/// Where a user stands with the tailnet.
pub const USER_STATUSES: &[&str] = &[
    "active",
    "idle",
    "suspended",
    "needs-approval",
    "over-billing-limit",
];

/// The three addresses a tailnet keeps.
pub const CONTACT_TYPES: &[&str] = &["account", "support", "security"];

/// [`USER_ROLES`] as a filter, where `all` means do not filter.
pub const USER_ROLE_FILTERS: &[&str] = &[
    "owner",
    "member",
    "admin",
    "it-admin",
    "network-admin",
    "billing-admin",
    "auditor",
    "all",
];

/// [`USER_TYPES`] as a filter, with the same `all`.
pub const USER_TYPE_FILTERS: &[&str] = &["member", "shared", "all"];

pub const KNOWN_VALUES: &[KnownValues] = &[
    ("User.role", USER_ROLES),
    ("User.type", USER_TYPES),
    ("User.status", USER_STATUSES),
    ("UserInvite.role", INVITE_ROLES),
    ("?contactType", CONTACT_TYPES),
    ("/tailnet/{tailnet}/users ?role", USER_ROLE_FILTERS),
    ("/tailnet/{tailnet}/users ?type", USER_TYPE_FILTERS),
    ("POST /users/{userId}/role body.role", USER_ROLES),
    ("POST /tailnet/{tailnet}/user-invites body[].role", INVITE_ROLES),
];

model! {
    /// A person with access to the tailnet.
    User {
        id: "id" => String,
        display_name: "displayName" => String,
        login_name: "loginName" => String,
        profile_pic_url: "profilePicUrl" => String,
        tailnet_id: "tailnetId" => String,
        created: "created" => String,
        /// One of [`USER_TYPES`].
        user_type: "type" => String,
        /// One of [`USER_ROLES`].
        role: "role" => String,
        /// One of [`USER_STATUSES`].
        status: "status" => String,
        device_count: "deviceCount" => i64,
        last_seen: "lastSeen" => String,
        currently_connected: "currentlyConnected" => bool,
    }

    /// An invitation for someone outside the tailnet to join it.
    UserInvite {
        id: "id" => String,
        /// The role the invitee gets on accepting. One of [`INVITE_ROLES`].
        role: "role" => String,
        tailnet_id: "tailnetId" => i64,
        inviter_id: "inviterId" => i64,
        /// Empty for an invite nobody was mailed, whose URL is shared by hand.
        email: "email" => String,
        last_email_sent_at: "lastEmailSentAt" => String,
        /// Anyone holding this link can accept, not only the addressee.
        invite_url: "inviteUrl" => String,
    }

    /// An address the tailnet's notices go to.
    ///
    /// There is one of these per contact kind — account, support, security —
    /// and the kind is in the path rather than in the body.
    Contact {
        /// The address in use, which only changes once the new one is verified.
        email: "email" => String,
        /// A newly set address that has not been verified yet.
        fallback_email: "fallbackEmail" => String,
        needs_verification: "needsVerification" => bool,
    }
}
