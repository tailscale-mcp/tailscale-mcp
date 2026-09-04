# 15 — Control-plane client foundation

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 07

The REST crate's transport: authentication in three modes with the agreed precedence, token minting and caching with refresh before expiry and a single eviction on rejection, and the environment variable names the existing ecosystem already uses.

Request policy: retries only where retrying is safe, honouring the server's backoff, with a bounded attempt count; no retry on the unsafe methods; a global concurrency limit; the call budget taken from the tool timeout. The base URL is pinned, with an override accepted only for tests and only over a secure or loopback address. A result above the size cap fails with its own code and a hint naming the narrowing available.

## Acceptance criteria
- Each authentication mode works against the fake server, and precedence is asserted when two are configured.
- A minted token is reused until near expiry, refreshed once, and re-minted exactly once after a rejection.
- A retryable status is retried and a non-retryable method is not, verified by request count.
- The concurrency limit is observed under parallel calls.
- An oversized response produces the size code, not a truncated body.
