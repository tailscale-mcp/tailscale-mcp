# 26 — End-to-end tests against a real node and tailnet

Status: ready-for-agent
Milestone: 4 — HTTP mode and subcommands
Blocked by: 25

Tests that exercise the real CLI on the host and the real control plane, gated behind environment variables so they are skipped by default and never run in continuous integration. Read-only by default; any test that writes must be opt-in separately and must clean up after itself.

## Acceptance criteria
- With the gates unset, the suite skips these tests and reports why.
- With a read-only credential set, the tailnet read paths pass against a real tailnet.
- With the CLI present, the local read paths pass on the developer machine.
- No test writes to a real tailnet unless a second, separate gate is set.
