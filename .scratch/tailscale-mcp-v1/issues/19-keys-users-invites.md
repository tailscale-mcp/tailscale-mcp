# 19 — Keys, users, invitations, contacts and settings toolsets

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 16

The 5 key tools, 7 user tools, 11 invitation tools across devices and users, 3 contact tools and 2 settings tools. Key creation takes the capabilities object in the API's own shape.

A newly minted secret is returned once, in full, with the tool description warning that it cannot be retrieved again, and is never logged. The endpoints that accept only a user-owned credential are registered as tools; under a tailnet-owned credential their failure carries a hint naming the requirement.

## Acceptance criteria
- A minted key's secret appears in the result and in no log line at any level.
- Creating a key with a capabilities object matching Tailscale's documented example succeeds unmodified.
- A user-owned-only endpoint called with a tailnet-owned credential produces the hint.
- Role changes, suspension, restoration and deletion are classified per the inventory.
- The key, user, contact and OAuth-app shapes deferred to this ticket in `schema_drift.rs`'s `DEFERRED` table are modelled, and their rows removed (Q64).
