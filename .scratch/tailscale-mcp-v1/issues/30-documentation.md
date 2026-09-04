# 30 — Documentation

Status: ready-for-agent
Milestone: 5 — Packaging
Blocked by: 29

The README and the documentation folder: what the server is, install per channel, configuration by environment variable and flag, the tier and preset model with the listing counts, the tool table generated from the metadata table rather than written by hand, the error codes with their meanings, and the comparison table against the three reference implementations that names the four deliberate exceptions so that the superset claim is honest.

## Acceptance criteria
- The tool table is generated and a test fails when it is stale.
- Every environment variable and flag is documented, with defaults.
- The comparison table lists each reference implementation's features and marks the four exceptions explicitly.
- The security section states what the tiers do and do not protect against, and that the server never escalates privileges.
