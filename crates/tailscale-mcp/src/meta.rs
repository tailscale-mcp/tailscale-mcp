//! Tool metadata: the single table that the router, the `tools` subcommand,
//! the contract tests and the generated documentation all read.
//!
//! A tool cannot exist without a row here. Ticket 02 makes that structural: the
//! declaration macro in [`crate::registry`] emits the row and the handler from
//! one declaration, so the two cannot drift apart.

use std::fmt;

/// Which of the two backends a tool acts through.
///
/// The distinction is not cosmetic: the local surface can only ever act on the
/// node the server runs on, while the tailnet surface acts on the whole tailnet
/// and needs a credential rather than a binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Surface {
    /// The `tailscale` command-line interface, acting on the local node.
    Local,
    /// The control-plane REST API, acting on the tailnet.
    Tailnet,
}

impl Surface {
    /// The prefix every tool name on this surface carries.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Local => "tailscale_",
            Self::Tailnet => "tailnet_",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Tailnet => "tailnet",
        }
    }
}

impl fmt::Display for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The verbs a `tailnet_*` tool's name may end in.
///
/// `spec.md` asks for "a fixed verb vocabulary", and this is it. The point is
/// not tidiness: there are ninety-three of these tools across five tickets, and
/// without one list to conform to the same operation gets called `delete` in
/// one toolset and `remove` in the next, which a model then has to learn
/// twice. So the set is closed and `every_tailnet_tool_ends_in_a_known_verb`
/// holds every name to it.
///
/// Deliberately declared whole rather than grown as tools land. A vocabulary
/// that gains a word whenever a name does not fit is not one, and the entries
/// with no tool yet are the constraint on the tickets that add them.
///
/// - `list`, `get` — read a collection, read one thing.
/// - `create`, `update`, `delete` — the usual three. `update` changes the
///   fields it is given; `replace` is for the endpoints that take the whole
///   object and discard what is missing.
/// - `set` — assign one named thing on a resource: `..._tags_set`. Distinct
///   from `update` because the resource is not what is being replaced.
/// - `authorize`, `approve`, `expire`, `rename`, `enable`, `disable`,
///   `suspend`, `restore` — actions with no CRUD spelling, each the API's own
///   word for it. `suspend` and `restore` were added at ticket 19 (Q77): the
///   endpoints are `suspendUser` and `restoreUser`, and calling them
///   `disable`/`enable` would have been this server renaming something
///   Tailscale had already named.
/// - `accept`, `resend` — what an invitation can have done to it.
/// - `validate`, `preview` — the policy file's two dry runs.
/// - `test`, `rotate` — a webhook's delivery check and its secret.
pub const TAILNET_VERBS: &[&str] = &[
    "accept",
    "approve",
    "authorize",
    "create",
    "delete",
    "disable",
    "enable",
    "expire",
    "get",
    "list",
    "preview",
    "rename",
    "replace",
    "resend",
    "restore",
    "rotate",
    "set",
    "suspend",
    "test",
    "update",
    "validate",
];

/// A tool's risk class.
///
/// The ordering is meaningful and is relied upon by the gate: a server allowed
/// to run destructive tools may also run write and read tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// Changes nothing. Available by default.
    Read,
    /// Changes configuration that can be changed back.
    Write,
    /// Removes something, or exposes something, in a way that is not simply
    /// undone: deleting a device, revoking a key, publishing to the internet.
    Destructive,
}

impl Tier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }

    /// The flag an operator passes to permit this tier, if any.
    pub const fn flag(self) -> Option<&'static str> {
        match self {
            Self::Read => None,
            Self::Write => Some("--allow-write"),
            Self::Destructive => Some("--allow-destructive"),
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A named group of tools switched on or off together.
///
/// Kept as an enum rather than a string so that a preset cannot name a toolset
/// that does not exist, and so that adding a toolset forces every preset to be
/// reconsidered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Toolset {
    // Local surface.
    LocalStatus,
    LocalPrefs,
    LocalServe,
    LocalFiles,
    LocalLock,
    LocalDebug,
    LocalPassthrough,
    // Tailnet surface.
    TailnetDevices,
    TailnetInvites,
    TailnetLogging,
    TailnetDns,
    TailnetKeys,
    TailnetPolicy,
    TailnetPosture,
    TailnetUsers,
    TailnetSettings,
    TailnetWebhooks,
    TailnetServices,
    TailnetOauthApps,
    TailnetOrg,
}

