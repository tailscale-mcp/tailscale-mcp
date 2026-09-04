//! `packaging/mcpb/manifest.json` is what an MCP bundle host will accept.
//!
//! A `.mcpb` is a zip holding this manifest and a binary; a client installs it
//! by reading the manifest, so a manifest with a typo in it is an install that
//! fails on somebody else's machine. Nothing in this repository reads the file,
//! so it is checked here — against a vendored copy of the bundle schema, the
//! way the registry listing is (`registry_listing_is_valid.rs`).
//!
//! Beyond the schema there are three agreements the schema cannot see, because
//! each is between the manifest and something outside it:
//!
//! * every `${user_config.…}` reference resolves to a setting the manifest
//!   declares, and every declared setting is used — a reference to a setting
//!   that is not there is substituted with nothing, silently;
//! * every variable the manifest sets is one this server reads, which is what
//!   makes the settings do anything at all;
//! * the entry point and the command name the same file, since the bundle
//!   holds one binary and the manifest names it twice.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;
mod schema;

use std::collections::BTreeSet;

use schema::validate;
use serde_json::Value;

/// Where the bundle's binary sits inside the zip, under the manifest.
const ENTRY_POINT: &str = "server/tailscale-mcp";

/// The vendored schema, and the manifest it judges.
fn schema_and_manifest() -> (Value, Value) {
    let dir = repo::root().join("packaging").join("mcpb");
    (
        schema::json_at(dir.join("mcpb-manifest.schema.json")),
        schema::json_at(dir.join("manifest.json")),
    )
}

/// Every `${user_config.name}` in a document, at any depth.
fn references_in(value: &Value) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    collect(value, &mut found);
    found
}

/// The walk `references_in` is the front of.
fn collect(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => {
            let mut rest = text.as_str();
            while let Some(start) = rest.find("${user_config.") {
                rest = &rest[start + "${user_config.".len()..];
                let Some(end) = rest.find('}') else { return };
                found.insert(rest[..end].to_owned());
                rest = &rest[end..];
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, found)),
        Value::Object(fields) => fields.values().for_each(|field| collect(field, found)),
        _ => {}
    }
}

#[test]
fn the_manifest_validates_against_the_bundle_schema() {
    let (schema, manifest) = schema_and_manifest();
    if let Err(why) = validate(schema, &manifest) {
        panic!("a bundle host would refuse manifest.json:\n{why}");
    }
}

#[test]
fn the_manifest_is_written_for_the_schema_it_is_checked_against() {
    // The bundle schema has no `$id` to compare a `$schema` against, but it
    // does pin the manifest version it describes. A vendored schema moved
    // forward without the manifest moving with it fails here rather than at
    // somebody's install.
    let (schema, manifest) = schema_and_manifest();
    assert_eq!(
        manifest["manifest_version"], schema["properties"]["manifest_version"]["const"],
        "manifest.json is written for one bundle version and checked against another"
    );
}

#[test]
fn the_manifest_is_at_the_version_being_released() {
    let (_, manifest) = schema_and_manifest();
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn every_setting_the_manifest_offers_is_one_it_uses() {
    let (_, manifest) = schema_and_manifest();
    let used = references_in(&manifest["server"]);
    let declared: BTreeSet<String> = manifest["user_config"]
        .as_object()
        .expect("user_config")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        used, declared,
        "a setting is offered to the operator and never read, or read and never offered"
    );
}

#[test]
fn every_variable_the_manifest_sets_is_one_this_server_reads() {
    let (_, manifest) = schema_and_manifest();
    let known: BTreeSet<&str> = tailscale_mcp::config::ENV_VARS
        .iter()
        .chain(tailscale_rest::credentials::ENV_VARS)
        .copied()
        .collect();
    for name in manifest["server"]["mcp_config"]["env"]
        .as_object()
        .expect("env")
        .keys()
    {
        assert!(
            known.contains(name.as_str()),
            "the manifest sets {name}, which this server does not read"
        );
    }
}

#[test]
fn the_manifest_names_the_same_binary_twice() {
    let (_, manifest) = schema_and_manifest();
    let server = &manifest["server"];
    assert_eq!(server["type"], "binary");
    assert_eq!(server["entry_point"], ENTRY_POINT);
    assert_eq!(
        server["mcp_config"]["command"],
        format!("${{__dirname}}/{ENTRY_POINT}"),
        "the command has to be the entry point, resolved against the unpacked bundle"
    );
}

#[test]
fn every_bundle_the_build_script_can_narrow_the_manifest_to_is_still_valid() {
    // `scripts/build-mcpb.sh` writes one platform into each bundle, since a
    // bundle carries the binary for one platform only. The checked-in manifest
    // is the one validated above; these are the manifests that actually ship,
    // one per platform, and each is judged as well.
    let (schema, manifest) = schema_and_manifest();
    let listed = manifest["compatibility"]["platforms"]
        .as_array()
        .expect("the platforms the manifest describes")
        .clone();
    assert!(!listed.is_empty(), "the manifest describes no platform");
    for platform in listed {
        let mut narrowed = manifest.clone();
        narrowed["compatibility"]["platforms"] = serde_json::json!([platform]);
        if let Err(why) = validate(schema.clone(), &narrowed) {
            panic!("a bundle host would refuse the {platform} bundle:\n{why}");
        }
    }
}

#[test]
fn the_check_catches_a_manifest_a_host_would_refuse() {
    // Each of these is a way the manifest could be wrong that reading it would
    // not catch, so the check is known to fire rather than assumed to.
    let (schema, manifest) = schema_and_manifest();
    schema::refuses(
        &schema,
        [
            ("a server with no entry point", {
                let mut broken = manifest.clone();
                broken["server"]
                    .as_object_mut()
                    .expect("the server")
                    .remove("entry_point");
                broken
            }),
            ("a server type no host implements", {
                let mut broken = manifest.clone();
                broken["server"]["type"] = "rust".into();
                broken
            }),
            ("an author given as a name rather than a person", {
                let mut broken = manifest.clone();
                broken["author"] = "tailscale-mcp".into();
                broken
            }),
            ("a setting whose type is not one a host can ask for", {
                let mut broken = manifest.clone();
                broken["user_config"]["api_key"]["type"] = "secret".into();
                broken
            }),
            ("a platform the bundle format does not know", {
                let mut broken = manifest.clone();
                broken["compatibility"]["platforms"] = serde_json::json!(["macos"]);
                broken
            }),
        ],
    );
}

#[test]
fn the_reference_reader_finds_what_it_claims_to() {
    // The agreement above is only as good as this walk, so it is exercised on
    // a shape that has a reference nested, two in one string, and one that is
    // not a reference at all.
    let found = references_in(&serde_json::json!({
        "command": "${__dirname}/server/tailscale-mcp",
        "env": {"A": "${user_config.one}", "B": "x${user_config.two}y${user_config.three}z"},
        "args": ["--flag", "${user_config.four}"],
        "n": 1,
    }));
    let expected: BTreeSet<String> = ["one", "two", "three", "four"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(found, expected);
}
