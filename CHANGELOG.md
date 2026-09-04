# Changelog

Every release of `tailscale-mcp`, newest first. Generated from the commit
history by [git-cliff](https://git-cliff.org); do not edit by hand.

## 1.0.0 — 2026-09-04

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


