---
status: accepted
---

# Drive the local node through the `tailscale` CLI, not tailscaled's LocalAPI

The `tailscale` CLI is itself a thin client of tailscaled's LocalAPI, so calling LocalAPI directly would give structured JSON for every operation and skip text parsing. We shell out to the CLI anyway: LocalAPI is undocumented, changes between releases, and is reached differently on each platform (a Unix socket on Linux, a per-user TCP port plus token on the macOS app variants), whereas the CLI is the interface Tailscale documents and keeps stable, and it already offers `--json` wherever structure matters. The local surface sits behind a `LocalBackend` trait so a LocalAPI implementation can be added later without touching the tools.

## Considered options

- **LocalAPI only.** Best data, but undocumented, platform-specific transport, and marked unstable in Tailscale's own source.
- **CLI for writes, LocalAPI for reads.** Two transports to keep working, for a gain limited to the handful of commands that have no `--json` flag.
- **CLI only.** Chosen.

## Consequences

- The server needs a `tailscale` binary on `PATH` and inherits that binary's privilege model: on Linux, write commands need root or the configured operator user, and the server never escalates.
- Commands without `--json` are parsed from text, and those parsers are the part most exposed to Tailscale releases.
