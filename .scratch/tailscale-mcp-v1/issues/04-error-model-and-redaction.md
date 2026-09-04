# 04 — Error model and secret redaction

Status: ready-for-agent
Milestone: 1 — Skeleton and core
Blocked by: 02

Implement the fourteen fixed error codes as tool-level results, not protocol errors: the eight agreed at the outset plus the five added later and the platform one. Each result carries a code and a message, and optionally an exit code, captured output, a status and a hint. Protocol errors are reserved for malformed requests.

Redaction belongs here because every error path can carry a secret: auth keys, API access tokens and client secrets are removed from messages, captured output, and any logged argument list, wherever they appear.

## Acceptance criteria
- Every code is produced by at least one path and is stable, documented text.
- A failure carrying a key-shaped value in its output has that value redacted in the result and in the log.
- A malformed request produces a protocol error; a failed operation never does.
- Hints are present on the codes where the design promised one, including the permission, version, platform, size and conflict codes.
