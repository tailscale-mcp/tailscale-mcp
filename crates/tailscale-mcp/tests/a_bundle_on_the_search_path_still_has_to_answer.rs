//! The binary, started with a search path that leads into the application
//! bundle before it reaches the command-line interface.
//!
//! A person can put the bundle's own directory on `PATH`, or link its
//! executable into one. Either way the server reaches the application's
//! executable, which starts the GUI rather than answering (Q157), so it has to
//! be asked like the bundle itself rather than believed on sight.
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn script(path: &Path, body: &str) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory is made");
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("the script is written");
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(path, perms).expect("the script is executable");
    path.to_owned()
}

/// The version `tailscale_version` reports from a server whose `PATH` is `dirs`.
fn version_found_through(dirs: &[PathBuf]) -> Value {
    let path = std::env::join_paths(dirs.iter().cloned().chain([PathBuf::from("/bin")]))
        .expect("a search path");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tailscale-mcp"))
        .arg("--no-tailnet")
        .env("PATH", path)
        .env_remove("TAILSCALE_MCP_CLI_PATH")
        .env_remove("TAILSCALE_API_KEY")
        .env_remove("TAILSCALE_OAUTH_CLIENT_ID")
        .env_remove("TAILSCALE_OAUTH_CLIENT_SECRET")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server starts");

    let mut stdin = child.stdin.take().expect("stdin is piped");
    for request in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}
        }}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "tailscale_version", "arguments": {}
        }}),
    ] {
        writeln!(stdin, "{request}").expect("the request is sent");
    }

    let stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
    let answer = stdout
        .lines()
        .map(|line| serde_json::from_str::<Value>(&line.expect("a line")).expect("JSON"))
        .find(|message| message["id"] == json!(2))
        .expect("the call is answered");
    drop(stdin);
    child.wait().expect("the server exits");
    answer["result"]["structuredContent"]["version"].clone()
}

#[test]
fn a_search_path_into_the_bundle_does_not_hide_the_cli_behind_it() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let macos = root.path().join("Tailscale.app/Contents/MacOS");
    let bundled = script(
        &macos.join("tailscale"),
        "echo 'The Tailscale GUI failed to start: (Tailscale.CLIError error 3.)'",
    );
    let linked = root.path().join("linked");
    std::fs::create_dir_all(&linked).expect("the directory is made");
    std::os::unix::fs::symlink(&bundled, linked.join("tailscale")).expect("the link is made");
    let cli = script(&root.path().join("usr-local-bin/tailscale"), "echo 1.102.4");
    let cli_dir = cli.parent().expect("a parent").to_owned();

    for (route, first) in [("the bundle's directory", macos), ("a link to it", linked)] {
        assert_eq!(
            version_found_through(&[first, cli_dir.clone()]),
            json!("1.102.4"),
            "with {route} first on PATH, the server should still reach the CLI behind it"
        );
    }
}
