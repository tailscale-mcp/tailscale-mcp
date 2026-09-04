# 20 — Webhooks, services, OAuth clients, logging and organisation toolsets

Status: ready-for-agent
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
