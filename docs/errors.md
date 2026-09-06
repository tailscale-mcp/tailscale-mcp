# Errors

A failed tool call comes back as an MCP tool result marked as an error, whose
content is one JSON object:

```json
{
  "code": "needs_operator",
  "message": "this user is not the local node's operator",
  "exit_code": 1,
  "stderr": "Access denied: this command is available only to the operator",
  "hint": "Run the server as the operator, or set one with `tailscale set --operator`."
}
```

`code` and `message` are always there. `exit_code` and `stderr` appear when a
process is what failed, `status` when the control plane is, and `hint` wherever
the fix is something the caller or the operator controls — which is most of
them.

**Branch on `code`, not on `message`.** The codes below are the whole set and
they do not change within a major version; the messages are written for a
reader and will.

**Nothing in an error is a secret.** Every string in the envelope has been
through the same redaction as everything else this server returns, so an auth
key that appeared in a command's standard error reaches the caller as
`tskey-…`.

## The codes

| Code | What happened | What to do |
|---|---|---|
| `cli_failed` | The `tailscale` binary ran and exited non-zero. | Read `stderr`, which is the command's own. This is the code for a Tailscale-level refusal that has no more specific code here. |
| `api_error` | The control plane returned a status this server does not model more precisely. | Read `status` and `message`. |
| `timeout` | The operation did not finish inside its budget. | Ask for less: a smaller count of probes, a narrower filter. Every long-running command here is bounded on purpose. |
| `not_permitted` | The tool exists, but this server was not started with the tier or toolset that permits it. | The `hint` names the flag that would offer it — `--allow-write`, `--allow-destructive`, or `--toolsets +<name>`. This is an operator's decision, not something a caller can pass an argument to change. |
| `needs_operator` | The local node refuses the command because this user is not its configured operator. | Run the server as the operator, or set one with `tailscale set --operator`. |
| `unsupported_version` | The installed `tailscale` is older than the command requires. | Update Tailscale. The `hint` names the version wanted. |
| `backend_unavailable` | What the tool's surface needs is absent: no binary on the path, no credential configured, or a Tailscale client that is not answering. | `tailscale-mcp diagnose` reports each of the three. In a normal session the tools of a surface that is not reachable are not offered at all, so this is what a surface that goes away mid-session looks like. |
| `invalid_args` | The arguments parsed but do not describe a workable request. | The `message` says which argument and why. |
| `unsupported_platform` | The command does not exist on this operating system. | Nothing — the tool is listed everywhere because the tool table is the same everywhere, but only some systems can run it. |
| `not_found` | The target of the operation does not exist. | Check the identifier. A device can be named by its node id, its numeric id, its MagicDNS name, its short name, its hostname or one of its addresses; a service name includes its `svc:` prefix. |
| `conflict` | The state changed underneath: a stale version identifier, or a resource that already exists. | Read the current state again and retry from it. For the policy file that means reading its version identifier immediately before writing. |
| `rate_limited` | The control plane asked us to slow down. | Wait and retry. |
| `result_too_large` | A tool result or a resource would exceed the configured size cap. | Ask for less, or raise `TAILSCALE_MCP_MAX_RESULT_BYTES`. The cap exists so that a result no model can hold fails as a sentence rather than as a wall of JSON. |
| `confirmation_required` | The operation is one the caller must state intent for. | Call it again with `confirm: true`. The tools that ask are the ones that affect the whole tailnet or that can cut this server off from what it is driving; the `hint` says which it is. |

## Two refusals that are not errors

A tool that is not offered is not listed. A session that has no control-plane
credential does not show the tailnet tools at all, rather than showing them and
answering `backend_unavailable` when they are called; the same goes for a tier
that has not been permitted. This is deliberate: a tool a caller can see is one
it can use.

A destructive tool that would sever this server from the node or tailnet it is
driving asks for `confirm` rather than refusing. The server does not decide
that the operation is wrong — it makes the caller say that it meant it.

## Naming a device ambiguously

The tailnet tools accept any of the names a device answers to — its MagicDNS
name, the short form of it, its hostname, or one of its addresses — as well as
its node id and its numeric id. Anything that is not already an identifier is
looked up in the tailnet's own device listing, once per ten seconds and shared
across the tools, so a run of calls naming devices costs one listing rather
than one each.

Hostnames are not unique: two machines called `macbook-air` is an ordinary
state of affairs. When a name matches more than one device the call is refused
with `invalid_args` naming the candidates, rather than resolved to whichever
the listing happened to return first. The alternative is a coin flip, and the
tools that take a device include the ones that delete it.
