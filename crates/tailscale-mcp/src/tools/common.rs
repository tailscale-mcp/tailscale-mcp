//! The small things every toolset does the same way.
//!
//! Building an argument, reporting what a command said on standard error, and
//! turning a report struct into the value a tool answers with. None of it is
//! interesting on its own; it lives here so that three modules cannot drift
//! into three spellings of it.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tailscale_cli::{Invocation, Output, SecretFile};

use crate::cli;
use crate::context::ToolContext;
use crate::error::{ErrorCode, ToolError, ToolResult};
use crate::meta::ToolMeta;

/// Render a boolean flag the way Go's flag package needs it: joined to its
/// value, so that it cannot be mistaken for a positional argument.
pub fn flag(name: &str, value: bool) -> String {
    format!("--{name}={value}")
}

/// Add `--name=value` when the caller gave a value, and nothing when it did
/// not. This is what keeps a call that changes one thing from restating the
/// rest.
pub fn push_bool(args: &mut Vec<String>, name: &str, value: Option<bool>) {
    if let Some(value) = value {
        args.push(flag(name, value));
    }
}

pub fn push_text(args: &mut Vec<String>, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        args.push(format!("--{name}={value}"));
    }
}

/// The CLI takes repeated values as one comma-separated argument.
pub fn push_list(args: &mut Vec<String>, name: &str, value: Option<&[String]>) {
    if let Some(values) = value {
        args.push(format!("--{name}={}", values.join(",")));
    }
}

/// Whatever a command said on standard error, when it said anything.
///
/// Carried on the answer rather than raised, because a command that succeeded
/// while warning about something has not failed — `netcheck` and `debug
/// hostinfo` both talk on standard error as a matter of course.
pub fn note(ctx: &ToolContext, stderr: &str) -> Option<String> {
    let redacted = ctx.redactor.apply(stderr);
    let trimmed = redacted.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// The text a command printed, with blank lines and comment lines dropped.
/// A JSON value as the text a person reads: indented, so the shape shows.
///
/// The fallback cannot happen for a value that came from a parser, and is a
/// fallback rather than an `unwrap` because `unwrap` is denied here.
pub fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Everything the client printed, redacted, standard output first.
///
/// Which stream a command reports on is not a property a caller should have to
/// know: `serve` names the new address on standard output and warns on
/// standard error, `file cp` reports progress on either depending on the
/// version, and a caller wants whichever it was.
pub fn printed(ctx: &ToolContext, output: &Output) -> Option<String> {
    let stdout = output.stdout_str();
    let joined = [stdout.as_ref(), output.stderr.as_str()]
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    note(ctx, &joined)
}

/// How much longer the client is given than the caller asked to wait, so that
/// a timeout is this server's own and not a command killed mid-sentence.
const GRACE: u64 = 5;

/// Bound a caller's wait, and give the client a little longer than we wait.
///
/// Returns the bound the caller is told about and the one the child is given,
/// which differ by the grace above: the caller is answered in its own terms.
pub fn bounded_wait(requested: Option<u64>, default: u64, longest: u64) -> (u64, Duration) {
    let seconds = requested.unwrap_or(default).clamp(1, longest);
    (seconds, Duration::from_secs(seconds + GRACE))
}

pub fn lines(text: &str) -> impl DoubleEndedIterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// Every whitespace-separated token beginning with one of `prefixes`, in the
/// order the text printed them and with trailing sentence punctuation removed.
///
/// The client hands back its products inside prose written for a person: a
/// login URL in a sentence, the disablement secrets `lock init` just minted,
/// the auth key `lock sign` countersigned. Each of those is recognisable by its
/// own prefix, which is part of the value's format, whereas the sentence around
/// it is not and changes between releases. So this looks for the prefix and
/// ignores the prose.
///
/// Whatever it finds is always offered *alongside* the client's own text rather
/// than instead of it, so a layout change costs the caller nothing.
pub fn tokens_with_prefix(text: &str, prefixes: &[&str]) -> Vec<String> {
    text.split_whitespace()
        .filter(|word| prefixes.iter().any(|prefix| word.starts_with(prefix)))
        .map(|word| word.trim_end_matches(['.', ',']).to_owned())
        .collect()
}

/// The first URL in a block of text.
///
/// Three commands hand a person something to open rather than doing it
/// themselves: `login` and an interactive `up` print a login URL, and `serve`
/// prints the address the handler is now reachable at.
pub fn find_url(text: &str) -> Option<String> {
    tokens_with_prefix(text, &["https://", "http://"])
        .into_iter()
        .next()
}

/// Refuse a path the caller cannot have meant, naming which one it was.
///
/// `-` is the client's spelling of "a stream rather than a file": the private
/// key on standard output for `cert`, standard input for `file cp`, the
/// disablement secret typed at a prompt for `lock disable`. A tool call has
/// none of those, so every command that offers the spelling is refused it here.
///
/// The configured [`PathPolicy`](crate::context::PathPolicy) is asked last, so
/// that a caller who named something impossible is told that rather than told
/// about the policy.
pub fn real_path(ctx: &ToolContext, what: &str, path: &str) -> ToolResult<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ToolError::invalid_args(format!("`{what}` cannot be empty")));
    }
    if trimmed == "-" {
        return Err(ToolError::invalid_args(format!(
            "`{what}` has to be a path on this machine; `-` means a stream, which a tool call has none of"
        )));
    }
    if !ctx.paths.permits(Path::new(trimmed)) {
        return Err(ToolError::new(
            ErrorCode::NotPermitted,
            format!("`{what}` is outside the paths this server may use"),
        )
        .with_hint("Name a path under one of the server's configured roots."));
    }
    // The validated spelling, so that what was checked is what runs: a path
    // the caller padded with a newline is the path it meant.
    Ok(trimmed.to_owned())
}

