//! Nothing recorded from a real tailnet reaches the repository.
//!
//! Fixtures come from running the real thing against a real network, which is
//! what makes them worth having and also what makes them dangerous: node
//! names, addresses, account names and keys all arrive with the response. The
//! rule is that every identifier in a fixture must be an obvious placeholder,
//! and this test decides what "obvious" means, so that the check is mechanical
//! rather than a habit somebody has to remember.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::path::{Path, PathBuf};

/// The only tailnet name a fixture may mention.
const TAILNET: &str = "example-tailnet";
/// The only mail domain a fixture may mention.
const MAIL_DOMAIN: &str = "example.com";
/// The MagicDNS resolver, which has this address on every tailnet and so
/// names nobody. A `dns query` answer that did not mention it would be a
/// misleading fixture.
const RESOLVER: &str = "100.100.100.100";
/// This file, which the check exempts from itself.
const THIS_FILE: &str = "fixtures_are_redacted.rs";

/// Something in a file that looks like it came from a real tailnet.
#[derive(Debug, PartialEq, Eq)]
struct Leak {
    what: &'static str,
    value: String,
}

/// Everything in `text` that fails the placeholder rules.
fn leaks(text: &str) -> Vec<Leak> {
    let mut found = Vec::new();

    for word in words(text) {
        // A MagicDNS name: only one tailnet may appear.
        if let Some(host) = word.strip_suffix(".ts.net")
            && !host.ends_with(&format!(".{TAILNET}"))
            && host != TAILNET
        {
            found.push(Leak {
                what: "MagicDNS name from another tailnet",
                value: word.clone(),
            });
        }

        // A Tailscale address: 100.64.0.0/10, of which only the first
        // hundred of 100.64.0.x are placeholders. A prefix length identifies
        // nobody, so it is set aside before the address is judged; leaving it
        // on would both reject `fd7a:115c:a1e0::1/128` and let
        // `100.101.102.103/32` through.
        let address = word.split('/').next().unwrap_or(&word);
        if is_tailscale_v4(address) && !is_placeholder_v4(address) && address != RESOLVER {
            found.push(Leak {
                what: "Tailscale IPv4 address",
                value: word.clone(),
            });
        }
        if address.starts_with("fd7a:115c:a1e0")
            && !is_placeholder_v6(address)
            && !is_via_route(address)
        {
            found.push(Leak {
                what: "Tailscale IPv6 address",
                value: word.clone(),
            });
        }

        // An account: only one mail domain may appear.
        if word.contains('@')
            && word.contains('.')
            && !word.ends_with(&format!("@{MAIL_DOMAIN}"))
            && !word.contains(&format!("@{MAIL_DOMAIN}"))
        {
            found.push(Leak {
                what: "account name",
                value: word.clone(),
            });
        }

        // A key of any kind: never recorded, only ever a placeholder.
        if word.starts_with("tskey-") && !marked_fake(&word) {
            found.push(Leak {
                what: "auth or API key",
                value: word.clone(),
            });
        }

        // A node or machine key: hexadecimal, so a placeholder has to be
        // recognisable by being all one character.
        for prefix in ["nodekey:", "mkey:", "discokey:"] {
            if let Some(hex) = word.strip_prefix(prefix)
                && !is_placeholder_hex(hex)
            {
                found.push(Leak {
                    what: "node, machine or disco key",
                    value: word.clone(),
                });
            }
        }

        // A control-plane device id: `n`, then digits and letters, then CNTRL.
        // A placeholder uses digits alone.
        if let Some(middle) = word.strip_prefix('n').and_then(|w| w.strip_suffix("CNTRL"))
            && !middle.is_empty()
            && !middle.chars().all(|c| c.is_ascii_digit())
        {
            found.push(Leak {
                what: "device identifier",
                value: word.clone(),
            });
        }
    }

    found
}

/// Split on everything that cannot be part of an identifier, so that quoting
/// and punctuation in JSON, Markdown or plain text does not hide a value.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| {
        !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '@' | '/'))
    })
    .map(|w| {
        w.trim_matches(|c| matches!(c, '.' | '-' | '/' | ':'))
            .to_owned()
    })
    .filter(|w| !w.is_empty())
    .collect()
}

fn is_tailscale_v4(word: &str) -> bool {
    let mut parts = word.split('.');
    let (Some(a), Some(b), Some(c), Some(d), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return false;
    };
    let parsed: Option<Vec<u8>> = [a, b, c, d].iter().map(|p| p.parse::<u8>().ok()).collect();
    // 100.64.0.0/10 is the range Tailscale assigns from.
    parsed.is_some_and(|octets| octets[0] == 100 && (64..128).contains(&octets[1]))
}

fn is_placeholder_v4(word: &str) -> bool {
    word.strip_prefix("100.64.0.")
        .and_then(|last| last.parse::<u8>().ok())
        .is_some_and(|last| last < 100)
}

