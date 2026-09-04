# 22 — Resources and prompts

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 18

The nine resources across the two schemes, including one template addressed by device identifier and the policy resource served with its document media type, and the three prompts. Resources are read-only and available whenever their surface is on; there are no subscriptions.

All three prompts must work under the read tier, since validation and preview do not mutate.

## Acceptance criteria
- Listing and reading each resource works through the in-process client; the template resolves for a valid identifier and errors cleanly for an unknown one.
- A resource whose surface is disabled is absent from the listing.
- Each prompt expands with and without its optional argument, and the policy prompt's guidance orders read, validate and preview before any write.
- No resource returns a value that would be redacted from a tool result.