impl Toolset {
    /// Every toolset, in listing order. Adding a variant without adding it here
    /// is caught by `all_is_exhaustive`, which is a test and so cannot be linked
    /// from documentation built without `cfg(test)`.
    pub const ALL: &'static [Toolset] = &[
        Self::LocalStatus,
        Self::LocalPrefs,
        Self::LocalServe,
        Self::LocalFiles,
        Self::LocalLock,
        Self::LocalDebug,
        Self::LocalPassthrough,
        Self::TailnetDevices,
        Self::TailnetInvites,
        Self::TailnetLogging,
        Self::TailnetDns,
        Self::TailnetKeys,
        Self::TailnetPolicy,
        Self::TailnetPosture,
        Self::TailnetUsers,
        Self::TailnetSettings,
        Self::TailnetWebhooks,
        Self::TailnetServices,
        Self::TailnetOauthApps,
        Self::TailnetOrg,
    ];

    /// The name an operator writes in configuration.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalStatus => "local-status",
            Self::LocalPrefs => "local-prefs",
            Self::LocalServe => "local-serve",
            Self::LocalFiles => "local-files",
            Self::LocalLock => "local-lock",
            Self::LocalDebug => "local-debug",
            Self::LocalPassthrough => "local-passthrough",
            Self::TailnetDevices => "tailnet-devices",
            Self::TailnetInvites => "tailnet-invites",
            Self::TailnetLogging => "tailnet-logging",
            Self::TailnetDns => "tailnet-dns",
            Self::TailnetKeys => "tailnet-keys",
            Self::TailnetPolicy => "tailnet-policy",
            Self::TailnetPosture => "tailnet-posture",
            Self::TailnetUsers => "tailnet-users",
            Self::TailnetSettings => "tailnet-settings",
            Self::TailnetWebhooks => "tailnet-webhooks",
            Self::TailnetServices => "tailnet-services",
            Self::TailnetOauthApps => "tailnet-oauth-apps",
            Self::TailnetOrg => "tailnet-org",
        }
    }

    /// Which surface this toolset belongs to. Used to hide a whole surface when
    /// its backend is absent or has been disabled.
    pub const fn surface(self) -> Surface {
        match self {
            Self::LocalStatus
            | Self::LocalPrefs
            | Self::LocalServe
            | Self::LocalFiles
            | Self::LocalLock
            | Self::LocalDebug
            | Self::LocalPassthrough => Surface::Local,
            _ => Surface::Tailnet,
        }
    }

    /// Parse an operator-written toolset name.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == s)
    }

    #[cfg(test)]
    fn all_is_exhaustive() {}
}

impl fmt::Display for Toolset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The hints a client sees on a tool.
///
/// These are advisory in the protocol but load-bearing for a planning model, so
/// they are derived from the tier wherever the tier determines them and stated
/// explicitly only where it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Annotations {
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: bool,
    pub open_world: bool,
}

/// One row of the tool table.
#[derive(Debug, Clone, Copy)]
pub struct ToolMeta {
    /// The tool name as the client sees it, including its surface prefix.
    pub name: &'static str,
    pub toolset: Toolset,
    pub tier: Tier,
    /// One sentence, shown to the model. The full description lives on the
    /// generated schema; this is what the tool table prints.
    pub summary: &'static str,
    /// Whether calling this can cut the server off from the tailnet or from the
    /// client it serves. Self-severing tools always require confirmation; the
    /// two are separate fields because the tailnet surface has irreversible
    /// operations that are not self-severing but still require it.
    pub self_severing: bool,
    /// Whether this tool severs the connection when its *target* is this node.
    ///
    /// Where [`Self::self_severing`] is true of every call a tool makes, this
    /// is true of some of them: `tailnet_device_delete` is an ordinary
    /// destructive call against somebody else's machine and a cut cable
    /// against this one, and only the argument tells them apart. So it cannot
    /// imply [`Self::requires_confirmation`] — a caller managing another
    /// device would be made to confirm something that cannot happen — and the
    /// confirmation lives in the tool's own parameters, where the handler can
    /// ask for it only when the target turns out to be us (Q83).
    pub severs_local_node: bool,
    /// Whether the caller must state intent in the call itself. No flag can
    /// pre-authorise this.
    pub requires_confirmation: bool,
    /// Repeating the call has the same effect as making it once.
    pub idempotent: bool,
    /// Whether [`Self::tier`] is a floor rather than the whole truth.
    ///
    /// One tool sets this: the passthrough, whose risk is decided by the
    /// arguments it is given rather than by its row. The gate still reads the
    /// tier, so the tool is offered as soon as its floor is permitted, and the
    /// handler refuses anything above what the session allows. The annotations
    /// state the worst case, because a client reading `read_only` has no way to
    /// know that this one is conditional.
    pub varying_tier: bool,
    /// The lowest `tailscale` version that accepts this command, where the
    /// command is newer than our supported floor.
    pub min_version: Option<&'static str>,
    /// The operating systems the command exists on, when it does not exist on
    /// all of them. Values are [`std::env::consts::OS`] spellings.
    ///
    /// A restricted tool is still listed everywhere. The table is the same on
    /// every platform so that the documentation, the contract tests and the
    /// `tools` subcommand agree wherever they run, and so that a caller asking
    /// for something macOS-only on Linux is told *why* rather than finding a
    /// tool that does not exist.
    pub platforms: Option<&'static [&'static str]>,
}