fn is_placeholder_v6(word: &str) -> bool {
    word.strip_prefix("fd7a:115c:a1e0::").is_some_and(|last| {
        !last.is_empty() && last.len() <= 2 && last.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// A 4via6 route, which is not an address the control plane ever assigned.
///
/// `fd7a:115c:a1e0:b1a::/64` is the block Tailscale reserves for 4via6, and
/// `tailscale debug via` fills it in locally by arithmetic on a site number and
/// an IPv4 prefix the caller typed. Nothing in it was handed out by a tailnet,
/// so the rule this exempts it from — "a Tailscale address identifies a node" —
/// has nothing to say about it.
///
/// The exemption is exactly that narrow. It says nothing about the IPv4 prefix
/// encoded in the low half, which is a LAN range rather than a tailnet
/// identity, and which this check does not judge in its plain form either: a
/// bare `10.1.0.0/16` in a fixture passes for the same reason. A node address
/// lives elsewhere in `fd7a:115c:a1e0::/48` and is still caught.
fn is_via_route(word: &str) -> bool {
    word.starts_with("fd7a:115c:a1e0:b1a:")
}

fn is_placeholder_hex(hex: &str) -> bool {
    let mut characters = hex.chars();
    let Some(first) = characters.next() else {
        return true;
    };
    characters.all(|c| c == first)
}

fn marked_fake(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    lower.contains("example") || lower.contains("redacted")
}

/// Every file the check applies to: the fixtures, and the test sources, where
/// a pasted response is just as likely to land.
fn files_to_check() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut found = Vec::new();
    let mut queue = vec![root];
    while let Some(directory) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else if path.file_name() != Some(std::ffi::OsStr::new(THIS_FILE)) {
                // This file holds the counter-examples on purpose; it is the
                // one place a real-looking identifier is the point.
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn no_fixture_or_test_carries_a_real_identity() {
    let mut offences = Vec::new();
    for path in files_to_check() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            // A binary fixture cannot be checked this way; there are none, and
            // one arriving should be a deliberate decision rather than a gap.
            panic!("{} is not text, so it cannot be checked", path.display());
        };
        for leak in leaks(&text) {
            offences.push(format!(
                "{}: {} `{}`",
                path.display(),
                leak.what,
                leak.value
            ));
        }
    }
    assert!(
        offences.is_empty(),
        "these look like they came from a real tailnet:\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_check_catches_what_a_recorded_response_looks_like() {
    // Each of these is the shape the real thing has, so a fixture pasted
    // straight from a live tailnet fails.
    for (what, text) in [
        ("a node from another tailnet", "laptop.otter-lynx.ts.net"),
        (
            "an address outside the placeholder block",
            "100.101.102.103",
        ),
        ("the same, written as a route", "100.101.102.103/32"),
        ("an IPv6 address", "fd7a:115c:a1e0:ab12:4843:cd96:6265:1234"),
        (
            // The via exemption is a prefix match on one reserved group, so a
            // node address that merely begins the same way must still fail.
            "an IPv6 address whose group only starts like the via block",
            "fd7a:115c:a1e0:b1ab:4843:cd96:6265:1234",
        ),
        ("an account", "someone@theircompany.io"),
        ("an auth key", "tskey-auth-kZ8Qc1CNTRL-3n2yP8dQx"),
        (
            "a node key",
            "nodekey:0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9",
        ),
        ("a device id", "n7C1Bx9CNTRL"),
    ] {
        assert!(!leaks(text).is_empty(), "{what} was not caught: {text}");
    }
}

#[test]
fn the_check_passes_what_a_placeholder_looks_like() {
    for text in [
        "workstation.example-tailnet.ts.net.",
        "100.64.0.1",
        "100.64.0.1/32",
        "fd7a:115c:a1e0::1",
        "fd7a:115c:a1e0::1/128",
        // A 4via6 route, which `tailscale debug via` computes rather than
        // receives: site 7 over 10.1.0.0/16.
        "fd7a:115c:a1e0:b1a:0:7:a01:0/112",
        "100.100.100.100",
        "someone@example.com",
        "tskey-api-redacted-example",
        "nodekey:1111111111111111111111111111111111111111111111111111111111111111",
        "n1111111CNTRL",
        r#"{"devices": [{"hostname": "workstation", "os": "macOS"}]}"#,
    ] {
        assert_eq!(
            leaks(text),
            Vec::new(),
            "a placeholder was rejected: {text}"
        );
    }
}

#[tokio::test]
async fn the_suite_answers_from_its_fakes_and_not_from_this_machine() {
    // Nothing here reads the environment, runs `tailscale`, or reaches the
    // network: what the server reports is what the fakes were told to say.
    let harness = harness::Setup::new().start().await;
    let instructions = harness.instructions();

    assert!(
        instructions.contains(harness::TEST_CLI_VERSION),
        "the version came from somewhere other than the fake: {instructions}"
    );
    assert!(
        instructions.contains("workstation.example-tailnet.ts.net"),
        "the identity came from somewhere other than the fake: {instructions}"
    );

    harness.shutdown().await;
}
