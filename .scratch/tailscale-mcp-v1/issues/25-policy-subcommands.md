# 25 — Policy validation and deployment subcommands

Status: done
Milestone: 4 — HTTP mode and subcommands
Blocked by: 24

Validate and deploy a policy file from the command line, reusing the same client code as the tools, including the version identifier guard. Intended for a continuous integration pipeline, so exit codes and output are the contract: quiet on success, diagnostic on failure.

## Acceptance criteria
- Validation of a malformed document exits non-zero and prints the upstream errors.
- Deployment sends the version identifier read immediately before, and a mismatch exits non-zero with the conflict explained.
- Both subcommands work against the fake server in tests and require no MCP client.

## As built

`tailscale-mcp policy check <file>` and `tailscale-mcp policy deploy <file>`,
in `crates/tailscale-mcp/src/subcommands.rs`.

Both go through the registry by tool name rather than calling a handler, so the
parameter parsing, the version guard, the error codes and the request shaping
are the ones a tool call gets: a pipeline checking a policy and an agent writing
one cannot disagree about what is valid (Q94). The gate is the policy toolset at
the destructive tier whatever the operator passed, because the tier constrains
an agent and there is no agent here.

`deploy` reads the policy immediately before writing it and sends back the
`ETag` that read answered with. The read is inside the subcommand and not an
argument: the guard exists so that the document being replaced is the one that
was read, and an `etag` carried in from an earlier pipeline step would be
guarding against the wrong thing. A tailnet whose read returns no `ETag` has an
untouched policy, and the write goes over the control plane's default instead.

Exit codes and output are the contract. Both print nothing at all on success,
because a pipeline log is read when something went wrong and a subcommand that
prints a paragraph on every green run trains people not to read it. A failure
prints the control plane's own message rather than a summary of it, and a
version mismatch — and only a version mismatch — adds the one sentence a person
needs: somebody else wrote to this tailnet, read it again and merge. A malformed
document already says what is wrong with it, and that advice on a 400 would send
somebody looking for a change nobody made.

Seven tests in `crates/tailscale-mcp/tests/policy_subcommands.rs`, all against the
fake control plane with no MCP client: the quiet success, the malformed document
with the upstream error passed through, the `If-Match` carrying the version read
a moment before and in that order, the 412 explained, a 400 explained without the
merge advice, the untouched tailnet, and a file that is not there failing rather
than panicking.
