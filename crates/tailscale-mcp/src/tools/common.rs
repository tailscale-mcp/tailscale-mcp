//! The small things every toolset does the same way.
//!
//! Building an argument, reporting what a command said on standard error, and
//! turning a report struct into the value a tool answers with. None of it is
//! interesting on its own; it lives here so that three modules cannot drift
//! into three spellings of it.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tailscale_cli::{Invocation, Output};

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

/// The same for a number, which has no natural empty value to elide.
pub fn push_number(args: &mut Vec<String>, name: &str, value: Option<u16>) {
    if let Some(value) = value {
        args.push(format!("--{name}={value}"));
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

/// The first URL in a block of text.
///
/// Three commands hand a person something to open rather than doing it
/// themselves: `login` and an interactive `up` print a login URL, and `serve`
/// prints the address the handler is now reachable at.
pub fn find_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("https://") || word.starts_with("http://"))
        .map(|url| url.trim_end_matches(['.', ',']).to_owned())
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
