//! The release workflow still matches what the registries were told.
//!
//! npm and crates.io publish a release by exchanging the workflow's GitHub
//! Actions identity token for a credential that lives minutes. Whether they
//! agree to is decided by a configuration somebody typed into a web form,
//! which matches on owner, repository and workflow filename — exactly, and
//! case-sensitively.
//!
//! Nothing else connects the two. Neither registry checks that the repository
//! or the workflow exists when the configuration is saved, so renaming
//! `release.yml` breaks all four registrations at once, silently, and the
//! failure arrives at the next release as npm's `404 Not Found - PUT` or
//! crates.io's `No Trusted Publishing config found for repository ...` —
//! neither of which points at the rename.
//!
//! So the coordinates are checked in, and this holds the workflow to them.
//! What it cannot do is check the registries: the file says what they were
//! told, and a value changed here and not retyped there makes this test pass
//! and the release fail. Issue 31 carries the procedure.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_norway::Value as Yaml;

/// `packaging/registry/trusted-publishers.toml`: what npm and crates.io were
/// told about this repository.
#[derive(Debug, Deserialize)]
struct Registered {
    owner: String,
    repository: String,
    workflow: String,
    /// Empty when no GitHub environment is registered, which is the case here:
    /// trust is scoped to the workflow alone.
    environment: String,
    /// GitHub's numeric id for the owner, which crates.io pins alongside the
    /// name. Nothing offline can check it against GitHub; it is here so that
    /// it is written down.
    owner_id: u64,
    npm: Vec<NpmPackage>,
    #[serde(rename = "crates")]
    crates: Vec<Crate>,
}

#[derive(Debug, Deserialize)]
struct NpmPackage {
    package: String,
    /// npm's "Allowed actions". `publish` is the opt-in to direct publishing;
    /// without it a configuration may only stage, and staging needs a person
    /// with 2FA to promote it — which this pipeline has nobody waiting to do.
    allowed_actions: String,
}

#[derive(Debug, Deserialize)]
struct Crate {
    #[serde(rename = "crate")]
    name: String,
}

fn registered() -> Registered {
    let path = repo::root()
        .join("packaging")
        .join("registry")
        .join("trusted-publishers.toml");
    let text = std::fs::read_to_string(&path).expect("the registered coordinates");
    toml::from_str(&text).expect("the coordinates parse")
}

/// Where the workflow the registries were told about would be.
///
/// Would be, not is: naming one that does not exist is the failure this file
/// was written for, so nothing here may assume the path resolves.
fn workflow_path(registered: &Registered) -> std::path::PathBuf {
    repo::root()
        .join(".github")
        .join("workflows")
        .join(&registered.workflow)
}

/// That workflow, as YAML rather than as text: the questions here are about a
/// job's guard and its permissions, and both are structure.
fn workflow(registered: &Registered) -> Yaml {
    let text = std::fs::read_to_string(workflow_path(registered))
        .unwrap_or_else(|why| panic!("{}: {why}", registered.workflow));
    serde_norway::from_str(&text).expect("the workflow parses")
}

/// Each job in the workflow, by name.
fn jobs(workflow: &Yaml) -> Vec<(String, &Yaml)> {
    workflow["jobs"]
        .as_mapping()
        .expect("the workflow has jobs")
        .iter()
        .map(|(name, job)| (name.as_str().expect("a job name").to_owned(), job as &Yaml))
        .collect()
}

/// Whether a job asks for a permission that lets it publish something.
///
/// An identity token is one it can exchange for a publishing credential;
/// `packages` writes this repository's container images; and `contents`
/// creates the GitHub release, which is where the npm launcher downloads a
/// binary from. All three outlive the run that used them.
fn can_publish(job: &Yaml) -> bool {
    let permissions = &job["permissions"];
    ["id-token", "packages", "contents"]
        .iter()
        .any(|name| permissions[*name].as_str() == Some("write"))
}

/// The jobs that can publish and do not say they only run for a tag.
///
/// A rule rather than an assertion, so that the test which proves it fires can
/// run it over a workflow doctored to break it.
fn unguarded(workflow: &Yaml) -> Vec<String> {
    jobs(workflow)
        .into_iter()
        .filter(|(name, job)| {
            can_publish(job)
                && name != REHEARSAL
                && !job["if"].as_str().is_some_and(|it| it.contains(TAG_GUARD))
        })
        .map(|(name, _)| name)
        .collect()
}

