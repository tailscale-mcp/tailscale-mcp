# 20 — Webhooks, services, OAuth clients, logging and organisation toolsets

Status: done
Milestone: 3 — Tailnet surface
Blocked by: 16

The remaining 30 tailnet tools: 7 webhook, 7 service, 5 OAuth client, 8 logging and 3 organisation tools. Webhook secret rotation returns its secret under the same rule as key creation. Logging covers audit and network log reading over a time window, log stream configuration and status, and the external identity operations.

The organisation tools are alpha upstream; their descriptions say so. Tailnet deletion is irreversible for everyone and requires a confirmation in addition to the destructive tier. The one endpoint that paginates is the only one whose pagination is exposed.

## Acceptance criteria
- All 30 tools are registered with the classifications from the inventory.
- Tailnet deletion refuses without a confirmation.
- The paginated listing follows its cursor and respects the API's maximum page size.
- The rotated webhook secret is returned once and never logged.
- Service naming follows the path the live API serves, not only the published description, with the divergence noted.
- The webhook, service and logging shapes deferred to this ticket in `schema_drift.rs`'s `DEFERRED` table are modelled, and their rows removed (Q64).

## As built

Thirty tools in five modules — `tailnet_webhooks` (7), `tailnet_services` (7),
`tailnet_oauth` (5), `tailnet_logging` (8) and `tailnet_org` (3) — the eleven
models the deferral table was holding for them, thirty contract rows, and the
tests each criterion asked for. This finishes the tailnet surface at 93 tools
and empties `DEFERRED`.

### Services: the path is asked for, not assumed

The vendored description documents `/tailnet/{tailnet}/services`; Tailscale's
own Go client calls `/tailnet/{tailnet}/vip-services`. The drift test has
recorded that disagreement since ticket 16 and neither source settles it. The
criterion says naming should follow the path the live API serves rather than
only the published description, and from here the only way to find that out is
to ask: these tools send the documented spelling and, on a 404, send the same
call again at the other one (Q81).

The retry is safe for these seven specifically. None acts before answering a
404, so a request that reached a base path the control plane does not serve did
nothing, and a service that is genuinely missing answers 404 at both spellings
— which is then what the caller gets. The service name is checked once before
either call rather than inside the retry, so a name that is not a path segment
is one refusal instead of two round trips.

`tailnet_service_approval_set` carries `varying: true` for the same reason
`tailnet_device_authorize` does (Q70): approving adds a host, withdrawing takes
a working one out of the service and stops traffic reaching it. The row is the
floor and the argument decides.

### The one paginated endpoint

`tailnet_organization_tailnet_list` is the only tool on either surface with
real pagination behind it. It follows the cursor by default and answers with
every tailnet, and takes a single page when the caller passes a `cursor` —
both halves of what the ticket asks for, and not in conflict (Q82). The walk is
bounded at ten pages, and an answer that stopped early carries the cursor and
says so, because an answer that quietly ended would be read as the whole
organisation. `limit` above 100 is refused rather than clamped, for the same
reason: a short page nobody was told about reads as a complete one.

This is the converse of Q69's client-side window, not a contradiction of it.
`tailnet_device_list` has no pagination to follow, so slicing here is all that
is available; where an endpoint really paginates, its own mechanism is used and
nothing is sliced.

### Deleting a tailnet

One of the four tailnet-scale operations `spec.md`'s closed list puts behind an
explicit confirmation, and the only one of the four outside tailnet lock. The
tailnet is named in the call rather than taken from the session's default,
because deleting whatever `TAILSCALE_TAILNET` happens to say is exactly the
accident the confirmation exists to prevent. All three organisation tools are
Alpha upstream and every description says so.

### Logging

The two readings take a window the API requires both ends of, and neither
paginates: the window is the only bound, which is why `start` and `end` are
required here rather than defaulted to something this server chose. The audit
filters are repeated query parameters rather than a joined list, because the
API reads one parameter per value and a comma-joined list would be one actor
whose name has commas in it.

