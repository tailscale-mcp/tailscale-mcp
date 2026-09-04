# 21 — Self-severing detection on the tailnet surface

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 17

Identify the local node's own device so that control-plane operations targeting it are recognised as self-severing: deletion, key expiry, de-authorisation and re-tagging. The identity comes from cached local status, refreshed on a sensible interval, and matching accepts either device identifier form.

When the local surface is disabled there is no local identity to match against; the behaviour in that case must be decided and documented, defaulting to treating the operation as ordinary rather than silently failing open on a claim it cannot check.

## Acceptance criteria
- An operation targeting the local node's device refuses without a confirmation and succeeds with one.
- The same operation against any other device is unaffected.
- With the local surface disabled, behaviour matches what the ticket documents and is covered by a test.
- The missing-confirmation refusal uses the distinct code agreed for it.

## As built

The machinery was already half there: `SelfIdentity` and `probe_identity` were
built in ticket 07 against this ticket, and `server.rs` has been reading status
at startup since. What was missing is the part that uses it, and the reason it
could not simply be `self_severing: true` on six rows.

### The row cannot know

`self_severing` means "true of every call this tool makes", and the registry
turns it into `requires_confirmation`. Six device tools are not that: deleting
somebody else's device is an ordinary destructive call, and deleting ours cuts
the cable the caller is talking over. Marking them `self_severing` would make
every device deletion in the tailnet ask for a confirmation, which the ticket
explicitly rules out — "the same operation against any other device is
unaffected".

So `ToolMeta` gained `severs_local_node`, which says the narrower thing and
implies no confirmation, and the six handlers ask `SelfIdentity::matches`
themselves. It is the arrangement Q70 already used for
`tailnet_device_authorize` and the passthrough: the row carries what is true of
the tool, the call decides what is true of this call (Q83).

The six are `_delete`, `_expire`, `_authorize`, `_tags_set`, `_ip_set` and
`_routes_set` — ticket 17's list, which is two longer than the four this
ticket names. Moving a device's address breaks every existing connection to it,
and disabling a route removes the way in for a caller reached over a subnet
this node advertises; both sever, and leaving them out because a summary listed
four would have been reading the list rather than the tailnet.

`_authorize` asks only when `authorized: false`. Authorising this node changes
nothing about a connection the caller is already using.

### The confirmation lives in the parameters

`resolve` strips a registry-added `confirm` before the handler runs, so a
handler that has to make the judgement would never see the answer. One
`SelfConfirmation` is flattened into the six parameter structs instead — one
field, one description, six uses — and the description says the thing that
matters, which is that it is needed only for this node.

A flag with nothing behind it would be a claim, so the registry refuses to
build a table where a `severs_local_node` tool's schema has no `confirm`
property, and a test asserts that refusal against a deliberately broken
declaration.

### No local surface, no identity, ordinary call

`SelfIdentity::default()` matches nothing, which is what a session with no
`tailscale` binary gets. The ticket asks for this to be decided and documented
rather than left to fall out; the alternative is refusing every device
operation on a suspicion the server cannot check, which would make the tailnet
surface unusable on its own. A test starts a session `without_cli` and asserts
the deletion goes through.

### What the tests pin

Four in `tests/tailnet_surface.rs`: this node refused and then confirmed; the
same call against `n2222222CNTRL` unaffected; this node recognised by node id,
by address and by MagicDNS name; and the no-local-surface case. The six
contract rows moved to `n2222222CNTRL` for the same reason — the harness's fake
`tailscale status` names `n1111111CNTRL` as this node, so a row using it would
have been testing the confirmation instead of the call.
