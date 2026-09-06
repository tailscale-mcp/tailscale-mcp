//! Nine resources and three prompts.
//!
//! **Two schemes, because there are two surfaces.** `tailscale://` is this
//! node as the local `tailscale` binary sees it; `tailnet://` is the tailnet
//! as the control plane describes it. A caller reading a URI knows which
//! backend answers it and therefore what a missing one means, which one scheme
//! would have hidden (Q85).
//!
//! **A resource is a read a caller did not have to spend a tool call on.**
//! Every one of them is something a Read-tier tool could also fetch, so none
//! carries a tier of its own: a resource is offered whenever its surface is,
//! and never otherwise. There are no subscriptions — `spec.md` puts them out
//! of scope — so nothing here is cached or invalidated; a read is a read.
//!
//! **The policy is the one that is not JSON.** It is HuJSON, and the comments
//! are the part a person wrote, so `tailnet://policy` is served
//! `application/hujson` as text rather than parsed into something smaller.
//!
//! **Nothing here answers with something a tool result would have redacted,
//! and nothing answers with more than one would have carried.** Each body goes
//! through the session's redactor on the way out, which is what `status
//! --json` needs: it carries key material for this node, and a resource is
//! exactly as public as a tool result. It is held to the session's result cap
//! for the same reason — `tailscale://status` and `tailscale_status` answer
//! with the same bytes, so a ceiling on one and not the other is a ceiling on
//! neither.

use std::sync::Arc;

use rmcp::model::{
    Prompt, PromptArgument, PromptMessage, ReadResourceResult, Resource, ResourceContents,
    ResourceTemplate, Role,
};
use serde_json::Value;
use tailscale_cli::Invocation;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::meta::Surface;

/// The media type everything but the policy answers with.
const JSON: &str = "application/json";
/// The policy file's own, which is not JSON: the comments are the point.
const HUJSON: &str = "application/hujson";

/// One resource: what it is called, what answers it, and which surface it
/// needs to be there.
pub struct ResourceEntry {
    /// The fixed URI, or the template for the one that takes an identifier.
    pub uri: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub mime_type: &'static str,
    pub surface: Surface,
    /// `true` for the one entry that is a template rather than a URI.
    pub templated: bool,
    read: fn(Arc<ToolContext>, String) -> tailscale_cli::BoxFuture<'static, ToolResult<String>>,
}

impl std::fmt::Debug for ResourceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceEntry")
            .field("uri", &self.uri)
            .field("surface", &self.surface)
            .finish_non_exhaustive()
    }
}

impl ResourceEntry {
    /// The resource as a client sees it in a listing.
    pub fn describe(&self) -> Resource {
        Resource::new(self.uri, self.name)
            .with_title(self.title)
            .with_description(self.description)
            .with_mime_type(self.mime_type)
    }

    /// The template as a client sees it in a template listing.
    pub fn describe_template(&self) -> ResourceTemplate {
        ResourceTemplate::new(self.uri, self.name)
            .with_title(self.title)
            .with_description(self.description)
            .with_mime_type(self.mime_type)
    }

    /// Whether `uri` is this entry, and the identifier it carried if so.
    ///
    /// A fixed URI matches itself. The template matches a prefix and yields
    /// what follows, which is then held to `path_segment` like any other
    /// identifier before it reaches a path.
    fn captures(&self, uri: &str) -> Option<String> {
        if !self.templated {
            return (uri == self.uri).then(String::new);
        }
        let prefix = self.uri.split_once('{')?.0;
        uri.strip_prefix(prefix)
            .filter(|rest| !rest.is_empty() && !rest.contains('/'))
            .map(str::to_owned)
    }
}

macro_rules! resources {
    ($(
        $uri:literal => $name:literal, $title:literal, $mime:expr, $surface:ident,
            templated: $templated:literal,
            $description:literal,
            $read:expr;
    )*) => {
        /// Every resource, in listing order.
        pub fn all() -> Vec<ResourceEntry> {
            vec![$(
                ResourceEntry {
                    uri: $uri,
                    name: $name,
                    title: $title,
                    description: $description,
                    mime_type: $mime,
                    surface: Surface::$surface,
                    templated: $templated,
                    read: |ctx, id| Box::pin($read(ctx, id)),
                },
            )*]
        }
    };
}

