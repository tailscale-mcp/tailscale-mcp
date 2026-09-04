# 17 — Devices and posture toolsets

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 16

The 15 device tools and the 5 posture integration tools. Device listing supports the API's field selection and server-side filters, plus the one client-side window agreed for it, since the endpoint offers no pagination. Device identifiers accept either form the API accepts.

Authorisation, tagging, renaming, key expiry, address assignment and deletion are classified per the inventory; deletion and de-authorisation are destructive.

## Acceptance criteria
- All 20 tools are registered with the classifications from the inventory.
- Listing honours field selection and filters, and the client-side window slices without changing the response shape.
- Both device identifier forms resolve.
- Posture attribute operations round-trip against the fake server.
- Parameters holding a documented string quote its known values, from the constant beside the model (carried over from ticket 16).
- The six transport tests in `tests/control_plane.rs` are re-expressed as tool calls and the file is deleted (Q59, Q63).
- The device and posture shapes deferred to this ticket in `schema_drift.rs`'s `DEFERRED` table are modelled, and their rows removed (Q64).
