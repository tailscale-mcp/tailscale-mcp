//! Which tools this server offers, and at which risk tier.
//!
//! The decision this module implements is that a tool the server may not run is
//! *hidden*, not listed-and-refused. A model that can see a tool will try it,
//! and a listing full of tools that always fail is worse than a shorter listing
//! that works. [`Gate::permits`] is therefore consulted when building the tool
//! list, not only when handling a call.
//!
//! Calls are still checked. A client may call a name it did not get from the
//! listing, and that is what [`crate::error::ToolError::not_permitted`] answers.

use std::collections::BTreeSet;
use std::fmt;

use thiserror::Error;

use crate::meta::{Surface, Tier, ToolMeta, Toolset};

/// A named starting selection of toolsets.
///
/// Presets exist because the useful selections are few and the combinatorics
/// are many. An operator who wants something else says so with modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// Just enough to answer "what is the state of my network": status, the
    /// device inventory, the policy file, DNS.
    Minimal,
    /// The default. Everything an agent needs for day-to-day work, and nothing
    /// that is either a diagnostic rabbit hole or an escape hatch.
    #[default]
    Core,
    /// Every typed toolset. Excludes the debug knobs and the passthrough,
    /// which are opt-in by name because they are not part of the supported
    /// surface in the same way.
    Full,
}

impl Preset {
    pub const ALL: &'static [Preset] = &[Self::Minimal, Self::Core, Self::Full];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Core => "core",
            Self::Full => "full",
        }
    }

    pub fn parse(s: &str) -> Result<Self, ConfigError> {
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| ConfigError::UnknownPreset(s.to_owned()))
    }

    /// The toolsets this preset selects.
    pub fn toolsets(self) -> BTreeSet<Toolset> {
        use Toolset::*;
        let minimal = [LocalStatus, TailnetDevices, TailnetPolicy, TailnetDns];
        match self {
            Self::Minimal => minimal.into_iter().collect(),
            Self::Core => minimal
                .into_iter()
                .chain([
                    LocalPrefs,
                    LocalServe,
                    LocalFiles,
                    TailnetKeys,
                    TailnetUsers,
                    TailnetInvites,
                    TailnetWebhooks,
                    TailnetSettings,
                    TailnetServices,
                ])
                .collect(),
            Self::Full => Toolset::ALL
                .iter()
                .copied()
                .filter(|t| !matches!(t, LocalDebug | LocalPassthrough))
                .collect(),
        }
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A configuration mistake that must stop the server before it serves anything.
///
/// These are all startup errors on purpose. A typo in a toolset name that
/// silently selected nothing would present as "the tools vanished", which is a
/// much worse afternoon than a refusal to start.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("unknown preset `{0}`; expected one of minimal, core, full")]
    UnknownPreset(String),

    #[error("unknown toolset `{0}`")]
    UnknownToolset(String),

    #[error(
        "toolset list mixes replacement and adjustment: write either a bare list \
         of toolsets, or a list where every entry begins with `+` or `-`"
    )]
    MixedToolsetSyntax,

    #[error(
        "the selected toolsets and tier leave no tools enabled; \
             widen the preset, the toolsets, or the tier"
    )]
    NoToolsEnabled,

    #[error("`{setting}` was set to `{value}`, which is not {expected}")]
    InvalidValue {
        setting: String,
        value: String,
        expected: &'static str,
    },
}

/// Parse a toolset selection string.
///
/// Two shapes, and never a mixture:
///
/// - `local-status,tailnet-devices` replaces the preset's selection outright.
/// - `+local-debug,-tailnet-org` adjusts it.
///
/// The mixture is rejected rather than resolved because there is no reading of
/// `local-status,-tailnet-org` that a reader would agree on.
pub fn apply_toolset_modifiers(
    base: BTreeSet<Toolset>,
    spec: &str,
) -> Result<BTreeSet<Toolset>, ConfigError> {
    let entries: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if entries.is_empty() {
        return Ok(base);
    }

    let adjusting = entries[0].starts_with(['+', '-']);
    if entries
        .iter()
        .any(|e| e.starts_with(['+', '-']) != adjusting)
    {
        return Err(ConfigError::MixedToolsetSyntax);
    }

    let mut selected = if adjusting { base } else { BTreeSet::new() };
    for entry in entries {
        let (remove, name) = match entry.strip_prefix('-') {
            Some(name) => (true, name),
            None => (false, entry.strip_prefix('+').unwrap_or(entry)),
        };
        let toolset =
            Toolset::parse(name).ok_or_else(|| ConfigError::UnknownToolset(name.to_owned()))?;
        if remove {
            selected.remove(&toolset);
        } else {
            selected.insert(toolset);
        }
    }
    Ok(selected)
}

