# 29 — Distribution channels

Status: done
Milestone: 5 — Packaging
Blocked by: 28

The five channels beyond the release binaries: the scoped npm launcher that downloads and verifies a release binary, the container image, the Homebrew tap, the registry listing, and the plugin manifest for the client that supports one. Names are settled and recorded in the research notes.

Publishing under the npm scope needs a token with write access to it, which the maintainer must supply at this point.

## Acceptance criteria
- The launcher installs and runs on both first-class platforms and verifies the checksum before executing.
- The container image runs the server with no arguments and honours the environment variables.
- The tap formula installs a working binary.
- The registry listing validates against the registry's schema, and the plugin manifest loads in its client.

## As built

Five channels, `packaging/` holding what each one needs, and every one of them
checked on a pull request rather than first exercised by a release (Q115).

**npm — `@tailscale-mcp/tailscale-mcp`.** A launcher that carries no binary:
`lib/launcher.js` works out the target triple for the machine, fetches the
release's `SHA256SUMS` and the archive together, refuses to go on unless the
archive hashes to what that file says, and only then unpacks it into a cache
under `XDG_CACHE_HOME` (or `LOCALAPPDATA`) so the download happens once.
`bin/tailscale-mcp.js` runs it with `stdio: "inherit"`, forwards `SIGINT`,
`SIGTERM` and `SIGHUP`, and exits with the server's status — or `128 + signal`
where a signal ended it. The scratch directory it unpacks into is inside the
cache, because the move at the end has to be a rename and a rename cannot cross
a filesystem.

Seven tests, run in `ci.yml` on Node 20 and 24 on both first-class platforms.
Three build a real archive with `tar` and serve it through an injected fetch: a
matching archive is unpacked, cached and not fetched again; a tampered one is
refused with nothing unpacked; one the release does not list is refused too.
Three cover the mapping from a machine to a target, where the cache goes, and
how `SHA256SUMS` is read. The seventh runs `bin/` itself against a warmed
cache, which is the part `npx` invokes — arguments reach the server, its
streams are its own, and its exit status comes back out.

**Container image — `ghcr.io/tailscale-mcp/tailscale-mcp`.** `rust:1-alpine`
builds a musl binary, `gcr.io/distroless/static-debian12:nonroot` runs it: 31.5
MB, uid 65532, no shell. Published for `amd64` and `arm64`, each built on a
runner of its own architecture and joined under one tag with
`docker buildx imagetools create` (Q114) — emulation would have meant a
third-party image running `--privileged` and an hour-long build.

`scripts/check-container-image.sh` is what makes "runs the server" mean
something: it checks the image runs as somebody other than root, sends it an
`initialize` frame over stdio and looks for a `serverInfo` in the answer, and
then checks the environment reaches it — a preset that does not exist is
refused, `minimal` and `full` give different tool counts. `ci.yml` runs it
against the image it just built, and the release runs it on both architectures
before pushing. The fake credential it needs lives in the script rather than in
the workflow, so that `ci_needs_no_credential` can go on refusing any
`TAILSCALE_`-named variable in a fork-reachable workflow without an exception
for this one.

**Homebrew — `tailscale-mcp/tap/tailscale-mcp`.**
`packaging/homebrew/tailscale-mcp.rb.in` is a template, not a formula: a
formula names archives and checksums that do not exist until the release that
produced them (Q113). `scripts/update-formula.sh` renders it from a release's
`SHA256SUMS` and refuses if a marker is left unfilled. The release attaches the
result; updating the tap — another repository — is committing it there.

Checked by installing it: the rendered formula went into a local tap and
`brew install` downloaded the archive, verified the checksum, installed the
binary and ran `brew test` against it. `brew audit` then found that an explicit
`version` is redundant with the one Homebrew scans from the archive name, so
the template carries none.

**MCP registry — `io.github.tailscale-mcp/tailscale-mcp`.** `server.json` at
the root, offering the npm package and the container image, each with the seven
environment variables an MCP client would set. Validated in the suite against a
vendored copy of the registry's own schema, with the listing's `$schema` and
the vendored `$id` required to agree — a listing written against a newer schema
and checked against an older one has been checked against nothing in
particular. The registry's own `mcp-publisher validate` was run against it too,
and says it is valid.