/// A secret on its way to the CLI, held open for as long as the call takes.
///
/// Answers with the *value* the argument should carry and the file keeping it
/// alive; how that value is spelled is the caller's business, because the
/// commands disagree — `set` wants `--auth-key=<value>` and `lock sign` wants a
/// bare positional. Drop the file once the command has returned, not before.
///
/// A value that is already a `file:` reference is passed through: it is a path,
/// not a secret, and re-copying it would gain nothing. Anything else is written
/// to a private temporary file, so that the secret itself never reaches an
/// argument list that `ps` can read.
pub fn secret_value(what: &str, value: &str) -> ToolResult<(String, Option<SecretFile>)> {
    if value.starts_with("file:") {
        return Ok((value.to_owned(), None));
    }
    let file = SecretFile::new(value).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("the {what} could not be written to a private file: {e}"),
        )
    })?;
    Ok((file.arg(), Some(file)))
}

/// Refuse a setting that does not exist on this operating system, naming both
/// the setting and where we are (DECISIONS Q20).
pub fn only_on(setting: &str, platforms: &[&str]) -> ToolResult<()> {
    if platforms.contains(&std::env::consts::OS) {
        return Ok(());
    }
    Err(ToolError::new(
        ErrorCode::UnsupportedPlatform,
        format!(
            "`{setting}` is a {} preference, and this node runs {}",
            platforms.join(" or "),
            std::env::consts::OS
        ),
    ))
}

/// A caller's identifier, checked before it becomes part of a URL.
///
/// The control plane accepts a device by either of two spellings — the node id
/// `n1234567CNTRL` or the numeric one — and an attribute by a `custom:`-
/// prefixed key, so the tools take whatever the caller has rather than making
/// it convert. What they will not take is anything that could change which
/// endpoint is being called: a segment carrying `/`, `?`, `#` or a space is a
/// path being written by the caller rather than an identifier, and is refused
/// here rather than escaped, because there is no legitimate identifier this
/// rejects and an escaped one would only fail further away.
pub fn path_segment(what: &str, value: &str) -> ToolResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ToolError::invalid_args(format!("`{what}` cannot be empty")));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '@');
    if let Some(bad) = trimmed.chars().find(|c| !allowed(*c)) {
        return Err(ToolError::invalid_args(format!(
            "`{what}` contains {bad:?}, which cannot appear in an identifier"
        )));
    }
    // `.` is in the allow-list because identifiers contain it, and `.` and `..`
    // are made of nothing else. A dot segment does not sit in the path: it
    // rewrites it when the URL is parsed, so `/api/v2/device/../routes`
    // reaches `/api/v2/routes` and a deletion aimed at `..` reaches
    // `/api/v2/`. That is the whole failure this guard exists to prevent, and
    // it is the one spelling the character check lets through.
    if trimmed.chars().all(|c| c == '.') {
        return Err(ToolError::invalid_args(format!(
            "`{what}` is `{trimmed}`, which is not an identifier"
        )));
    }
    Ok(trimmed.to_owned())
}