/// The decision procedure for "may this server offer this tool".
#[derive(Debug, Clone)]
pub struct Gate {
    toolsets: BTreeSet<Toolset>,
    max_tier: Tier,
    /// Surfaces whose backend is absent. Their tools are hidden however the
    /// toolsets were selected, because offering a tool that can only answer
    /// `backend_unavailable` helps nobody.
    unavailable: BTreeSet<Surface>,
}

impl Gate {
    /// Build a gate, refusing a configuration that would offer nothing.
    ///
    /// `all_tools` is the full metadata table; the emptiness check is made
    /// against it rather than against the toolset set, because a selection can
    /// be non-empty and still yield no tools once the tier is applied.
    pub fn new(
        toolsets: BTreeSet<Toolset>,
        max_tier: Tier,
        unavailable: BTreeSet<Surface>,
        all_tools: &[ToolMeta],
    ) -> Result<Self, ConfigError> {
        let gate = Self {
            toolsets,
            max_tier,
            unavailable,
        };
        if !all_tools.iter().any(|t| gate.permits(t)) {
            return Err(ConfigError::NoToolsEnabled);
        }
        Ok(gate)
    }

    /// A gate for tests and for the `tools` subcommand, which must be able to
    /// describe an empty selection rather than refuse to.
    pub fn unchecked(
        toolsets: BTreeSet<Toolset>,
        max_tier: Tier,
        unavailable: BTreeSet<Surface>,
    ) -> Self {
        Self {
            toolsets,
            max_tier,
            unavailable,
        }
    }

    pub fn permits(&self, tool: &ToolMeta) -> bool {
        self.toolsets.contains(&tool.toolset)
            && tool.tier <= self.max_tier
            && !self.unavailable.contains(&tool.surface())
    }

    /// Why a tool was not offered, phrased as the flag that would offer it.
    /// Used to build the hint on `not_permitted`.
    pub fn needs(&self, tool: &ToolMeta) -> String {
        if self.unavailable.contains(&tool.surface()) {
            return match tool.surface() {
                Surface::Local => "a working `tailscale` binary".to_owned(),
                Surface::Tailnet => "a control-plane credential".to_owned(),
            };
        }
        let mut needs = Vec::new();
        if !self.toolsets.contains(&tool.toolset) {
            needs.push(format!("`--toolsets +{}`", tool.toolset));
        }
        if tool.tier > self.max_tier
            && let Some(flag) = tool.tier.flag()
        {
            needs.push(format!("`{flag}`"));
        }
        if needs.is_empty() {
            // Not reachable through `permits`, but a caller may ask about a
            // tool it is permitted to run.
            return "no additional permission".to_owned();
        }
        needs.join(" and ")
    }

    pub fn toolsets(&self) -> &BTreeSet<Toolset> {
        &self.toolsets
    }

    /// Whether this session offers any tool on `surface`.
    ///
    /// Both halves matter and neither alone is the answer: a surface whose
    /// toolsets nobody selected offers nothing, and so does one that was
    /// selected but is not there — no binary, no credential, or an operator
    /// switch. [`permits`](Self::permits) asks the same two questions of one
    /// tool; this asks them of a whole surface, for the instructions, which
    /// have to describe what a session can actually do rather than what it was
    /// asked for.
    pub fn offers(&self, surface: Surface) -> bool {
        !self.unavailable.contains(&surface) && self.toolsets.iter().any(|t| t.surface() == surface)
    }

