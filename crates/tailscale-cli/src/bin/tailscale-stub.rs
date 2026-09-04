//! A stand-in for the `tailscale` binary, used by the process-execution tests.
//!
//! Faking [`LocalBackend`](tailscale_cli::LocalBackend) skips exactly the
//! behaviour worth testing here — argument construction, environment scrubbing,
//! the timeout, graceful termination before the kill — so those tests spawn a
//! real process, and this is it.
//!
//! Not part of the published crate: `exclude` in the manifest keeps it out.

// Standing in for a real tool means panicking on a broken environment too:
// this binary exists to be a fixture, not to be robust.
#![allow(clippy::print_stdout, clippy::expect_used)]

use std::io::{Read as _, Write as _};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = &args[1..];
    match args.first().map(String::as_str) {
        // Print the arguments we were handed, one per line, so a test can
        // assert on the exact list rather than on a re-quoted string.
        Some("echo-args") => {
            for arg in rest {
                println!("{arg}");
            }
            std::process::ExitCode::SUCCESS
        }
        // Print the environment we were handed, sorted.
        Some("dump-env") => {
            let mut vars: Vec<(String, String)> = std::env::vars().collect();
            vars.sort();
            for (key, value) in vars {
                println!("{key}={value}");
            }
            std::process::ExitCode::SUCCESS
        }
        // Write to stderr and exit non-zero, the shape of a real failure.
        Some("fail") => {
            let code: u8 = rest.first().and_then(|c| c.parse().ok()).unwrap_or(1);
            let message = rest.get(1).map(String::as_str).unwrap_or("stub failure");
            eprint!("{message}");
            std::process::ExitCode::from(code)
        }
        // Copy standard input to standard output.
        Some("cat") => {
            let mut buf = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut buf);
            let _ = std::io::stdout().write_all(&buf);
            std::process::ExitCode::SUCCESS
        }
        // Sleep, so the caller's timeout fires.
        Some("sleep") => {
            let ms: u64 = rest.first().and_then(|m| m.parse().ok()).unwrap_or(60_000);
            std::thread::sleep(std::time::Duration::from_millis(ms));
            std::process::ExitCode::SUCCESS
        }
        // Say something and then wait forever, the shape of a client that is
        // waiting on a human: `tailscale funnel` prints the URL that enables
        // Funnel and then polls until someone visits it.
        Some("say-then-hang") => {
            let message = rest.first().map(String::as_str).unwrap_or("waiting");
            println!("{message}");
            let _ = std::io::stdout().flush();
            eprintln!("still waiting");
            std::thread::sleep(std::time::Duration::from_secs(600));
            std::process::ExitCode::SUCCESS
        }
        // Refuse to die politely: record that the signal arrived, then keep
        // running so that the caller has to escalate to a kill.
        Some("ignore-term") => ignore_term(rest.first().map(String::as_str)),
        other => {
            eprintln!("tailscale-stub: unknown command {other:?}");
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(unix)]
fn ignore_term(marker: Option<&str>) -> std::process::ExitCode {
    let marker = marker.map(std::path::PathBuf::from);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime");
    runtime.block_on(async move {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("a SIGTERM handler");
        // Announce readiness so the caller is not racing our handler.
        println!("ready");
        let _ = std::io::stdout().flush();
        term.recv().await;
        if let Some(marker) = marker {
            let _ = std::fs::write(marker, "sigterm");
        }
        // Ignore it, and wait to be killed.
        std::future::pending::<()>().await;
    });
    std::process::ExitCode::SUCCESS
}

#[cfg(not(unix))]
fn ignore_term(_marker: Option<&str>) -> std::process::ExitCode {
    // There is no SIGTERM to ignore; hang until killed.
    std::thread::sleep(std::time::Duration::from_secs(600));
    std::process::ExitCode::SUCCESS
}
