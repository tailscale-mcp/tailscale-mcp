//! The targets the release builds are the targets every channel knows about.
//!
//! Adding a platform means editing six files in four languages: the release
//! matrix builds it, the npm launcher maps a machine to it, the formula
//! template and the script that renders it name it, the bundle script maps it
//! to a bundle platform, and the container image is built for it. Nothing in
//! any of those files refers to any other, so a sixth target added to the
//! matrix and forgotten in the launcher is an npm package that tells that
//! platform's users there is no binary — while a binary sits in the release.
//!
//! So the matrix is the source of truth and this holds the rest to it. Two
//! differences are deliberate and written down here rather than discovered:
//! Homebrew does not run on Windows, so the formula covers four of the five;
//! and the container image is not built from these targets at all — it
//! compiles its own binary against musl, because the released Linux binaries
//! are linked against the runner's glibc (Q109).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

use std::collections::BTreeSet;

/// The platform families anything is built for, as they appear in a target
/// triple. `scripts/build-mcpb.sh` matches on these same three.
const FAMILIES: &[&str] = &["-apple-darwin", "-unknown-linux-", "-pc-windows-"];

/// The targets `.github/workflows/release.yml` builds, read from the matrix.
///
/// Structured rather than by shape, so that this reading and `triples_in`
/// below are independent: one finds what is built, the other finds what each
/// channel claims, and a check that read both the same way would agree with
/// itself for free.
fn released_targets(workflow: &str) -> BTreeSet<String> {
    workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("target: "))
        // `--target ${{ matrix.target }}` is the matrix being used, not set.
        .filter(|value| !value.contains('{'))
        .map(str::to_owned)
        .collect()
}

/// Every target triple named anywhere in a file, whatever the language.
///
/// A triple is found by its platform family and then grown outwards: the
/// architecture is the run of word characters before it and the environment
/// the run after, so `"aarch64-apple-darwin"`, `@VERSION@-x86_64-apple-darwin`
/// and `…-1.0.0-x86_64-unknown-linux-gnu.tar.gz` all yield the triple and
/// nothing around it. That is what makes this work on JavaScript, Ruby, shell
/// and YAML alike; `the_matrix_builds_nothing_this_cannot_see` is what stops
/// the list of families from being a silent blind spot.
fn triples_in(text: &str) -> BTreeSet<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut found = BTreeSet::new();
    for family in FAMILIES {
        let mut rest = text;
        while let Some(at) = rest.find(*family) {
            let (before, after) = rest.split_at(at);
            let architecture: String = before
                .chars()
                .rev()
                .take_while(|c| word(*c))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let after = &after[family.len()..];
            let environment: String = after.chars().take_while(|c| word(*c)).collect();
            if !architecture.is_empty() {
                found.insert(format!("{architecture}{family}{environment}"));
            }
            rest = after;
        }
    }
    found
}

/// One file from the repository.
fn text_at(path: &str) -> String {
    let mut where_ = repo::root();
    where_.extend(path.split('/'));
    std::fs::read_to_string(&where_).unwrap_or_else(|why| panic!("{path}: {why}"))
}

/// What the release builds.
fn released() -> BTreeSet<String> {
    let targets = released_targets(&text_at(".github/workflows/release.yml"));
    assert!(
        targets.len() >= 2,
        "the release matrix was not read: {targets:?}"
    );
    targets
}

#[test]
fn the_matrix_builds_nothing_this_cannot_see() {
    // `triples_in` finds a target by its platform family, so a target from a
    // family it does not know would be invisible to every check below and
    // they would all pass. This is the check that cannot happen to.
    for target in released() {
        let families: Vec<&&str> = FAMILIES
            .iter()
            .filter(|family| target.contains(*family))
            .collect();
        assert_eq!(
            families.len(),
            1,
            "{target} belongs to {} of the families this test knows",
            families.len()
        );
    }
}

#[test]
fn the_npm_launcher_has_a_name_for_every_target() {
    // The launcher maps this machine to a target and downloads the archive
    // named for it. A target missing here is a platform told there is no
    // binary; one that is here and not built is a download that 404s.
    assert_eq!(
        triples_in(&text_at("packaging/npm/lib/launcher.js")),
        released(),
        "the launcher and the release disagree about what is built"
    );
}

#[test]
fn the_formula_covers_every_target_homebrew_runs_on() {
    // Homebrew is macOS and Linux; Windows is the deliberate difference.
    let expected: BTreeSet<String> = released()
        .into_iter()
        .filter(|target| !target.contains("-pc-windows-"))
        .collect();
    for file in [
        "packaging/homebrew/tailscale-mcp.rb.in",
        "scripts/update-formula.sh",
    ] {
        assert_eq!(
            triples_in(&text_at(file)),
            expected,
            "{file} and the release disagree about what is built"
        );
    }
}

#[test]
fn the_bundle_script_knows_what_to_do_with_every_target() {
    // It matches on the family rather than on the triple, so what has to be
    // there is an arm for each family the matrix builds for.
    let script = text_at("scripts/build-mcpb.sh");
    for target in released() {
        let family = FAMILIES
            .iter()
            .find(|family| target.contains(*family))
            .expect("a family, which the check above insists on");
        assert!(
            script.contains(family),
            "nothing in build-mcpb.sh handles {target} (family {family})"
        );
    }
}

#[test]
fn the_readings_find_what_they_think_they_do() {
    // Both of these read a file by its shape, and a reading that quietly
    // found nothing would leave every check above comparing two empty sets.
    let matrix = "\
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: macos-latest
            target: aarch64-apple-darwin
    steps:
      - run: cargo build --target ${{ matrix.target }}
";
    assert_eq!(
        released_targets(matrix),
        ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );

    // Quoted, bare, in a path and in a URL — the four shapes these files use.
    let mixed = r#"
  "darwin/arm64": "aarch64-apple-darwin",
  targets="x86_64-pc-windows-msvc aarch64-unknown-linux-gnu"
  url ".../tailscale-mcp-1.0.0-x86_64-apple-darwin.tar.gz"
  echo not-a-target-at-all
"#;
    assert_eq!(
        triples_in(mixed),
        [
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect(),
        "the reading changed shape"
    );
}