resources! {
    "tailscale://status" => "status", "Node status", JSON, Local,
        templated: false,
        "This node and the peers it knows about, as `tailscale status --json` \
         reports them: backend state, addresses, MagicDNS names and who is \
         online.",
        |ctx, _id| local_json(ctx, ["status", "--json"]);

    "tailscale://prefs" => "prefs", "Node preferences", JSON, Local,
        templated: false,
        "How this node is configured: routes it advertises, whether it is an \
         exit node, DNS and subnet-route acceptance, and the rest of what \
         `tailscale set` writes.",
        // `tailscale get --json`, not `debug prefs`: the latter prints this
        // node's private keys along with its preferences and is on
        // `local_debug::EXCLUDED` for that reason, which a resource must not
        // be a way around (Q89). `tailscale_prefs_get` reads the same
        // preferences from the same place.
        |ctx, _id| local_json(ctx, ["get", "--json"]);

    "tailscale://netcheck" => "netcheck", "Connectivity report", JSON, Local,
        templated: false,
        "What this node can reach: DERP latencies, whether UDP works, the \
         NAT mapping it sees and whether it has IPv6.",
        |ctx, _id| local_json(ctx, ["netcheck", "--format=json"]);

    "tailscale://lock" => "lock", "Tailnet lock status", JSON, Local,
        templated: false,
        "Whether tailnet lock is on for this tailnet, this node's own lock \
         key, and the signing nodes it trusts.",
        |ctx, _id| local_json(ctx, ["lock", "status", "--json"]);

    "tailnet://policy" => "policy", "Policy file", HUJSON, Tailnet,
        templated: false,
        "The tailnet policy file as written, comments and all. HuJSON, not \
         JSON: `tailnet_policy_get` with `format: \"json\"` parses it, and \
         loses the comments doing so.",
        |ctx, _id| policy(ctx);

    "tailnet://devices" => "devices", "Tailnet devices", JSON, Tailnet,
        templated: false,
        "Every device in the tailnet, as the control plane lists them.",
        |ctx, _id| tailnet_json(ctx, "/devices");

    "tailnet://device/{device_id}" => "device", "One device", JSON, Tailnet,
        templated: true,
        "One device by its node id (`n1234567CNTRL`) or its numeric id, as \
         `tailnet://devices` reports them.",
        device;

    "tailnet://dns" => "dns", "Tailnet DNS", JSON, Tailnet,
        templated: false,
        "The tailnet's whole DNS configuration: nameservers, split DNS, \
         search paths and MagicDNS.",
        |ctx, _id| tailnet_json(ctx, "/dns/configuration");

    "tailnet://settings" => "settings", "Tailnet settings", JSON, Tailnet,
        templated: false,
        "Tailnet-wide settings: device and user approval, key durations, \
         automatic updates and network flow logging.",
        |ctx, _id| tailnet_json(ctx, "/settings");
}

/// A local read, as the pretty-printed JSON the command produced.
async fn local_json<const N: usize>(ctx: Arc<ToolContext>, argv: [&str; N]) -> ToolResult<String> {
    let output = ctx
        .local
        .run(Invocation::read(argv.map(str::to_owned)))
        .await
        .map_err(|error| {
            ToolError::backend_unavailable("the `tailscale` command", &error.to_string())
        })?;
    if !output.success() {
        return Err(ToolError::cli_failed(
            "tailscale",
            output.exit_code,
            &output.stderr,
        ));
    }
    Ok(output.stdout_str().trim().to_owned())
}

/// A control-plane read of a tailnet-scoped path.
async fn tailnet_json(ctx: Arc<ToolContext>, rest: &str) -> ToolResult<String> {
    let client = ctx.tailnet()?;
    let answer = client
        .get(client.tailnet_path(None, rest))
        .send_as::<Value>()
        .await?;
    Ok(crate::tools::common::pretty(&answer))
}

