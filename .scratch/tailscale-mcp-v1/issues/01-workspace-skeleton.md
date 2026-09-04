# 01 — Cargo workspace and toolchain policy

Status: done
Milestone: 1 — Skeleton and core
Blocked by: —

Create the workspace of three publishable crates: the control-plane REST client, the CLI wrapper, and the server binary depending on both. Edition 2024, MSRV pinned to the SDK's, version 1.0.0 across the workspace. Dependencies per the agreed stack: the official MCP SDK, an async runtime, an HTTP client on rustls with no OpenSSL, serde, schema generation, a derive-based argument parser, tracing to stderr controlled by the log environment variable, a typed error library in the libraries and a contextual one in the binary.

Lint policy is part of this ticket: warnings denied in CI, formatting enforced, and a dependency licence check that rejects copyleft, since ADR-0002 turns on the licence boundary.

## Acceptance criteria
- The workspace builds and the three crates are individually publishable, each with description, licence, repository and keywords set.
- The MSRV is declared and a build on that exact toolchain succeeds.
- Formatting, linting and the licence check pass and are wired into the same commands CI will run.
- No dependency pulls in OpenSSL; the licence check fails the build if a copyleft dependency is introduced.

## Answer

Done. The workspace builds, `cargo fmt --check` and `cargo clippy -D warnings` are clean, `cargo +1.88 build` succeeds on the declared MSRV, and `cargo deny check` reports advisories, bans, licenses and sources all ok. No OpenSSL is in the dependency tree.

Two things worth knowing for later tickets:

- The licence allow-list rejected `webpki-roots`, which ships Mozilla's root certificate store under `CDLA-Permissive-2.0`. That is a permissive data licence, not copyleft, so it was added with the reasoning recorded in `deny.toml`. This is the fail-closed behaviour the policy intends: a new licence stops the build until someone reads it.
- Workspace lints encode two design constraints as compiler errors rather than review habits: writing to standard output is denied because it corrupts the stdio transport, and unwrapping is denied because a panic on caller input drops the connection instead of returning a tool error. Logging to standard error stays allowed. Recorded as DECISIONS.md Q4.

## As built

Built with tickets 01–06 in one commit, `9d56516 Skeleton, core plumbing and
server bootstrap (tickets 01-06)`, because the six are one another's
prerequisites: a tool cannot be declared without a tier to declare, a tier is
unobservable without an error model, and none of it runs without a way to
execute a process. That commit's message is the record of what was built; no
ticket in the group wrote an "As built" section, and the practice started at
ticket 11. The status is corrected here rather than the prose invented after
the fact.
