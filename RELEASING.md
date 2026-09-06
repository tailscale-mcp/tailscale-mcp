# Releasing

A release is a tag. Pushing one starts `.github/workflows/release.yml`, which
builds five binaries, creates the GitHub release, and publishes to npm, the
container registry, the MCP registry and crates.io. crates.io goes last of
those, because a crate cannot be taken back and everything else can. The
others are not strictly ordered — the container image is built while npm
publishes, and the MCP registry listing goes out alongside the crates.

**No secret is needed and none is held.** Every registry authenticates the
workflow by its own GitHub Actions identity: npm and crates.io exchange it for
a token that lives minutes, the MCP registry grants the `io.github.<owner>`
namespace, and the container registry answers to the per-run `GITHUB_TOKEN`.
What npm and crates.io were told about this repository is written down in
[`packaging/registry/trusted-publishers.toml`](packaging/registry/trusted-publishers.toml).

## Making one

```sh
scripts/prepare-release.sh          # or --version x.y.z to override the rule
git diff                            # read it
git commit -am "chore(release): x.y.z"
git push
git tag vx.y.z && git push origin vx.y.z
```

`prepare-release.sh` works out the version from the commit messages since the
last tag — breaking is major, a feature is minor, anything else is a patch —
writes it into the six places that carry it, and regenerates the changelog. It
does not commit, tag or publish: the diff is meant to be read first.

The tag must match the version in `Cargo.toml`, and the changelog's newest
section must name it. The workflow checks both and refuses rather than
releasing something inconsistent.

A pre-release version — anything with a hyphen, such as `1.1.0-rc.1` — goes to
npm's `next` dist-tag and does not move the container image's `latest`. It is
a real release in every other way.

## Rehearsing one

Run the workflow by hand from the Actions tab. It builds every target, runs
the suite, reports what `cargo publish` would upload, checks the MCP registry
listing, and asks npm and crates.io whether they would accept a release from
this workflow — which is the part most likely to have broken without anyone
touching this repository. It creates nothing and publishes nothing.

The MCP registry is not asked. Its trust is this repository's own identity
rather than a configuration somebody typed, so there is nothing there to
drift; what can be wrong is the listing, and that is what `validate` checks.

Rehearse after anything that changes this repository's identity: a rename, a
transfer to another owner, or a change to `release.yml`'s filename. Any of
those silently invalidates the trusted-publisher configurations, and neither
registry will say so until a release tries to publish.

## When it goes wrong

The jobs are ordered so that a failure costs as little as possible, and
`rehearse` runs every check that does not need the release to exist yet. What
to do depends on where it stopped.

Jobs that share a `needs:` run together, so "what exists already" is what has
*finished*, not a strict prefix of the list.

| Stopped at | What exists already | What to do |
|---|---|---|
| `build` or `rehearse` | Nothing | Fix, delete the tag, tag again |
| `release` | Nothing published | As above; delete the GitHub release if one was created |
| `npm` | The GitHub release; the image may be building | Fix, then re-run the failed jobs from the Actions tab |
| `image` / `image-manifest` | The release, and npm if it finished | Re-run the failed jobs |
| `registry` | The release, npm, the image; crates.io may be running | Re-run; the MCP registry is in preview and does not gate crates.io |
| `publish` | The release, npm, the image; the listing may be out | Re-run. If some crates uploaded, `cargo publish` skips those already at this version |

A version on crates.io cannot be deleted, only yanked. A version on npm can be
unpublished within its first 72 hours. A GitHub release and a container tag can
be removed. That ordering is why `publish` is last.

## Changing where it publishes from

Renaming this repository, transferring it to another owner, or renaming
`release.yml` breaks all four trusted-publisher configurations at once. Each
has to be recreated: they cannot be edited, and neither registry validates one
when it is saved.

So does deleting the GitHub organisation and creating it again under the same
name. crates.io stores the owner's numeric id as well as its name and checks
it, so every value in `trusted-publishers.toml` would still look right while
the release failed. The id is recorded there for exactly that morning.

`trusted_publishing_matches.rs` holds this repository to the coordinates in
`trusted-publishers.toml`, so an inconsistency here fails the suite. It cannot
check the registries — editing that file changes nothing until somebody retypes
it into npmjs.com and crates.io.
