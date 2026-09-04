# 10 — Serve and funnel toolset

Status: done
Milestone: 2 — Local surface
Blocked by: 09

The 10 tools covering background serve and funnel: status, the set and clear operations, reset, and the configuration exchange. Foreground forms are excluded. Configuration is exchanged as JSON inline, using a temporary file where the CLI insists on one.

Funnel is classified destructive rather than write, because it publishes to the public internet, and its description says so plainly.

## Acceptance criteria
- Serve tools are available at the write tier; funnel tools only at the destructive tier.
- Reading the configuration and writing it back unchanged is a no-op.
- No tool leaves a foreground process running; every call returns.
- The configuration temporary file is private and removed after the call.