async fn device(ctx: Arc<ToolContext>, device_id: String) -> ToolResult<String> {
    // The template accepts what the tools accept, which is the whole point of
    // the resolution: `tailnet://device/laptop` and `tailnet_device_get` are
    // two ways of asking the same question and should take the same answer.
    let resolved = crate::tools::tailnet_devices::resolve(&ctx, &device_id).await?;
    let client = ctx.tailnet()?;
    let answer = client
        .get(crate::tools::tailnet_devices::device_path(&resolved, "")?)
        .send_as::<Value>()
        .await?;
    Ok(crate::tools::common::pretty(&answer))
}

/// The policy file as text, which is what makes it the odd one out.
async fn policy(ctx: Arc<ToolContext>) -> ToolResult<String> {
    let client = ctx.tailnet()?;
    // Text, not JSON: HuJSON is not JSON to parse, and the comments are the
    // part a person wrote.
    let body = client
        .get(client.tailnet_path(None, "/acl"))
        .header("Accept", HUJSON)
        .send_text()
        .await?;
    Ok(body.text)
}

/// Read one resource, or say why it is not there.
///
/// The surface check comes first and says the same thing an absent tool says:
/// a resource whose backend is missing is not listed, so a client asking for
/// one by name is asking for something it was never offered.
pub async fn read(
    ctx: &Arc<ToolContext>,
    offers: impl Fn(Surface) -> bool,
    uri: &str,
) -> ToolResult<ReadResourceResult> {
    for entry in all() {
        let Some(id) = entry.captures(uri) else {
            continue;
        };
        if !offers(entry.surface) {
            return Err(ToolError::not_found(&format!(
                "the resource `{uri}`, because this server has no {} surface",
                entry.surface.as_str()
            )));
        }
        let body = (entry.read)(Arc::clone(ctx), id).await?;
        // Exactly as public as a tool result: `status --json` carries this
        // node's key material, and a resource is not a way around that.
        let body = ctx.redactor.apply(&body).into_owned();
        // And held to the same ceiling, for the same reason. Redaction is one
        // of the two things a tool result passes on the way out; a resource
        // that skipped the other would fetch, uncapped, the document the tool
        // beside it had just refused — `tailscale://status` and
        // `tailscale_status` answer with the same bytes. The default cap is a
        // mebibyte, so this changes nothing until an operator asks for a
        // smaller one, which is the operator who wanted the ceiling.
        if body.len() > ctx.max_result_bytes {
            return Err(ToolError::result_too_large(
                body.len(),
                ctx.max_result_bytes,
            ));
        }
        return Ok(ReadResourceResult::new(vec![
            ResourceContents::text(body, uri).with_mime_type(entry.mime_type),
        ]));
    }
    Err(ToolError::not_found(&format!("the resource `{uri}`")))
}

/// Which surfaces a session has, for the prompt steps that reach across both.
///
/// [`PromptEntry::surface`] decides whether a prompt is offered at all; this
/// is the finer question the ones that reach across have to ask. A numbered
/// procedure that tells the model to call `tailnet_device_list` in a session
/// with no credential is naming a tool that does not exist there, which is the
/// thing [`crate::gating::Gate::offers`] was written to stop the instructions
/// doing.
#[derive(Clone, Copy, Debug)]
pub struct Surfaces {
    local: bool,
    tailnet: bool,
}

impl Surfaces {
    /// From the same question the resource listing asks of every entry.
    pub fn new(offers: impl Fn(Surface) -> bool) -> Self {
        Self {
            local: offers(Surface::Local),
            tailnet: offers(Surface::Tailnet),
        }
    }

    pub const fn has(self, surface: Surface) -> bool {
        match surface {
            Surface::Local => self.local,
            Surface::Tailnet => self.tailnet,
        }
    }
}