/// What an endpoint that answers with nothing answers with here.
///
/// The control plane sends an empty body for a deletion and for six of the
/// writes, which reaches a tool as `null`. A caller cannot tell that apart
/// from a tool that lost its answer, so the tool says what it did instead
/// (Q67). Deliberately not shaped like a resource: `done` is a phrase for a
/// reader rather than a status to branch on, and `about` carries only the
/// identifiers the call was given, so nothing here can be mistaken for
/// something the control plane said.
#[derive(Debug, Serialize)]
pub struct Done {
    done: &'static str,
    #[serde(flatten)]
    about: serde_json::Map<String, Value>,
}

impl Done {
    pub fn new(done: &'static str) -> Self {
        Self {
            done,
            about: serde_json::Map::new(),
        }
    }

    pub fn about(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.about.insert(name.to_owned(), value.into());
        self
    }
}

/// Hold a value to a documented string's known values, or refuse quoting them.
///
/// Q60 keeps the description's enumerations as `&[&str]` constants beside the
/// models rather than as Rust enums, and the drift test holds each constant to
/// the description — so a value this accepts is a value the description knows,
/// and a refusal names today's whole list. Checked here rather than left to the
/// control plane because a 400 with no vocabulary in it tells a caller nothing
/// about what to send instead.
///
/// `what` is the caller's own parameter name, so a refusal points at the
/// argument they wrote rather than at whatever the description calls it.
pub fn one_of(what: &str, value: &str, allowed: &[&str]) -> ToolResult<String> {
    if allowed.contains(&value) {
        return Ok(value.to_owned());
    }
    Err(ToolError::invalid_args(format!(
        "`{what}` is one of {}; `{value}` is none of them",
        allowed.join(", ")
    )))
}

/// Every entry of a list, trimmed, with nothing blank among them.
///
/// A blank entry is not an empty list: several endpoints here take an empty
/// list as a documented instruction to remove everything, while `[""]` is a
/// caller that meant that and got it wrong — and the control plane answers it
/// with a 400 naming no entry.
pub fn each_present(what: &str, given: Vec<String>) -> ToolResult<Vec<String>> {
    given
        .into_iter()
        .map(|entry| {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                return Err(ToolError::invalid_args(format!(
                    "`{what}` has an empty entry; send `[]` to remove everything"
                )));
            }
            Ok(trimmed.to_owned())
        })
        .collect()
}

/// Forward what the control plane answered, or say what was done if it said
/// nothing.
///
/// Several control-plane writes are documented as answering with the resource
/// as it now stands, and answer with an empty body in practice. Forwarding the
/// answer is worth more to a caller than a report that the call worked, so the
/// answer wins when there is one; an empty body falls back to the report the
/// rest of this surface uses (Q67), rather than to `null`.
pub fn answered_or(answer: Value, otherwise: Done) -> ToolResult<Value> {
    match answer {
        Value::Null => report(otherwise),
        answer => Ok(answer),
    }
}

/// Turn a serialisable report into the value a tool answers with.
///
/// Serialisation of a plain struct of owned strings and numbers cannot fail, so
/// the fallback is unreachable; it exists because a panic in a tool handler
/// would take the whole session down.
pub fn report(value: impl Serialize) -> ToolResult<Value> {
    serde_json::to_value(value).map_err(|e| {
        ToolError::new(
            ErrorCode::CliFailed,
            format!("the report did not build: {e}"),
        )
    })
}

/// Run a command that prints a JSON document, and return the document.
///
/// Tolerant of a non-zero exit *when a document still came back*: `status`
/// prints a complete report while telling the shell that the node is not
/// running. A refusal with nothing to parse is read as an ordinary failure.
///
/// Only standard output is parsed. `netcheck` and several `debug` commands
/// write log lines to standard error while writing clean JSON to stdout.
pub async fn document(
    ctx: &ToolContext,
    meta: &ToolMeta,
    invocation: Invocation,
) -> ToolResult<Value> {
    let display = invocation.display();
    let output = cli::run_tolerant(ctx, meta, invocation).await?;
    let stdout = output.stdout_str();
    match serde_json::from_str::<Value>(stdout.trim()) {
        Ok(value) => Ok(value),
        Err(_) if !output.success() => Err(cli::command_failure(ctx, meta, &display, &output)),
        Err(e) => Err(ToolError::new(
            ErrorCode::CliFailed,
            format!("`{display}` did not print JSON: {e}"),
        )),
    }
}

