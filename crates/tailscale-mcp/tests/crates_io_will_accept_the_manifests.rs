//! crates.io's limits on the metadata a manifest carries.
//!
//! `cargo publish` does not check these locally. It packages the crate, sends
//! it, and learns the answer from the server — so a manifest that breaks one
//! of them fails at the upload, which `release.yml` runs last, after the
//! GitHub release, the npm package and the container image have all gone out.
//! Worse, `--workspace` uploads in dependency order, so the crate that fails
//! can be the third of three, leaving two versions on crates.io that cannot
//! be taken back and a release that cannot be completed at that version.
//!
//! That is not hypothetical: 1.0.0 was published with a 22-character keyword
//! and `tailscale-mcp` was refused after `tailscale-rest` and `tailscale-cli`
//! were already up. These are the registry's limits, checked where they cost
//! a test run instead of a version number.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

/// crates.io accepts at most this many keywords on a crate.
const MAX_KEYWORDS: usize = 5;

/// And refuses any keyword that is not shorter than this — its own words are
/// "keywords must have less than 20 characters".
const MAX_KEYWORD_LEN: usize = 20;

/// Every manifest this workspace publishes, as (crate directory, contents).
fn manifests() -> Vec<(String, String)> {
    let crates = repo::root().join("crates");
    let mut found: Vec<(String, String)> = std::fs::read_dir(&crates)
        .expect("the crates directory")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.join("Cargo.toml").is_file())
        .map(|path| {
            let name = path
                .file_name()
                .expect("a directory name")
                .to_string_lossy()
                .into_owned();
            let manifest =
                std::fs::read_to_string(path.join("Cargo.toml")).expect("a crate manifest");
            (name, manifest)
        })
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no crate manifests found under crates/");
    found
}

/// The `keywords = [...]` list, which is written on one line in every manifest
/// here. A crate that omits it has none, which crates.io allows.
fn keywords(manifest: &str) -> Vec<String> {
    let Some(line) = manifest
        .lines()
        .find_map(|line| line.strip_prefix("keywords = ["))
    else {
        return Vec::new();
    };
    line.split(']')
        .next()
        .expect("the list closes on its own line")
        .split(',')
        .map(|word| word.trim().trim_matches('"').to_owned())
        .filter(|word| !word.is_empty())
        .collect()
}

#[test]
fn every_keyword_is_one_crates_io_accepts() {
    for (name, manifest) in manifests() {
        let words = keywords(&manifest);
        assert!(
            words.len() <= MAX_KEYWORDS,
            "{name} carries {} keywords; crates.io accepts {MAX_KEYWORDS}",
            words.len()
        );
        for word in words {
            assert!(
                word.chars().count() < MAX_KEYWORD_LEN,
                "{name}'s keyword `{word}` is {} characters; crates.io refuses \
                 anything that is not under {MAX_KEYWORD_LEN}",
                word.chars().count()
            );
        }
    }
}

#[test]
fn the_check_catches_a_manifest_crates_io_would_refuse() {
    // The keyword that 1.0.0 was refused for, and one keyword too many.
    let refused = "keywords = [\"model-context-protocol\"]\n";
    let too_many = "keywords = [\"a\", \"b\", \"c\", \"d\", \"e\", \"f\"]\n";

    let words = keywords(refused);
    assert_eq!(words, ["model-context-protocol"]);
    assert!(
        words[0].chars().count() >= MAX_KEYWORD_LEN,
        "the keyword crates.io refused should be over the limit this checks"
    );
    assert!(keywords(too_many).len() > MAX_KEYWORDS);

    // And that a manifest without the field is not read as having a broken one.
    assert!(keywords("name = \"nothing\"\n").is_empty());
}