/// One prompt: a name, an optional argument, and the guidance it expands to.
pub struct PromptEntry {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The one optional argument, and what it is for.
    pub argument: (&'static str, &'static str),
    /// The surface the prompt cannot work without, and so the one that decides
    /// whether it is listed at all — the rule [`read`] already applies to a
    /// resource.
    pub surface: Surface,
    expand: fn(Option<&str>, Surfaces) -> String,
}

impl std::fmt::Debug for PromptEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptEntry")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PromptEntry {
    pub fn describe(&self) -> Prompt {
        let (name, about) = self.argument;
        Prompt::new(
            self.name,
            Some(self.description),
            Some(vec![
                PromptArgument::new(name)
                    .with_description(about)
                    .with_required(false),
            ]),
        )
        .with_title(self.title)
    }

    pub fn expand(&self, argument: Option<&str>, surfaces: Surfaces) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            Role::User,
            (self.expand)(argument, surfaces),
        )]
    }
}

/// Every prompt, in listing order.
///
/// All three work under the read tier, which is why none of them tells the
/// model to write anything: validation and preview do not mutate, and a prompt
/// that ended in a write would be one a read-only session could not finish.
///
/// Tier is not the only way a step can be out of reach. Each prompt also names
/// the surface it cannot work without, so that a session missing that surface
/// is not offered a procedure whose every step it would refuse; and the one
/// prompt that reads from both drops the steps whose surface is absent.
pub fn prompts() -> Vec<PromptEntry> {
    vec![
        PromptEntry {
            name: "diagnose_connectivity",
            title: "Diagnose connectivity",
            description: "Work out why this node cannot reach something, using only reads.",
            argument: (
                "peer",
                "The peer that cannot be reached, by name or address.",
            ),
            // The question is why *this node* cannot reach something, and the
            // first three steps are what answers it. The control plane
            // corroborates; it does not stand in for the node.
            surface: Surface::Local,
            expand: |peer, surfaces| {
                let subject = match peer {
                    Some(peer) => format!("the peer `{peer}`"),
                    None => "the tailnet in general".to_owned(),
                };
                // Steps 4 and 5 are control-plane reads. A session with no
                // credential has neither tool, and a numbered procedure that
                // names them there sends the model at something it was never
                // offered — so the list ends at 3 and says so.
                let control_plane = if surfaces.has(Surface::Tailnet) {
                    "4. `tailnet_device_list` — does the control plane agree the peer exists, \
                        is it authorised, and has its key expired?\n\
                     5. `tailnet_policy_preview` — would the policy in force let these two \
                        talk?\n"
                } else {
                    ""
                };
                format!(
                    "Diagnose connectivity between this node and {subject}, using read-only \
                     tools only. Work outwards:\n\
                     \n\
                     1. `tailscale_status` — is the backend running, and is the peer known and \
                        online?\n\
                     2. `tailscale_netcheck` — can this node reach a DERP relay, does UDP work, \
                        and what NAT does it see?\n\
                     3. `tailscale_ping` — does traffic actually arrive, and does it go direct \
                        or over a relay?\n\
                     {control_plane}\
                     \n\
                     Report what you found at each step and name the first one that explains the \
                     failure. Do not change anything: every tool above is a read, and a fix is \
                     the operator's to approve."
                )
            },
        },
        PromptEntry {
            name: "review_policy_change",
            title: "Review a policy change",
            description: "Read, validate and preview a policy change before anyone writes it.",
            argument: (
                "goal",
                "What the change is meant to achieve, in a sentence.",
            ),
            surface: Surface::Tailnet,
            expand: |goal, _| {
                let purpose = match goal {
                    Some(goal) => format!("The change is meant to: {goal}\n\n"),
                    None => String::new(),
                };
                format!(
                    "{purpose}Review a change to the tailnet policy file. In this order, and \
                     without writing anything:\n\
                     \n\
                     1. `tailnet_policy_get` — read the policy as it stands, and keep the `etag` \
                        it answers with. Every later step is about *this* version.\n\
                     2. `tailnet_policy_validate` — send the proposed document and read back the \
                        warnings and errors. A document that does not validate is not a change \
                        worth discussing.\n\
                     3. `tailnet_policy_preview` — for each access the change is supposed to \
                        grant or remove, ask what the rule actually does. A rule that reads \
                        correctly and matches nothing is the commonest mistake.\n\
                     4. Say what the change does that the goal did not ask for. Widened tags, \
                        an `autogroup:member` where a group was meant, an `accept` that shadows \
                        a later rule.\n\
                     \n\
                     Then stop and report. Writing the policy is `tailnet_policy_set`, it needs \
                     the `etag` from step 1, and it is the operator's call — not this review's."
                )
            },
        },
        PromptEntry {
            name: "audit_tailnet_access",
            title: "Audit tailnet access",
            description: "Survey who and what can reach the tailnet, using only reads.",
            argument: (
                "subject",
                "A user, tag or device to audit rather than the whole tailnet.",
            ),
            surface: Surface::Tailnet,
            expand: |subject, _| {
                let scope = match subject {
                    Some(subject) => format!("Limit the audit to `{subject}`.\n\n"),
                    None => String::new(),
                };
                format!(
                    "{scope}Audit who can reach what in this tailnet, using read-only tools \
                     only:\n\
                     \n\
                     1. `tailnet_user_list` — who has an account, what role each holds, and who \
                        is suspended or waiting for approval.\n\
                     2. `tailnet_key_list` — which credentials exist, which are close to \
                        expiring, and which have capabilities wider than their purpose.\n\
                     3. `tailnet_device_list` — which devices are unauthorised, which have key \
                        expiry disabled, and which are tagged.\n\
                     4. `tailnet_policy_get`, then `tailnet_policy_preview` for the accesses \
                        that matter — what the rules grant, rather than what they appear to.\n\
                     5. `tailnet_settings_get` — is device approval on, is user approval on, and \
                        how long may a key live?\n\
                     \n\
                     Report by risk, worst first, and say for each what you read that shows it. \
                     Do not change anything."
                )
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_nine_resources_across_two_schemes_and_one_is_a_template() {
        let all = all();
        assert_eq!(all.len(), 9, "`spec.md` says nine");

        let templated: Vec<_> = all.iter().filter(|r| r.templated).collect();
        assert_eq!(templated.len(), 1, "and one template, addressed by device");
        assert!(templated[0].uri.contains("{device_id}"));

        for entry in &all {
            let scheme = match entry.surface {
                Surface::Local => "tailscale://",
                Surface::Tailnet => "tailnet://",
            };
            assert!(
                entry.uri.starts_with(scheme),
                "`{}` should say which backend answers it",
                entry.uri
            );
        }
    }

    #[test]
    fn the_policy_is_the_one_resource_that_is_not_json() {
        let all = all();
        let odd: Vec<_> = all.iter().filter(|r| r.mime_type != JSON).collect();
        assert_eq!(odd.len(), 1);
        assert_eq!(odd[0].uri, "tailnet://policy");
        assert_eq!(odd[0].mime_type, HUJSON);
    }

    #[test]
    fn the_template_captures_an_identifier_and_nothing_that_is_not_one() {
        let all = all();
        let device = all
            .iter()
            .find(|r| r.uri == "tailnet://device/{device_id}")
            .expect("declared");

        assert_eq!(
            device.captures("tailnet://device/n1111111CNTRL").as_deref(),
            Some("n1111111CNTRL")
        );
        // Not a device: no identifier, or one carrying a path.
        assert_eq!(device.captures("tailnet://device/"), None);
        assert_eq!(device.captures("tailnet://device/n1/routes"), None);
        assert_eq!(device.captures("tailnet://devices"), None);

        let devices = all
            .iter()
            .find(|r| r.uri == "tailnet://devices")
            .expect("declared");
        assert_eq!(devices.captures("tailnet://devices").as_deref(), Some(""));
        assert_eq!(devices.captures("tailnet://devices/n1"), None);
    }

    /// A session that has both surfaces, which is what most of these ask about.
    fn both() -> Surfaces {
        Surfaces::new(|_| true)
    }

    #[test]
    fn every_prompt_expands_with_and_without_its_argument() {
        for prompt in prompts() {
            let (name, _) = prompt.argument;
            let without = prompt.expand(None, both());
            let with = prompt.expand(Some("example"), both());
            assert_eq!(without.len(), 1);
            assert_ne!(
                format!("{with:?}"),
                format!("{without:?}"),
                "`{}`'s `{name}` should change what it expands to",
                prompt.name
            );
            assert!(
                format!("{with:?}").contains("example"),
                "`{}` should use the argument it was given",
                prompt.name
            );

            // The argument is declared optional, which is what lets a client
            // ask for the prompt without it.
            let described = prompt.describe();
            let arguments = described.arguments.expect("one argument");
            assert_eq!(arguments.len(), 1);
            assert_eq!(arguments[0].required, Some(false));
        }
    }

    #[test]
    fn the_policy_prompt_orders_read_validate_and_preview_before_any_write() {
        let prompts = prompts();
        let policy = prompts
            .iter()
            .find(|p| p.name == "review_policy_change")
            .expect("declared");
        let text = format!("{:?}", policy.expand(None, both()));

        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle} is named"))
        };
        assert!(at("tailnet_policy_get") < at("tailnet_policy_validate"));
        assert!(at("tailnet_policy_validate") < at("tailnet_policy_preview"));
        assert!(
            at("tailnet_policy_preview") < at("tailnet_policy_set"),
            "the write comes last, and only as the operator's call"
        );
    }

    #[test]
    fn no_prompt_asks_for_a_tool_that_needs_more_than_the_read_tier() {
        // All three have to work under the read tier, so a prompt naming a
        // write tool would be one a read-only session could not finish.
        let table = crate::tools::entries();
        for prompt in prompts() {
            let text = format!("{:?}", prompt.expand(Some("example"), both()));
            for entry in &table {
                if entry.meta.tier != crate::meta::Tier::Read && text.contains(entry.meta.name) {
                    // `tailnet_policy_set` is named as the thing *not* to do.
                    assert_eq!(
                        entry.meta.name, "tailnet_policy_set",
                        "`{}` names `{}`, which needs more than the read tier",
                        prompt.name, entry.meta.name
                    );
                }
            }
        }
    }

    /// The surface half of the question the test above asks about the tier.
    ///
    /// A prompt is listed when its own surface is there, so what it expands to
    /// may still name the *other* one. Every tool it names has to be on a
    /// surface the session actually has — otherwise the procedure sends the
    /// model at a tool that does not exist for it, which is the thing hiding
    /// a toolset was meant to prevent.
    #[test]
    fn no_prompt_names_a_tool_from_a_surface_the_session_lacks() {
        let table = crate::tools::entries();
        for present in [Surface::Local, Surface::Tailnet] {
            let only = Surfaces::new(|surface| surface == present);
            for prompt in prompts() {
                if prompt.surface != present {
                    continue;
                }
                let text = format!("{:?}", prompt.expand(Some("example"), only));
                for entry in &table {
                    let named = entry.meta.surface() != present && text.contains(entry.meta.name);
                    assert!(
                        !named || entry.meta.name == "tailnet_policy_set",
                        "with only the {} surface, `{}` still names `{}`",
                        present.as_str(),
                        prompt.name,
                        entry.meta.name
                    );
                }
            }
        }
    }

    /// And the listing rule itself: each prompt says which surface it needs.
    #[test]
    fn every_prompt_declares_the_surface_it_cannot_work_without() {
        let expected = [
            ("diagnose_connectivity", Surface::Local),
            ("review_policy_change", Surface::Tailnet),
            ("audit_tailnet_access", Surface::Tailnet),
        ];
        let declared: Vec<_> = prompts()
            .iter()
            .map(|prompt| (prompt.name, prompt.surface))
            .collect();
        assert_eq!(
            declared,
            expected.to_vec(),
            "a prompt changing surface changes which sessions are offered it"
        );
    }
}