/// A document that is an object, forwarded as it stands.
///
/// A document that is not an object is wrapped rather than rejected: a client
/// destructures the answer, and a bare array or string would leave it nothing
/// to destructure.
pub async fn object(
    ctx: &ToolContext,
    meta: &ToolMeta,
    invocation: Invocation,
) -> ToolResult<Value> {
    let value = document(ctx, meta, invocation).await?;
    Ok(if value.is_object() {
        value
    } else {
        json!({ "document": value })
    })
}

/// A subcommand this server does not offer, and why.
///
/// The reason is not decoration. It is what the passthrough tells a caller that
/// asked for one of these, and what a later reader of such a list has to argue
/// with before adding one back.
///
/// It lives here rather than beside either list because there are two:
/// [`crate::tools::local_debug::EXCLUDED`] holds the hidden `debug` subcommands
/// that never become tools, and the passthrough's own `EXCLUDED` holds the
/// documented commands this server will not run at all. The passthrough refuses
/// both from one iterator — [`crate::tools::passthrough::excluded`] — which
/// needs them to be one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Excluded {
    /// The subcommand as it is written after `tailscale`, words separated by
    /// spaces, so that a caller's argument list can be matched against it.
    pub path: &'static str,
    /// Why it is not offered, phrased to be read by whoever asked for it.
    pub reason: &'static str,
}

/// Refuse an argument that raises a call above the session's tier.
///
/// For the tools whose row is a floor rather than the whole truth (`varying:
/// true`): the gate has already let the call through on the row's tier, and
/// this asks the same question the gate asked, about the argument it could not
/// see (Q70).
pub fn require_destructive(ctx: &ToolContext, what: &str) -> ToolResult<()> {
    if ctx.max_tier < crate::meta::Tier::Destructive {
        return Err(ToolError::not_permitted(what, "--allow-destructive"));
    }
    Ok(())
}

/// The confirmation a call aimed at this node has to carry.
///
/// Flattened into the parameters of every tool whose row says
/// `severs_local: true`, so that the field, its documentation and its meaning
/// are written once rather than six times. The registry refuses to build a
/// table where a tool claims to sever the local node without offering this,
/// which is what keeps the flag and the schema in step.
#[derive(Debug, Default, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct SelfConfirmation {
    /// Set to `true` to confirm an operation on the device this server runs
    /// on, which would cut the connection this call is being made over.
    /// Needed only then: any other device is an ordinary call and ignores it.
    #[serde(default)]
    pub confirm: Option<bool>,
}

/// Refuse a call aimed at this node that did not say it meant to be.
///
/// `what` names the operation as the refusal should read it — "deleting this
/// node's own device", not the tool name — because the caller is being told
/// what would happen, not which function it reached.
///
/// Identity is read from local status and refreshed as it ages, and a session
/// with no local surface has none: `SelfIdentity::default()` matches nothing,
/// so the call is treated as ordinary. That is deliberate. The alternative is
/// refusing every device operation on a suspicion the server cannot check,
/// which would make the tailnet surface unusable on its own for the sake of a
/// guess (Q83).
pub async fn not_at_ourselves(
    ctx: &ToolContext,
    what: &str,
    target: &str,
    confirmation: &SelfConfirmation,
) -> ToolResult<()> {
    if confirmation.confirm == Some(true) || !ctx.names_us(target).await {
        return Ok(());
    }
    Err(ToolError::new(
        ErrorCode::ConfirmationRequired,
        format!(
            "`{target}` is the device this server runs on, so {what} can cut this \
             session off from it, and this call did not say it meant to"
        ),
    )
    .with_hint("Pass `confirm: true` to do it anyway."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dot_segment_is_not_an_identifier() {
        // The one spelling the character allow-list lets through, and the
        // worst: `..` is normalised away when the URL is parsed, so it does
        // not address a device — it addresses a different endpoint.
        for bad in [".", "..", "...", " .. "] {
            let error = path_segment("device_id", bad).expect_err("a dot segment");
            assert_eq!(
                serde_json::to_value(&error).expect("reportable")["code"],
                serde_json::json!("invalid_args"),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_identifier_containing_a_dot_is_still_an_identifier() {
        // MagicDNS names reach these tools too, and dropping every dot would
        // reject the commonest way a person names a device.
        for good in [
            "n1111111CNTRL",
            "123456789",
            "laptop.example-tailnet.ts.net",
            "custom:a.b",
        ] {
            assert_eq!(
                path_segment("device_id", good).expect("a valid identifier"),
                good
            );
        }
    }
}