The stream configuration is passed through as the caller wrote it (ADR-0004).
Nineteen fields, most conditional on `destinationType`, and a struct here would
have to encode which are required for S3 with `rolearn` versus S3 with
`accesskey` versus Splunk — and would be wrong the day a destination is added.
What *is* checked is the three closed lists, and only when present. `logType`
is dropped from the body rather than checked against the path: it is read-only
upstream and in the path already, and a body that says it too can say it
differently.

The tool descriptions state that `token`, `s3SecretAccessKey` and
`gcsCredentials` are write-only and never come back, because the obvious use —
read the configuration, change one field, write it back — silently erases the
credential that authenticates the stream.

### Webhooks and OAuth apps

The rotated secret is forwarded whole and `tests/minted_secrets.rs` holds it to
the same rule as a minted key: it reaches the caller, and it reaches no log
line, asserted against what a `trace` session actually collected rather than
against a reading of the code. Rotation sits at the destructive tier because
the old secret stops verifying the moment the new one exists, so every receiver
checking signatures rejects every delivery until it is handed the new one.

OAuth *apps* are not the OAuth *clients* in `tailnet_keys`, and both sets of
descriptions say which is which: a client is a credential this tailnet mints
for its own automation, an app is a registration for someone else's software to
run the authorization-code flow. An update is a `PUT` with the same three
required fields as the create, so all three are required here too — Q80 is
about fields the description marks optional, and these it marks required.

### The contract table grew an arm

`api_contract!` gained a second form taking `also "<other path>"`, for a tool
that retries at a second path. Arranging only one would have tested the fake's
own fallback rather than the tool: the services failure case needs both
spellings refused, so that what the caller sees is the tool's second 404 and
not the fake's 501.

### From the review

**A gate that refused work the API would have done.** `checked_configuration`
held `destinationType` to `DESTINATION_TYPES`, which lists eight systems and no
`gcs` — while `gcsBucket` in the same document says it is "Required if the
destinationType is `gcs`". The tool's own parameter documentation told callers
to send `gcsBucket`, and the tool then refused the destination it needs. That
turned out to be one instance of a pattern: tickets 17 to 20 had been using
Q60's known-value constants as request gates, which quietly reversed Q60's
finding that none of the twenty-two is closed. Eight gates are gone, five kept
for stated reasons, and `log_type` now names the two on the way back rather
than refusing on the way out (Q84).

**Two names against Q72.** `tailnet_log_stream_set` sends the whole
nineteen-field endpoint document — its own description says "the whole
endpoint, not a merge" — which is `_replace`. `tailnet_webhook_update` replaces
the entire subscription list, which is the shape `tailnet_dns_nameservers_replace`
is named for; it is `tailnet_webhook_subscriptions_replace` now, naming the
thing that is actually replaced. Both were worth fixing before release rather
than after: ADR-0005 makes a rename a major version.

**One name against `CONTEXT.md`.** The glossary says to avoid "host"; a node
seen through the REST API is a Device. The path is `/devices`, the parameter is
`device_id`, and only the tool name said hosts — `tailnet_service_devices_list`
now. The response envelope stays `hosts`, because that is what the control
plane sends and ADR-0004 forwards it verbatim.

**Bodies that bypassed their own models.** `CreateWebhook` and `UpdateWebhook`
duplicated `CreateWebhookRequest` and `UpdateWebhookRequest`, added in the same
commit and referenced nowhere, so the drift tripwire guarded a shape nothing
sent. The same for the two AWS requests. All four now send the model. The
siblings that do hand-roll a body (`KeyBody`, `IntegrationBody`) each say why
in a doc comment; these had no reason to give.

**Smaller things.** `service_approval_set` and `device_authorize` had the same
tier guard written twice — one `common::require_destructive` now.
`MAX_PAGE` (a size) and `MAX_PAGES` (a count) were one letter apart and are
`LARGEST_PAGE` and `PAGES_FOLLOWED`. `checked_name` was pure delegation to
`path_segment`, and the reasoning it carried belongs on `either_spelling`,
which is what constrains it. `organization` documents `-`. The service listing's
description now notes the VIP-services divergence, which had been recorded only
in a module comment and a decision.
