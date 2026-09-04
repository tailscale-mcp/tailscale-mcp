---
status: accepted
---

# Write our own control-plane REST client instead of depending on rtailscale

rtailscale is the only existing Rust client for Tailscale's control-plane API, but it is licensed AGPL-3.0 with a commercial dual licence, and this project is Apache-2.0. Depending on it, vendoring it, or forking it would put this project under AGPL terms or require a commercial licence, so the control-plane client is written in-house as its own crate in the workspace. rtailscale stays a reference for behaviour only; no code is copied from it.

## Considered options

- **Depend on rtailscale.** Fastest start, but AGPL contamination of an Apache-2.0 project.
- **Fork or vendor rtailscale.** Same licence problem, plus its untyped JSON models and single action-enum tool do not fit the per-verb typed tools chosen for this server.
- **Own client.** Chosen.

## Consequences

- `cargo-deny` rejects copyleft licences so the dependency cannot creep back in through a transitive crate.
- Coverage is our responsibility: every documented operation, plus the two OAuth token endpoints the schema omits, is written and tested here.
