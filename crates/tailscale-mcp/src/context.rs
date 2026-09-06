//! What a tool handler is given.
//!
//! Deliberately a plain struct of backends and limits rather than the server
//! itself: a handler that could reach the server could reach the router, and
//! then the tests would have to construct one to call anything.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tailscale_cli::LocalBackend;

use crate::error::Redactor;
use crate::meta::Tier;
use crate::version::Version;

/// Where a tool may write when the caller names a path on this machine.
///
/// In this release the tier is what confines host filesystem access: those
/// tools sit at the write tier and no higher, so a read-only session reaches
/// none of them. The allow-list is the mechanism meant to confine them further,
/// and it is here rather than in a comment so that switching it on is a matter
/// of populating one value: every tool that takes a path already asks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PathPolicy {
    /// Any path the caller names. What this release ships.
    #[default]
    Unrestricted,
    /// Only paths under one of these roots.
    Within(Vec<PathBuf>),
}

impl PathPolicy {
    /// Whether this policy would let a tool write to `path`.
    #[must_use]
    pub fn permits(&self, path: &Path) -> bool {
        match self {
            Self::Unrestricted => true,
            // A `..` walks out of whatever root it is checked against, so a
            // path carrying one is refused rather than resolved. Resolving
            // would have to touch the filesystem, and the path a caller names
            // here is usually one that does not exist yet.
            Self::Within(roots) => {
                !path.components().any(|c| c == Component::ParentDir)
                    && roots.iter().any(|root| path.starts_with(root))
            }
        }
    }
}

/// The identity of the node this server runs on, read from status at startup.
///
/// Used to recognise a control-plane operation aimed at ourselves, which is the
/// difference between deleting a device and severing the connection the caller
/// is talking over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelfIdentity {
    /// The node id, which is the identifier the control plane prefers:
    /// `status --json` reports it as `Self.ID`, and it looks like
    /// `n1234567CNTRL`.
    pub node_id: Option<String>,
    /// The numeric device id, which the control plane accepts for the same
    /// device.
    ///
    /// Not in status — the local node has never been told it — so it is
    /// resolved from the control plane when there is a credential, and stays
    /// `None` when there is not. A caller naming this node by its numeric id
    /// in a session with no credential is therefore not recognised, which is
    /// the same blind spot as a session with no local surface and is handled
    /// the same way: the call is treated as ordinary.
    pub numeric_id: Option<String>,
    /// Tailscale addresses assigned to this node.
    pub addresses: Vec<String>,
    /// The node's MagicDNS name.
    pub dns_name: Option<String>,
}

impl SelfIdentity {
    /// Whether `target` names this node. Matching is generous on purpose: a
    /// caller may refer to the node by any of the identifiers the API accepts,
    /// and a missed match is the expensive direction.
    pub fn matches(&self, target: &str) -> bool {
        let target = target.trim().trim_end_matches('.');
        if target.is_empty() {
            return false;
        }
        let same = |candidate: &Option<String>| {
            candidate
                .as_deref()
                .is_some_and(|c| c.trim_end_matches('.').eq_ignore_ascii_case(target))
        };
        same(&self.node_id)
            || same(&self.numeric_id)
            || same(&self.dns_name)
            || self.addresses.iter().any(|a| a == target)
            // A MagicDNS name may be given unqualified.
            || self
                .dns_name
                .as_deref()
                .and_then(|n| n.split('.').next())
                .is_some_and(|short| short.eq_ignore_ascii_case(target))
    }
}

/// How long a reading of who we are is trusted before status is asked again.
///
/// An address or a name can change under a running server — a node is renamed,
/// re-tagged, or moves onto a different address — and an identity that went
/// stale would stop recognising an operation aimed at this node, which is the
/// expensive direction to be wrong in. A minute is short enough that the window
/// is small and long enough that a burst of device calls does not become a
/// burst of `tailscale status` (Q87).
pub const IDENTITY_FRESH_FOR: Duration = Duration::from_secs(60);

/// The local node's identity, kept current.
///
/// Cheap to clone and shared between clones, so that one refresh serves every
/// handler rather than each holding its own idea of who we are.
#[derive(Clone, Default)]
pub struct Identity {
    held: Arc<Mutex<Held>>,
    /// Whether status can be asked again at all. False when the local surface
    /// is not offered, in which case there was nothing to read to begin with
    /// and re-reading nothing on a timer is only noise.
    live: bool,
}

