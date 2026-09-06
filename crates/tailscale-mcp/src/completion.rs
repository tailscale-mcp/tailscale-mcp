//! `completion/complete`: values for the slots whose set the server can know.
//!
//! The protocol completes two things and only two: a prompt's argument
//! (`ref/prompt`) and a variable in a resource template (`ref/resource`).
//! **Tool arguments are not completable** — there is no reference type for
//! one — so this cannot help the hundred-odd tools that take a device. What it
//! can do is the four slots the prompts and the one template expose, and the
//! device slot is the one that carries its weight: the template asks for the
//! same identifier the tools do, and a caller who can pick a device from a
//! list is a caller who never has to guess at a node id.
//!
//! Three properties hold everywhere below, and each is a decision rather than
//! an accident:
//!
//! - **It never fails.** Every source can be unreachable — no credential, a
//!   node that is not running, a control plane answering 429 — and a completion
//!   is an autocomplete popup, not an operation. An empty list is what a
//!   client can render; an error is what it has to explain. What went wrong
//!   goes to the log instead.
//! - **Every value it offers resolves.** The device slot offers full MagicDNS
//!   names, not hostnames, because a hostname can name two machines and
//!   resolution refuses those. Completing to a value the next call would
//!   reject is worse than completing to nothing.
//! - **It is rate limited.** The specification says servers SHOULD limit this
//!   method and its security section says MUST; a keystroke-driven method
//!   reaching a rate-limited control plane is the reason why.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use rmcp::model::{CompleteRequestParams, CompleteResult, CompletionInfo, Reference};

use crate::context::ToolContext;
use crate::gating::Gate;
use crate::meta::Surface;

/// The device resource template, whose one variable is a device.
pub const DEVICE_TEMPLATE: &str = "tailnet://device/{device_id}";

/// A slot this server can offer values for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// `tailnet://device/{device_id}` — every device in the tailnet.
    Device,
    /// `diagnose_connectivity`'s `peer` — every peer this node can see.
    Peer,
    /// `audit_tailnet_access`'s `subject` — the users and tags of the tailnet.
    Subject,
}

/// Which slot a request names, if any.
///
/// A reference this server does not serve, or an argument name that is not the
/// one the prompt declares, resolves to nothing and is answered with an empty
/// list. That includes `review_policy_change`'s `goal`, which is deliberate:
/// the argument is a sentence saying what a policy change is meant to achieve,
/// and there is no set of those to draw from. A slot that returned guesses
/// there would be inventing intent.
fn slot_for(reference: &Reference, argument: &str) -> Option<Slot> {
    match reference {
        Reference::Resource(template) if template.uri == DEVICE_TEMPLATE => {
            (argument == "device_id").then_some(Slot::Device)
        }
        Reference::Prompt(prompt) => match (prompt.name.as_str(), argument) {
            ("diagnose_connectivity", "peer") => Some(Slot::Peer),
            ("audit_tailnet_access", "subject") => Some(Slot::Subject),
            _ => None,
        },
        Reference::Resource(_) => None,
        _ => None,
    }
}

impl Slot {
    /// The surface the slot's values come from, and so the one whose absence
    /// makes it empty. Asked before the source is, so that a session without a
    /// credential does not attempt a request it knows will fail.
    const fn surface(self) -> Surface {
        match self {
            Self::Device | Self::Subject => Surface::Tailnet,
            Self::Peer => Surface::Local,
        }
    }
}

/// One completable thing: the value offered, and everything it answers to.
///
/// The distinction is the point. A device is found by its hostname or its
/// address but offered as its MagicDNS name, because what a client inserts is
/// exactly the string in `values` — the protocol has no display label — and
/// the string that goes back to the server has to be one the server accepts.
#[derive(Debug)]
struct Candidate {
    value: String,
    /// Lowercased, including the value itself.
    known_as: Vec<String>,
}

impl Candidate {
    fn new(value: impl Into<String>, also: impl IntoIterator<Item = String>) -> Self {
        let value = value.into();
        let mut known_as = vec![normalise(&value)];
        known_as.extend(
            also.into_iter()
                .map(|other| normalise(&other))
                .filter(|other| !other.is_empty()),
        );
        known_as.sort_unstable();
        known_as.dedup();
        Self { value, known_as }
    }

