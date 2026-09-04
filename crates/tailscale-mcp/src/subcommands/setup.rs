//! A configuration snippet for an MCP client.
//!
//! **Prints and writes nothing.** A client's configuration file has the
//! operator's own edits in it, and a server that rewrote one would be making a
//! decision that is not its to make (ticket 24).
//!
//! **Each client's own shape**, because a snippet in the wrong shape is a
//! snippet that does not work, and the criterion is that pasting it "produces
//! a working server". Three of the five take `mcpServers`, VS Code takes
//! `servers`, Zed takes `context_servers`, and Claude Code's `add-json` takes
//! the server object on its own rather than any wrapper (Q98).

use crate::config::{Client, Config, NOT_IN_A_SNIPPET};

use super::Report;
use crate::tools::common::pretty;

/// Where the snippet goes, which is the part a person cannot guess.
fn goes_in(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => ".mcp.json in the project, or the user configuration",
        Client::ClaudeDesktop => "claude_desktop_config.json",
        Client::Vscode => ".vscode/mcp.json in the project, or the user settings",
        Client::Cursor => "~/.cursor/mcp.json, or .cursor/mcp.json in the project",
        Client::Zed => "the Zed settings file",
    }
}

/// The key a client's configuration keeps its servers under.
fn servers_key(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode | Client::ClaudeDesktop | Client::Cursor => "mcpServers",
        Client::Vscode => "servers",
        Client::Zed => "context_servers",
    }
}

/// A configuration snippet for a client, printed and not written.
///
/// Deliberately writes nothing. A client's configuration file is one an
/// operator has their own edits in, and a server that rewrote it would be
/// making a decision that is not its to make — so this prints, and the operator
/// pastes (ticket 24).
pub fn setup(client: Client, config: &Config) -> Report {
    let mut env = serde_json::Map::new();
    for (key, value) in config.changed_settings() {
        if NOT_IN_A_SNIPPET.contains(&key) {
            continue;
        }
        env.insert(key.to_owned(), serde_json::Value::String(value));
    }

    let command = env!("CARGO_PKG_NAME");
    let mut server = serde_json::json!({"command": command, "args": []});
    if !env.is_empty() {
        server["env"] = serde_json::Value::Object(env);
    }

    let snippet = serde_json::json!({servers_key(client): {"tailscale": server}});
    let mut text = format!(
        "# {}: put this in {}\n{}\n",
        name_of(client),
        goes_in(client),
        pretty(&snippet)
    );
    if client == Client::ClaudeCode {
        // `claude mcp add-json` takes the server object, not the wrapper the
        // file keeps it under, so a snippet offered for that command has to
        // say which half.
        text.push_str(&format!(
            "\n# Or, from a terminal:\n#   claude mcp add-json tailscale '{}'\n",
            serde_json::to_string(&server).unwrap_or_default()
        ));
    }
    text.push_str(
        "\n# The credential is not in the snippet: set TAILSCALE_API_KEY, or\n\
         # TAILSCALE_OAUTH_CLIENT_ID and TAILSCALE_OAUTH_CLIENT_SECRET, where the\n\
         # client can see them.\n",
    );
    Report::ok(text)
}

/// What the client calls itself, as the command line spells it.
fn name_of(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => "claude-code",
        Client::ClaudeDesktop => "claude-desktop",
        Client::Vscode => "vscode",
        Client::Cursor => "cursor",
        Client::Zed => "zed",
    }
}
