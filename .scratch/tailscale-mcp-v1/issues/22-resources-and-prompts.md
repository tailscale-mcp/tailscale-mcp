# 22 — Resources and prompts

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 18

The nine resources across the two schemes, including one template addressed by device identifier and the policy resource served with its document media type, and the three prompts. Resources are read-only and available whenever their surface is on; there are no subscriptions.

All three prompts must work under the read tier, since validation and preview do not mutate.

## Acceptance criteria
- Listing and reading each resource works through the in-process client; the template resolves for a valid identifier and errors cleanly for an unknown one.
- A resource whose surface is disabled is absent from the listing.
- Each prompt expands with and without its optional argument, and the policy prompt's guidance orders read, validate and preview before any write.
- No resource returns a value that would be redacted from a tool result.

## As built

`resources.rs`: nine resources, three prompts, four handler methods, and one
leak the ticket's last criterion found.

### Two schemes, because there are two backends

`tailscale://` is this node as the local binary sees it; `tailnet://` is the
tailnet as the control plane describes it. Which one answers a URI is the thing
a caller most needs to know about it — `tailscale://status` fails when there is
no binary and `tailnet://devices` fails when there is no credential, and those
are different problems with different fixes (Q85).

The nine are the readings an agent would otherwise spend a tool call on:
`status`, `prefs`, `netcheck` and `lock` locally; `policy`, `devices`,
`device/{device_id}`, `dns` and `settings` on the tailnet. That is a superset of
the four the reference implementations offer.

A resource carries no tier. Every one is something a Read-tier tool could also
fetch, so it is offered whenever its surface is and never otherwise — and a
client asking for one whose surface is missing is told *which* surface, rather
than given an empty answer it would read as an empty tailnet.

### The template

`tailnet://device/{device_id}` is the one, listed through
`resources/templates/list` rather than `resources/list` so a client is not
offered a URI with a brace in it as though it were readable. `captures` takes
the identifier and refuses anything carrying a `/` or nothing at all; what it
yields then goes through the same `path_segment` every device tool uses, so a
resource cannot reach a path a tool could not.

### The policy is the one that is not JSON

Served `application/hujson`, as text, with `Accept: application/hujson` on the
wire — asserted, because a resource that quietly asked for JSON would answer
with the policy minus every comment, which is the part a person wrote.

### The leak the last criterion found

"No resource returns a value that would be redacted from a tool result" turned
out to be a claim the redactor could not keep: it knew `tskey-…` and
`Bearer …`, and `tailscale status --json` and `debug prefs` both print
`privkey:` and `nlpriv:` — the private halves of this node's key material,
which are more sensitive than an auth key. Both shapes are in `redact` now, and
the public halves (`nodekey:`, `tlpub:`, `discokey:`) are deliberately not: they
are identifiers a caller reads to tell one node from another.

Every resource body goes through the session's redactor on the way out, and the
test that checks it feeds a document with both shapes in it rather than hoping
the real ones never appear.

### Prompts

Three, each with exactly one optional argument, so each has something to expand
differently with and without. All three work under the read tier, which is why
none ends in a write: `review_policy_change` orders read, validate, preview,
and then names `tailnet_policy_set` only as the operator's call. A test walks
the tool table and fails if any prompt names a tool above the read tier, with
that one exception spelled out.

### No subscriptions

`enable_resources()` and `enable_prompts()`, but not
`enable_resources_subscribe()`. `spec.md` puts subscriptions out of scope, and
advertising a capability this server does not serve is a lie a client acts on.
