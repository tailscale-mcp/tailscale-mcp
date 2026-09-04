# 25 — Policy validation and deployment subcommands

Status: ready-for-agent
Milestone: 4 — HTTP mode and subcommands
Blocked by: 24

Validate and deploy a policy file from the command line, reusing the same client code as the tools, including the version identifier guard. Intended for a continuous integration pipeline, so exit codes and output are the contract: quiet on success, diagnostic on failure.

## Acceptance criteria
- Validation of a malformed document exits non-zero and prints the upstream errors.
- Deployment sends the version identifier read immediately before, and a mismatch exits non-zero with the conflict explained.
- Both subcommands work against the fake server in tests and require no MCP client.
