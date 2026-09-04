# 12 — Tailnet lock toolset

Status: ready-for-agent
Milestone: 2 — Local surface
Blocked by: 09

The 8 tailnet lock tools: status, log with a bounded limit, signing, and the key and node operations. Initialisation, disabling and key revocation change the tailnet's trust root, are irreversible for everyone, and require a confirmation in addition to the destructive tier.

## Acceptance criteria
- Status and log are read tier; the log honours its default and cap.
- Initialise, disable and revoke keys refuse without a confirmation even when the destructive tier is enabled.
- Signing a node succeeds against the fake backend and reports the resulting state.
