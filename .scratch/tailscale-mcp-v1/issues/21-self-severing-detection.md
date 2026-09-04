# 21 — Self-severing detection on the tailnet surface

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 17

Identify the local node's own device so that control-plane operations targeting it are recognised as self-severing: deletion, key expiry, de-authorisation and re-tagging. The identity comes from cached local status, refreshed on a sensible interval, and matching accepts either device identifier form.

When the local surface is disabled there is no local identity to match against; the behaviour in that case must be decided and documented, defaulting to treating the operation as ordinary rather than silently failing open on a claim it cannot check.

## Acceptance criteria
- An operation targeting the local node's device refuses without a confirmation and succeeds with one.
- The same operation against any other device is unaffected.
- With the local surface disabled, behaviour matches what the ticket documents and is covered by a test.
- The missing-confirmation refusal uses the distinct code agreed for it.