    /// How well the candidate answers to what has been typed, smaller being
    /// better: an exact match, then something starting with it, then something
    /// merely containing it. Substring rather than prefix because the names
    /// worth completing are compound — someone typing `laptop` is looking for
    /// `alices-laptop`, and a prefix match would find nothing.
    fn rank(&self, typed: &str) -> Option<u8> {
        if typed.is_empty() {
            return Some(2);
        }
        self.known_as
            .iter()
            .filter_map(|known| {
                if known == typed {
                    Some(0)
                } else if known.starts_with(typed) {
                    Some(1)
                } else if known.contains(typed) {
                    Some(2)
                } else {
                    None
                }
            })
            .min()
    }
}

/// One spelling of a name, for comparing against another.
///
/// Case folded, and the root label taken off: `DNSName` is fully qualified and
/// carries a trailing dot, which is a thing nobody types but a thing anybody
/// might paste.
fn normalise(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// How often a session may ask.
///
/// A token bucket rather than a fixed window: completion arrives in bursts,
/// one per keystroke, and a window that refused the tail of every burst would
/// be worse than useless. Twenty in reserve absorbs a typed word; the refill
/// is what a sustained hold gets.
#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// The state a session keeps: the bucket, and nothing else. The values
/// themselves are cached where they are read from — the device listing on
/// [`ToolContext`], the rest not at all, being one command or one request.
#[derive(Clone, Debug)]
pub struct Limiter {
    bucket: Arc<Mutex<Bucket>>,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    /// Requests held in reserve, and the sustained rate in requests a second.
    const BURST: f64 = 20.0;
    const PER_SECOND: f64 = 20.0;

    pub fn new() -> Self {
        Self {
            bucket: Arc::new(Mutex::new(Bucket {
                tokens: Self::BURST,
                last: Instant::now(),
            })),
        }
    }

    /// Take a token, or say there was none.
    ///
    /// A poisoned lock answers yes. The alternative is a session whose
    /// completion is permanently off because a request panicked once, and this
    /// mutex guards two numbers.
    fn allow(&self) -> bool {
        let Ok(mut bucket) = self.bucket.lock() else {
            return true;
        };
        let now = Instant::now();
        let earned = now.duration_since(bucket.last).as_secs_f64() * Self::PER_SECOND;
        bucket.tokens = (bucket.tokens + earned).min(Self::BURST);
        bucket.last = now;
        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

/// Answer a `completion/complete`.
///
/// Infallible by construction: the return type has no error arm, because every
/// way this can go wrong is a way that should still render an empty popup.
pub async fn complete(
    ctx: &ToolContext,
    gate: &Gate,
    limiter: &Limiter,
    request: &CompleteRequestParams,
) -> CompleteResult {
    let Some(slot) = slot_for(&request.r#ref, &request.argument.name) else {
        return nothing();
    };
    if !gate.offers(slot.surface()) {
        return nothing();
    }
    if !limiter.allow() {
        tracing::debug!(
            slot = ?slot,
            "completion refused: this session is asking faster than the limit"
        );
        return nothing();
    }

    let candidates = match gather(ctx, slot).await {
        Ok(candidates) => candidates,
        Err(why) => {
            // Logged, not returned: see the module note. `why` is already a
            // tool error, so it has been through the redactor.
            tracing::debug!(slot = ?slot, %why, "completion found nothing to offer");
            return nothing();
        }
    };
    answer(ctx, &candidates, &request.argument.value)
}

/// Rank, order and cut, and say honestly how much was left out.
fn answer(ctx: &ToolContext, candidates: &[Candidate], typed: &str) -> CompleteResult {
    // The same normalisation the names get, so that a caller who pasted a
    // fully qualified name out of `tailscale status` — root label and all —
    // is matched rather than told there is nothing.
    let typed = normalise(typed);
    let mut matched: Vec<(u8, &str)> = candidates
        .iter()
        .filter_map(|candidate| Some((candidate.rank(&typed)?, candidate.value.as_str())))
        .collect();
    // Rank first, then alphabetically, so that the order is the same every
    // time the same thing is typed. A popup that reshuffles between keystrokes
    // is one nobody can click.
    matched.sort_unstable();

    let total = matched.len();
    let values: Vec<String> = matched
        .into_iter()
        .take(CompletionInfo::MAX_VALUES)
        .map(|(_, value)| ctx.redactor.apply(value).into_owned())
        .collect();
    let has_more = total > values.len();
    let mut completion = CompletionInfo::new(values).unwrap_or_default();
    completion.total = u32::try_from(total).ok();
    completion.has_more = Some(has_more);
    CompleteResult::new(completion)
}

fn nothing() -> CompleteResult {
    CompleteResult::new(CompletionInfo::default())
}

/// Everything the slot could be, unranked.
async fn gather(ctx: &ToolContext, slot: Slot) -> crate::error::ToolResult<Vec<Candidate>> {
    match slot {
        Slot::Device => devices(ctx).await,
        Slot::Peer => peers(ctx).await,
        Slot::Subject => subjects(ctx).await,
    }
}

/// Every device, offered as its MagicDNS name and found by any of them.
///
/// Shares the listing the tools resolve against, so a session that has just
/// named a device completes without a second request.
async fn devices(ctx: &ToolContext) -> crate::error::ToolResult<Vec<Candidate>> {
    Ok(ctx
        .tailnet_devices()
        .await?
        .iter()
        .filter(|device| !device.name.is_empty())
        .map(|device| {
            let mut also = vec![
                device.hostname.clone(),
                device.short_name().to_owned(),
                device.node_id.clone(),
            ];
            also.extend(device.addresses.iter().cloned());
            Candidate::new(device.name.clone(), also)
        })
        .collect())
}

/// Every peer this node can see, offered by the name a person would type.
///
/// Offline peers are included, and that is the case the prompt exists for:
/// `diagnose_connectivity` is asked about a peer that cannot be reached, so a
/// list that dropped the unreachable ones would omit every useful answer.
/// This node is excluded, for the same reason — it can always reach itself.
async fn peers(ctx: &ToolContext) -> crate::error::ToolResult<Vec<Candidate>> {
    // The same reading of `status` that startup does, and the same tolerance:
    // a node that is not running answers nothing rather than an error, which
    // is what this wants anyway.
    let Some(status) = crate::cli::status_document(&*ctx.local).await else {
        return Ok(Vec::new());
    };
    let Some(peers) = status["Peer"].as_object() else {
        return Ok(Vec::new());
    };
    Ok(peers.values().filter_map(peer_candidate).collect())
}

/// One peer, as something completable — or nothing, for a peer with no name
/// at all.
fn peer_candidate(peer: &serde_json::Value) -> Option<Candidate> {
    let dns = peer["DNSName"].as_str().unwrap_or_default();
    let hostname = peer["HostName"].as_str().unwrap_or_default();
    // A peer with no MagicDNS name has an empty `DNSName`, in which case its
    // hostname is the only name it has. Otherwise the short form is what a
    // person types, and what `tailscale ping` is usually given.
    let short = dns.split('.').next().unwrap_or_default();
    let value = if short.is_empty() { hostname } else { short };
    if value.is_empty() {
        return None;
    }
    let mut also = vec![hostname.to_owned(), dns.to_owned()];
    if let Some(addresses) = peer["TailscaleIPs"].as_array() {
        also.extend(
            addresses
                .iter()
                .filter_map(|address| Some(address.as_str()?.to_owned())),
        );
    }
    Some(Candidate::new(value, also))
}

/// Who and what an audit can be narrowed to: the tailnet's users, and its tags.
///
/// Tags come from both ends, because neither alone is the answer. The policy
/// file's `tagOwners` declares every tag that may be applied, including ones
/// nothing wears yet; the device listing shows the tags actually in use,
/// including — on a tailnet whose policy this credential cannot read — the only
/// ones visible at all.
async fn subjects(ctx: &ToolContext) -> crate::error::ToolResult<Vec<Candidate>> {
    let client = ctx.tailnet()?;
    let mut found: Vec<String> = Vec::new();

    let users = client
        .get(client.tailnet_path(None, "/users"))
        .send_as::<serde_json::Value>()
        .await?;
    if let Some(listed) = users["users"].as_array() {
        found.extend(
            listed
                .iter()
                .filter_map(|user| Some(user["loginName"].as_str()?.to_owned())),
        );
    }

    // The policy file is the one source here that a read-only credential may
    // still be refused, and losing it should cost the tags it declares rather
    // than the users listed above.
    match client
        .get(client.tailnet_path(None, "/acl"))
        .send_as::<serde_json::Value>()
        .await
    {
        Ok(policy) => {
            if let Some(owners) = policy["tagOwners"].as_object() {
                found.extend(owners.keys().cloned());
            }
        }
        Err(why) => tracing::debug!(%why, "completion could not read the policy file for its tags"),
    }

    if let Ok(devices) = ctx.tailnet_devices().await {
        found.extend(
            devices
                .iter()
                .flat_map(|device| device.tags.iter().cloned()),
        );
    }

    found.sort_unstable();
    found.dedup();
    Ok(found
        .into_iter()
        .filter(|subject| !subject.is_empty())
        .map(|subject| Candidate::new(subject, []))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// The reserve is spent, and then refused.
    #[test]
    fn a_burst_is_allowed_up_to_the_reserve_and_then_refused() {
        let limiter = Limiter::new();
        for spent in 0..Limiter::BURST as usize {
            assert!(
                limiter.allow(),
                "the reserve is {} and this is request {}",
                Limiter::BURST,
                spent + 1
            );
        }
        assert!(
            !limiter.allow(),
            "the reserve is spent, so the next one waits"
        );
    }

    /// And comes back, at the rate it says.
    ///
    /// The clock is moved rather than waited on: a test that sleeps for a
    /// second to watch a bucket refill is a second every run pays for.
    #[test]
    fn the_reserve_refills() {
        let limiter = Limiter::new();
        while limiter.allow() {}
        {
            let mut bucket = limiter.bucket.lock().expect("the bucket");
            bucket.last -= Duration::from_secs(1);
        }
        assert!(
            limiter.allow(),
            "a second's worth of refill is {} requests",
            Limiter::PER_SECOND
        );
    }

    /// What a peer answers to, and what it is offered as.
    ///
    /// The two differ, and the difference is the point: `DNSName` arrives
    /// fully qualified, and a name carrying the root label is one no client
    /// should insert and no caller should have to type.
    #[test]
    fn a_peer_answers_to_every_spelling_and_is_offered_as_the_short_one() {
        let peer = serde_json::json!({
            "HostName": "laptop-1",
            "DNSName": "laptop.example-tailnet.ts.net.",
            "TailscaleIPs": ["100.64.0.2"]
        });
        let candidate = peer_candidate(&peer).expect("a named peer");
        assert_eq!(candidate.value, "laptop");
        assert_eq!(
            candidate.known_as,
            vec![
                "100.64.0.2",
                "laptop",
                "laptop-1",
                "laptop.example-tailnet.ts.net"
            ],
            "no spelling should carry the root label"
        );
        // Typed either way, because a caller may well have pasted it.
        for typed in [
            "laptop.example-tailnet.ts.net",
            "laptop.example-tailnet.ts.net.",
        ] {
            assert_eq!(
                candidate.rank(&normalise(typed)),
                Some(0),
                "`{typed}` names this peer exactly"
            );
        }
    }

    /// A peer with no name at all is not offered, there being nothing to offer.
    #[test]
    fn a_nameless_peer_is_not_offered() {
        assert!(peer_candidate(&serde_json::json!({"TailscaleIPs": ["100.64.0.9"]})).is_none());
    }

    /// And one with no MagicDNS name falls back to its hostname.
    #[test]
    fn a_peer_without_a_magicdns_name_is_offered_as_its_hostname() {
        let peer = serde_json::json!({"HostName": "printer", "DNSName": ""});
        assert_eq!(peer_candidate(&peer).expect("a peer").value, "printer");
    }

    /// Only the four slots, and only under the names the prompts declare.
    #[test]
    fn a_slot_is_recognised_by_its_reference_and_its_argument() {
        let device = Reference::for_resource(DEVICE_TEMPLATE);
        assert_eq!(slot_for(&device, "device_id"), Some(Slot::Device));
        assert_eq!(slot_for(&device, "id"), None);
        assert_eq!(
            slot_for(&Reference::for_prompt("diagnose_connectivity"), "peer"),
            Some(Slot::Peer)
        );
        assert_eq!(
            slot_for(&Reference::for_prompt("audit_tailnet_access"), "subject"),
            Some(Slot::Subject)
        );
        // Free text, and so nothing to draw on.
        assert_eq!(
            slot_for(&Reference::for_prompt("review_policy_change"), "goal"),
            None
        );
        assert_eq!(
            slot_for(
                &Reference::for_resource("tailnet://device/{other}"),
                "other"
            ),
            None
        );
    }
}
