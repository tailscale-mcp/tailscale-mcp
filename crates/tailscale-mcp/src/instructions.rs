//! What the server tells a model about itself.
//!
//! Written per-server rather than as a constant, because the useful half is
//! what *this* server can do: which surfaces are live, which tier is permitted,
//! and therefore which of the model's likely next moves will not work. A model
//! that has been told the tailnet surface is off will stop proposing tailnet
//! tools; one that has to discover it by being refused will not.

use std::fmt::Write as _;

use crate::context::ToolContext;
use crate::gating::Gate;
use crate::meta::{Surface, Tier, Toolset};

/// Compose the instructions for a server in this configuration.
pub fn render(gate: &Gate, ctx: &ToolContext) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str(
        "This server drives Tailscale through two separate surfaces.\n\n\
         Tools named `tailscale_*` act on the node this server runs on, through its \
         command-line interface. They can only ever affect that one machine.\n\n\
         Tools named `tailnet_*` act on the whole tailnet through the control-plane API. \
         They affect every device and user in it, including this node.\n\n",
    );

    // What the session can actually do, not what it was asked for. A surface
    // whose toolsets were selected but whose backend is absent offers nothing,
    // and telling a model otherwise is worse than telling it nothing: it will
    // keep proposing tools that are not in the listing.
    let local = gate.offers(Surface::Local);
    let tailnet = gate.offers(Surface::Tailnet);
    match (local, tailnet) {
        (true, true) => {}
        (true, false) => out.push_str(
            "The tailnet surface is not available in this session, so no `tailnet_*` tool is \
             offered. Questions about other devices, users, keys or policy cannot be answered \
             here.\n\n",
        ),
        (false, true) => out.push_str(
            "The local surface is not available in this session, so no `tailscale_*` tool is \
             offered. Nothing can be read from or changed on this machine directly.\n\n",
        ),
        (false, false) => out.push_str("No tools are available in this session.\n\n"),
    }

    let _ = write!(out, "Permitted tier: {}. ", describe_tier(gate.max_tier()));
    match gate.max_tier() {
        Tier::Read => out.push_str(
            "Only tools that change nothing are offered. Tools that would change or remove \
             something are hidden, not merely refused, so a tool you cannot see does not exist \
             for this session. Do not describe a change as done; say what would be needed.\n\n",
        ),
        Tier::Write => out.push_str(
            "Tools that change configuration are offered. Tools that remove something, or \
             expose it to the public internet, are hidden.\n\n",
        ),
        Tier::Destructive => out.push_str(
            "Every tier is offered, including tools that delete, revoke, or publish to the \
             public internet. Prefer reading before writing, and say what a call will do before \
             making it.\n\n",
        ),
    }

    if gate.toolsets().contains(&Toolset::LocalPassthrough) {
        // Without this the two halves of what the session is told contradict
        // each other: the tier paragraph says which tools are offered, and
        // `tailscale_run` is annotated destructive at every tier because its
        // arguments decide what it does and nobody has seen them yet.
        out.push_str(
            "`tailscale_run` is the exception to the paragraph above. It runs a `tailscale` \
             subcommand no other tool covers, so what it does is decided by its arguments \
             rather than by the tool, and it is annotated as destructive for that reason \
             alone. It is still held to the permitted tier, command by command, and it \
             refuses outright the commands this server never runs. Prefer a typed tool \
             wherever one exists.\n\n",
        );
    }

    out.push_str(
        "Some tools take a `confirm` argument. They are the ones that cannot be undone, or that \
         can cut this server off from the tailnet it is managing. They refuse to run without it. \
         Set it only when the person you are working for has asked for that specific action; it \
         is not a formality to be filled in.\n\n",
    );

    out.push_str(
        "Identifiers: a device can be named by its node ID, one of its Tailscale IP addresses, or \
         its MagicDNS name. Where a tailnet must be named, `-` means the tailnet the credential \
         belongs to and is almost always right.\n\n",
    );

    if let Some(version) = ctx.cli_version {
        let _ = writeln!(
            out,
            "The local `tailscale` binary reports version {version}."
        );
    }
    if let Some(name) = ctx.identity.last_known().dns_name.as_deref() {
        let _ = writeln!(out, "This node is {}.", name.trim_end_matches('.'));
    }

    let _ = write!(
        out,
        "\nToolsets offered: {}.",
        gate.offered_toolsets()
            .map(|toolset| toolset.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    out
}

const fn describe_tier(tier: Tier) -> &'static str {
    match tier {
        Tier::Read => "read",
        Tier::Write => "read and write",
        Tier::Destructive => "read, write and destructive",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use tailscale_cli::Unavailable;

    use super::*;
    use crate::context::{Identity, PathPolicy, SelfIdentity};
    use crate::error::Redactor;
    use crate::gating::Preset;
    use crate::meta::Toolset;
    use crate::version::Version;

    fn context() -> ToolContext {
        ToolContext {
            local: Arc::new(Unavailable::default()),
            tailnet: None,
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity {
                dns_name: Some("workstation.example-tailnet.ts.net.".to_owned()),
                ..SelfIdentity::default()
            }
            .into(),
            cli_version: Some(Version::new(1, 102, 2)),
            paths: PathPolicy::default(),
            max_tier: Tier::Destructive,
        }
    }

    fn gate(toolsets: BTreeSet<Toolset>, tier: Tier) -> Gate {
        Gate::unchecked(toolsets, tier, BTreeSet::new())
    }

    #[test]
    fn both_surfaces_are_explained() {
        let text = render(&gate(Preset::Core.toolsets(), Tier::Read), &context());
        assert!(text.contains("tailscale_*"), "{text}");
        assert!(text.contains("tailnet_*"), "{text}");
    }

    #[test]
    fn a_missing_surface_is_stated_rather_than_left_to_be_discovered() {
        let local_only: BTreeSet<Toolset> = BTreeSet::from([Toolset::LocalStatus]);
        let text = render(&gate(local_only, Tier::Read), &context());
        assert!(text.contains("tailnet surface is not available"), "{text}");

        let tailnet_only: BTreeSet<Toolset> = BTreeSet::from([Toolset::TailnetDevices]);
        let text = render(&gate(tailnet_only, Tier::Read), &context());
        assert!(text.contains("local surface is not available"), "{text}");
    }

    #[test]
    fn a_surface_that_was_asked_for_and_is_not_there_is_stated_too() {
        // The case a session actually meets: both surfaces selected, and the
        // control plane has no credential. Reading the selection alone said
        // the tailnet surface was present while every tailnet tool was hidden,
        // which is the one thing these instructions exist to prevent.
        let both = BTreeSet::from([Toolset::LocalStatus, Toolset::TailnetDevices]);
        let no_control_plane =
            Gate::unchecked(both.clone(), Tier::Read, BTreeSet::from([Surface::Tailnet]));
        let text = render(&no_control_plane, &context());
        assert!(text.contains("tailnet surface is not available"), "{text}");
        assert!(!text.contains("local surface is not available"), "{text}");

        let no_cli = Gate::unchecked(both, Tier::Read, BTreeSet::from([Surface::Local]));
        let text = render(&no_cli, &context());
        assert!(text.contains("local surface is not available"), "{text}");
    }

    #[test]
    fn the_tier_is_stated_and_hiding_is_explained() {
        let text = render(&gate(Preset::Core.toolsets(), Tier::Read), &context());
        assert!(text.contains("Permitted tier: read."), "{text}");
        assert!(text.contains("hidden, not merely refused"), "{text}");

        let text = render(
            &gate(Preset::Core.toolsets(), Tier::Destructive),
            &context(),
        );
        assert!(text.contains("read, write and destructive"), "{text}");
    }

    #[test]
    fn the_passthrough_is_explained_only_where_it_is_offered() {
        // Its annotations say destructive at every tier, which contradicts the
        // tier paragraph unless the session is told why.
        let text = render(&gate(Preset::Core.toolsets(), Tier::Read), &context());
        assert!(!text.contains("tailscale_run"), "{text}");

        let mut with_passthrough = Preset::Core.toolsets();
        with_passthrough.insert(Toolset::LocalPassthrough);
        let text = render(&gate(with_passthrough, Tier::Read), &context());
        assert!(text.contains("`tailscale_run` is the exception"), "{text}");
        assert!(text.contains("held to the permitted tier"), "{text}");
    }

    #[test]
    fn confirmation_is_explained_as_intent_rather_than_ceremony() {
        let text = render(&gate(Preset::Core.toolsets(), Tier::Read), &context());
        assert!(text.contains("`confirm`"), "{text}");
        assert!(text.contains("not a formality"), "{text}");
    }

    #[test]
    fn what_was_learned_at_startup_is_passed_on() {
        let text = render(&gate(Preset::Core.toolsets(), Tier::Read), &context());
        assert!(text.contains("1.102.2"), "{text}");
        assert!(
            text.contains("This node is workstation.example-tailnet.ts.net.\n"),
            "{text}"
        );
        assert!(
            !text.contains("ts.net.."),
            "the trailing dot of a fully-qualified name should not be doubled: {text}"
        );
    }

    #[test]
    fn nothing_is_claimed_that_was_not_learned() {
        let bare = ToolContext {
            identity: Identity::default(),
            cli_version: None,
            paths: PathPolicy::default(),
            ..context()
        };
        let text = render(&gate(Preset::Core.toolsets(), Tier::Read), &bare);
        assert!(!text.contains("reports version"), "{text}");
        assert!(!text.contains("This node is"), "{text}");
    }

    #[test]
    fn the_toolsets_on_offer_are_listed() {
        let text = render(
            &gate(
                BTreeSet::from([Toolset::LocalStatus, Toolset::TailnetDns]),
                Tier::Read,
            ),
            &context(),
        );
        assert!(text.contains("local-status, tailnet-dns"), "{text}");
    }
}