The registry does not take our word for it that these packages are ours: it
pulls each one and looks for its own name for this server inside. So the npm
package carries `"mcpName"` and the image carries a
`io.modelcontextprotocol.server.name` label, and a test holds all three to the
same string; the OCI identifier carries the version as a tag, because that is
the format the registry documents and without it the listing would offer
whatever `:latest` happened to be. A `registry` job publishes the listing after
both packages exist, authenticating with a GitHub Actions identity token rather
than a secret — the namespace that grants is `io.github.<owner>`, which is the
name this listing already had.

**Plugin bundle — `.mcpb`.** `packaging/mcpb/manifest.json` at manifest version
0.3, which is what `mcpb-manifest-latest.schema.json` is (Q111). Eight
`user_config` settings — the two credential shapes, the tailnet, the preset,
the two tiers and the path to the `tailscale` binary — substituted into the
environment the server reads. A setting left blank becomes an empty variable,
and every variable this server reads treats empty as absent, so a blank install
is a read-only server with the local tools and no error; that was checked, not
assumed. `scripts/build-mcpb.sh` assembles one bundle per released binary,
narrowing `compatibility.platforms` to the platform that bundle is for (Q112).

Nine tests. Beyond the schema, three agreements the schema cannot see: every
`${user_config.…}` reference resolves to a setting the manifest declares and
every declared setting is used; every variable the manifest sets is one the
server reads, taken from `config::ENV_VARS` and `credentials::ENV_VARS` rather
than written out again; and the entry point and the command name the same file.

**What the release does with all this.** `release.yml` gained the assembly and
four publishing jobs, ordered by how far each can be taken back (Q116): the
GitHub release, then npm and the container registry, then crates.io. The
listing goes out beside crates.io rather than before it, so an outage at a
registry still in preview cannot stop a release. The release job builds the
five bundles from the archives, checksums archives and bundles into one
`SHA256SUMS`, renders the formula, appends the formula's own checksum, and
attaches the lot — twelve files. Where a version can float, a pre-release does
not take it: `npm publish --tag next` and no `:latest` on the image. The
rehearsal now checks for `NPM_TOKEN` as well as `CARGO_REGISTRY_TOKEN`, so a
tag missing either fails before anything is created.

**One more thing that is now checked mechanically.** The five targets are named
in six places in four languages, and nothing in any of them refers to any
other: `every_target_is_distributed.rs` reads the release matrix and holds the
launcher, the formula template, the rendering script and the bundle script to
it, with Homebrew's missing Windows written down as the one deliberate
difference. Both halves of it were shown to fire by removing a target from the
launcher and misspelling one in the formula.

**Two things the maintainer has to do once.** Add `NPM_TOKEN` — a token with
write access to the `@tailscale-mcp` scope — as a repository secret. And, after
the first release, make the `ghcr.io` package public: the first push creates it
private, and that is a setting rather than something a workflow should change.
Nothing else needs a credential: the container registry and the MCP registry
both take the identity GitHub already gives the workflow.

**The version now lives in six files** (Q117). `scripts/prepare-release.sh`
writes all of them and rolls every one back if any step fails;
`release_is_one_version.rs`, `registry_listing_is_valid.rs` and
`plugin_manifest_is_valid.rs` refuse a tree where they disagree. Checked by
bumping a copy of the tree to 1.1.0 with the script and running those tests
there.

**Not done.** The launcher, the formula and the bundles have not been exercised
against a real release, because there is not one yet: what has been proved is
that each does the right thing with archives built here, and that every refusal
in them fires. Nothing has been published — the four publishing jobs have never
run, and the first tag is where they are first tried. The tap repository does
not exist, and the listing does not offer the bundles: a bundle package needs
the `fileSha256` of an artefact that only exists once a release has built it,
which a checked-in listing cannot carry.
