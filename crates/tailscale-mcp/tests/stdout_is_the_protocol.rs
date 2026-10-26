//! The binary, run as a client would run it.
//!
//! One rule is worth a process to check: everything the server has to say to
//! the operator goes to standard error, because standard output is the
//! protocol. A single stray line there ends the session for every client.
//!
//! Unix only: the fake `tailscale` is a shell script, and the failure this
//! guards against is not platform-specific.
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write as _;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// A `tailscale` that reports a version below the supported floor, so that the
/// server has something to warn about while it is talking to a client.
fn fake_tailscale(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("tailscale");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         case \"$1\" in\n\
         version) echo 1.72.0 ;;\n\
         status) echo '{}' ;;\n\
         *) echo 'unknown command' >&2; exit 1 ;;\n\
         esac\n",
    )
    .expect("the fake is written");
    let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&path, perms).expect("the fake is executable");
    path
}

fn line(message: Value) -> String {
    format!("{message}\n")
}

/// Run the server over stdio with no credential, send `requests`, close its
/// input, and return what it wrote: the messages on standard output, each
/// checked to be one, and standard error as text.
fn exchange(requests: &[Value]) -> (Vec<Value>, String) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let cli = fake_tailscale(dir.path());

    let mut child = Command::new(env!("CARGO_BIN_EXE_tailscale-mcp"))
        .arg("--cli-path")
        .arg(&cli)
        // No credential is supplied, so the tailnet surface is off anyway;
        // saying so keeps the run independent of the developer's environment.
        .arg("--no-tailnet")
        .env_remove("TAILSCALE_API_KEY")
        .env_remove("TAILSCALE_OAUTH_CLIENT_ID")
        .env_remove("TAILSCALE_OAUTH_CLIENT_SECRET")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");

    let mut stdin = child.stdin.take().expect("stdin is piped");
    for request in requests {
        stdin
            .write_all(line(request.clone()).as_bytes())
            .expect("the request is sent");
    }
    stdin.flush().expect("flushed");
    drop(stdin);

    let output = child.wait_with_output().expect("the server exits");
    let stdout = String::from_utf8(output.stdout).expect("standard output is text");
    let stderr = String::from_utf8(output.stderr).expect("standard error is text");

    // Every line of standard output is a JSON-RPC message and nothing else.
    let messages: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| {
                panic!("standard output carried something that is not a message: {l:?} ({e})")
            })
        })
        .collect();
    (messages, stderr)
}

#[test]
fn the_startup_warning_goes_to_standard_error_and_leaves_the_protocol_clean() {
    let (messages, stderr) = exchange(&[
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }),
    ]);

    assert!(
        messages.iter().all(|m| m["jsonrpc"] == "2.0"),
        "{messages:#?}"
    );

    let initialize = messages
        .iter()
        .find(|m| m["id"] == 1)
        .expect("the handshake is answered");
    assert_eq!(initialize["result"]["serverInfo"]["name"], "tailscale-mcp");
    assert!(
        initialize["result"]["instructions"]
            .as_str()
            .is_some_and(|i| i.contains("tailscale_*")),
        "{initialize}"
    );

    let listing = messages
        .iter()
        .find(|m| m["id"] == 2)
        .expect("the listing is answered");
    let tools = listing["result"]["tools"]
        .as_array()
        .expect("a listing is an array");
    assert!(!tools.is_empty(), "the local surface should be on offer");
    assert!(
        tools.iter().all(|t| t["name"]
            .as_str()
            .is_some_and(|n| n.starts_with("tailscale_"))),
        "a tailnet tool survived --no-tailnet: {tools:#?}"
    );

    // The same run warned about the old binary, on the other stream.
    assert!(
        stderr.contains("1.72.0") && stderr.contains("Nothing is hidden"),
        "the version warning is missing from standard error: {stderr}"
    );
    assert!(
        stderr.contains("tailnet surface is switched off"),
        "{stderr}"
    );
}

#[test]
fn every_listing_under_2026_07_28_carries_its_cache_hints() {
    // From 2026-07-28 a list result must say how long it stays fresh and who
    // may share it. Claude Code rejects a `tools/list` without `ttlMs` and
    // `cacheScope`, and then shows the server as connected with no tools
    // (issue #2).
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "0" }
    });
    let lists = [
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
    ];
    let mut requests = vec![json!({
        "jsonrpc": "2.0", "id": 0, "method": "server/discover", "params": { "_meta": meta }
    })];
    requests.extend(lists.iter().zip(1..).map(|(method, id)| {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": { "_meta": meta } })
    }));
    let (messages, _) = exchange(&requests);

    for (method, id) in lists.iter().zip(1..) {
        let reply = messages
            .iter()
            .find(|m| m["id"] == id)
            .unwrap_or_else(|| panic!("{method} is answered: {messages:#?}"));
        let result = &reply["result"];
        assert!(
            result["ttlMs"].is_u64(),
            "{method} has no ttlMs: {}",
            result["ttlMs"]
        );
        assert!(
            matches!(result["cacheScope"].as_str(), Some("public" | "private")),
            "{method} has no cacheScope: {}",
            result["cacheScope"]
        );
    }
}
