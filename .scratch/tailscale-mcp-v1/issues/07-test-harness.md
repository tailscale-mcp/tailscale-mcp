# 07 — Contract test harness

Status: done
Milestone: 1 — Skeleton and core
Blocked by: 06

Build the primary seam: an in-process client connected to a fully constructed server, with a fake local backend and a fake control-plane HTTP server underneath. Add the table-driven contract test that walks the metadata table and asserts, for each tool, its tier, its toolset, its annotations, one success case and one error case.

Fixtures are recorded responses with identity redacted. The harness must make adding a tool without a contract row a test failure.

## Acceptance criteria
- The harness constructs the server with an arbitrary preset and tier combination and drives it as a client.
- The contract test enumerates the metadata table, and a tool with no case fails the suite.
- Fixtures contain no node names, tailnet names, addresses or account identifiers.
- The suite runs offline and does not require the CLI or credentials.
