# 23 — Streamable HTTP transport

Status: done
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

## As built

`crates/tailscale-mcp/src/http.rs` holds the transport and, in front of it, the
checks. `Guard::admit` asks four questions in a fixed order — host, origin,
rate, token — so that a request from the wrong host is refused before its token
is looked at, and a page probing for a valid token learns nothing from the
timing. `GET /health` sits outside the middleware and answers anyone; a test
asserts the same request one path over is still refused, which is what says the
exemption belongs to the health endpoint and not to everyone.

The host allow-list is loopback, `localhost`, this node's addresses and its
MagicDNS name — short and fully qualified — read from status at startup, plus
whatever `--http-allow-host` adds. It is only ever added to: a list an operator
could narrow to nothing is a list they can lock themselves out with. Ports and
case are ignored when matching, and an IPv6 literal keeps its brackets.

Origins are refused unless `--http-allow-origin` names them, which is the
opposite of rmcp's own default and the reason the checks are in front rather
than left to it (Q90). rmcp is handed the same host list and the same body cap,
so the two cannot disagree.

The token is compared with the length folded into the accumulator rather than
checked first, so a wrong-length token takes the same path as a wrong one of the
right length. It arrives in `TAILSCALE_MCP_HTTP_TOKEN` and there is no flag for
it, because this repo has never had a way to put a secret on its own command
line (Q91). A non-loopback bind without one refuses to start unless
`--http-no-auth` says the address really should answer anyone.

The rate limit is a token bucket per address, 120 in 60 seconds, refilled by the
passage of time. An address whose bucket has fully refilled is forgotten, so a
server up for a year does not hold one entry per address that ever reached it.

`Caller` carries the address and, where status could name it, the node it
belongs to; it is resolved once in the middleware and put in the request's
extensions. It carries more than the transport needs today because the ticket
asks that identity-derived authorisation be addable "without changing the
transport", and a hook that carried only an address would have to change shape
the first time a rule wanted a name.

Sessions are on unless `--http-stateless` turns them off. `--http` alone binds
`127.0.0.1:8449` (Q86).

The body limit is rmcp's to enforce, not this module's:
`axum::extract::DefaultBodyLimit` sets an extension that axum's own extractors
consult, and the MCP path is a service that reads the body for itself, so a
layer here would have been a cap in name only. rmcp is given
`MAX_BODY_BYTES`, and a 5 MiB POST is answered 413.

`--http-stateless` is honest about its reach: it sets rmcp's
`legacy_session_mode`, which the SDK applies only below protocol version
2026-07-28. From that version the protocol has no sessions and the flag changes
nothing, which the flag's own help now says.

Nine tests in `crates/tailscale-mcp/tests/http_transport.rs` drive the assembled
router — one per acceptance criterion, plus the caller the transport is handed
and the stateless switch reaching it — and ten more in `http.rs` ask the guard
its questions directly.
