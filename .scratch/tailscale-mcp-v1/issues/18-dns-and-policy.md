# 18 — DNS and policy toolsets

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 16

The 11 DNS tools and the 4 policy tools. The policy read returns the document together with its version identifier, which is the single documented exception to forwarding the response verbatim, because the identifier is a header. Both document formats are supported, and the detail form returns the upstream warnings and errors.

A policy write must carry the version identifier or an explicit statement that it is writing over the default, and a version mismatch produces the conflict code with a hint to re-read. Validation accepts either a proposed document or a set of tests, and treats an empty success response as a pass. Preview takes a subject and a subject type.

## Acceptance criteria
- Reading returns identifier, format and document; writing without either guard is rejected before the request is sent.
- A mismatched identifier produces the conflict code.
- Validation of a set of tests against the current policy and of a hypothetical document are distinguished correctly.
- All DNS tools round-trip against the fake server, and the split configuration update and replace forms behave differently as the API defines them.
- The DNS and policy shapes deferred to this ticket in `schema_drift.rs`'s `DEFERRED` table are modelled, and their rows removed (Q64).

## As built

Fifteen tools in two modules — `tailnet_dns` (11) and `tailnet_policy` (4) —
the seven models the deferral table was holding for them, and the transport
support the policy file needed and nobody had built yet.

### DNS: the name says whether it replaces

Six of the eleven endpoints overwrite a whole list or a whole document and one
merges, and the API calls them all `set…`. Here they do not: `_replace`
overwrites, `_update` merges, `_set` carries a single value (Q72). An agent
calling `tailnet_dns_nameservers_set` with the one nameserver it wanted to add
would remove every other nameserver in the tailnet, silently, and the tool name
is the only place a caller sees the difference before making the call.
`split-dns` makes the case plainest: `PATCH` and `PUT` are the same resource
and differ only in this, so `tailnet_dns_split_update` and
`tailnet_dns_split_replace` differ only in this too.

MagicDNS follows the nameservers — removing the last one turns it off, turning
it on without one is refused — and neither rule is enforced here. The control
plane owns them and states them better than a guess would; both are in the tool
descriptions, so an agent that reads them will not have to learn them from a
failure.

The one thing checked before sending is a blank entry in a list, by
`common::each_present`, which names the parameter it was called for. `[]` is
documented and deliberate: it is how a list is emptied. `[""]` is a caller that
meant `[]`, and the control plane answers that with a 400 that names no entry.

Three list checks that had grown three spellings — `fields`, the posture
providers, and the policy formats — now go through one `common::one_of`, which
quotes the constant the drift test holds to the description.

`tailnet_dns_configuration_replace` takes the document as a `Value` rather than
as `DnsConfiguration`, because the models live in `tailscale-rest` and do not
derive `JsonSchema`. That is ADR-0004's shape anyway — a body that is
Tailscale's is accepted in Tailscale's shape — so the parameter documents the
field names and the tool checks only that a document arrived rather than a list
or a string.

### Policy: three things unlike the rest of the surface

**The document is not JSON.** A policy file is HuJSON, and the comments are the
part a person wrote. So it travels as text under `application/hujson` and comes
back as text, and `format: "json"` is how a caller asks for it parsed with the
comments gone. This needed two things that did not exist: `RequestBuilder::text`
for a body that is not JSON, and `fake::Response::text` so a fake could stand
in for the one endpoint whose document is not.

**The read carries a version.** `spec.md` names this as the one documented
exception to answering with the control plane's body and nothing else, and the
reason is mechanical: the version is an `ETag` header, and a header has nowhere
to go in a body. So the read answers `{etag, format, policy}`. The write
answers the same shape for the same reason — its body is HuJSON too, and the
new `etag` is what the next write quotes — which is a second exception, and is
recorded as one (Q75).

`details: true` asks for the control plane's own report instead. It is refused
alongside `format`, because the description says not to send an `Accept` with
it, and it answers `{etag, details}` rather than reusing `policy`: the report
is not the policy, and a `format` of `"details"` would be a value that `format`
does not accept back.

The write is not declared idempotent, unusually for a replace: the guard makes
the second call fail.

**The write is guarded.** A write with no `If-Match` is a valid request to the
control plane meaning "replace whatever is there", so the failure mode is a
success — which is why the guard is here and not there (Q73). `etag` or
`over_default: true`, exactly one: neither is refused naming both, and both
together is refused rather than ranked, because they say different things and
picking one would be this server deciding which of two contradictory
instructions a caller meant. A stale version comes back as `conflict` with a
hint naming `tailnet_policy_get`, from a new 412 arm in `From<ApiError>` —
general because the description gives 412 to no other endpoint.

The write is Destructive and does not require a confirmation. `spec.md`'s
confirmation list is closed and names four tailnet-scale operations; this is
not one of them, and the guard already means a caller either read the document
or said it was writing over an untouched default.

`tailnet_policy_validate` is one endpoint with two meanings, told apart by the
body's own JSON type: an array runs tests against the policy in force, an
object is a hypothetical document. Sending both would be one body that cannot
be both, so it is refused rather than resolved by a rule a caller cannot see. A
pass is an empty body, which is reported as `{"passed": true, "checked": …}`
for the same reason as Q67.

### The drift walk, widened again

Modelling the policy shapes found the walk reading one media type per body —
whichever the parsed map yielded first. `acl/validate` describes its real
request shape under `application/json` and a bare string under
`application/hujson`, and the string won: a five-property test case was
invisible while the test stayed green (Q71). Every media type is now walked,
named only where a body carries more than one, and a test asserts both halves
so a simplification back to `find_map` fails rather than under-reading again.

That widening turned up exactly one new object. With the seven models added
here the deferral table drops from thirty-four rows to twenty-eight, and the
walk now reaches 91 objects against 63 models.