    pub fn max_tier(&self) -> Tier {
        self.max_tier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Toolset::*;

    fn tool(name: &'static str, toolset: Toolset, tier: Tier) -> ToolMeta {
        ToolMeta {
            name,
            toolset,
            tier,
            summary: "",
            self_severing: false,
            requires_confirmation: false,
            idempotent: true,
            varying_tier: false,
            min_version: None,
            platforms: None,
        }
    }

    #[test]
    fn presets_nest_from_minimal_to_full() {
        let minimal = Preset::Minimal.toolsets();
        let core = Preset::Core.toolsets();
        let full = Preset::Full.toolsets();
        assert!(minimal.is_subset(&core), "core should contain minimal");
        assert!(core.is_subset(&full), "full should contain core");
        assert_eq!(minimal.len(), 4);
        assert_eq!(core.len(), 13);
        assert_eq!(full.len(), Toolset::ALL.len() - 2);
    }

    #[test]
    fn full_withholds_the_two_opt_in_toolsets() {
        let full = Preset::Full.toolsets();
        assert!(!full.contains(&LocalDebug));
        assert!(!full.contains(&LocalPassthrough));
    }

    #[test]
    fn preset_names_round_trip() {
        for p in Preset::ALL {
            assert_eq!(Preset::parse(p.as_str()).expect("known preset"), *p);
        }
        assert!(matches!(
            Preset::parse("everything"),
            Err(ConfigError::UnknownPreset(_))
        ));
    }

    #[test]
    fn a_bare_list_replaces_the_preset() {
        let got = apply_toolset_modifiers(Preset::Core.toolsets(), "local-status,tailnet-dns")
            .expect("a valid list");
        assert_eq!(got, BTreeSet::from([LocalStatus, TailnetDns]));
    }

    #[test]
    fn a_prefixed_list_adjusts_the_preset() {
        let got = apply_toolset_modifiers(Preset::Minimal.toolsets(), "+local-debug,-tailnet-dns")
            .expect("a valid list");
        assert_eq!(
            got,
            BTreeSet::from([LocalStatus, TailnetDevices, TailnetPolicy, LocalDebug])
        );
    }

    #[test]
    fn a_mixed_list_is_refused_rather_than_guessed_at() {
        assert_eq!(
            apply_toolset_modifiers(Preset::Core.toolsets(), "local-status,-tailnet-dns"),
            Err(ConfigError::MixedToolsetSyntax)
        );
    }

    #[test]
    fn an_unknown_toolset_name_is_refused() {
        assert_eq!(
            apply_toolset_modifiers(Preset::Core.toolsets(), "+local-stats"),
            Err(ConfigError::UnknownToolset("local-stats".to_owned()))
        );
    }

    #[test]
    fn an_empty_spec_leaves_the_preset_alone() {
        let base = Preset::Core.toolsets();
        assert_eq!(
            apply_toolset_modifiers(base.clone(), "  ,, ").expect("empty is fine"),
            base
        );
    }

    #[test]
    fn the_gate_hides_by_toolset_tier_and_surface() {
        let gate = Gate::unchecked(
            BTreeSet::from([LocalStatus, TailnetDevices]),
            Tier::Write,
            BTreeSet::from([Surface::Tailnet]),
        );
        assert!(gate.permits(&tool("a", LocalStatus, Tier::Read)));
        assert!(gate.permits(&tool("b", LocalStatus, Tier::Write)));
        // Above the tier.
        assert!(!gate.permits(&tool("c", LocalStatus, Tier::Destructive)));
        // Toolset not selected.
        assert!(!gate.permits(&tool("d", LocalServe, Tier::Read)));
        // Surface unavailable, even though the toolset is selected.
        assert!(!gate.permits(&tool("e", TailnetDevices, Tier::Read)));
    }

    #[test]
    fn a_configuration_offering_nothing_is_a_startup_error() {
        let all = [tool("a", LocalDebug, Tier::Read)];
        assert_eq!(
            Gate::new(
                Preset::Minimal.toolsets(),
                Tier::Read,
                BTreeSet::new(),
                &all
            )
            .err(),
            Some(ConfigError::NoToolsEnabled)
        );
    }

    #[test]
    fn the_gate_explains_what_would_enable_a_tool() {
        let gate = Gate::unchecked(BTreeSet::from([LocalStatus]), Tier::Read, BTreeSet::new());
        assert_eq!(
            gate.needs(&tool("a", LocalServe, Tier::Destructive)),
            "`--toolsets +local-serve` and `--allow-destructive`"
        );
        assert_eq!(
            gate.needs(&tool("b", LocalStatus, Tier::Write)),
            "`--allow-write`"
        );

        let no_backend = Gate::unchecked(
            BTreeSet::from([TailnetDevices]),
            Tier::Read,
            BTreeSet::from([Surface::Tailnet]),
        );
        assert_eq!(
            no_backend.needs(&tool("c", TailnetDevices, Tier::Read)),
            "a control-plane credential"
        );
    }
}
