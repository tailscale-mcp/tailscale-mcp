# 17 — Devices and posture toolsets

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 16

The 15 device tools and the 5 posture integration tools. Device listing supports the API's field selection and server-side filters, plus the one client-side window agreed for it, since the endpoint offers no pagination. Device identifiers accept either form the API accepts.

Authorisation, tagging, renaming, key expiry, address assignment and deletion are classified per the inventory; deletion and de-authorisation are destructive.

## Acceptance criteria
- All 20 tools are registered with the classifications from the inventory.
- Listing honours field selection and filters, and the client-side window slices without changing the response shape.
- Both device identifier forms resolve.
- Posture attribute operations round-trip against the fake server.
- Parameters holding a documented string quote its known values, from the constant beside the model (carried over from ticket 16).
- The six transport tests in `tests/control_plane.rs` are re-expressed as tool calls and the file is deleted (Q59, Q63).
- The device and posture shapes deferred to this ticket in `schema_drift.rs`'s `DEFERRED` table are modelled, and their rows removed (Q64).

## As built

Twenty tools in two modules — `tailnet_devices` (15) and `tailnet_posture` (5)
— the first on the tailnet surface, plus the eleven route-shaped models they
send and receive, twenty contract rows, and the twelve tests in
`tests/tailnet_surface.rs`, five of them absorbed from the file this ticket
deletes.

### Naming

`tailnet_<resource>_<verb>`, verb last, from a closed list of nineteen declared
in `meta::TAILNET_VERBS` and enforced over the whole table (Q66). The list is
written whole rather than grown, because ninety-three tools arrive across five
tickets and a vocabulary that gains a word whenever a name does not fit is not
one. Verb last also sorts usefully: `tailnet_device_routes_get` and
`..._routes_set` are adjacent, where verb-first would separate them.

### Devices

Both identifier forms work, because both are what the API takes and neither is
converted: the node id and the numeric id go into the path as the caller wrote
them. What does not go into the path is anything that could change which
endpoint is called — `path_segment` refuses a segment carrying `/`, `?`, `#` or
a space rather than escaping it, since no legitimate identifier contains one
and an escaped one would only fail somewhere less legible.

The listing carries the API's field selection and its server-side filters, and
adds the one client-side window `spec.md` allows for it. The window is this
server's own: the request is unchanged, the whole listing is fetched, and the
slice happens here — which is what the endpoint's lack of pagination leaves
available. A windowed answer gains a `window` object saying `total`,
`returned`, `offset` and the `limit` asked for, because without it a truncated
list and a short tailnet look identical (Q69). An unwindowed one — no `limit`,
no `offset` — is the API's answer, byte for byte and nothing else, which is the
default and the common case.

A filter may be a value or a list of values, since the API ANDs a repeated
filter and a caller wanting both `tag:prod` and `tag:subnet` has no other way
to say so. A filter named `fields` is refused: it is a filter that would
silently change which fields came back.

The window refuses a body with no `devices` array rather than reporting an
empty tailnet, which is what slicing a missing list would otherwise produce.

`fields` is checked against `DEVICE_FIELDS` before the call rather than sent on
to be rejected, and the refusal quotes the values. That constant is the one the
drift test holds to the description, so a value this accepts is a value the
description knows — which is what ticket 16's carried-over criterion asked for.
The same for the posture providers.

Endpoints that answer with an empty body answer with a small report —
`{"done": "deleted", "device_id": "…"}` — rather than `null` (Q67). A caller
cannot tell `null` from a tool that lost its answer, and an agent that cannot
tell success from breakage retries a deletion. One report shape, built by
`common::Done`, is used at all nine sites across the two modules.

Setting one posture attribute is the one write here the description gives a
`200` body — the device's attributes as they now stand — and the control plane
also answers it with nothing at all. `common::answered_or` forwards the body
when there is one and falls back to the report when there is not, so neither
case answers `null`.

`tailnet_device_authorize` is registered at the Write tier with `varying: true`
and refuses `authorized: false` below the destructive tier (Q70). The danger is
in the argument, not the tool: authorising is how a tailnet with device
approval admits a machine, and de-authorising disconnects one. The passthrough
already carries a floor for the same reason.

The batch attribute update checks every key before sending. The call is
all-or-nothing, so one bad key among a hundred devices would fail the whole
batch after it had been sent; the refusal names the device and the key.

### Posture

The five integration tools, and one thing running through them: the client
secret goes to the control plane and never comes back, which the tool
descriptions say and a test asserts against the fake. An update sends only the
fields it was given, so omitting the secret keeps the one already configured;
an update given nothing at all is refused rather than sent, because a `PATCH`
with an empty body succeeds and changes nothing, which reads as success.

`provider` is not sent on an update. The control plane ignores it there, and
sending it would let a caller believe a provider had been changed when it had
not.

### The transport tests, absorbed

`tests/control_plane.rs` is gone and `tests/tailnet_surface.rs` replaces it, as
Q59 promised and Q63 re-aimed. Five of the six became ordinary tool calls
unchanged in substance. The sixth could not: `control_plane.rs` asserted that a
session without a credential answers with a sentence naming
`TAILSCALE_API_KEY`, and through a session that sentence is unreachable — a
surface with no credential is switched off at startup, so its tools are never
offered and no call arrives to be refused. What a caller can see is the absence
and the instructions, so that is what the replacement asserts. The sentence
still exists for the case that can reach it, a credential that stops working
mid-session, and `context.rs` gained the test that asserts it — the assertion
was relocated, not dropped.

The session result cap and the transport's own ceiling report the same code, so
the cap test asserts the message too: the transport's names the request and the
ceiling, where the result cap's states an exact size the transport never learns
because it stops reading.

### Secrets

`IntegrationBody.client_secret` is a `tailscale_rest::Secret`, whose `Debug`
redacts, and both parameter structs carrying one have a hand-written `Debug`
that prints `[redacted]` in its place — a derived one over a `String` is how a
credential reaches a log. "Nothing to change" is asked of the fields rather
than of the serialised body: serialising can fail, and a check that sends the
request when it cannot tell is a check that fails open.

### Path segments

`path_segment` also refuses a segment made only of dots. `.` is in the
allow-list because identifiers contain it, and `.` and `..` are made of nothing
else; a dot segment does not sit in the path but rewrites it when the URL is
parsed, so `tailnet_device_delete` with `device_id: ".."` would have sent
`DELETE /api/v2/`.

### Two older defects, fixed here (Q68)

`redact` scanned by byte index and sliced without checking for a character
boundary, so any message or hint carrying a character outside ASCII panicked —
and a panic in a handler takes the session down rather than failing the call.
It was unreachable until the refusals written here put em dashes in hints.

`instructions::render` decided a surface was present from the toolsets
selected, ignoring whether the backend was there. A session that selected
`tailnet-devices` with no credential hid every tailnet tool and then told the
model the tailnet surface was available — precisely what that module's own doc
comment says it exists to prevent. `Gate::offers` now asks both questions.

### Not done here

Six of these tools can cut this node off from the tailnet it is being reached
over: deleting it, expiring its key, de-authorising it, moving its address,
retagging it, or dropping its routes. None is marked self-severing, because
nothing yet recognises *this* node among the arguments — that is ticket 21, and
marking them first would put a claim in the annotations that the server cannot
keep.
