# 31 — Trusted publishing

Status: in-progress — waiting on 1.0.0
Milestone: 5 — Packaging
Blocked by: 30

The release workflow stops holding publishing secrets. npm and crates.io both
accept a GitHub Actions identity token in exchange for a short-lived publishing
credential, the same way the container registry and the MCP registry already
do, so afterwards this repository holds no publishing credential at all —
nothing to leak, nothing to rotate, and nothing a workflow change could
exfiltrate.

Neither registry will register a trusted publisher for a package that does not
exist. So this cannot be the way 1.0.0 is published: 1.0.0 goes out through the
workflow as it stands, with two scoped tokens that are revoked the same day,
and the conversion lands afterwards. That is why this ticket carries a
procedure as well as an implementation, and why the implementation waits on a
branch rather than landing on `main` — a converted `main` would make `v1.0.0`
untaggable.

Trust is scoped to owner, repository and workflow filename, with no GitHub
environment. Neither registry validates that configuration when it is saved,
and both match the workflow filename exactly: renaming `release.yml` breaks all
four registrations silently, and the failure appears only at the next release,
as npm's misleading `404 Not Found - PUT` or crates.io's `No Trusted Publishing
config found for repository …`. So the registered coordinates are checked in
and a test holds the workflow to them.

## Acceptance criteria
- No publishing job reads a repository secret, and no publishing secret is
  required for a release.
- Every publishing job states its own tag guard rather than inheriting one
  through `needs:`.
- `rehearse` proves the trust configuration of every registry that has one,
  before the GitHub release exists, and does so on a by-hand run as well as on
  a tag.
- The registered trusted-publisher coordinates are checked in, and a test fails
  when the workflow no longer matches them.
- `RELEASING.md` describes the steady-state release, and the one-time bootstrap
  is recorded here.

## The one-time bootstrap

This happens once, in this order. Steps 1–3 are the 1.0.0 release; the rest
convert the pipeline and are what this ticket's branch is waiting for.

1. **Mint two scoped tokens.** On npmjs.com, a granular access token limited to
   the `@tailscale-mcp` scope with read-and-write, given the shortest expiry
   offered. On crates.io, an API token with the `publish-new` scope — the
   crates do not exist yet — restricted to the three crate names. Add them as
   the repository secrets `NPM_TOKEN` and `CARGO_REGISTRY_TOKEN`.
2. **Tag `v1.0.0`.** The workflow publishes as it does today. The npm package
   gets its provenance attestation from `--provenance`, and all three crates go
   out from CI rather than from a laptop.
3. **Revoke both tokens and delete both secrets.** Immediately, not later. The
   expiry is the backstop, not the plan.
4. **Register four trusted publishers**, all naming the same owner, repository
   and `release.yml`, with no environment. One for the npm package, with
   **Allowed actions** opted into direct `npm publish` rather than stage-only;
   one each for `tailscale-rest`, `tailscale-cli` and `tailscale-mcp`. The
   values are in `packaging/registry/trusted-publishers.toml`; that file is
   what the test compares the workflow against, and editing it changes nothing
   until somebody retypes it into the two web forms.
5. **Merge this branch**, and release 1.0.1 — the first release to publish with
   no secret anywhere.
6. **Turn on npm's "Require two-factor authentication and disallow tokens"**,
   once that release has succeeded and not before. It makes trusted publishing
   enforced rather than conventional: without it, a forgotten token is still a
   way in. Turning it back on is 2FA-gated, which is the right amount of
   friction for undoing it.

## As built

The workflow holds no publishing secret. `secrets.GITHUB_TOKEN` is the only
`secrets.` reference left in it, and that one is minted per run by GitHub
rather than stored on the repository.

**npm.** The `npm` job runs on Node 24 and installs npm 11 explicitly —
trusted publishing wants npm 11.5.1 or newer, which wants Node 22.14 or newer,
and the npm a Node release bundles moves with its patch releases while the
requirement here is a floor. `NODE_AUTH_TOKEN` is gone, and `setup-node` moved
from v4 to v7 because of it: up to v6 that action exported
`NODE_AUTH_TOKEN=XXXXX-XXXXX-XXXXX-XXXXX` whenever the caller had not set one,
so dropping the secret would have left an `.npmrc` authenticating with a
placeholder rather than with nothing. v7 exports the variable only when it was
given (actions/setup-node#1558). `registry-url` stays, matching npm's own
example. `--provenance` is gone too, and not merely because it is redundant:
npm enables provenance automatically only while the setting is at its default,
so passing the flag would switch off npm's own skip for the cases where
attestation cannot work.

**crates.io.** The `publish` job takes its token from
`rust-lang/crates-io-auth-action`, pinned to a commit, placed last so that the
thirty minutes it lives are not spent installing a toolchain. One exchange
covers all three crates, because a token is minted for every crate whose
configuration names this run — which is what keeps `cargo publish --workspace`
a single step. The publish step carries its own `timeout-minutes: 25`, under
the token's life, so a hang fails saying so rather than dying half way through
three crates on an expired credential.

**Every job that can publish states its own tag guard**, the GitHub release
included — that one had been guarded by event type rather than by the ref,
which is the same thing only for as long as a tag is this workflow's only
`push` trigger. The rest inherited their protection from it through `needs:`,
jobs away from what it protected.

**`rehearse` asks the registries instead of counting secrets.** The old step
checked that two secrets existed. The new ones perform the crates.io exchange
through the action, which revokes it at job end; validate the MCP registry
listing, which needs no credential; and perform the npm exchange by hand
against the documented endpoint, reading only the status code and writing the
token to `/dev/null`. All three run on a by-hand rehearsal as well as on a tag.
`scripts/install-mcp-publisher.sh` now holds the pinned version and digest,
since two jobs fetch that binary.

**`packaging/registry/trusted-publishers.toml`** records what the two
registries were told — one npm package, three crates, all naming the same
owner, repository and `release.yml`, with no environment — and
`trusted_publishing_matches.rs` holds the workflow to it in seven tests: the
workflow named must exist and must publish something, the coordinates must be
this repository, the npm package must be the one this repository publishes and
must be allowed to publish directly, every workspace member must have a
registration under the name its own manifest gives it, every job that can
publish must state the tag guard, and nothing may read a repository secret but
`GITHUB_TOKEN`. Each was made to fail once while it was written, by doctoring
the file it guards; the last of the seven keeps doing that in the suite, by
running the rules over the real workflow with a guard removed from it.

**`RELEASING.md`** carries the steady state: what to run, what a tag starts,
what to do when it stops at each job, and the fact that renaming the repository
or the workflow invalidates all four registrations at once.

**Not done, and blocking.** Steps 1 to 6 of the bootstrap above. None of them
can happen until 1.0.0 is published, which is why this branch is not merged.
