//! The tools that touch the local filesystem.
//!
//! Everything else on the local surface reads or writes state inside
//! `tailscaled`. These eleven read and write *files on the local filesystem*:
//! they send
//! files to a peer and take files out of the Taildrop inbox, write a TLS
//! certificate and a metrics dump to paths the caller names, point `kubectl` at
//! a cluster, and share a directory over Taildrive. Every tool description says
//! so, because the caller is choosing a path on a machine it cannot see.
//!
//! Paths are taken as the caller gives them, at the write tier and no higher:
//! the tier is what confines them in this release, so a read-only session
//! reaches none of this. The allow-list that would confine them to a configured
//! root is [`PathPolicy`](crate::context::PathPolicy), which every path here is
//! already checked against and which ships as `Unrestricted`; switching it on
//! is a matter of populating it rather than of finding the places that must ask.
//!
//! Two rules the certificate tool exists to enforce. Naming neither output path
//! makes the client write `DOMAIN.crt` and `DOMAIN.key` into whatever directory
//! it happens to be in, so both paths are required here. Naming `-` makes it
//! write the certificate — and the private key — to standard output, which
//! would put key material into the answer, the transcript and the log, so `-`
//! is refused before anything runs.
//!
//! Taildrop and ACME are both slow by nature, so `tailscale_file_cp` and
//! `tailscale_cert` carry the longer budgets agreed in DECISIONS Q29 rather
//! than the ordinary 30 seconds.

use std::time::Duration;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tailscale_cli::{Invocation, Output};

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};
use crate::meta::ToolMeta;
use crate::tools::common::{
    bounded_wait, flag, note, object, printed, push_bool, push_text, real_path, report,
};

crate::tools! {
    /// Send files from **the local filesystem** to another node over
    /// Taildrop. The files are read from the paths given here; the peer takes
    /// delivery with `tailscale_file_get` on its own machine. Transfers are
    /// slow, so this call allows several minutes.
    tailscale_file_cp => FileCpParams, file_cp,
        toolset: LocalFiles, tier: Write;

    /// List the nodes this one may send files to with `tailscale_file_cp`,
    /// with the address and name to use for each. Reads only.
    tailscale_file_targets => NoParams, file_targets,
        toolset: LocalFiles, tier: Read, idempotent: true;

    /// Move files waiting in this node's Taildrop inbox into a directory on
    /// **the local filesystem**, emptying the inbox. Returns straight away
    /// whether or not anything was waiting: it never blocks for a file to
    /// arrive and never runs on in a loop.
    tailscale_file_get => FileGetParams, file_get,
        toolset: LocalFiles, tier: Write;

    /// Obtain a TLS certificate for a name in this tailnet and **write it to
    /// two files on the local filesystem**. Both output paths are required, and neither
    /// may be `-`: the private key is never printed into the answer. Issuance
    /// talks to a certificate authority, so this call allows a minute or two.
    tailscale_cert => CertParams, cert,
        toolset: LocalFiles, tier: Write;

    /// Write this node's client metrics, in Prometheus text format, to a file
    /// on **the local filesystem** — the form a node exporter's textfile
    /// collector reads. Use `tailscale_metrics_print` to read the same numbers
    /// without writing anything.
    tailscale_metrics_write => MetricsWriteParams, metrics_write,
        toolset: LocalFiles, tier: Write, idempotent: true, since: "1.78";

    /// Add a context to the **local kubectl configuration file** for a Kubernetes
    /// cluster reached through a Tailscale auth proxy running on the named
    /// peer. Alpha in the client, and it edits the kubeconfig in place.
    tailscale_configure_kubeconfig => KubeconfigParams, configure_kubeconfig,
        toolset: LocalFiles, tier: Write, idempotent: true;

    /// Reload the local node's MDM and system policy settings even when nothing has
    /// changed, and report what the reload produced. Use
    /// `tailscale_syspolicy_list` to read the settings without reloading.
    tailscale_syspolicy_reload => NoParams, syspolicy_reload,
        toolset: LocalFiles, tier: Write, idempotent: true, since: "1.72";

    /// List the directories on **the local filesystem** that Taildrive shares with the
    /// tailnet, with the local user each is served as. Reads only.
    tailscale_drive_list => NoParams, drive_list,
        toolset: LocalFiles, tier: Read, idempotent: true;

    /// Share a directory on **the local filesystem** with the tailnet over
    /// Taildrive, under a name peers will see. Sharing the same name and path
    /// again changes nothing.
    tailscale_drive_share => DriveShareParams, drive_share,
        toolset: LocalFiles, tier: Write, idempotent: true;

    /// Rename an existing Taildrive share. Peers see the new name; the
    /// directory on the local filesystem is untouched.
    tailscale_drive_rename => DriveRenameParams, drive_rename,
        toolset: LocalFiles, tier: Write;

    /// Stop sharing a directory over Taildrive. Every peer loses access at
    /// once; the files stay on disk. Restoring it needs
    /// `tailscale_drive_share` and the original path.
    tailscale_drive_unshare => DriveUnshareParams, drive_unshare,
        toolset: LocalFiles, tier: Destructive;
}

// ---------------------------------------------------------------------------
// Budgets
// ---------------------------------------------------------------------------

