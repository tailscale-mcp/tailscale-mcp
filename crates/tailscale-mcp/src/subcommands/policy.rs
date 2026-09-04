//! Validating and deploying a policy file from a terminal.
//!
//! **The same code path as the tools**, deliberately: these go through the
//! registry by name rather than calling a handler, so the version guard, the
//! error codes and the request shaping are the ones a tool call gets. A
//! pipeline checking a policy and an agent writing one should not be able to
//! disagree about what is valid (ticket 25, Q94).
//!
//! **Quiet on success.** A pipeline's log is read when something went wrong,
//! and a subcommand that prints a paragraph on every green run trains people
//! not to read it.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::config::{Config, PolicyCommand};
use crate::context::ToolContext;
use crate::gating::Gate;
use crate::meta::{Tier, Toolset};
use crate::registry::Registry;
use crate::server::Backends;

use super::Report;
use crate::tools::common::pretty;

pub async fn policy(config: &Config, backends: Backends, action: &PolicyCommand) -> Report {
    let path = match action {
        PolicyCommand::Check { file } | PolicyCommand::Deploy { file } => file.as_path(),
    };
    let document = match std::fs::read_to_string(path) {
        Ok(document) => document,
        Err(error) => {
            return Report {
                text: format!("could not read {}: {error}\n", path.display()),
                ok: false,
            };
        }
    };

    // Built the same way a session is, so that a policy deployed from a
    // pipeline goes through the client an agent's call would have gone
    // through, with the same base address, timeout and tailnet.
    let context = match crate::server::build(config, crate::tools::entries(), backends).await {
        Ok(startup) => Arc::clone(startup.server.context()),
        Err(error) => {
            return Report {
                text: format!("{error}\n"),
                ok: false,
            };
        }
    };

    match action {
        PolicyCommand::Check { .. } => check_policy(&context, &document).await,
        PolicyCommand::Deploy { .. } => deploy_policy(&context, &document, path).await,
    }
}

async fn check_policy(
    context: &std::sync::Arc<crate::context::ToolContext>,
    document: &str,
) -> Report {
    match call(
        context,
        "tailnet_policy_validate",
        json!({"policy": document}),
    )
    .await
    {
        Ok(_) => Report::ok(""),
        Err(refused) => Report {
            text: refused.text,
            ok: false,
        },
    }
}

/// Read the version identifier, then write against it.
///
/// The read happens here and not in the caller's head: the whole point of the
/// guard is that the document being replaced is the one that was read, and a
/// pipeline that carried an `etag` from an earlier step would be guarding
/// against the wrong thing.
async fn deploy_policy(context: &Arc<ToolContext>, document: &str, path: &Path) -> Report {
    let current = match call(context, "tailnet_policy_get", json!({})).await {
        Ok(current) => current,
        Err(refused) => {
            return Report {
                text: refused.text,
                ok: false,
            };
        }
    };
    let etag = current["etag"].as_str().map(str::to_owned);

    let mut args = json!({"policy": document});
    match &etag {
        Some(etag) => args["etag"] = Value::String(etag.clone()),
        // No `ETag` came back, which is what an untouched tailnet looks like:
        // there is no version to guard against because nothing has been
        // written yet.
        None => args["over_default"] = Value::Bool(true),
    }

    match call(context, "tailnet_policy_set", args).await {
        Ok(_) => Report::ok(""),
        // The conflict advice belongs to a conflict and to nothing else. A
        // malformed document already says what is wrong with it, and telling
        // somebody to merge would send them looking for a change nobody made.
        Err(Refused {
            text,
            conflict: false,
        }) => Report { text, ok: false },
        Err(Refused { text, .. }) => Report {
            text: format!(
                "{text}\nThe policy was read at {}, so somebody else wrote to this tailnet in \
                 between. Read it again, merge {} into what is there now, and deploy that.\n",
                etag.as_deref().unwrap_or("no version"),
                path.display()
            ),
            ok: false,
        },
    }
}

/// A refused call, and whether it was refused for the one reason a person is
/// told what to do about.
struct Refused {
    text: String,
    conflict: bool,
}

/// One tool call, with the tool's own error turned into the text a pipeline
/// reads.
async fn call(context: &Arc<ToolContext>, tool: &str, args: Value) -> Result<Value, Refused> {
    let registry = match Registry::new(crate::tools::entries()) {
        Ok(registry) => registry,
        Err(error) => {
            return Err(Refused {
                text: format!("{error}\n"),
                conflict: false,
            });
        }
    };
    // Destructive, because this is not an MCP session and the operator asked
    // for this by typing it: the tier exists to constrain an agent, and there
    // is no agent here.
    let gate = Gate::unchecked(
        BTreeSet::from([Toolset::TailnetPolicy]),
        Tier::Destructive,
        BTreeSet::new(),
    );
    let arguments = args.as_object().cloned().unwrap_or_default();
    let refused = |error: crate::error::ToolError| Refused {
        conflict: error.code == crate::error::ErrorCode::Conflict,
        text: pretty(&error.to_value()) + "\n",
    };
    let (entry, arguments) = registry.resolve(tool, arguments, &gate).map_err(refused)?;
    (entry.invoke)(context.clone(), arguments)
        .await
        .map_err(refused)
}
