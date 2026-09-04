# 16 — Typed models and the schema drift test

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 15

Hand-written models for the API's schemas, each retaining unknown fields, per ADR-0003. Every client method returns the parsed model together with the raw body and the headers that matter, so the server can forward raw output while tests assert on typed fields.

Enums are strict only for genuinely closed sets; the sets known to drift are documented strings. The drift test parses the vendored API description and fails when a schema property has no corresponding field.

## Acceptance criteria
- A response containing an unknown field parses, and the field is retrievable.
- The drift test passes against the vendored description and fails when a property is removed from a model.
- The known divergences between the description and the live API are recorded where the test can explain them rather than silently pass.
- Documented strings carry their known values in the parameter description.

## As built

Forty-five model shapes in `tailscale-rest::models`, `Answer<T>` on the client,
and `tests/schema_drift.rs`. Still no tool: ticket 17 is the first caller.

### The models

Nine modules — `device` (12 shapes), `dns` (5), `key` (4), `logging` (8),
`service` (4), `tailnet` (8), `user` (3), `webhook` (1), and `policy`, which
holds a constant and the models ticket 18 will add — declared through one
`model!` macro. Every field is an `Option` of the type inside it, every name is
Tailscale's spelled once, and every struct ends in a `#[serde(flatten)]` map of
whatever the description did not mention. A body from a control plane ahead of
this build parses, the new field is readable, and it comes back out as it went
in; `skip_serializing_if` keeps an unset field out of a `PATCH` rather than
spelling it `null`, which would ask the control plane to clear it.

The macro exists because of the drift test rather than for brevity (Q61). It
writes the struct and the `ModelShape` row from the same source text, so a field
cannot be deleted from a model and left behind in the table the test reads —
which would be a green test over a model that had stopped being true. It also
grew an alias form, `VipServiceInfoPut as "VIPServiceInfoPut" is VipServiceInfo;`,
for the one schema the description declares twice to re-describe a field's
meaning on the way in. The alternative was six duplicated fields and a
conversion between them that says nothing.

Nine properties are secrets — a minted auth key, an OAuth client secret, a
webhook signing secret, a log stream's bearer token and cloud credentials — and
hold `Secret` rather than `String` (Q62), because `missing_debug_implementations`
means every model derives `Debug` and a derived `Debug` on a `tracing` field is
how a just-minted key reaches a log file. `Secret` gained `Serialize` and
`Deserialize` to fit, so serialising a model does write the value in the clear;
that is the tool result the caller asked for and the one place the value is meant
to travel, while printing — the accident — still redacts.

None of the description's thirty-three enumerations turned out to be a closed
set, so each is a documented string with a `&[&str]` beside it (Q60, counted
correctly in Q65). Making them Rust enums would either reject a value the
control plane invented next month or, with a catch-all, discard the string it
sent, which ADR-0004 forbids.

### `Answer<T>`

ADR-0003 asks for "the parsed model together with the raw body and the headers
that matter". `send_answer::<T>()` returns all three, and deserialises `T` from
the `Value` it already parsed rather than from the bytes a second time, so the
raw half and the typed half cannot disagree about the same response. A tool
forwards `raw`; the server reads `value` when it has to act on the answer. The
three `send` methods now share one body parser, so they cannot come to disagree
about what an empty body is — which also gave `send_as` the empty-body handling
it never had.

An empty body reads as `Value::Null`, and `T` has to be a type that can read
null. A model cannot: the flattened map of unknown fields makes every one of
them a map to serde, and serde will not read a map from null. That is the right
way round — the endpoints that answer with nothing are the deletions, and a
deletion has no model to answer with — but it is a real constraint on callers
and is documented on the method rather than left to be discovered.

### The drift test

`tests/schema_drift.rs` parses the vendored description with `serde_norway` — a
dev-dependency of this crate alone — and walks it whole: `components/schemas`,
`components/parameters`, and every request body, response and parameter under
`paths`. Each object is recorded by the path it is reached by, because eleven of
the description's object types are inline inside schemas and forty-six more are
inline under `paths`; a walk that skipped them would be silent about exactly the
places a refresh adds a field. It finds 90 objects and 33 enumerations, and
holds the models to both in both directions.

Forty-five of the ninety are modelled here. The other forty-five are request
bodies a tool builds from its parameters and envelopes a listing arrives in, and
belong to tickets 17 through 20; each sits in a `DEFERRED` table naming the
ticket it is waiting for (Q64). The table is checked too — a row must name a
path the description still has and must not name one that is modelled — so it
can only shrink, and an object the description grows that is in neither place
fails the walk.

A drift test that only ever passes proves nothing, so the comparison is a pure
function over two maps and two tests hand it doctored ones: a field taken off
`Device`, a whole model removed, a field the description never had, a value
dropped from `Key.keyType`. Each is asserted to produce the sentence naming it.

Seven known divergences are asserted rather than written down. Three are the
document against itself or against this crate: the OAuth token endpoint
`token.rs` hard-codes is absent; the HTTPS setting is `httpsEnabled` in the
schema and `httpsCertificates` in the prose beside it; and the `all` parameter
on a key listing is marked required and then described as one that may be unset.
Four are the document against Tailscale's own Go client, which is the closest
thing to a view of the live API: the posture providers `fleet` and `huntress`;
the `/services` and `/vip-services` spellings, and the `annotations` field the
Go client's service carries; split DNS in its two shapes; and the sharpest one,
that `LogstreamEndpointConfiguration` has four `gcs*` fields while its
`destinationType` does not offer `gcs`, so nothing can select the destination
they configure. A refresh that settles any of them fails the test excusing it.

### Widened after review

The walk originally started at `components/schemas` and stopped there, which
left forty-six objects and ten enumerations outside it — including the three
different `keyType` lists, where a create tool quoting the response list would
have offered `api`, a value the control plane rejects on the way in. Both review
axes found it independently. Widening the walk then immediately failed on an
`anyOf` branch carrying properties in the bulk device-attributes body: a union
the old walk would have read as nothing at all. Q64 records the widening and the
deferral table; Q65 corrects Q60's count of the enumerations from twenty-two.

### Not done here

"Documented strings carry their known values in the parameter description" is
half done, and the missing half is a tool rather than a constant. All thirty-
three enumerations now have a constant and every field's doc comment points at
the one that applies — including `?fields`, which ticket 17 needs, and the
create and update `keyType` lists, which are narrower than the response's — but
no tailnet tool exists to have a parameter description yet. The criterion moves
to ticket 17 rather than being called satisfied by a constant nobody reads.

`tests/control_plane.rs` was to be absorbed here, per Q59. It cannot be: this
ticket landed no tool either, so the six tests are still the only thing holding
the transport to its contract. Re-aimed at ticket 17 and recorded as Q63, which
is the last ticket it can slip to.

Research ambiguity §8 #4 stands as written: the description has
`/device/{deviceId}/attributes/{attributeKey}` but only with `POST` and
`DELETE`, so the `GET` the knowledge base lists is still absent. An earlier
draft of this section claimed the ambiguity was stale, on the strength of the
path existing; the methods are what it was about.