/// How long a Taildrop transfer may run before the call gives up. Far above
/// the ordinary budget, because a large file over a relayed path takes minutes
/// (DECISIONS Q29).
const DEFAULT_TRANSFER_TIMEOUT: u64 = 300;

/// How long certificate issuance may run. Shorter than a transfer: an ACME
/// exchange that has not finished in two minutes has gone wrong rather than
/// slow.
const DEFAULT_CERT_TIMEOUT: u64 = 120;

/// The most either of them may be asked to wait for.
const MAX_LONG_TIMEOUT: u64 = 600;

/// Bound a caller's wait against the cap these longer commands share.
fn budget(requested: Option<u64>, default: u64) -> (u64, Duration) {
    bounded_wait(requested, default, MAX_LONG_TIMEOUT)
}

/// Repainting a progress line is for a terminal. Nothing here has one, and the
/// escape sequences would be the bulk of what the caller read back, so the
/// display is switched off rather than captured.
const NO_PROGRESS: &str = "--update-interval=0";

/// What the client prints when it can talk to Taildrive but this build does
/// not let the command line configure it — the macOS GUI packaging, where the
/// application owns the share list.
const NO_TAILDRIVE: &str = "Taildrive CLI commands are not supported";

/// A tool that takes nothing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FileCpParams {
    /// Paths on the local filesystem to send. At least one, and none may be `-`:
    /// a tool call has no standard input to read a file from.
    pub files: Vec<String>,
    /// The node to send to, by name, FQDN or Tailscale IP, as
    /// `tailscale_file_targets` reports it. A trailing colon is added if it is
    /// missing.
    pub target: String,
    /// Deliver under this name instead of the name on disk.
    #[serde(default)]
    pub name: Option<String>,
    /// Report each file as it goes.
    #[serde(default)]
    pub verbose: Option<bool>,
    /// How long to allow, in seconds. Default 300, capped at 600.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// What to do about a file in the inbox whose name is already taken in the
/// target directory.
#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Conflict {
    /// Leave the conflicting file in the inbox and report it, taking every
    /// other file. The client's own default, and the only one that destroys
    /// nothing.
    #[default]
    Skip,
    /// Replace the file already in the directory.
    Overwrite,
    /// Write to a new name with a number appended.
    Rename,
}