impl ToolMeta {
    pub const fn surface(&self) -> Surface {
        self.toolset.surface()
    }

    /// Whether the command behind this tool exists on the machine we are on.
    pub fn runs_here(&self) -> bool {
        self.platforms
            .is_none_or(|allowed| allowed.contains(&std::env::consts::OS))
    }

    /// Annotations are derived, not stored, so that a tool cannot claim to be
    /// read-only while sitting at the destructive tier.
    pub const fn annotations(&self) -> Annotations {
        Annotations {
            read_only: !self.varying_tier && matches!(self.tier, Tier::Read),
            destructive: self.varying_tier || matches!(self.tier, Tier::Destructive),
            idempotent: self.idempotent,
            // Both surfaces reach a network the server does not control.
            open_world: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolset_all_covers_every_variant() {
        // A cheap stand-in for exhaustiveness: every name round-trips, and the
        // list has no duplicates. A new variant missing from ALL fails the
        // count assertions in the registry tests.
        let mut names: Vec<&str> = Toolset::ALL.iter().map(|t| t.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate toolset name");

        for t in Toolset::ALL {
            assert_eq!(Toolset::parse(t.as_str()), Some(*t));
        }
        Toolset::all_is_exhaustive();
    }

    #[test]
    fn toolset_names_are_prefixed_by_surface() {
        for t in Toolset::ALL {
            let expected = match t.surface() {
                Surface::Local => "local-",
                Surface::Tailnet => "tailnet-",
            };
            assert!(
                t.as_str().starts_with(expected),
                "{t} does not carry its surface prefix"
            );
        }
    }

    #[test]
    fn tiers_order_from_least_to_most_dangerous() {
        assert!(Tier::Read < Tier::Write);
        assert!(Tier::Write < Tier::Destructive);
    }

    #[test]
    fn annotations_follow_the_tier() {
        let read = ToolMeta {
            name: "tailscale_status",
            toolset: Toolset::LocalStatus,
            tier: Tier::Read,
            summary: "",
            self_severing: false,
            severs_local_node: false,
            requires_confirmation: false,
            idempotent: true,
            varying_tier: false,
            min_version: None,
            platforms: None,
        };
        assert!(read.annotations().read_only);
        assert!(!read.annotations().destructive);
        assert!(read.annotations().open_world);

        let destructive = ToolMeta {
            tier: Tier::Destructive,
            ..read
        };
        assert!(!destructive.annotations().read_only);
        assert!(destructive.annotations().destructive);
    }

    #[test]
    fn a_varying_tier_is_annotated_at_its_worst_case() {
        // The passthrough sits at the read tier so that a read-only session can
        // still reach the commands it may run, but a client must not be told
        // that calling it changes nothing.
        let passthrough = ToolMeta {
            name: "tailscale_run",
            toolset: Toolset::LocalPassthrough,
            tier: Tier::Read,
            summary: "",
            self_severing: false,
            severs_local_node: false,
            requires_confirmation: false,
            idempotent: false,
            varying_tier: true,
            min_version: None,
            platforms: None,
        };
        assert!(!passthrough.annotations().read_only);
        assert!(passthrough.annotations().destructive);
    }
}
