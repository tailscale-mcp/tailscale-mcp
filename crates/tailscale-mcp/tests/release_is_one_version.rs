//! The three crates release together, at one version, and everything built
//! around them says the same one.
//!
//! `cargo publish --workspace` uploads all three in dependency order, and the
//! published crates depend on each other by version as well as by path — so a
//! version that has moved in one place and not another is not a tidiness
//! problem, it is a release where `tailscale-mcp` asks for a `tailscale-rest`
//! that was never uploaded. The manifest is arranged so that cannot happen
//! (every crate inherits `version.workspace`), and this checks the
//! arrangement rather than trusting it.
//!
//! Outside the manifest the version is written down again by each thing built
//! around the crates, and a release is one version everywhere or it is a
//! release where the npm package fetches an archive that was never published.
//! This covers the changelog's newest heading and the npm package; the other
//! two carry more than a version and are checked where they are read, in
//! `registry_listing_is_valid.rs` and `plugin_manifest_is_valid.rs`.
//!
//! It is the check `scripts/prepare-release.sh` is written against: run that,
//! and this passes.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

/// The version this test binary was compiled at, which is the crate's own and
/// therefore the workspace's.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The workspace manifest.
fn workspace_manifest() -> String {
    std::fs::read_to_string(repo::root().join("Cargo.toml")).expect("the workspace manifest")
}

/// The value of the first `version = "…"` line in `text`, which in the
/// workspace manifest is `[workspace.package]`'s.
fn first_version(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
}

#[test]
fn the_workspace_names_the_version_this_crate_was_built_at() {
    let manifest = workspace_manifest();
    assert_eq!(
        first_version(&manifest),
        Some(VERSION),
        "`[workspace.package]` and this crate disagree about the version"
    );
}

#[test]
fn every_crate_takes_its_version_from_the_workspace() {
    for crate_name in ["tailscale-rest", "tailscale-cli", "tailscale-mcp"] {
        let path = repo::root()
            .join("crates")
            .join(crate_name)
            .join("Cargo.toml");
        let manifest = std::fs::read_to_string(&path).expect("a crate manifest");
        assert!(
            manifest.contains("version.workspace = true"),
            "{crate_name} sets its own version, so the three can drift apart"
        );
    }
}

#[test]
fn the_internal_dependencies_ask_for_the_version_being_released() {
    // A path dependency that also names a version is what lets these crates be
    // published at all: the path is used here, the version is what the
    // published crate carries. Naming a version that is not the one going out
    // publishes a `tailscale-mcp` that cannot resolve.
    let manifest = workspace_manifest();
    for crate_name in ["tailscale-rest", "tailscale-cli"] {
        let line = manifest
            .lines()
            .find(|line| line.starts_with(&format!("{crate_name} = {{")))
            .unwrap_or_else(|| panic!("`{crate_name}` is not a workspace dependency"));
        assert!(
            line.contains(&format!("version = \"{VERSION}\"")),
            "`{crate_name}` is depended on at another version: {line}"
        );
    }
}

/// The version in the changelog's newest release heading, which is the first
/// `## ` line and reads `## 1.2.3 — 2026-01-01`. The release workflow takes
/// the release notes from that same section, so this is the heading the
/// release page will carry.
fn newest_release(changelog: &str) -> Option<&str> {
    changelog
        .lines()
        .find_map(|line| line.strip_prefix("## "))
        .and_then(|heading| heading.split_whitespace().next())
}

#[test]
fn the_changelog_leads_with_the_version_being_released() {
    let changelog =
        std::fs::read_to_string(repo::root().join("CHANGELOG.md")).expect("the changelog");
    let newest = newest_release(&changelog).expect("the changelog has no release heading");
    assert_eq!(
        newest, VERSION,
        "the changelog's newest release is {newest} and the crates are at {VERSION}; \
         run `scripts/prepare-release.sh`"
    );
}

#[test]
fn the_npm_package_is_at_the_version_being_released() {
    // The launcher fetches
    // `tailscale-mcp-<its own version>-<target>.tar.gz` from the release, so
    // a package left behind at the previous version installs the previous
    // binary — or, on a first release, nothing at all.
    let path = repo::root()
        .join("packaging")
        .join("npm")
        .join("package.json");
    let package: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the npm package"))
            .expect("the npm package is JSON");
    assert_eq!(
        package["version"], VERSION,
        "the npm package would fetch the archives of another release"
    );
}

#[test]
fn the_checks_read_a_manifest_and_a_changelog_the_way_they_think_they_do() {
    // Both of these read one line out of a file by its shape, and a reading
    // that quietly found nothing would leave the checks above passing on an
    // empty answer.
    assert_eq!(
        first_version("[workspace.package]\nversion = \"2.3.4\"\nedition = \"2024\"\n"),
        Some("2.3.4")
    );
    // `rust-version` is not the version, and neither is a crate's inherited one.
    assert_eq!(first_version("rust-version = \"1.88\"\n"), None);
    assert_eq!(first_version("version.workspace = true\n"), None);

    assert_eq!(
        newest_release("# Changelog\n\n## 2.3.4 — 2026-01-01\n\n## 2.3.3 — 2025-12-31\n"),
        Some("2.3.4")
    );
    // A heading that is not a release, and a changelog with no release at all.
    assert_eq!(newest_release("# Changelog\n\nNothing yet.\n"), None);
}