impl Conflict {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Overwrite => "overwrite",
            Self::Rename => "rename",
        }
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FileGetParams {
    /// The directory on the local filesystem to move the waiting files into. It has to
    /// exist already.
    pub directory: String,
    /// What to do about a name that is already taken there. Defaults to
    /// leaving the conflicting file in the inbox.
    #[serde(default)]
    pub conflict: Conflict,
    /// Report each file as it is moved.
    #[serde(default)]
    pub verbose: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CertParams {
    /// The name to certify. It has to be a name in this tailnet, with HTTPS
    /// enabled for the tailnet; `tailscale_status` reports this node's own.
    pub domain: String,
    /// Where on the local filesystem to write the certificate. Required, and `-` is
    /// refused.
    pub cert_file: String,
    /// Where on the local filesystem to write the private key. Required, and `-` is
    /// refused: a key printed to standard output would end up in the answer.
    pub key_file: String,
    /// Renew if the existing certificate has less than this long to run, in
    /// seconds. Omitted means take whatever lifetime the authority gives.
    #[serde(default)]
    pub min_validity_seconds: Option<u64>,
    /// How long to allow, in seconds. Default 120, capped at 600.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct MetricsWriteParams {
    /// The file on the local filesystem to write the metrics to. It is replaced, not
    /// appended to.
    pub path: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KubeconfigParams {
    /// The Tailscale hostname or FQDN of the peer running the auth proxy.
    pub hostname: String,
    /// Reach the proxy over plain HTTP. Ignored when the hostname already
    /// carries a scheme.
    #[serde(default)]
    pub http: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DriveShareParams {
    /// The name peers will see the share under.
    pub name: String,
    /// The directory on the local filesystem to share.
    pub path: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DriveRenameParams {
    /// The share's current name.
    pub name: String,
    /// The name to give it.
    pub new_name: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DriveUnshareParams {
    /// The name of the share to withdraw.
    pub name: String,
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// The answer to a call that sent files somewhere.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TransferReport {
    /// The node the files went to, as the client was asked for it.
    pub target: String,
    /// The paths that were sent.
    pub files: Vec<String>,
    /// The name they were delivered under, when it was not the name on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_as: Option<String>,
    /// How long the call was prepared to wait, after bounding.
    pub timeout_seconds: u64,
    /// Anything the client said while transferring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

/// One node this one may send files to.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Target {
    /// The Tailscale address to send to.
    pub address: String,
    /// The node's name.
    pub hostname: String,
    /// What the client noted about it, such as how long it has been offline.
    /// Absent for a node that is up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TargetsReport {
    /// Every node that would accept a file from this one.
    pub targets: Vec<Target>,
}

/// The answer to emptying the inbox.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InboxReport {
    /// Where the files were moved to.
    pub directory: String,
    /// The rule that was applied to names already taken there.
    pub conflict: String,
    /// What the client said, which is where the file names appear. Absent when
    /// the inbox was empty, which is not an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

/// The answer to issuing a certificate. The material itself is on disk and is
/// deliberately not here.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CertificateReport {
    /// The name that was certified.
    pub domain: String,
    /// Where the certificate was written.
    pub cert_file: String,
    /// Where the private key was written. Its contents are never reported.
    pub key_file: String,
    /// How long the call was prepared to wait, after bounding.
    pub timeout_seconds: u64,
    /// Anything the client said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed: Option<String>,
}

/// The answer to a call that did one thing to one named subject.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OutcomeReport {
    /// What was acted on: a path, a hostname, a share name.
    pub subject: String,
    /// What was done to it.
    pub outcome: String,
    /// Anything the client said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One directory shared from the local filesystem over Taildrive.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Share {
    /// The name peers see.
    pub name: String,
    /// The directory on the local filesystem.
    pub path: String,
    /// The local user the share is served as.
    pub as_user: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SharesReport {
    /// Every directory shared over Taildrive, empty when none are.
    pub shares: Vec<Share>,
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// The trailing colon `file cp` insists on, added when the caller left it off.
///
/// The client refuses a target without one — "final argument to 'tailscale file
/// cp' must end in colon" — which is a syntax detail of the command line rather
/// than anything about the node, so it is settled here.
fn transfer_target(target: &str) -> ToolResult<String> {
    let trimmed = target.trim();
    if trimmed.is_empty() || trimmed == ":" {
        return Err(ToolError::invalid_args("`target` has to name a node"));
    }
    Ok(if trimmed.ends_with(':') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}:")
    })
}

/// Run a Taildrive command, telling "this build cannot" apart from "it failed".
///
/// The macOS GUI packaging carries the subcommands and refuses them, because
/// the application owns the share list. That is a fact about the build rather
/// than about the request, so it is reported as `unsupported_platform` and not
/// as a command failure. It is detected in the client's own words rather than
/// declared as a platform gate on the tool, because a Mac running the
/// `tailscaled` packaging supports every one of these (DECISIONS Q31).
async fn drive(ctx: &ToolContext, meta: &ToolMeta, invocation: Invocation) -> ToolResult<Output> {
    let display = invocation.display();
    let output = cli::run_tolerant(ctx, meta, invocation).await?;
    if output.success() {
        return Ok(output);
    }
    let said = ctx.redactor.apply(&output.stderr);
    if said.contains(NO_TAILDRIVE) {
        return Err(ToolError::new(
            ErrorCode::UnsupportedPlatform,
            format!(
                "Taildrive is not configurable through the client on this node: {}",
                said.trim()
            ),
        )
        .with_hint("Configure Taildrive in the Tailscale application on this node instead."));
    }
    Err(cli::command_failure(ctx, meta, &display, &output))
}

// ---------------------------------------------------------------------------
// Taildrop
// ---------------------------------------------------------------------------

async fn file_cp(ctx: &ToolContext, params: FileCpParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_file_cp;
    if params.files.is_empty() {
        return Err(ToolError::invalid_args("`files` needs at least one path"));
    }
    let files = params
        .files
        .iter()
        .map(|file| real_path(ctx, "files", file))
        .collect::<ToolResult<Vec<String>>>()?;
    let target = transfer_target(&params.target)?;
    let (seconds, timeout) = budget(params.timeout_seconds, DEFAULT_TRANSFER_TIMEOUT);

    let mut args = vec!["file".to_owned(), "cp".to_owned(), NO_PROGRESS.to_owned()];
    push_text(&mut args, "name", params.name.as_deref());
    push_bool(&mut args, "verbose", params.verbose);
    // Last, because Go stops reading flags at the first positional, and the
    // target has to be the very last of those.
    args.extend(files.iter().cloned());
    args.push(target.clone());

    let output = cli::run(
        ctx,
        meta,
        Invocation::mutate_shared(args).with_timeout(timeout),
    )
    .await?;
    report(TransferReport {
        target,
        files,
        delivered_as: params.name,
        timeout_seconds: seconds,
        printed: printed(ctx, &output),
    })
}

async fn file_targets(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_file_targets;
    let text = cli::run_text(
        ctx,
        meta,
        Invocation::read(["file", "cp", &flag("targets", true)]),
    )
    .await?;
    report(TargetsReport {
        targets: parse_targets(&text),
    })
}

/// The tab-separated table `file cp --targets` prints: address, name, and for
/// a node that is not up, a note saying so.
fn parse_targets(text: &str) -> Vec<Target> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let mut columns = line.split('\t').map(str::trim);
            let address = columns.next()?;
            let hostname = columns.next().unwrap_or_default();
            let status = columns.next().filter(|note| !note.is_empty());
            Some(Target {
                address: address.to_owned(),
                hostname: hostname.to_owned(),
                status: status.map(str::to_owned),
            })
        })
        .collect()
}

async fn file_get(ctx: &ToolContext, params: FileGetParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_file_get;
    let directory = real_path(ctx, "directory", &params.directory)?;

    let args = vec![
        "file".to_owned(),
        "get".to_owned(),
        format!("--conflict={}", params.conflict.as_str()),
        // Named rather than left to their defaults, so that the promise the
        // tool description makes — this call never blocks and never runs on —
        // is visible in what was actually run.
        flag("wait", false),
        flag("loop", false),
        flag("verbose", params.verbose.unwrap_or(false)),
        directory.clone(),
    ];

    let output = cli::run(ctx, meta, Invocation::mutate_shared(args)).await?;
    report(InboxReport {
        directory,
        conflict: params.conflict.as_str().to_owned(),
        printed: printed(ctx, &output),
    })
}

