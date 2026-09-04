# 27 — Continuous integration

Status: done
Milestone: 5 — Packaging
Blocked by: 07

Test on the two first-class platforms and build on the third. Separate jobs for the minimum supported toolchain, linting, formatting and the dependency licence check. End-to-end tests never run here, and no job requires a credential.

## Acceptance criteria
- The matrix runs on both first-class platforms and builds on the best-effort one.
- The minimum-toolchain job fails if a dependency raises the requirement.
- Linting, formatting and licence checks fail the build on violation.
- A pull request from a fork runs the full suite without secrets.

## As built

`.github/workflows/ci.yml`, six jobs, none of them dependent on another so a
fork's pull request gets all six at once.

**test** — a matrix over `ubuntu-latest` and `macos-latest`, the two first-class
platforms, running `cargo test --workspace --all-targets --locked` and then
`--doc` separately, since `--all-targets` skips doctests. `fail-fast` is off:
one platform failing is a fact about that platform, and finding out about the
other in the same run is the point of testing both. The suite is offline — no
`tailscale` binary, no credential, no network — and the end-to-end tests report
themselves skipped because their gates are set nowhere in the file.

**build** — `windows-latest`, `cargo build --workspace --locked`. Best-effort
means the behaviour is not promised, not that it need not compile, so this
fails the run like any other job (Q104). It builds the binary and not the test
targets: test code may assume Unix freely.

**msrv** — the toolchain named by `rust-version` in the workspace manifest, read
out of the file when the job runs rather than written down a second time
(Q102), then `cargo check --workspace --all-targets --locked`. `--locked` is
what makes the criterion real: the root manifest sets `resolver = "3"`, so
without it the MSRV-aware resolver would quietly pick a compatible version and
the job would pass over the dependency that raised the requirement.

**lint** — `cargo clippy --workspace --all-targets --locked -- -D warnings`,
then `cargo doc --no-deps` under `RUSTDOCFLAGS=-D warnings`. Rustdoc is here and
not in a job of its own because a broken intra-doc link is a lint on the same
code clippy is already reading, and docs.rs builds exactly this output for the
three published crates (Q101).

**format** — `cargo fmt --all --check`.

**licences** — `cargo deny --all-features check`, whole rather than a subset.
`advisories` was left out at first, because its database is published elsewhere
and can turn a pull request red for something that is not in it; that reasoning
argued against itself, since the point of `deny.toml` is that a declared rule is
enforced, and `yanked = "deny"` is declared there as plainly as the licence
boundary (Q101). It passes on the tree as it stands.

No third-party actions (Q100): `actions/checkout` and `actions/cache` are
GitHub's own, rustup installs the toolchains, and `cargo-deny` is built from
source at a pinned version and cached on it. The cache covers the crates
registry only, not `target`, so each job compiles the tree — minutes, in
exchange for a run that depends on nothing outside the repository. The toolchain
and cache lines repeat across jobs; GitHub Actions cannot share steps short of a
composite action, and one that could not be run or tested from here would break
all six jobs rather than one, so the repetition stands.

The token is `contents: read` at the workflow level, and nothing reads a secret.
`crates/tailscale-mcp/tests/ci_needs_no_credential.rs` holds that mechanically
for everything a fork's pull request can reach — a workflow that runs on
`pull_request`, and any file under `.github` that is not a workflow, since a
pull-request workflow could pull it in. A release workflow driven by a tag is
out of that scope and may hold what tickets 28 and 29 will need. Refused: any
`secrets.`, any name beginning `TAILSCALE_` (the credentials and the end-to-end
gates in one rule), any setting granting write access — read after the comment,
quotes, braces and commas come off, so `contents: write # for the assets` and
`permissions: { contents: 'write' }` are caught too — and `pull_request_target`
anywhere at all, which is the trigger that would hand a fork this repository's
secrets. It also requires that some workflow actually run the suite on a pull
request, without which every rule above is a claim about nothing. Proved to fire
by adding `contents: write` and one gate to the real file and watching both come
back named (Q103).

`tests/repo/mod.rs` holds the walk up to the workspace root that this and
`fixtures_are_redacted` both needed.

**One bug, found by running the matrix rather than by reading it.** The suite
did not pass on Linux: `the_covered_table_follows_the_tools_it_claims_to_follow`
drives every local tool and reads back the command each ran, and
`tailscale_configure_sysext_status` is macOS-only, so off macOS it refuses
before spawning and runs none. Fixed by the branch the rest of that file already
uses — a tool that does not run here contributes its path from its contract row
instead of from a run, and its `COVERED` row is judged on the platform that has
the command (Q105).

Run here as the jobs run them. On macOS: 19 test binaries pass, the one doctest
is `ignore`d, clippy and rustdoc are silent, `cargo fmt --check` is clean,
`cargo deny --all-features check` answers "advisories ok, bans ok, licenses ok,
sources ok", and `cargo +1.88 check --workspace --all-targets --locked` finishes
clean. On Linux, in a `rust:latest` container over the same tree: the full suite
passes. The Windows job is the one thing not verified here — `tailscale-cli`,
which holds every Unix-specific line, cross-compiles clean to
`x86_64-pc-windows-msvc`, but `ring`'s C will not cross-compile without a
Windows sysroot, so the other two crates get their first evidence from the first
run.
