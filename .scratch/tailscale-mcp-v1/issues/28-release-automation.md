# 28 — Release automation

Status: done
Milestone: 5 — Packaging
Blocked by: 27

Binary release building and publishing for the supported platforms, and automated version and changelog management driven by commit messages in the conventional form. All three crates publish together at the same version. The first release is 1.0.0 per ADR-0005.

## Acceptance criteria
- A tagged release produces binaries for the supported platforms with checksums.
- The changelog is generated from commit history and the version bump follows the commit types.
- A dry run publishes nothing and reports what it would publish.
- The three crates publish in dependency order.

## As built

`.github/workflows/release.yml`, `cliff.toml`, `scripts/prepare-release.sh`,
`scripts/check-commit-messages.sh` and a generated `CHANGELOG.md`. No
`cargo-dist` and no `release-plz`: a release workflow is the last place to relax
Q100's refusal of third-party actions, since it is the job holding the registry
token, and what is left after removing them is four commands that already exist
(Q106).

**Preparing.** `scripts/prepare-release.sh` asks `git cliff` what the commits
since the last tag make the next version, writes it into the three places the
workspace manifest carries it, runs `cargo update --workspace`, and regenerates
`CHANGELOG.md`. Then it stops: the commit and the tag are a person's.
`--dry-run` prints the version it would move to, the changelog diff, and the
crates it would publish, and changes nothing. A version it will not write — not
semver, or carrying a quote or an `@` that `perl` would interpret — is refused
before anything is touched, and a failure part-way through puts the manifest,
the lockfile and the changelog back.

**Releasing.** Pushing `v<version>` starts the workflow. A run started by hand
is a rehearsal instead — there is no `dry_run` input, because a boolean that
defaults to safe is one somebody will eventually get wrong in the other
direction, and a tag is already the deliberate act (Q108).

- **build** — five targets on five native runners: `x86_64`/`aarch64` Linux,
  `x86_64`/`aarch64` macOS, `x86_64` Windows (Q109). Nothing is cross-compiled,
  which matters because `ring` builds C. The Intel Mac runs on
  `macos-15-intel`; `macos-13`, the label this first reached for, was retired in
  December 2025. The tag and the manifest must agree on the version or the job
  fails before building. Each archive is a `tar.gz` of the binary, `README.md`
  and `LICENSE`.
- **rehearse** — runs the suite, checks `CARGO_REGISTRY_TOKEN` exists, then
  `cargo publish --workspace --dry-run --locked`. The suite is here because
  `ci.yml` triggers on a push to main and on a pull request, and a tag is
  neither: without it a tag pointing at an untested commit would be released.
  The token check is here so a tag missing it fails before a release exists
  rather than after. A run started by hand ends at this job.
- **release** — one `SHA256SUMS` over the five archives in the format
  `sha256sum -c` reads, release notes taken out of `CHANGELOG.md` rather than
  generated again — a second generator run on a different day would date the
  release differently from the file the repository ships — and `gh release
  create --verify-tag`. The only job with `contents: write`.
- **publish** — `cargo publish --workspace --locked`, last. A crates.io upload
  cannot be taken back and a GitHub release can, so everything reversible has
  already succeeded before it runs. `--workspace` is what publishes the three in
  dependency order and refuses to start if any of them would fail (Q106).

**Conventional commits from here on, and checked.** None of the commits before
this one is in that form, so `cliff.toml` keeps unconventional commits rather
than dropping them — the 1.0.0 changelog *is* that history, and a first release
whose notes omitted the work that built it would be the worse artefact (Q107).
`[bump] initial_tag = "v1.0.0"` makes the first release 1.0.0 rather than
git-cliff's default 0.1.0, which is the number ADR-0005 exists to argue against.
Saying the convention is adopted is not the same as adopting it, so
`scripts/check-commit-messages.sh` holds every commit after a named baseline to
it and `ci.yml` runs it as a job of its own (Q110).

**A third credential the maintainer must supply.** The spec names two — a
read-only control-plane credential and an npm token. Publishing to crates.io
needs `CARGO_REGISTRY_TOKEN` as a repository secret, and nothing said so; the
rehearsal now fails on a tag without it, with the reason.

**Verified here.** `cargo publish --workspace --dry-run` packages all three and
names them in dependency order — `tailscale-cli`, `tailscale-rest`,
`tailscale-mcp` — and uploads nothing. Against a clone of this repository tagged
`v1.0.0`: `fix:` bumps to 1.0.1, `feat:` to 1.1.0, `feat!:` to 2.0.0, `docs:`
and `chore:` to 1.0.1, and a feature and a fix together to 1.1.0. Running the
script for real in that clone moved all three version sites, updated
`Cargo.lock`, and put a `## 1.1.0` section at the top of the changelog; making
`cargo update` fail part-way through put all three back. The changelog's
sections come out in the order `cliff.toml` names rather than alphabetically,
which needed the `<!-- n -->` prefixes git-cliff sorts on. The commit-message
check passes on the current history and fails, naming them, when the baseline is
moved back two commits.

`crates/tailscale-mcp/tests/release_is_one_version.rs` holds the pieces
together: every crate takes its version from the workspace, both internal
dependencies name that version — a published `tailscale-mcp` asking for a
`tailscale-rest` that was never uploaded is the failure this prevents — and the
changelog's newest heading is the version being released, which is also the
section the release notes are cut from. Proved to fire by moving the changelog's
heading to 0.9.0.

**Not verified here:** the workflow itself has never run. The commands it runs
have, but the runners, the artefact upload and `gh release create` get their
first evidence from the first tag.

**For ticket 29:** the Linux archives are linked against the runner's glibc, so
the container image has to build its own binary rather than unpack a release
archive. That is where the musl question belongs.