// ---------------------------------------------------------------------------
// Certificates
// ---------------------------------------------------------------------------

async fn cert(ctx: &ToolContext, params: CertParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_cert;
    let cert_file = real_path(ctx, "cert_file", &params.cert_file)?;
    let key_file = real_path(ctx, "key_file", &params.key_file)?;
    if params.domain.trim().is_empty() {
        return Err(ToolError::invalid_args("`domain` cannot be empty"));
    }
    let (seconds, timeout) = budget(params.timeout_seconds, DEFAULT_CERT_TIMEOUT);

    let mut args = vec![
        "cert".to_owned(),
        format!("--cert-file={cert_file}"),
        format!("--key-file={key_file}"),
    ];
    if let Some(validity) = params.min_validity_seconds {
        args.push(format!("--min-validity={validity}s"));
    }
    args.push(params.domain.clone());

    let output = cli::run(
        ctx,
        meta,
        Invocation::mutate_shared(args).with_timeout(timeout),
    )
    .await?;
    report(CertificateReport {
        domain: params.domain,
        cert_file,
        key_file,
        timeout_seconds: seconds,
        printed: printed(ctx, &output),
    })
}

// ---------------------------------------------------------------------------
// Host configuration
// ---------------------------------------------------------------------------

async fn metrics_write(ctx: &ToolContext, params: MetricsWriteParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_metrics_write;
    let path = real_path(ctx, "path", &params.path)?;
    let output = cli::run(
        ctx,
        meta,
        Invocation::mutate_shared(["metrics", "write", &path]),
    )
    .await?;
    report(OutcomeReport {
        subject: path,
        outcome: "client metrics written in Prometheus text format".to_owned(),
        note: note(ctx, &output.stderr),
    })
}

async fn configure_kubeconfig(ctx: &ToolContext, params: KubeconfigParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_configure_kubeconfig;
    if params.hostname.trim().is_empty() {
        return Err(ToolError::invalid_args("`hostname` cannot be empty"));
    }
    let mut args = vec!["configure".to_owned(), "kubeconfig".to_owned()];
    push_bool(&mut args, "http", params.http);
    args.push(params.hostname.clone());

    let output = cli::run(ctx, meta, Invocation::mutate(args)).await?;
    report(OutcomeReport {
        subject: params.hostname,
        outcome: "added to the local kubectl configuration".to_owned(),
        note: printed(ctx, &output),
    })
}

async fn syspolicy_reload(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_syspolicy_reload;
    object(
        ctx,
        meta,
        Invocation::mutate(["syspolicy", "reload", &flag("json", true)]),
    )
    .await
}

// ---------------------------------------------------------------------------
// Taildrive
// ---------------------------------------------------------------------------

async fn drive_list(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_drive_list;
    let output = drive(ctx, meta, Invocation::read(["drive", "list"])).await?;
    report(SharesReport {
        shares: parse_shares(&output.stdout_str()),
    })
}

/// Where each column of a padded table starts, counted in characters.
///
/// A column begins wherever a run of spaces ends, which is what the header row
/// of `drive list` describes for every row beneath it.
fn column_starts(header: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut previous = ' ';
    for (index, character) in header.chars().enumerate() {
        if previous == ' ' && character != ' ' {
            starts.push(index);
        }
        previous = character;
    }
    starts
}

/// Cut a row at the offsets its header established and trim the padding off.
///
/// Offsets are counted in characters, not bytes, because Go pads these columns
/// by rune count. A cell the row is too short to reach is empty rather than
/// missing, which is how a share with no `as` user reads.
fn cells(row: &str, starts: &[usize]) -> Vec<String> {
    let characters: Vec<char> = row.chars().collect();
    starts
        .iter()
        .enumerate()
        .map(|(nth, &from)| {
            let to = starts.get(nth + 1).copied().unwrap_or(characters.len());
            let cell: String = characters
                .get(from..to.min(characters.len()))
                .unwrap_or_default()
                .iter()
                .collect();
            cell.trim().to_owned()
        })
        .collect()
}

/// The padded table `drive list` prints: a `name path as` header, a row of
/// dashes, then one row per share.
///
/// Every row is cut at the offsets the header gives rather than split on
/// whitespace, because splitting cannot tell a column gap from a path that
/// contains spaces, and because it silently loses any row that does not yield
/// the expected number of columns — which is every row on a platform that
/// cannot share as another user, where the last column is blank.
fn parse_shares(text: &str) -> Vec<Share> {
    let mut rows = text.lines().filter(|line| !line.trim().is_empty());
    // No header means no table: an empty listing and a missing one look the
    // same, which is what a caller can act on either way.
    let Some(header) = rows.next() else {
        return Vec::new();
    };
    let starts = column_starts(header);
    rows
        // The dashes under the header, dropped by what the row is rather than
        // by counting, so that a share is never dropped for being first.
        .filter(|row| !row.chars().all(|c| c == '-' || c == ' '))
        .map(|row| {
            let cells = cells(row, &starts);
            Share {
                name: cells.first().cloned().unwrap_or_default(),
                path: cells.get(1).cloned().unwrap_or_default(),
                as_user: cells.get(2).cloned().unwrap_or_default(),
            }
        })
        .filter(|share| !share.name.is_empty())
        .collect()
}