#[derive(Debug, Default)]
struct Held {
    known: SelfIdentity,
    /// When `known` was read. `None` before the first reading.
    read_at: Option<Instant>,
}

impl Identity {
    /// An identity that was read from status and may be read again.
    pub fn probed(known: SelfIdentity) -> Self {
        Self {
            held: Arc::new(Mutex::new(Held {
                known,
                read_at: Some(Instant::now()),
            })),
            live: true,
        }
    }

    /// An identity fixed at what it was given: what tests and a session with
    /// no local surface get.
    pub fn fixed(known: SelfIdentity) -> Self {
        Self {
            held: Arc::new(Mutex::new(Held {
                known,
                read_at: None,
            })),
            live: false,
        }
    }

    /// The last reading, without asking for a new one.
    ///
    /// For the places that run once at startup and would gain nothing from a
    /// refresh, such as the instructions.
    pub fn last_known(&self) -> SelfIdentity {
        self.held
            .lock()
            .map(|held| held.known.clone())
            .unwrap_or_default()
    }

    /// Whether the last reading is old enough to be worth replacing.
    fn stale(&self) -> bool {
        self.live
            && self.held.lock().is_ok_and(|held| {
                held.read_at
                    .is_none_or(|at| at.elapsed() >= IDENTITY_FRESH_FOR)
            })
    }

    /// Store a fresh reading, keeping a numeric id the reading cannot carry.
    fn store(&self, mut known: SelfIdentity) {
        if let Ok(mut held) = self.held.lock() {
            if known.numeric_id.is_none() && held.known.node_id == known.node_id {
                known.numeric_id = held.known.numeric_id.take_if(|_| true);
            }
            held.known = known;
            held.read_at = Some(Instant::now());
        }
    }

    /// Store a numeric id resolved from the control plane.
    fn store_numeric(&self, numeric_id: String) {
        if let Ok(mut held) = self.held.lock() {
            held.known.numeric_id = Some(numeric_id);
        }
    }
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("known", &self.last_known())
            .field("live", &self.live)
            .finish()
    }
}

impl From<SelfIdentity> for Identity {
    fn from(known: SelfIdentity) -> Self {
        Self::fixed(known)
    }
}

/// One device, in the fields anything outside the device tools needs of it.
///
/// The control plane's device object is large and mostly irrelevant here: what
/// resolution and completion both want is the set of names a person might use
/// for a machine, and the identifier the API will actually take in exchange.
#[derive(Clone, Debug)]
pub struct Device {
    /// What the control plane accepts, and what resolution answers with.
    pub node_id: String,
    /// The MagicDNS name, fully qualified.
    pub name: String,
    /// The machine's own name for itself, which need not be unique.
    pub hostname: String,
    /// Every Tailscale address it answers on.
    pub addresses: Vec<String>,
}

impl Device {
    /// The label before the first dot of the MagicDNS name.
    ///
    /// `laptop.example-tailnet.ts.net` is what a listing prints and `laptop` is
    /// what a person types, so both have to name the same device.
    #[must_use]
    pub fn short_name(&self) -> &str {
        self.name.split('.').next().unwrap_or(&self.name)
    }

    /// Whether an already-lowercased value is one of this device's names.
    ///
    /// Exact against each field rather than a prefix: this decides which device
    /// a caller meant, and a value that merely begins like a name is not an
    /// answer to that. Completion matches loosely; addressing does not.
    #[must_use]
    pub fn answers_to(&self, lowercased: &str) -> bool {
        self.name.to_ascii_lowercase() == lowercased
            || self.hostname.to_ascii_lowercase() == lowercased
            || self.short_name().to_ascii_lowercase() == lowercased
            || self
                .addresses
                .iter()
                .any(|address| address.to_ascii_lowercase() == lowercased)
    }
}

/// The tailnet's device list, held briefly.
///
/// Two callers read it and both read it in bursts: resolving an identifier
/// happens once per device-addressing call, and completing one happens once per
/// keystroke. Ten seconds is longer than either burst and shorter than anyone's
/// patience for a device that has since been renamed.
///
/// The lock is never held across an await — the listing is fetched outside it
/// and stored after — so two callers racing simply both fetch, which costs a
/// request and no correctness.
/// What the cache holds: when it was read, and what it read.
type Listing = Arc<Mutex<Option<(Instant, Arc<[Device]>)>>>;

