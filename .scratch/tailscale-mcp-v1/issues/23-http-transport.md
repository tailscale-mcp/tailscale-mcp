# 23 — Streamable HTTP transport

Status: ready-for-agent
Milestone: 4 — HTTP mode and subcommands
Blocked by: 22

Serve the same handler over Streamable HTTP on the agreed default loopback address. Bearer token compared in constant time, optional on loopback and required elsewhere unless the explicit no-authentication flag is passed. Host allow-list of loopback plus the local node's own tailnet names read from status at startup, extendable by flag. Browser origins rejected unless listed. Open health endpoint, a body limit large enough for a policy file, a per-address rate limit, stateful sessions by default with a stateless mode available.

Each request logs the caller resolved from the tailnet where the peer address allows it. The per-request hook is shaped so that identity-derived authorisation can be added later without changing the transport.

## Acceptance criteria
- A non-loopback bind without a token refuses to start unless the explicit flag is given.
- A request with a wrong or missing token is rejected; a correct one succeeds.
- A request whose host header is not allowed is rejected; the node's own tailnet name is allowed without configuration.
- A browser origin is rejected by default and accepted when listed.
- The health endpoint answers without a token; the rate limit triggers and recovers.