/// The repository secrets a workflow reads, every one of them.
///
/// Every occurrence on a line and not just the first: two on one line is
/// exactly how the one that matters would be missed.
fn secrets_read(text: &str) -> Vec<String> {
    text.match_indices("secrets.")
        .map(|(at, found)| {
            text[at + found.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

/// The one job that may hold such a permission without a tag guard.
///
/// It exists to ask each registry whether it would accept a release, which is
/// worth asking on a run started by hand as well as on a tag — that is what
/// makes a rehearsal an answer rather than a partial one. It publishes
/// nothing: what it does with the credentials it obtains is throw them away.
const REHEARSAL: &str = "rehearse";

/// The guard every other such job carries.
const TAG_GUARD: &str = "startsWith(github.ref, 'refs/tags/v')";

/// The paths the workspace lists as its members.
///
/// The whole `[...]`, not one line of it: a manifest that wraps the list would
/// otherwise yield a short set that quietly agrees with a short registration.
fn workspace_members() -> Vec<String> {
    let manifest = std::fs::read_to_string(repo::root().join("Cargo.toml")).expect("Cargo.toml");
    let (_, after) = manifest
        .split_once("members = [")
        .expect("the workspace lists its members");
    let (list, _) = after.split_once(']').expect("the list ends");
    list.split(',')
        .map(|entry| entry.trim().trim_matches('"').to_owned())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// What the crate in a directory calls itself, which is what crates.io
/// registers — and not the directory's own name, which is only conventionally
/// the same. It takes the directory rather than finding it, so that the test
/// below can hand it one where the two differ.
fn crate_name(directory: &std::path::Path) -> String {
    let manifest = directory.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|why| panic!("{}: {why}", manifest.display()));
    text.lines()
        .find_map(|line| line.trim().strip_prefix("name = "))
        .unwrap_or_else(|| panic!("{} does not name itself", manifest.display()))
        .trim_matches(['"', ' '])
        .to_owned()
}

#[test]
fn the_workflow_the_registries_were_told_about_is_the_one_that_publishes() {
    // The filename is matched exactly by both registries and is the single
    // point of contact between them and this repository.
    let registered = registered();
    assert!(
        workflow_path(&registered).is_file(),
        "the registries were told about {}, which is not a workflow here",
        registered.workflow
    );
    let workflow = workflow(&registered);
    let publishes = jobs(&workflow).iter().any(|(_, job)| can_publish(job));
    assert!(
        publishes,
        "{} publishes nothing, so naming it to a registry grants nothing",
        registered.workflow
    );
}

#[test]
fn the_coordinates_are_this_repository() {
    // An owner or a repository that is not ours is a configuration that will
    // never match, and the workspace manifest already says which we are.
    let registered = registered();
    let manifest = std::fs::read_to_string(repo::root().join("Cargo.toml")).expect("Cargo.toml");
    let url = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("repository = "))
        .expect("the workspace names its repository")
        .trim_matches(['"', ' ']);
    let expected = format!(
        "https://github.com/{}/{}",
        registered.owner, registered.repository
    );
    assert_eq!(
        url, expected,
        "the registered coordinates do not name this repository"
    );
    assert!(
        registered.environment.is_empty(),
        "an environment is registered, so every publishing job has to declare it"
    );
    assert_ne!(
        registered.owner_id, 0,
        "crates.io pins the owner's numeric id; it has to be written down"
    );
}

#[test]
fn the_npm_package_registered_is_the_one_this_repository_publishes() {
    // npm's configuration is per package, and a registration naming a package
    // this repository does not publish protects nothing.
    let registered = registered();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo::root()
                .join("packaging")
                .join("npm")
                .join("package.json"),
        )
        .expect("the npm package"),
    )
    .expect("the npm package parses");
    let published = manifest["name"].as_str().expect("the package's name");
    let names: Vec<&str> = registered
        .npm
        .iter()
        .map(|entry| entry.package.as_str())
        .collect();
    assert_eq!(
        names,
        vec![published],
        "the registered npm packages are not the one this repository publishes"
    );
    for entry in &registered.npm {
        assert_eq!(
            entry.allowed_actions, "publish",
            "{} may not publish directly, so the release would have to be promoted by hand",
            entry.package
        );
    }
}

#[test]
fn every_crate_this_workspace_publishes_has_a_registration() {
    // crates.io keys a configuration by crate, and one exchange covers every
    // crate whose configuration matches the run — so a crate without one is a
    // crate `cargo publish --workspace` cannot upload.
    let registered = registered();
    let members: BTreeSet<String> = workspace_members()
        .iter()
        .map(|member| crate_name(&repo::root().join(member)))
        .collect();
    let have: BTreeSet<String> = registered
        .crates
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(
        have, members,
        "the registered crates and the workspace's members disagree"
    );
}

#[test]
fn every_job_that_can_publish_states_its_own_tag_guard() {
    // Inherited through `needs:`, the guard is jobs away from the one it
    // protects and one reordering from being gone. The property worth reading
    // off a job is that it can only run for a release, and it should be
    // readable on the job.
    let registered = registered();
    let workflow = workflow(&registered);
    assert_eq!(
        unguarded(&workflow),
        Vec::<String>::new(),
        "these jobs can publish without a tag"
    );
    let publishing = jobs(&workflow)
        .into_iter()
        .filter(|(name, job)| can_publish(job) && name != REHEARSAL)
        .count();
    assert!(
        publishing >= 5,
        "only {publishing} jobs can publish; the workflow changed shape"
    );
}

#[test]
fn nothing_that_publishes_reads_a_repository_secret() {
    // The point of the exercise: after this, a release needs no secret that
    // could be stolen, forgotten or rotated. `GITHUB_TOKEN` is the exception
    // and not a repository secret — GitHub mints it per run, and it is what
    // the container registry and the release itself answer to.
    let registered = registered();
    let text = std::fs::read_to_string(workflow_path(&registered)).expect("the workflow");
    let read = secrets_read(&text);
    assert!(
        !read.is_empty(),
        "no `secrets.` at all; the check is vacuous"
    );
    for named in read {
        assert_eq!(
            named, "GITHUB_TOKEN",
            "the release reads the repository secret {named}"
        );
    }
}

#[test]
fn the_checks_catch_a_workflow_that_no_longer_matches() {
    // The rules above, run over the real workflow with one thing broken in
    // it. Building a synthetic job instead would test the helper rather than
    // the rule: what has to be known here is that a job of this workflow's
    // actual shape, missing its guard, is named — and that the guard being
    // there is why the rule is quiet the rest of the time.
    let registered = registered();
    let workflow = workflow(&registered);

    // A workflow named to the registries that is not here any more.
    let mut renamed = registered;
    renamed.workflow = "publish.yml".to_owned();
    assert!(
        !workflow_path(&renamed).is_file(),
        "a renamed workflow would still be found"
    );

    // A real publishing job that loses its guard.
    let victim = jobs(&workflow)
        .into_iter()
        .find(|(name, job)| can_publish(job) && name != REHEARSAL)
        .map(|(name, _)| name)
        .expect("some job publishes");
    let mut broken = workflow.clone();
    broken["jobs"][victim.as_str()]
        .as_mapping_mut()
        .expect("the job is a mapping")
        .remove("if");
    assert_eq!(
        unguarded(&broken),
        vec![victim.clone()],
        "removing `{victim}`'s guard went unnoticed"
    );

    // A crate renamed in its manifest and not in its directory. It cannot be
    // done to a real member — the workspace would stop resolving — so this is
    // a directory named one thing holding a crate named another.
    let elsewhere = tempfile::tempdir().expect("a directory");
    std::fs::write(
        elsewhere.path().join("Cargo.toml"),
        "[package]\nname = \"not-the-directory\"\n",
    )
    .expect("a manifest");
    assert_eq!(
        crate_name(elsewhere.path()),
        "not-the-directory",
        "the directory is being read instead of the manifest"
    );

    // And a secret creeping back, on a line that already has an allowed one.
    let named = secrets_read(
        "        env: { A: ${{ secrets.GITHUB_TOKEN }}, B: ${{ secrets.NPM_TOKEN }} }",
    );
    assert_eq!(named, vec!["GITHUB_TOKEN", "NPM_TOKEN"]);
}