#[derive(Clone, Debug, Default)]
pub struct DeviceCache {
    held: Listing,
}

impl DeviceCache {
    const TTL: Duration = Duration::from_secs(10);

    fn fresh(&self) -> Option<Arc<[Device]>> {
        let held = self.held.lock().ok()?;
        let (at, devices) = held.as_ref()?;
        (at.elapsed() < Self::TTL).then(|| Arc::clone(devices))
    }

    fn put(&self, devices: &Arc<[Device]>) {
        if let Ok(mut held) = self.held.lock() {
            *held = Some((Instant::now(), Arc::clone(devices)));
        }
    }
}

/// Everything a handler may reach.
#[derive(Clone)]
pub struct ToolContext {
    /// The local node. Present even when the local surface is disabled, in
    /// which case it is a backend that reports the binary as missing.
    pub local: Arc<dyn LocalBackend>,
    /// The control plane, when there is a credential to reach it with.
    ///
    /// Deliberately not `pub`: a handler asks [`ToolContext::tailnet`] for it
    /// and gets either the client or the sentence explaining its absence, so
    /// that the ninety-three tailnet tools do not each find their own words
    /// for the same missing credential.
    pub(crate) tailnet: Option<tailscale_rest::Client>,
    /// Removes secrets from anything on its way out.
    pub redactor: Redactor,
    /// The size above which a result is refused rather than truncated.
    pub max_result_bytes: usize,
    /// Who we are on the tailnet, when we could find out.
    pub identity: Identity,
    /// The version the local CLI reports, when it could be read.
    pub cli_version: Option<Version>,
    /// Where the tools that take a path are allowed to write.
    pub paths: PathPolicy,
    /// The tailnet's device list, cached for a few seconds.
    ///
    /// The only mutable state a session holds. It exists because two features
    /// ask the same question repeatedly — which device did you mean, and which
    /// could you have meant — and neither should cost a request each time.
    pub devices: DeviceCache,
    /// The most dangerous tier this session permits.
    ///
    /// The gate is what normally applies this, before a handler is reached, so
    /// no typed tool has to look at it. The passthrough does: its row carries a
    /// floor rather than its real tier, so it is the one tool that has to make
    /// the same decision the gate makes, against the command it was given.
    pub max_tier: Tier,
}

