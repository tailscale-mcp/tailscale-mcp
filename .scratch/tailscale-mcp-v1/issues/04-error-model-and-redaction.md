# 04 — Error model and secret redaction

Status: done
Milestone: 1 — Skeleton and core
Blocked by: 02

Implement the fourteen fixed error codes as tool-level results, not protocol errors: the eight agreed at the outset plus the five added later and the platform one. Each result carries a code and a message, and optionally an exit code, captured output, a status and a hint. Protocol errors are reserved for malformed requests.

Redaction belongs here because every error path can carry a secret: auth keys, API access tokens and client secrets are removed from messages, captured output, and any logged argument list, wherever they appear.

## Acceptance criteria
- Every code is produced by at least one path and is stable, documented text.
- A failure carrying a key-shaped value in its output has that value redacted in the result and in the log.
- A malformed request produces a protocol error; a failed operation never does.
- Hints are present on the codes where the design promised one, including the permission, version, platform, size and conflict codes.

## As built

Built with tickets 01–06 in one commit, `9d56516 Skeleton, core plumbing and
server bootstrap (tickets 01-06)`, because the six are one another's
prerequisites: a tool cannot be declared without a tier to declare, a tier is
unobservable without an error model, and none of it runs without a way to
execute a process. That commit's message is the record of what was built; no
ticket in the group wrote an "As built" section, and the practice started at
ticket 11. The status is corrected here rather than the prose invented after
the fact.
