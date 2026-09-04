//! The tools that change the tailnet's trust root.
//!
//! Tailnet lock decides which nodes the tailnet will accept, independently of
//! the control plane: a node is admitted only once a key the tailnet
//! already trusts has signed it. These eight tools act through *this* node's
//! own tailnet-lock key, so what they can do is what this node is trusted to
//! do. Reading the state is elsewhere — `tailscale_lock_status` and
//! `tailscale_lock_log` sit with the other read-only status tools.
//!
//! Three of them are irreversible for the whole tailnet rather than for this
//! node, so they carry a confirmation on top of the destructive tier:
//! initialising the lock, disabling it with a disablement secret, and revoking
//! keys. `tailscale_lock_local_disable` is destructive without one, because it
//! changes only what this node will accept and leaves the tailnet's lock
//! standing.
//!
//! Two kinds of secret pass through here, and they are handled differently
//! because the client handles them differently.
//!
//! A **node or auth key** being signed may be given as `file:<path>`, which the
//! client reads itself, so `tailscale_lock_sign` never has to see it: a literal
//! is written to a private temporary file and the path is what runs. A
//! **disablement secret** has no such form — `lock disable` and `lock
//! disablement-kdf` take it as a positional argument and nothing else — so this
//! module honours `file:<path>` on their behalf by reading the file itself.
//! That keeps the secret out of the conversation, which is the exposure a
//! caller controls; it cannot keep it off the argument list, which is the one
//! the client's own interface fixes (DECISIONS Q38).
//!
//! `tailscale_lock_init` always passes `--confirm=true`. Without it the client
//! prints its warning, reads end-of-file from a standard input that a tool call
//! does not have, and exits successfully having done nothing at all — a silent
//! no-op reported as success, which is the worst answer available. The
//! confirmation the caller gave this server is the same question, asked where
//! there is someone to answer it (DECISIONS Q39).
//!
//! Both `tailscale_lock_init` and `tailscale_lock_sign` can mint a secret, and
//! a minted secret comes back whole in the answer to the call that minted it:
//! it is the product of the call, and redacting it would leave the caller with
//! nothing. Nothing here stores one, nothing here logs one, and no later call
//! can produce one again.
//!
//! Whether that answer also keeps the text the secret was read out of differs
//! between the two, because what a lost secret costs differs between the two
//! (DECISIONS Q43). `tailscale_lock_sign` withholds it: if the parse missed the
//! auth key, sign again. `tailscale_lock_init` keeps it, so that a parse
//! written against today's output is never the only copy of the disablement
//! secrets — they cannot be minted twice, and without one the lock can never be
//! turned off.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_cli::Invocation;

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};
use crate::tools::common::{
    flag, note, printed, push_bool, push_text, real_path, report, secret_value, tokens_with_prefix,
};

crate::tools! {
    /// Turn on tailnet lock for the whole tailnet, trusting the tailnet-lock
    /// keys given here to sign nodes and to make further lock changes. Mints
    /// the disablement secrets that are the only way back, and returns them
    /// once: store them before answering, because nothing here keeps a copy.
    /// Affects every node in the tailnet, so it needs a confirmation as well
    /// as the destructive tier.
    tailscale_lock_init => LockInitParams, lock_init,
        toolset: LocalLock, tier: Destructive, confirm: true;

    /// Trust more tailnet-lock keys, so that the nodes holding them can sign
    /// nodes and change tailnet lock. Adding a key that is already trusted
    /// changes nothing.
    tailscale_lock_add => LockKeysParams, lock_add,
        toolset: LocalLock, tier: Write, idempotent: true;

    /// Stop trusting tailnet-lock keys. Signatures those keys made are re-signed
    /// by default so that the nodes they admitted stay admitted; turning that
    /// off locks those nodes out. Use `tailscale_lock_revoke_keys` instead if a
    /// key was compromised.
    tailscale_lock_remove => LockRemoveParams, lock_remove,
        toolset: LocalLock, tier: Destructive;

    /// Sign a node key, admitting that node to a tailnet under lock, or sign a
    /// pre-approved auth key so that it can bring nodes up. Give the key
    /// directly or as `file:<path>` to a file holding it. A signed auth key is
    /// returned once and is not stored.
    tailscale_lock_sign => LockSignParams, lock_sign,
        toolset: LocalLock, tier: Write;

    /// Turn tailnet lock off for the whole tailnet by spending one of the
    /// disablement secrets minted when it was initialised. The secret is
    /// consumed and becomes public; re-enabling the lock means initialising it
    /// again from scratch. Needs a confirmation as well as the destructive
    /// tier.
    tailscale_lock_disable => LockDisableParams, lock_disable,
        toolset: LocalLock, tier: Destructive, confirm: true;

    /// Compute the public disablement value that corresponds to a disablement
    /// secret, without disabling anything and without contacting anything.
    /// Local arithmetic on the value given, for checking a stored secret
    /// against what `tailscale_lock_status` reports.
    tailscale_lock_disablement_kdf => LockDisablementKdfParams, lock_disablement_kdf,
        toolset: LocalLock, tier: Read, idempotent: true;

    /// Make this node accept traffic from nodes that tailnet lock has locked
    /// out. Affects this node only: the tailnet's lock stays on and every other
    /// node keeps enforcing it.
    tailscale_lock_local_disable => NoParams, lock_local_disable,
        toolset: LocalLock, tier: Destructive;

    /// Retroactively revoke compromised tailnet-lock keys, so that every node
    /// they signed loses its authorisation and must be signed again. Several
    /// signing nodes have to co-sign: start with `keys`, then re-run with the
    /// `recovery_blob` the previous step printed and `cosign` on each further
    /// node, then once with `finish`. Needs a confirmation as well as the
    /// destructive tier.
    tailscale_lock_revoke_keys => LockRevokeKeysParams, lock_revoke_keys,
        toolset: LocalLock, tier: Destructive, confirm: true;
}

