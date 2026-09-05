# Changelog

Every release of `tailscale-mcp`, newest first. Generated from the commit
history by [git-cliff](https://git-cliff.org); do not edit by hand.

## 1.0.3 — 2026-09-05

### Fixed

- The startup note named toolsets the session offers nothing from
- The public-bind refusal did not parse as a sentence
- Version said it also speaks the protocol it just named
- Tools ignored the surface switches it accepts and documents
- The result cap did not cover resources
- The instructions named toolsets the session had just said were hidden
- Two more places named a toolset the session does not offer

### Documentation

- Ticket 26's tailnet run is no longer outstanding
- Ticket 26's write path is blocked by the billing plan
- The Homebrew channel was documented but never published (ticket 29)
- The tap follows the release rather than being pushed to (Q124)
- Record that the tap now follows the release by itself
- A private channel for reporting a vulnerability

### Tests

- Call the DNS tool that exists (ticket 26)
- The write path, as far as a real tailnet will take it (ticket 26)

### Build and CI

- Poke the tap when a release publishes (Q125)
- Let the tap poke be run by hand, and say when its token is missing
- Keep asking about advisories after the pushes stop (Q126)
- Move off the actions that still target Node 20

## 1.0.2 — 2026-09-05

### Documentation

- Record the three calls the 1.0.0 bootstrap forced

### Build and CI

- Publish by trusted publishing, not by token (ticket 31)

## 1.0.1 — 2026-09-05

### Fixed

- The two things crates.io and the MCP registry refused

## 1.0.0 — 2026-09-05

### Added

- Five ways to install the server (ticket 29)

### Documentation

- The README and the reference pages (ticket 30)

### Build and CI

- Build, checksum and publish a tagged release (ticket 28)
- Set the default toolchain without --default

### Changes

- Initial commit
- Skeleton, core plumbing and server bootstrap (tickets 01-06)
- Contract test harness (ticket 07)
- Add the local status toolset: 25 read-tier tools
- Add the local preferences toolset: 8 tools that change this node
- Add the serve and funnel toolset: 10 tools that publish from this node
- Add the local files toolset: 11 tools that touch the local filesystem
- Add the tailnet lock toolset: the eight tools that change the trust root
- Add the debug toolset: 22 readers and 8 runtime knobs
- Add the passthrough: one tool for a command no other tool covers
- Ticket 15 — control-plane REST client foundation
- Typed models and the schema drift tripwire (ticket 16)
- Devices and posture toolsets (ticket 17)
- DNS and policy toolsets (ticket 18)
- Ticket 18 review fixes, and the models ticket 19 needs
- Keys, users, invitations, contacts and settings (ticket 19)
- Ticket 19 review: stop requiring what the API does not
- Webhooks, services, OAuth apps, logging and organisation (ticket 20)
- Self-severing detection, and stop gating on open enums (tickets 20 review, 21)
- Nine resources and three prompts (ticket 22)
- Apply the ticket 21-22 reviews
- Serve MCP over Streamable HTTP (ticket 23)
- Four subcommands and the policy pair (tickets 24, 25)
- End-to-end tests against a real node and tailnet (ticket 26)
- Continuous integration, and the Linux bug it found (ticket 27)