async fn drive_share(ctx: &ToolContext, params: DriveShareParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_drive_share;
    let path = real_path(ctx, "path", &params.path)?;
    if params.name.trim().is_empty() {
        return Err(ToolError::invalid_args("`name` cannot be empty"));
    }
    let output = drive(
        ctx,
        meta,
        Invocation::mutate(["drive", "share", &params.name, &path]),
    )
    .await?;
    report(OutcomeReport {
        subject: params.name,
        outcome: format!("`{path}` shared with the tailnet over Taildrive"),
        note: printed(ctx, &output),
    })
}

async fn drive_rename(ctx: &ToolContext, params: DriveRenameParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_drive_rename;
    if params.name.trim().is_empty() || params.new_name.trim().is_empty() {
        return Err(ToolError::invalid_args(
            "`name` and `new_name` both have to be given",
        ));
    }
    let output = drive(
        ctx,
        meta,
        Invocation::mutate(["drive", "rename", &params.name, &params.new_name]),
    )
    .await?;
    report(OutcomeReport {
        subject: params.name,
        outcome: format!("renamed to `{}`", params.new_name),
        note: printed(ctx, &output),
    })
}

async fn drive_unshare(ctx: &ToolContext, params: DriveUnshareParams) -> ToolResult<Value> {
    let meta = &metas::tailscale_drive_unshare;
    if params.name.trim().is_empty() {
        return Err(ToolError::invalid_args("`name` cannot be empty"));
    }
    let output = drive(
        ctx,
        meta,
        Invocation::mutate(["drive", "unshare", &params.name]),
    )
    .await?;
    report(OutcomeReport {
        subject: params.name,
        outcome: "no longer shared over Taildrive; the files are untouched".to_owned(),
        note: printed(ctx, &output),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::context::{PathPolicy, SelfIdentity};
    use crate::error::Redactor;
    use crate::meta::{Tier, Toolset};
    use crate::testing::{Reply, StubBackend};

    /// What `file cp --targets` prints, with this tailnet's own names replaced
    /// by documentation ones.
    const TARGETS: &str = "100.64.0.2\tworkstation\n\
        100.64.0.3\tlaptop\toffline; last seen 66h38m0s ago\n";

    /// What `drive list` prints, padded the way the client pads it.
    const SHARES: &str = "name       path                  as\n\
        ----       ----                  --\n\
        docs       /srv/shared docs      alice\n\
        photos     /srv/photos           alice\n";

    fn context(backend: Arc<StubBackend>) -> ToolContext {
        ToolContext {
            local: backend as Arc<dyn tailscale_cli::LocalBackend>,
            redactor: Redactor::default(),
            max_result_bytes: 1 << 20,
            identity: SelfIdentity::default(),
            cli_version: None,
            paths: PathPolicy::default(),
        }
    }

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

    // -- Taildrop ------------------------------------------------------------

    #[tokio::test]
    async fn sending_a_file_puts_the_target_last_and_gives_it_a_colon() {
        let (answer, argv) = against(
            Reply::ok("workstation.crt: 1.2 kB\n"),
            |ctx, p| async move { file_cp(&ctx, p).await },
            FileCpParams {
                files: vec!["/tmp/notes.txt".to_owned()],
                target: "laptop".to_owned(),
                ..FileCpParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            [
                "file",
                "cp",
                "--update-interval=0",
                "/tmp/notes.txt",
                "laptop:"
            ]
        );
        assert_eq!(answer["target"], json!("laptop:"));
        assert_eq!(answer["timeout_seconds"], json!(DEFAULT_TRANSFER_TIMEOUT));
    }

    #[tokio::test]
    async fn a_target_that_already_ends_in_a_colon_is_left_alone() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { file_cp(&ctx, p).await },
            FileCpParams {
                files: vec!["/tmp/notes.txt".to_owned()],
                target: "laptop:".to_owned(),
                ..FileCpParams::default()
            },
        )
        .await;
        assert_eq!(only(&argv).last().expect("a target"), "laptop:");
    }

    #[tokio::test]
    async fn every_flag_precedes_every_path() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { file_cp(&ctx, p).await },
            FileCpParams {
                files: vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()],
                target: "laptop".to_owned(),
                name: Some("bundle".to_owned()),
                verbose: Some(true),
                ..FileCpParams::default()
            },
        )
        .await;
        let args = only(&argv);
        let first_path = args
            .iter()
            .position(|arg| arg == "/tmp/a")
            .expect("the first file");
        assert!(
            args[2..first_path].iter().all(|arg| arg.starts_with("--")),
            "Go stops reading flags at the first positional: {args:?}"
        );
        assert_eq!(&args[first_path..], ["/tmp/a", "/tmp/b", "laptop:"]);
    }

    #[tokio::test]
    async fn standard_input_is_not_a_file_a_tool_call_can_send() {
        let error = refused(
            |ctx, p| async move { file_cp(&ctx, p).await },
            FileCpParams {
                files: vec!["-".to_owned()],
                target: "laptop".to_owned(),
                ..FileCpParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn sending_nothing_is_refused() {
        let error = refused(
            |ctx, p| async move { file_cp(&ctx, p).await },
            FileCpParams {
                target: "laptop".to_owned(),
                ..FileCpParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn a_transfer_may_be_lengthened_but_only_so_far() {
        let (answer, _) = against(
            Reply::ok(""),
            |ctx, p| async move { file_cp(&ctx, p).await },
            FileCpParams {
                files: vec!["/tmp/big.iso".to_owned()],
                target: "laptop".to_owned(),
                timeout_seconds: Some(9_999),
                ..FileCpParams::default()
            },
        )
        .await;
        assert_eq!(answer["timeout_seconds"], json!(MAX_LONG_TIMEOUT));
    }

    #[test]
    fn targets_are_read_column_by_column() {
        let targets = parse_targets(TARGETS);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].hostname, "workstation");
        assert_eq!(targets[0].status, None);
        assert_eq!(targets[1].address, "100.64.0.3");
        assert_eq!(
            targets[1].status.as_deref(),
            Some("offline; last seen 66h38m0s ago")
        );
    }

    #[tokio::test]
    async fn listing_targets_asks_for_them_and_sends_no_file() {
        let (answer, argv) = against(
            Reply::ok(TARGETS),
            |ctx, p| async move { file_targets(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(only(&argv), ["file", "cp", "--targets=true"]);
        assert_eq!(answer["targets"].as_array().expect("a list").len(), 2);
    }

    #[tokio::test]
    async fn receiving_files_never_waits_and_never_loops() {
        // The acceptance criterion, asserted where a caller could observe it:
        // the two flags that would make this call outlive the request are
        // switched off by name.
        let (answer, argv) = against(
            Reply::ok("notes.txt\n"),
            |ctx, p| async move { file_get(&ctx, p).await },
            FileGetParams {
                directory: "/tmp/inbox".to_owned(),
                ..FileGetParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            [
                "file",
                "get",
                "--conflict=skip",
                "--wait=false",
                "--loop=false",
                "--verbose=false",
                "/tmp/inbox"
            ]
        );
        assert_eq!(answer["directory"], json!("/tmp/inbox"));
        assert_eq!(answer["conflict"], json!("skip"));
    }

    #[tokio::test]
    async fn the_conflict_rule_reaches_the_client_and_the_answer() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { file_get(&ctx, p).await },
            FileGetParams {
                directory: "/tmp/inbox".to_owned(),
                conflict: Conflict::Rename,
                verbose: Some(true),
            },
        )
        .await;
        let args = only(&argv);
        assert!(args.contains(&"--conflict=rename".to_owned()), "{args:?}");
        assert!(args.contains(&"--verbose=true".to_owned()), "{args:?}");
        assert_eq!(answer["conflict"], json!("rename"));
    }

    #[tokio::test]
    async fn an_empty_inbox_is_an_answer_rather_than_a_failure() {
        let (answer, _) = against(
            Reply::ok(""),
            |ctx, p| async move { file_get(&ctx, p).await },
            FileGetParams {
                directory: "/tmp/inbox".to_owned(),
                ..FileGetParams::default()
            },
        )
        .await;
        assert_eq!(answer["printed"], json!(null));
    }

    // -- certificates --------------------------------------------------------

    #[tokio::test]
    async fn issuing_a_certificate_names_both_files() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { cert(&ctx, p).await },
            CertParams {
                domain: "workstation.example-tailnet.ts.net".to_owned(),
                cert_file: "/etc/ssl/node.crt".to_owned(),
                key_file: "/etc/ssl/node.key".to_owned(),
                ..CertParams::default()
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            [
                "cert",
                "--cert-file=/etc/ssl/node.crt",
                "--key-file=/etc/ssl/node.key",
                "workstation.example-tailnet.ts.net"
            ]
        );
        assert_eq!(answer["timeout_seconds"], json!(DEFAULT_CERT_TIMEOUT));
    }

    #[tokio::test]
    async fn no_certificate_path_may_be_standard_output() {
        // The acceptance criterion the tool exists to hold: `-` is how the
        // client is told to print the private key, and no call can ask for it.
        for (cert_file, key_file) in [("-", "/etc/ssl/node.key"), ("/etc/ssl/node.crt", "-")] {
            let error = refused(
                |ctx, p| async move { cert(&ctx, p).await },
                CertParams {
                    domain: "workstation.example-tailnet.ts.net".to_owned(),
                    cert_file: cert_file.to_owned(),
                    key_file: key_file.to_owned(),
                    ..CertParams::default()
                },
            )
            .await;
            assert_eq!(error.code, ErrorCode::InvalidArgs, "{cert_file} {key_file}");
        }
    }

    #[tokio::test]
    async fn a_certificate_needs_both_paths_to_be_real() {
        let error = refused(
            |ctx, p| async move { cert(&ctx, p).await },
            CertParams {
                domain: "workstation.example-tailnet.ts.net".to_owned(),
                cert_file: "/etc/ssl/node.crt".to_owned(),
                key_file: "   ".to_owned(),
                ..CertParams::default()
            },
        )
        .await;
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[tokio::test]
    async fn a_minimum_validity_is_rendered_as_a_duration() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { cert(&ctx, p).await },
            CertParams {
                domain: "workstation.example-tailnet.ts.net".to_owned(),
                cert_file: "/etc/ssl/node.crt".to_owned(),
                key_file: "/etc/ssl/node.key".to_owned(),
                min_validity_seconds: Some(604_800),
                ..CertParams::default()
            },
        )
        .await;
        let args = only(&argv);
        assert!(
            args.contains(&"--min-validity=604800s".to_owned()),
            "{args:?}"
        );
    }

    #[tokio::test]
    async fn the_key_never_appears_in_the_answer() {
        // Whatever the client prints reaches `printed`; what is reported for
        // the key is its path and nothing else.
        let (answer, _) = against(
            Reply::ok(""),
            |ctx, p| async move { cert(&ctx, p).await },
            CertParams {
                domain: "workstation.example-tailnet.ts.net".to_owned(),
                cert_file: "/etc/ssl/node.crt".to_owned(),
                key_file: "/etc/ssl/node.key".to_owned(),
                ..CertParams::default()
            },
        )
        .await;
        assert_eq!(answer["key_file"], json!("/etc/ssl/node.key"));
        assert!(
            !answer.to_string().contains("PRIVATE KEY"),
            "{answer:?} carries key material"
        );
    }

    // -- local configuration -------------------------------------------------

    #[tokio::test]
    async fn writing_metrics_names_the_file_it_wrote() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { metrics_write(&ctx, p).await },
            MetricsWriteParams {
                path: "/var/lib/node-exporter/tailscaled.prom".to_owned(),
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            ["metrics", "write", "/var/lib/node-exporter/tailscaled.prom"]
        );
        assert_eq!(
            answer["subject"],
            json!("/var/lib/node-exporter/tailscaled.prom")
        );
    }

    #[tokio::test]
    async fn kubeconfig_puts_its_flag_before_the_hostname() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { configure_kubeconfig(&ctx, p).await },
            KubeconfigParams {
                hostname: "cluster".to_owned(),
                http: Some(true),
            },
        )
        .await;
        assert_eq!(
            only(&argv),
            ["configure", "kubeconfig", "--http=true", "cluster"]
        );
    }

    #[tokio::test]
    async fn reloading_policy_asks_for_json_and_forwards_the_document() {
        let (answer, argv) = against(
            Reply::ok(r#"{"Summary":{"Scope":"Device"},"Settings":{}}"#),
            |ctx, p| async move { syspolicy_reload(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(only(&argv), ["syspolicy", "reload", "--json=true"]);
        assert_eq!(answer["Summary"]["Scope"], json!("Device"));
    }

    // -- Taildrive -----------------------------------------------------------

    #[test]
    fn shares_survive_a_path_with_a_space_in_it() {
        let shares = parse_shares(SHARES);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "docs");
        assert_eq!(shares[0].path, "/srv/shared docs");
        assert_eq!(shares[0].as_user, "alice");
        assert_eq!(shares[1].path, "/srv/photos");
    }

    // -- The path allow-list seam -------------------------------------------

    /// Unrestricted is what ships, so the seam costs a caller nothing today.
    #[test]
    fn any_path_is_allowed_while_the_allow_list_is_off() {
        let ctx = context(Arc::new(StubBackend::ok("")));
        assert!(real_path(&ctx, "path", "/anywhere/at/all").is_ok());
    }

    /// Populating the policy is the whole of switching it on: every tool that
    /// takes a path already asks it.
    #[test]
    fn a_populated_allow_list_confines_a_path_to_its_roots() {
        let mut ctx = context(Arc::new(StubBackend::ok("")));
        ctx.paths = PathPolicy::Within(vec![PathBuf::from("/srv/exports")]);

        assert!(real_path(&ctx, "path", "/srv/exports/report.txt").is_ok());

        let refused = real_path(&ctx, "path", "/etc/shadow").unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotPermitted);
    }

    /// A root check that resolved nothing would be walked straight out of.
    #[test]
    fn a_parent_directory_step_cannot_leave_an_allowed_root() {
        let mut ctx = context(Arc::new(StubBackend::ok("")));
        ctx.paths = PathPolicy::Within(vec![PathBuf::from("/srv/exports")]);

        let refused = real_path(&ctx, "path", "/srv/exports/../../etc/shadow").unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotPermitted);
    }

    #[test]
    fn an_empty_share_table_is_an_empty_list() {
        assert!(parse_shares("name    path    as\n----    ----    --\n").is_empty());
    }

    /// On a platform that cannot share as another user the client leaves the
    /// last column blank, and the row must still be reported.
    #[test]
    fn a_share_with_no_as_user_is_still_a_share() {
        let shares = parse_shares(
            "name       path                  as\n\
             ----       ----                  --\n\
             docs       /srv/shared docs      \n\
             photos     /srv/photos\n",
        );
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].path, "/srv/shared docs");
        assert_eq!(shares[0].as_user, "");
        assert_eq!(shares[1].name, "photos");
        assert_eq!(shares[1].as_user, "");
    }

    /// Two spaces inside a path are a path, not a column break: splitting on
    /// whitespace would have lost the row entirely.
    #[test]
    fn a_path_may_hold_consecutive_spaces() {
        let shares = parse_shares(
            "name       path                  as\n\
             ----       ----                  --\n\
             docs       /srv/two  spaces      alice\n",
        );
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].path, "/srv/two  spaces");
        assert_eq!(shares[0].as_user, "alice");
    }

    /// The header is dropped for being the header, so a share is not dropped
    /// for happening to be called one of its column names.
    #[test]
    fn a_share_called_name_is_not_mistaken_for_the_header() {
        let shares = parse_shares(
            "name       path                  as\n\
             ----       ----                  --\n\
             name       /srv/awkward          alice\n",
        );
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "name");
        assert_eq!(shares[0].path, "/srv/awkward");
    }

    #[tokio::test]
    async fn listing_shares_reads_the_table() {
        let (answer, argv) = against(
            Reply::ok(SHARES),
            |ctx, p| async move { drive_list(&ctx, p).await },
            NoParams {},
        )
        .await;
        assert_eq!(only(&argv), ["drive", "list"]);
        assert_eq!(answer["shares"].as_array().expect("a list").len(), 2);
    }

    #[tokio::test]
    async fn sharing_a_directory_passes_the_name_then_the_path() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { drive_share(&ctx, p).await },
            DriveShareParams {
                name: "docs".to_owned(),
                path: "/srv/docs".to_owned(),
            },
        )
        .await;
        assert_eq!(only(&argv), ["drive", "share", "docs", "/srv/docs"]);
        assert_eq!(answer["subject"], json!("docs"));
    }

    #[tokio::test]
    async fn renaming_passes_the_old_name_then_the_new() {
        let (_, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { drive_rename(&ctx, p).await },
            DriveRenameParams {
                name: "docs".to_owned(),
                new_name: "handbook".to_owned(),
            },
        )
        .await;
        assert_eq!(only(&argv), ["drive", "rename", "docs", "handbook"]);
    }

    #[tokio::test]
    async fn unsharing_names_only_the_share() {
        let (answer, argv) = against(
            Reply::ok(""),
            |ctx, p| async move { drive_unshare(&ctx, p).await },
            DriveUnshareParams {
                name: "docs".to_owned(),
            },
        )
        .await;
        assert_eq!(only(&argv), ["drive", "unshare", "docs"]);
        assert!(
            answer["outcome"]
                .as_str()
                .expect("an outcome")
                .contains("untouched"),
            "{answer:?}"
        );
    }

    #[tokio::test]
    async fn a_build_without_taildrive_says_so_rather_than_failing() {
        // The macOS GUI packaging carries the subcommands and refuses them.
        // That is a fact about the build, so it is not reported as though the
        // request were wrong (DECISIONS Q31).
        let backend = Arc::new(StubBackend::always(Reply::failed(
            1,
            "Taildrive CLI commands are not supported when using the macOS GUI app.",
        )));
        let ctx = context(Arc::clone(&backend));
        let error = drive_list(&ctx, NoParams {})
            .await
            .expect_err("the handler refuses");
        assert_eq!(error.code, ErrorCode::UnsupportedPlatform);
    }

    #[tokio::test]
    async fn an_ordinary_taildrive_failure_is_still_a_failure() {
        let backend = Arc::new(StubBackend::always(Reply::failed(
            1,
            "share \"docs\" does not exist",
        )));
        let ctx = context(Arc::clone(&backend));
        let error = drive_unshare(
            &ctx,
            DriveUnshareParams {
                name: "docs".to_owned(),
            },
        )
        .await
        .expect_err("the handler refuses");
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    // -- the table -----------------------------------------------------------

    #[test]
    fn the_toolset_is_the_size_it_was_scoped_to() {
        assert_eq!(entries().len(), 11);
        for entry in entries() {
            assert_eq!(entry.meta.toolset, Toolset::LocalFiles);
        }
    }

    #[test]
    fn nothing_here_is_reachable_from_a_read_only_session_except_the_two_readers() {
        // Every tool in this toolset touches the local filesystem, so the
        // read tier holds only the two that read and change nothing.
        let readers: Vec<&str> = entries()
            .iter()
            .filter(|entry| entry.meta.tier == Tier::Read)
            .map(|entry| entry.meta.name)
            .collect();
        assert_eq!(readers, ["tailscale_file_targets", "tailscale_drive_list"]);
    }

    #[test]
    fn withdrawing_a_share_is_the_one_destructive_tool() {
        let destructive: Vec<&str> = entries()
            .iter()
            .filter(|entry| entry.meta.tier == Tier::Destructive)
            .map(|entry| entry.meta.name)
            .collect();
        assert_eq!(destructive, ["tailscale_drive_unshare"]);
    }

    #[test]
    fn every_description_says_which_files_it_touches() {
        // The acceptance criterion a caller relies on: a tool that reads or
        // writes the local filesystem says so where the tool is chosen, not
        // only where it fails.
        //
        // `syspolicy_reload` is the one tool here that takes no path and
        // touches no file of the caller's choosing — it reloads settings the
        // node already has — so it is not asked to claim otherwise.
        for entry in entries()
            .iter()
            .filter(|entry| entry.meta.name != "tailscale_syspolicy_reload")
        {
            let summary = entry.meta.summary.to_lowercase();
            assert!(
                summary.contains("local filesystem")
                    || summary.contains("file")
                    || summary.contains("directories"),
                "`{}` does not say what it touches: {}",
                entry.meta.name,
                entry.meta.summary
            );
        }
    }
}
