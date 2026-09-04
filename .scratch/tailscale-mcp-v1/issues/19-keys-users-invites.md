# 19 — Keys, users, invitations, contacts and settings toolsets

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 16

The 5 key tools, 7 user tools, 11 invitation tools across devices and users, 3 contact tools and 2 settings tools. Key creation takes the capabilities object in the API's own shape.

A newly minted secret is returned once, in full, with the tool description warning that it cannot be retrieved again, and is never logged. The endpoints that accept only a user-owned credential are registered as tools; under a tailnet-owned credential their failure carries a hint naming the requirement.

## Acceptance criteria
- A minted key's secret appears in the result and in no log line at any level.
- Creating a key with a capabilities object matching Tailscale's documented example succeeds unmodified.
- A user-owned-only endpoint called with a tailnet-owned credential produces the hint.
- Role changes, suspension, restoration and deletion are classified per the inventory.
- The key, user, contact and OAuth-app shapes deferred to this ticket in `schema_drift.rs`'s `DEFERRED` table are modelled, and their rows removed (Q64).

## As built

Twenty-eight tools in four modules — `tailnet_keys` (5), `tailnet_users` (7),
`tailnet_invites` (11) and `tailnet_settings` (5, being three contacts and two
settings) — and the seventeen models the deferral table was holding for them.

### Keys

One endpoint family over four kinds of credential, which is why `key_type`
governs so much of it. The description gives that field three different lists —
[`KEY_TYPES`] on the way out, `CREATE_KEY_TYPES` on a create,
`UPDATE_KEY_TYPES` on an update — because `api` is a real key type that cannot
be minted here and `auth` is a real key type that cannot be reconfigured. The
check takes the list that applies to the call it is for, so a refusal quotes
the vocabulary that would actually have worked.

`capabilities` goes through as the caller wrote it (ADR-0004), so Tailscale's
documented example is what reaches the wire. A unit test asserts exactly that,
and the request assertion in `minted_secrets.rs` asserts it again through a
whole session.

`all` on the listing is always sent, defaulting to true (Q74). The description
marks it required while its text calls it optional, and the two readings give
different listings; sending it always takes the ambiguity off the wire, and
true is the default because a listing that silently omits keys is the worse
failure.

### The minted secret, and the log line nobody had checked

The criterion — "appears in the result and in no log line at any level" — is
asserted in `tests/minted_secrets.rs` against a real subscriber, because
reading the code and concluding it is fine is an argument rather than a test.

Writing it found a real gap. Nothing in this workspace logs a response body,
but `rmcp` traces whole JSON-RPC messages at `TRACE` and `DEBUG`, results
included. The default filter never reached that — but an operator setting
`--log=debug` to follow this server's own work would have written every minted
auth key, OAuth client secret and invite URL to standard error without asking
for any of it. The resolved filter now carries `rmcp=info` unless the operator
names `rmcp` themselves (Q79), and the test installs the filter an operator
actually gets rather than a bare `TRACE` nobody runs.

The same test covers the invite URL, which is the other credential this surface
hands out: anyone holding one can accept, which is why an invite created
without an email exists at all.

### Users

Five of the seven change somebody's standing, and four of those are the same
bodyless `POST` under a different last segment — written once rather than four
times. All five refuse to act on the credential's own user, which is the
control plane's rule and is stated in the descriptions rather than enforced
here.

The two listing filters accept everything the field accepts plus `all`, so they
are separate constants: `role: "all"` is a filter and not a role anybody can
hold, and mixing the two lists would either reject a legitimate filter or
accept a role that cannot be set.

`suspend` and `restore` were not in Q66's vocabulary, and the test enforcing it
failed. They were added rather than renamed (Q77): they are the API's own
words, and `disable`/`enable` would have been this server renaming something
Tailscale had already named. The check did its job — it made this an explicit
decision instead of a name that quietly did not fit.

### Invitations

Six of the eleven accept only a credential owned by a person, and this server
cannot tell what kind it holds — a bearer token does not say what minted it.
So the requirement is added to the refusal rather than checked before the call
(Q76), and only to a refusal the control plane made about permission: a 404 is
a missing invitation and a 429 is the documented rate limit, and hanging an
explanation about credentials off either would send a caller the wrong way.

Four of them answer with a bare JSON array, where every other listing on this
surface arrives wrapped. Structured content is an object, so there is nothing
to forward verbatim and the choice is which envelope: `{"invites": […]}`, the
API's own convention for every listing it does wrap (Q78). The wrap is
conditional on the body really being an array, so a control plane that starts
wrapping them itself is followed rather than double-wrapped.

### Contacts and settings

Together in one toolset because both are the tailnet talking about itself. A
contact change is not immediate — the new address is mailed a verification link
and sits in `fallbackEmail` until it is followed — which the descriptions say,
because a caller who reads the answer back and finds the old address would
otherwise think the call failed.

`tailnet_settings_update` refuses an empty document: a `PATCH` with nothing in
it succeeds and changes nothing, which reads as the change having been made.
