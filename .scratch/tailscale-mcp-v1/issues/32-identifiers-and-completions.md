# 32 — Device identifiers, and argument completions

Status: done
Milestone: 3 — Tailnet surface
Blocked by: —

Two changes, one release. The second is only worth having because of the first.

## A — the tailnet surface accepts the identifiers it claims to

**Done.** All 17 tools and the resource template resolve; the instructions and
every `device_id` description now describe what the server actually accepts.

Every session reads this, from `instructions.rs`:

> Identifiers: a device can be named by its node ID, one of its Tailscale IP
> addresses, or its MagicDNS name.

Measured against a real tailnet, that is true of the local surface and false of
the tailnet one:

| Form | `tailscale_ip` (local) | `tailnet_device_get` |
|---|---|---|
| MagicDNS name | accepted | `not_found` |
| hostname | accepted | refused |
| Tailscale IP | accepted | `not_found` |
| node id | — | accepted |
| numeric id | — | accepted |

The node id is the only form the tailnet tools take, and it is the one a model
is least likely to be holding. The per-parameter descriptions are already
honest — "The device's node id (`n1234567CNTRL`, the `nodeId` in a listing) or
its numeric `id`. Either works." — so it is the instructions that contradict
the schemas, in the same way Q139's prompts contradicted the tool listing.

So the seventeen tools taking `device_id`, and the `tailnet://device/{device_id}`
template, resolve a MagicDNS name, hostname or address against the tailnet's
device list before addressing the control plane. Anything already shaped like an
identifier — `^n[0-9a-zA-Z]+CNTRL$`, or all digits — is passed through
untouched, so every call that works today works identically.

Resolution refuses rather than guesses. A value matching more than one device
comes back with the candidates named; a value matching none says which fields
were searched. A failure of the lookup itself is reported as itself, never
flattened into "no such device". Success is silent and no result shape changes.
The five destructive tools resolve like the rest: ambiguity is already refused,
and `confirm` is where deliberateness lives.

## B — argument completions

**Done.** Three slots draw on real sources, the fourth deliberately offers
nothing, and the capability is declared so clients that support it will ask.

`completion/complete`, which this server currently answers without advertising,
because rmcp's default handler returns an empty success. Advertising follows
`enable_resources_subscribe`'s rule in reverse: serving what is not advertised
is the same lie as advertising what is not served.

The protocol completes two things — resource template variables and prompt
arguments — and this server has four such slots:

| Slot | Source | Values |
|---|---|---|
| `tailnet://device/{device_id}` | control plane | MagicDNS names |
| `diagnose_connectivity.peer` | local `tailscale status` | peers, not self, offline included |
| `audit_tailnet_access.subject` | users and policy tags | login names, tags |
| `review_policy_change.goal` | — | empty; prose cannot be enumerated |

`CompletionInfo` carries bare strings with no display label, so the value shown
is the value inserted. That is why B waits on A: without resolution the only
correct completion for a device is its node id, and a picker of `n8f3k2CNTRL`
is correct and useless.

Matching is case-insensitive substring across every field an entity is known
by, one canonical value per entity, ordered exact then prefix then alphabetical.
Empty input returns everything up to the hundred-value ceiling, with `total`
and `hasMore` reported rather than truncating in silence. A slot whose surface
is absent returns empty, as Q139 taught the prompts to.

Completions never fail: an unreachable backend returns no values and a debug
line, because an autocomplete has nowhere to put an error and the argument can
always be typed by hand. The spec asks servers to rate limit this method, so a
per-session token bucket does, returning empty when it trips.

Both halves read the device list through one ten-second cache — the first
mutable state the server holds — so a burst of keystrokes or of device-addressing
calls costs one lookup rather than one each.

## Reach, recorded rather than assumed

Among the five clients `tailscale-mcp setup` targets, `completion/complete` is
implemented by Claude Code for resource templates only, and by VS Code for both
kinds. Claude Desktop, Cursor and Zed send it for neither. So A improves every
session on every client; B's device slot serves two of five and its prompt slots
serve one. That was known before the work started.