// ---------------------------------------------------------------------------
// Shapes the client insists on
// ---------------------------------------------------------------------------

/// What every tailnet-lock key starts with. Public by design: a `tlpub:` key
/// identifies a signing node and is meant to be copied between them, so unlike
/// everything else in this module it goes on the argument list as it stands and
/// is echoed back in the answer.
const KEY_PREFIX: &str = "tlpub:";

/// What an auth key starts with, which is how the answer to a `lock sign` tells
/// a signed auth key from a signed node key without being told which was asked
/// for — the caller may have named a file rather than a key.
const AUTH_KEY_PREFIX: &str = "tskey-";

/// The private half of a disablement, as `lock init` mints it.
const DISABLEMENT_SECRET_PREFIX: &str = "disablement-secret:";

/// The public half, as `lock disablement-kdf` computes it and `lock status`
/// reports it.
const DISABLEMENT_VALUE_PREFIX: &str = "disablement:";

/// A tool that takes nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

const fn default_gen_disablements() -> u32 {
    1
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LockInitParams {
    /// The tailnet-lock keys to trust initially, each beginning with `tlpub:`.
    /// This node's own key, which `tailscale_lock_status` reports, has to be
    /// among them or the client refuses. At least one.
    pub trusted_keys: Vec<String>,
    /// How many disablement secrets to mint. At least one — the client refuses
    /// a lock with no way back — and they are returned only by this call.
    #[serde(default = "default_gen_disablements")]
    pub gen_disablements: u32,
    /// Also mint a disablement secret and send it to Tailscale, so that
    /// Tailscale support can disable the lock. Recommended by the client, and
    /// off unless asked for, because it hands a third party the way back.
    #[serde(default)]
    pub gen_disablement_for_support: bool,
}

/// Hand-written rather than derived so that the empty value matches the schema:
/// a derived default would mint zero disablements, which this tool refuses.
impl Default for LockInitParams {
    fn default() -> Self {
        Self {
            trusted_keys: Vec::new(),
            gen_disablements: default_gen_disablements(),
            gen_disablement_for_support: false,
        }
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LockKeysParams {
    /// The tailnet-lock keys to trust, each beginning with `tlpub:`.
    pub keys: Vec<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LockRemoveParams {
    /// The tailnet-lock keys to stop trusting, each beginning with `tlpub:`.
    pub keys: Vec<String>,
    /// Re-sign the signatures that removing these keys would invalidate, so
    /// that the nodes they admitted stay admitted. The client's own default is
    /// to re-sign; setting this to `false` locks those nodes out.
    #[serde(default)]
    pub re_sign: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LockSignParams {
    /// The node key to sign, or the pre-approved auth key to sign. Either the
    /// key itself or `file:<path>` to a file holding it; a key given directly
    /// is written to a private file before the client is run, so it never
    /// reaches an argument list.
    pub key: String,
    /// The rotation key to attach to a node signature, in the same two forms.
    /// Only meaningful when signing a node key.
    #[serde(default)]
    pub rotation_key: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LockDisableParams {
    /// One of the disablement secrets minted when tailnet lock was
    /// initialised, beginning with `disablement-secret:` or `disablement:`.
    /// May instead be `file:<path>` to a file holding it.
    pub secret: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LockDisablementKdfParams {
    /// A disablement secret, as hex or with its `disablement-secret:` prefix,
    /// or `file:<path>` to a file holding it.
    pub secret: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LockRevokeKeysParams {
    /// The compromised tailnet-lock keys to revoke, each beginning with
    /// `tlpub:`. Names the keys only on the call that starts a revocation.
    #[serde(default)]
    pub keys: Vec<String>,
    /// The recovery blob the previous step printed, which continues a
    /// revocation already begun. Give this instead of `keys`.
    #[serde(default)]
    pub recovery_blob: Option<String>,
    /// Co-sign the recovery with this node's tailnet-lock key. Run once on each
    /// further signing node, each time with the blob the last step printed.
    #[serde(default)]
    pub cosign: bool,
    /// Transmit the revocation, ending the process. Valid once there are more
    /// co-signatures than keys being revoked.
    #[serde(default)]
    pub finish: bool,
    /// The parent AUM hash to rewrite from. Advanced; leave it out unless the
    /// recovery instructions said otherwise.
    #[serde(default)]
    pub fork_from: Option<String>,
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// What a lock change did to a named set of keys.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LockReport {
    /// What is true now that was not true before.
    pub outcome: String,
    /// The tailnet-lock keys the call named, echoed so that the answer stands
    /// on its own. Public keys only; nothing secret is ever echoed here. Absent
    /// on a revocation continued from a blob, which names no keys.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Whatever the client printed, which for a multi-step revocation is the
    /// command to run next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

/// What a lock change did, where there is no key to name: the subject is the
/// lock itself, on the tailnet or on this node.
#[derive(Debug, Serialize, JsonSchema)]
pub struct StateReport {
    /// What is true now that was not true before.
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct InitReport {
    pub outcome: String,
    /// The keys tailnet lock now trusts.
    pub trusted_keys: Vec<String>,
    /// The disablement secrets this call minted, in the order the client
    /// printed them. This server does not store them and no later call can
    /// produce them again, so this answer is the only time they are offered.
    /// Losing them means the lock can never be disabled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disablement_secrets: Vec<String>,
    /// Whether a further secret was also sent to Tailscale support.
    pub gen_disablement_for_support: bool,
    /// The client's own text, kept alongside the secrets read out of it so that
    /// a parse written against today's output is never the only copy of
    /// something that cannot be minted twice (DECISIONS Q43).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SignReport {
    pub outcome: String,
    /// The signed auth key, present only when an auth key was what got signed.
    /// It is the product of the call, so it is returned whole, and the text it
    /// was read out of is withheld rather than repeated (DECISIONS Q43).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_auth_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DisablementReport {
    /// The public disablement value, as `disablement:<hex>`. Safe to compare
    /// against what `tailscale_lock_status` reports; the secret it came from is
    /// not repeated.
    pub disablement_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Check a list of tailnet-lock keys and hand back the trimmed spellings.
///
/// A rejected key is reported by its position rather than its value: the value
/// is the caller's to see and ours not to repeat, and a caller that mistook an
/// auth key for a lock key would otherwise have pasted a secret into an error
/// message.
fn public_keys(keys: &[String], what: &str) -> ToolResult<Vec<String>> {
    let trimmed: Vec<String> = keys
        .iter()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
        .collect();
    if trimmed.is_empty() {
        return Err(ToolError::invalid_args(format!(
            "`{what}` needs at least one tailnet-lock key"
        )));
    }
    if let Some(nth) = trimmed.iter().position(|key| !key.starts_with(KEY_PREFIX)) {
        return Err(ToolError::invalid_args(format!(
            "value {} of `{what}` is not a tailnet-lock key: they begin with `{KEY_PREFIX}`",
            nth + 1
        ))
        .with_hint("`tailscale_lock_status` reports this node's own tailnet-lock key."));
    }
    Ok(trimmed)
}

/// A disablement secret, read out of a file when the caller pointed at one.
///
/// Neither command that takes one has a `file:` form of its own, so this server
/// honours one on their behalf. What that buys is real but partial: the secret
/// need not be pasted into the conversation, though it still reaches the
/// argument list the client insists on (DECISIONS Q38).
///
/// The mirror image of [`secret_value`], which writes a secret *out* to a
/// private file so that a literal never reaches an argument list.
fn disablement_secret(ctx: &ToolContext, what: &str, value: &str) -> ToolResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ToolError::invalid_args(format!("`{what}` cannot be empty")));
    }
    let Some(path) = value.strip_prefix("file:") else {
        return Ok(value.to_owned());
    };
    let path = real_path(ctx, what, path)?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => ErrorCode::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorCode::NotPermitted,
            _ => ErrorCode::CliFailed,
        };
        ToolError::new(
            code,
            format!("`{what}` names a file that could not be read: {e}"),
        )
    })?;
    let text = text.trim();
    if text.is_empty() {
        return Err(ToolError::invalid_args(format!(
            "`{what}` names a file with nothing in it"
        )));
    }
    Ok(text.to_owned())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn lock_init(ctx: &ToolContext, params: LockInitParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_init;
    let trusted_keys = public_keys(&params.trusted_keys, "trusted_keys")?;
    // The client's own rule, applied here so that it costs a message rather
    // than a spawn. There is deliberately no ceiling: how many places a caller
    // has to keep a secret safe is theirs to know, not this server's to guess.
    if params.gen_disablements == 0 {
        return Err(ToolError::invalid_args(
            "`gen_disablements` has to be at least 1: a lock with no disablement secret can never be turned off",
        ));
    }

    let mut args = vec![
        "lock".to_owned(),
        "init".to_owned(),
        // Never omitted: see the module note on the silent no-op.
        flag("confirm", true),
        flag(
            "gen-disablement-for-support",
            params.gen_disablement_for_support,
        ),
        format!("--gen-disablements={}", params.gen_disablements),
    ];
    args.extend(trusted_keys.iter().cloned());

    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    let printed_text = output.stdout_str();
    report(InitReport {
        outcome: format!(
            "tailnet lock is enabled for the tailnet, trusting {} key(s)",
            trusted_keys.len()
        ),
        trusted_keys,
        disablement_secrets: tokens_with_prefix(&printed_text, &[DISABLEMENT_SECRET_PREFIX]),
        gen_disablement_for_support: params.gen_disablement_for_support,
        printed: printed(ctx, &output),
    })
}

async fn lock_add(ctx: &ToolContext, params: LockKeysParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_add;
    let keys = public_keys(&params.keys, "keys")?;
    let mut args = vec!["lock".to_owned(), "add".to_owned()];
    args.extend(keys.iter().cloned());

    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    report(LockReport {
        outcome: "tailnet lock now trusts these keys to sign nodes and to change the lock"
            .to_owned(),
        keys,
        printed: printed(ctx, &output),
    })
}

async fn lock_remove(ctx: &ToolContext, params: LockRemoveParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_remove;
    let keys = public_keys(&params.keys, "keys")?;
    let mut args = vec!["lock".to_owned(), "remove".to_owned()];
    push_bool(&mut args, "re-sign", params.re_sign);
    args.extend(keys.iter().cloned());

    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    let outcome = if params.re_sign == Some(false) {
        "tailnet lock no longer trusts these keys, and the nodes they signed are locked out"
    } else {
        "tailnet lock no longer trusts these keys; the nodes they signed were re-signed and stay admitted"
    };
    report(LockReport {
        outcome: outcome.to_owned(),
        keys,
        printed: printed(ctx, &output),
    })
}

async fn lock_sign(ctx: &ToolContext, params: LockSignParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_sign;
    let key = params.key.trim();
    if key.is_empty() {
        return Err(ToolError::invalid_args("`key` cannot be empty"));
    }
    let (key_value, key_file) = secret_value("key", key)?;
    let mut args = vec!["lock".to_owned(), "sign".to_owned(), key_value];

    let rotation = params
        .rotation_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rotation_file = match rotation {
        Some(rotation) => {
            let (value, file) = secret_value("rotation_key", rotation)?;
            args.push(value);
            file
        }
        None => None,
    };

    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    drop(key_file);
    drop(rotation_file);

    // Which of the two things the client did is read off what it printed rather
    // than off what was asked for: a caller that named a file did not tell us
    // which kind of key was in it.
    let signed_auth_key = tokens_with_prefix(&output.stdout_str(), &[AUTH_KEY_PREFIX])
        .into_iter()
        .next();
    let (outcome, printed_text) = match &signed_auth_key {
        // Standard output holds the signed key, which the field above already
        // carries whole; repeating it redacted would only be confusing, so only
        // the client's commentary is passed on.
        Some(_) => (
            "the auth key is signed and can now bring nodes up under tailnet lock",
            note(ctx, &output.stderr),
        ),
        None => (
            "the node key is signed and the signature is with the control plane",
            printed(ctx, &output),
        ),
    };
    report(SignReport {
        outcome: outcome.to_owned(),
        signed_auth_key,
        printed: printed_text,
    })
}

async fn lock_disable(ctx: &ToolContext, params: LockDisableParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_disable;
    let secret = disablement_secret(ctx, "secret", &params.secret)?;
    if !secret.starts_with(DISABLEMENT_SECRET_PREFIX)
        && !secret.starts_with(DISABLEMENT_VALUE_PREFIX)
    {
        return Err(ToolError::invalid_args(format!(
            "`secret` has to carry its `{DISABLEMENT_SECRET_PREFIX}` or `{DISABLEMENT_VALUE_PREFIX}` prefix, which says which half of the disablement it is"
        )));
    }

    let output = cli::run(
        ctx,
        meta,
        Invocation::mutate(["lock".to_owned(), "disable".to_owned(), secret]),
    )
    .await?;
    report(StateReport {
        outcome: "tailnet lock is off for the whole tailnet, and the secret that turned it off is now public".to_owned(),
        printed: printed(ctx, &output),
    })
}

async fn lock_disablement_kdf(
    ctx: &ToolContext,
    params: LockDisablementKdfParams,
) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_disablement_kdf;
    let secret = disablement_secret(ctx, "secret", &params.secret)?;
    // This command wants bare hex while `lock disable` insists on a prefix and
    // `lock init` mints the prefixed form, so a caller holding a minted secret
    // has the wrong shape for it. Taking the prefix off here rather than making
    // that the caller's problem is the whole of the difference.
    let hex = secret
        .strip_prefix(DISABLEMENT_SECRET_PREFIX)
        .unwrap_or(&secret);
    if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ToolError::invalid_args(
            "`secret` has to be a hex-encoded disablement secret, with or without its `disablement-secret:` prefix",
        )
        .with_hint(
            "A `disablement:` value is the public half and is what this computes, not what it takes.",
        ));
    }

    let output = cli::run(
        ctx,
        meta,
        Invocation::read([
            "lock".to_owned(),
            "disablement-kdf".to_owned(),
            hex.to_owned(),
        ]),
    )
    .await?;
    let text = output.stdout_str();
    let disablement_value = tokens_with_prefix(&text, &[DISABLEMENT_VALUE_PREFIX])
        .into_iter()
        .next()
        .unwrap_or_else(|| text.trim().to_owned());
    report(DisablementReport {
        disablement_value,
        note: note(ctx, &output.stderr),
    })
}

async fn lock_local_disable(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_local_disable;
    let output = cli::run(ctx, meta, Invocation::mutate(["lock", "local-disable"])).await?;
    report(StateReport {
        outcome: "this node now accepts traffic from nodes tailnet lock has locked out; the tailnet's lock is untouched".to_owned(),
        printed: printed(ctx, &output),
    })
}

async fn lock_revoke_keys(ctx: &ToolContext, params: LockRevokeKeysParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_lock_revoke_keys;
    if params.cosign && params.finish {
        return Err(ToolError::invalid_args(
            "`cosign` and `finish` are the two ways of continuing a revocation and only one applies to a call: `cosign` on each further signing node, `finish` once there are more co-signatures than keys",
        ));
    }

    let mut args = vec!["lock".to_owned(), "revoke-keys".to_owned()];
    if params.cosign {
        args.push(flag("cosign", true));
    }
    if params.finish {
        args.push(flag("finish", true));
    }
    push_text(&mut args, "fork-from", params.fork_from.as_deref());

    let blob = params
        .recovery_blob
        .as_deref()
        .map(str::trim)
        .filter(|blob| !blob.is_empty());
    let keys = match blob {
        Some(blob) => {
            if !params.keys.is_empty() {
                return Err(ToolError::invalid_args(
                    "name `keys` to start a revocation or `recovery_blob` to continue one, not both",
                ));
            }
            args.push(blob.to_owned());
            Vec::new()
        }
        None => {
            if params.cosign || params.finish {
                return Err(ToolError::invalid_args(
                    "`cosign` and `finish` continue a revocation, so they need the `recovery_blob` the step before printed",
                ));
            }
            let keys = public_keys(&params.keys, "keys")?;
            args.extend(keys.iter().cloned());
            keys
        }
    };

    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    let outcome = if params.finish {
        "the keys are revoked: every node they signed has lost its authorisation and has to be signed again"
    } else if params.cosign {
        "this node has co-signed the revocation; run the printed command on the next signing node, or finish once the co-signatures outnumber the keys"
    } else {
        "the revocation has begun; run the printed command on the next signing node to co-sign it"
    };
    report(LockReport {
        outcome: outcome.to_owned(),
        keys,
        printed: printed(ctx, &output),
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use super::*;

    use crate::testing::{Reply, StubBackend, context};

    /// Documentation values throughout: a real `tlpub:` key names a real
    /// signing node, and none of this tailnet's belong in a repository.
    const TLPUB: &str = "tlpub:0000000000000000000000000000000000000000000000000000000000000000";
    const OTHER: &str = "tlpub:1111111111111111111111111111111111111111111111111111111111111111";
    const NODEKEY: &str =
        "nodekey:0000000000000000000000000000000000000000000000000000000000000000";
    const HEX: &str = "00112233445566778899aabbccddeeff";

    /// Run a handler against a scripted client and report both what it answered
    /// and what it ran.
    async fn against<F, P, Fut>(reply: Reply, handler: F, params: P) -> (Value, Vec<Vec<String>>)
    where
        F: FnOnce(ToolContext, P) -> Fut,
        Fut: Future<Output = ToolResult<Value>>,
    {
        let backend = Arc::new(StubBackend::always(reply));
        let ctx = context(Arc::clone(&backend));
        let value = handler(ctx, params).await.expect("the handler succeeds");
        (value, backend.argv())
    }

    /// The same for a call that should be refused, which is only interesting
    /// alongside the fact that nothing was run.
    async fn refused<F, P, Fut>(handler: F, params: P) -> ToolError
    where
        F: FnOnce(ToolContext, P) -> Fut,
        Fut: Future<Output = ToolResult<Value>>,
    {
        let backend = Arc::new(StubBackend::always(Reply::ok("")));
        let ctx = context(Arc::clone(&backend));
        let error = handler(ctx, params).await.expect_err("the handler refuses");
        assert!(
            backend.argv().is_empty(),
            "nothing should have run: {:?}",
            backend.argv()
        );
        error
    }

    /// The argument list of the single command a handler ran.
    fn only(argv: &[Vec<String>]) -> &[String] {
        assert_eq!(argv.len(), 1, "one command should have run: {argv:?}");
        &argv[0]
    }

    // -- init ---------------------------------------------------------------

    /// The confirmation is not optional: without it the client reads end-of-file
    /// from a standard input a tool call does not have and exits successfully
    /// having initialised nothing.
    #[tokio::test]
    async fn initialising_always_confirms_and_states_every_flag() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_init(&ctx, p).await },
            LockInitParams {
                trusted_keys: vec![TLPUB.to_owned(), OTHER.to_owned()],
                gen_disablements: 3,
                gen_disablement_for_support: true,
            },
        )
        .await;

        assert_eq!(
            only(&argv),
            [
                "lock",
                "init",
                "--confirm=true",
                "--gen-disablement-for-support=true",
                "--gen-disablements=3",
                TLPUB,
                OTHER,
            ]
        );
    }

    /// The minted secrets are the whole point of the call, so they are read out
    /// of what the client printed *and* left in it: they cannot be minted a
    /// second time, so a parse written against today's output must not be the
    /// only copy (DECISIONS Q43).
    #[tokio::test]
    async fn initialising_reads_the_minted_secrets_out_of_what_was_printed() {
        let printed = format!(
            "Tailnet lock is now enabled.\n\
             Disablement secrets:\n  disablement-secret:{HEX}\n  disablement-secret:ffee\n"
        );
        let (answer, _) = against(
            Reply::ok(printed),
            |ctx, p| async move { lock_init(&ctx, p).await },
            LockInitParams {
                trusted_keys: vec![TLPUB.to_owned()],
                ..LockInitParams::default()
            },
        )
        .await;

        assert_eq!(
            answer["disablement_secrets"],
            serde_json::json!([
                format!("disablement-secret:{HEX}"),
                "disablement-secret:ffee"
            ])
        );
        assert!(
            answer["printed"]
                .as_str()
                .is_some_and(|text| text.contains(HEX)),
            "the client's own text is kept as well: {answer}"
        );
    }

    /// The struct's empty value has to agree with the schema's, or a caller that
    /// omits the field gets a refusal the schema said would not happen.
    #[test]
    fn the_default_number_of_disablements_is_the_one_the_schema_advertises() {
        assert_eq!(LockInitParams::default().gen_disablements, 1);
    }

    #[tokio::test]
    async fn a_lock_with_no_way_back_is_refused() {
        let error = refused(
            |ctx, p| async move { lock_init(&ctx, p).await },
            LockInitParams {
                trusted_keys: vec![TLPUB.to_owned()],
                gen_disablements: 0,
                gen_disablement_for_support: false,
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    // -- add and remove -----------------------------------------------------

    #[tokio::test]
    async fn adding_keys_puts_them_after_the_subcommand() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_add(&ctx, p).await },
            LockKeysParams {
                keys: vec![TLPUB.to_owned(), OTHER.to_owned()],
            },
        )
        .await;

        assert_eq!(only(&argv), ["lock", "add", TLPUB, OTHER]);
        assert_eq!(answer["keys"], serde_json::json!([TLPUB, OTHER]));
    }

    /// A key that is not a lock key is reported by position, so that a caller
    /// who pasted a secret into the wrong parameter does not get it back in an
    /// error message.
    #[tokio::test]
    async fn a_key_of_the_wrong_kind_is_named_by_position_and_not_by_value() {
        let error = refused(
            |ctx, p| async move { lock_add(&ctx, p).await },
            LockKeysParams {
                keys: vec![
                    TLPUB.to_owned(),
                    "tskey-auth-example-secretvalue".to_owned(),
                ],
            },
        )
        .await;

        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert!(error.message.contains("value 2"), "{}", error.message);
        assert!(
            !error.message.contains("tskey-auth-example-secretvalue"),
            "the rejected value is not repeated: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn no_keys_at_all_is_refused() {
        let error = refused(
            |ctx, p| async move { lock_add(&ctx, p).await },
            LockKeysParams::default(),
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    /// Re-signing is the client's default, so an unspecified call says nothing
    /// about it rather than restating it.
    #[tokio::test]
    async fn removing_keys_mentions_re_signing_only_when_it_was_asked_about() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_remove(&ctx, p).await },
            LockRemoveParams {
                keys: vec![TLPUB.to_owned()],
                re_sign: None,
            },
        )
        .await;
        assert_eq!(only(&argv), ["lock", "remove", TLPUB]);
        assert!(
            answer["outcome"]
                .as_str()
                .is_some_and(|text| text.contains("stay admitted")),
            "{answer}"
        );

        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_remove(&ctx, p).await },
            LockRemoveParams {
                keys: vec![TLPUB.to_owned()],
                re_sign: Some(false),
            },
        )
        .await;
        assert_eq!(only(&argv), ["lock", "remove", "--re-sign=false", TLPUB]);
        assert!(
            answer["outcome"]
                .as_str()
                .is_some_and(|text| text.contains("locked out")),
            "{answer}"
        );
    }

    // -- sign ---------------------------------------------------------------

    #[tokio::test]
    async fn a_key_given_directly_reaches_the_client_as_a_private_file() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_sign(&ctx, p).await },
            LockSignParams {
                key: NODEKEY.to_owned(),
                rotation_key: None,
            },
        )
        .await;

        let argv = only(&argv);
        assert_eq!(&argv[..2], ["lock", "sign"]);
        assert!(
            argv[2].starts_with("file:") && argv[2].ends_with(".key"),
            "the key itself must not reach the argument list: {argv:?}"
        );
    }

    /// A caller that already has the key in a file is handing over a path, and
    /// copying it somewhere else would gain nothing.
    #[tokio::test]
    async fn a_file_reference_is_passed_through_as_it_stands() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_sign(&ctx, p).await },
            LockSignParams {
                key: "file:/var/keys/node.key".to_owned(),
                rotation_key: Some(NODEKEY.to_owned()),
            },
        )
        .await;

        let argv = only(&argv);
        assert_eq!(argv[2], "file:/var/keys/node.key");
        assert!(
            argv[3].starts_with("file:") && argv[3].ends_with(".key"),
            "the rotation key gets a private file of its own: {argv:?}"
        );
    }

    /// A signed auth key is what the call was for, so it comes back whole. The
    /// shape-based redaction would remove exactly the answer.
    #[tokio::test]
    async fn a_signed_auth_key_comes_back_whole_and_only_once() {
        let signed = "tskey-auth-example0CNTRL-signedvalue";
        let (answer, _) = against(
            Reply::ok(format!("{signed}\n")),
            |ctx, p| async move { lock_sign(&ctx, p).await },
            LockSignParams {
                key: "tskey-auth-example0CNTRL".to_owned(),
                rotation_key: None,
            },
        )
        .await;

        assert_eq!(answer["signed_auth_key"], signed);
        assert!(
            answer["printed"].is_null(),
            "the key is carried once, not twice: {answer}"
        );
    }

    #[tokio::test]
    async fn signing_a_node_key_reports_no_auth_key() {
        let (answer, _) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_sign(&ctx, p).await },
            LockSignParams {
                key: NODEKEY.to_owned(),
                rotation_key: None,
            },
        )
        .await;

        assert!(answer["signed_auth_key"].is_null(), "{answer}");
        assert!(
            answer["outcome"]
                .as_str()
                .is_some_and(|text| text.contains("node key")),
            "{answer}"
        );
    }

    #[tokio::test]
    async fn an_empty_key_is_refused() {
        let error = refused(
            |ctx, p| async move { lock_sign(&ctx, p).await },
            LockSignParams {
                key: "   ".to_owned(),
                rotation_key: None,
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    // -- disable and the KDF ------------------------------------------------

    /// The prefix says which half of the disablement a value is, and the client
    /// refuses a bare one. Refusing it here keeps it off the argument list.
    #[tokio::test]
    async fn a_disablement_without_its_prefix_is_refused() {
        let error = refused(
            |ctx, p| async move { lock_disable(&ctx, p).await },
            LockDisableParams {
                secret: HEX.to_owned(),
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    /// `lock disable` has no `file:` form of its own, so this server reads the
    /// file. The secret still reaches the argument list, which is the client's
    /// interface and not ours to change (DECISIONS Q38).
    #[tokio::test]
    async fn a_disablement_secret_can_be_kept_in_a_file() {
        let mut file = tempfile::NamedTempFile::new().expect("a temporary file");
        writeln!(file, "disablement-secret:{HEX}").expect("the secret is written");
        let path = file.path().display().to_string();

        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_disable(&ctx, p).await },
            LockDisableParams {
                secret: format!("file:{path}"),
            },
        )
        .await;

        assert_eq!(
            only(&argv),
            ["lock", "disable", &format!("disablement-secret:{HEX}")]
        );
    }

    #[tokio::test]
    async fn a_file_that_is_not_there_is_not_found() {
        let error = refused(
            |ctx, p| async move { lock_disable(&ctx, p).await },
            LockDisableParams {
                secret: "file:/nowhere/at/all/secret.txt".to_owned(),
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    /// The two commands want the same secret in two shapes. Which one the caller
    /// happens to hold is not a thing to make it think about.
    #[tokio::test]
    async fn the_kdf_takes_the_prefix_off_a_minted_secret() {
        let (answer, argv) = against(
            Reply::ok("disablement:756fe19f200fbfc9ad431e75c7942b82\n"),
            |ctx, p| async move { lock_disablement_kdf(&ctx, p).await },
            LockDisablementKdfParams {
                secret: format!("disablement-secret:{HEX}"),
            },
        )
        .await;

        assert_eq!(only(&argv), ["lock", "disablement-kdf", HEX]);
        assert_eq!(
            answer["disablement_value"],
            "disablement:756fe19f200fbfc9ad431e75c7942b82"
        );
    }

    #[tokio::test]
    async fn the_kdf_refuses_something_that_is_not_hex() {
        let error = refused(
            |ctx, p| async move { lock_disablement_kdf(&ctx, p).await },
            LockDisablementKdfParams {
                secret: "disablement:756fe1".to_owned(),
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn disabling_this_node_only_says_so() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_local_disable(&ctx, p).await },
            NoParams {},
        )
        .await;

        assert_eq!(only(&argv), ["lock", "local-disable"]);
        assert!(
            answer["outcome"]
                .as_str()
                .is_some_and(|text| text.contains("this node")),
            "{answer}"
        );
    }

    // -- revoke-keys --------------------------------------------------------

    #[tokio::test]
    async fn starting_a_revocation_names_the_keys() {
        let (answer, argv) = against(
            Reply::ok("next: tailscale lock revoke-keys --cosign blob\n"),
            |ctx, p| async move { lock_revoke_keys(&ctx, p).await },
            LockRevokeKeysParams {
                keys: vec![TLPUB.to_owned()],
                ..LockRevokeKeysParams::default()
            },
        )
        .await;

        assert_eq!(only(&argv), ["lock", "revoke-keys", TLPUB]);
        assert!(
            answer["printed"]
                .as_str()
                .is_some_and(|text| text.contains("--cosign")),
            "the command to run next is what the caller needs: {answer}"
        );
    }

    #[tokio::test]
    async fn continuing_a_revocation_puts_the_blob_after_its_flags() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { lock_revoke_keys(&ctx, p).await },
            LockRevokeKeysParams {
                recovery_blob: Some("abcdef".to_owned()),
                cosign: true,
                fork_from: Some("aum-hash".to_owned()),
                ..LockRevokeKeysParams::default()
            },
        )
        .await;

        assert_eq!(
            only(&argv),
            [
                "lock",
                "revoke-keys",
                "--cosign=true",
                "--fork-from=aum-hash",
                "abcdef",
            ]
        );
    }

    #[tokio::test]
    async fn the_two_ways_of_continuing_are_not_both_at_once() {
        let error = refused(
            |ctx, p| async move { lock_revoke_keys(&ctx, p).await },
            LockRevokeKeysParams {
                recovery_blob: Some("abcdef".to_owned()),
                cosign: true,
                finish: true,
                ..LockRevokeKeysParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn continuing_without_the_blob_from_the_step_before_is_refused() {
        let error = refused(
            |ctx, p| async move { lock_revoke_keys(&ctx, p).await },
            LockRevokeKeysParams {
                keys: vec![TLPUB.to_owned()],
                finish: true,
                ..LockRevokeKeysParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn keys_and_a_blob_together_are_refused() {
        let error = refused(
            |ctx, p| async move { lock_revoke_keys(&ctx, p).await },
            LockRevokeKeysParams {
                keys: vec![TLPUB.to_owned()],
                recovery_blob: Some("abcdef".to_owned()),
                ..LockRevokeKeysParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }
}