impl ToolContext {
    /// The tailnet's devices, from the cache when it is warm enough.
    ///
    /// The error is the one the caller would have got anyway: without a
    /// credential this is the missing-credential sentence, and a control-plane
    /// failure is reported as itself rather than as an absent device.
    pub async fn tailnet_devices(&self) -> crate::error::ToolResult<Arc<[Device]>> {
        if let Some(warm) = self.devices.fresh() {
            return Ok(warm);
        }
        let client = self.tailnet()?;
        let answer = client
            .get(client.tailnet_path(None, "/devices"))
            .send_as::<serde_json::Value>()
            .await?;
        let devices: Arc<[Device]> = answer["devices"]
            .as_array()
            .map(|listed| {
                listed
                    .iter()
                    .filter_map(|device| {
                        Some(Device {
                            node_id: device["nodeId"].as_str()?.to_owned(),
                            name: device["name"].as_str().unwrap_or_default().to_owned(),
                            hostname: device["hostname"].as_str().unwrap_or_default().to_owned(),
                            addresses: device["addresses"]
                                .as_array()
                                .map(|addresses| {
                                    addresses
                                        .iter()
                                        .filter_map(|address| Some(address.as_str()?.to_owned()))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_else(|| Vec::new().into());
        self.devices.put(&devices);
        Ok(devices)
    }

    /// Whether `target` names the node this server runs on.
    ///
    /// Two sources, because the control plane accepts two identifiers for the
    /// same device and the local node only knows one of them. Status gives the
    /// node id, the addresses and the name, and is re-read as it ages. The
    /// numeric id has to be asked of the control plane — and is, only when the
    /// answer could turn on it: a target that is not all digits is not a
    /// numeric id, so the overwhelming majority of calls cost nothing extra,
    /// and the one that does costs one request per process (Q87).
    pub async fn names_us(&self, target: &str) -> bool {
        if self.identity.stale() {
            self.identity
                .store(crate::cli::probe_identity(self.local.as_ref()).await);
        }
        let known = self.identity.last_known();
        if known.matches(target) {
            return true;
        }

        let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if known.numeric_id.is_some() || !numeric(target.trim()) {
            return false;
        }
        let (Some(node_id), Some(client)) = (&known.node_id, self.tailnet.as_ref()) else {
            return false;
        };
        // A device's numeric id does not change while its node id stays the
        // same, so this is asked once and then remembered.
        let Ok(path) = crate::tools::tailnet_devices::device_path(node_id, "") else {
            return false;
        };
        let Ok(device) = client.get(path).send_as::<serde_json::Value>().await else {
            return false;
        };
        let Some(numeric_id) = device["id"].as_str() else {
            return false;
        };
        self.identity.store_numeric(numeric_id.to_owned());
        self.identity.last_known().matches(target)
    }

    /// The control plane, or the reason there is none.
    pub fn tailnet(&self) -> crate::error::ToolResult<&tailscale_rest::Client> {
        self.tailnet.as_ref().ok_or_else(|| {
            crate::error::ToolError::backend_unavailable(
                "the tailnet surface",
                "no control-plane credential was found; set TAILSCALE_API_KEY, or \
                 TAILSCALE_OAUTH_CLIENT_ID and TAILSCALE_OAUTH_CLIENT_SECRET",
            )
        })
    }
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No credential-bearing field is printed, and none should be added.
        f.debug_struct("ToolContext")
            .field("tailnet", &self.tailnet.is_some())
            .field("max_result_bytes", &self.max_result_bytes)
            .field("identity", &self.identity)
            .field("cli_version", &self.cli_version)
            .field("max_tier", &self.max_tier)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> SelfIdentity {
        SelfIdentity {
            node_id: Some("n1234567CNTRL".to_owned()),
            numeric_id: Some("92960230385".to_owned()),
            addresses: vec!["100.64.0.1".to_owned(), "fd7a::1".to_owned()],
            dns_name: Some("workstation.example-tailnet.ts.net.".to_owned()),
        }
    }

    #[test]
    fn a_node_is_recognised_by_any_name_the_api_accepts() {
        let id = identity();
        for name in [
            "n1234567CNTRL",
            // Both identifier forms the control plane accepts for a device.
            "92960230385",
            "100.64.0.1",
            "fd7a::1",
            "workstation.example-tailnet.ts.net",
            "workstation.example-tailnet.ts.net.",
            "workstation",
            "WORKSTATION",
        ] {
            assert!(id.matches(name), "{name} should name this node");
        }
    }

    #[test]
    fn another_node_is_not() {
        let id = identity();
        for name in [
            "n7654321CNTRL",
            "92960230386",
            // A public key is not an identifier the control plane accepts, so
            // matching one would be a claim this server cannot cash.
            "nodekey:1111111111111111111111111111111111111111111111111111111111111111",
            "100.64.0.2",
            "laptop.example-tailnet.ts.net",
            "laptop",
            "",
            "   ",
        ] {
            assert!(!id.matches(name), "{name} should not name this node");
        }
    }

    #[test]
    fn an_unknown_identity_matches_nothing() {
        assert!(!SelfIdentity::default().matches("anything"));
    }

    #[test]
    fn a_context_with_no_credential_names_the_variables_that_would_give_it_one() {
        // Reachable only when a session has the tailnet surface but no client:
        // startup switches the surface off when there is no credential, so the
        // tools are not offered and no call arrives (`tailnet_surface.rs`
        // asserts that absence). What is left is a credential that stops being
        // usable mid-session, and this is the sentence such a call gets. It is
        // asserted here because there is nowhere else it can be seen.
        let ctx = crate::testing::context(std::sync::Arc::new(crate::testing::StubBackend::ok("")));
        let error = ctx.tailnet().expect_err("no credential was configured");
        let reported = serde_json::to_value(&error).expect("reportable");
        assert_eq!(reported["code"], serde_json::json!("backend_unavailable"));
        let message = reported["message"].as_str().expect("a message");
        for variable in [
            "TAILSCALE_API_KEY",
            "TAILSCALE_OAUTH_CLIENT_ID",
            "TAILSCALE_OAUTH_CLIENT_SECRET",
        ] {
            assert!(message.contains(variable), "{message}");
        }
    }
}
