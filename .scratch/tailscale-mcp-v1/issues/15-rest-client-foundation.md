# 15 — Control-plane client foundation

Status: done
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

## As built

Three new modules in `tailscale-rest` — `error.rs`, `token.rs`, `client.rs` —
and the wiring that hands the result to every session. No tool uses it yet;
ticket 16 onwards is what it is for.

### The failure type

`ApiError` distinguishes the eight ways a call can end badly. The five that
describe a call — `Status`, `Transport`, `Timeout`, `TooLarge`, `Malformed` —
each carry the request they belong to, because "404" on its own tells a caller
nothing. The other three do not, and cannot: `Token`, `JwtFile` and `Config` are
failures of the credential or of the client itself, which happen before any
request exists or without one at all, and a request named in them would be a
guess. Two questions are asked of it often enough to be methods rather than
matches: `status()`, and `is_transient()` — whether asking again could
plausibly work, which is true for 429 and the five hundreds, and for
`Transport`, since a request that never reached a response was not acted on.

`describe()` is where a failure becomes a sentence: the API's own
`{"message": …}` field when there is one, else the body, else the status's
canonical reason. Reading the body first matters because the control plane says
useful things there, and a caller shown "400 Bad Request" has to guess at what
this one meant.

`tailscale_rest` names its variants for what happened rather than for what a
caller should be told, so that the crate can be used without this server's error
model. `impl From<ApiError> for ToolError` in `tailscale-mcp` is the other half
of that arrangement, and the one place the translation happens: every tailnet
tool will reach the control plane through `?`, so nothing has to remember to
call it. 404 and 409 become the codes named for them, 429 carries the wait, 401
and 403 become `api_error` with a hint about the credential — deliberately not
`not_permitted`, whose hint names a server flag that has nothing to do with a
refusal from the control plane — and `TooLarge` becomes `result_too_large` with
the hint that names the narrowing available, which is the clause the ticket asks
for and which nothing else fulfilled.

That last one keeps `ApiError`'s own message rather than building one: the
transport refuses before the whole body has arrived, so it knows the cap and not
the size, and `ToolError::result_too_large` states an exact number. Both spell
the hint from one `TOO_LARGE_HINT`, so a single cap cannot grow two answers.

### Authentication

`Tokens` holds the credential and whatever was last minted. An API key is the
bearer token itself and is returned verbatim; the other two modes exchange for
one at `/api/v2/oauth/token`, differing only in what they send — client id and
secret, or a JWT assertion read fresh from disk on every exchange, because the
file is refreshed by something else and a cached copy is a token that stops
working at a time nobody chose.

A minted token is reused until 60s before its stated expiry, and a token that
arrives without one is assumed to last five minutes. `minted_bearer` holds one
mutex across the whole exchange rather than around each half, so a cold cache
under eight concurrent calls produces one exchange and not eight.

Rejection is where the care is. Every in-flight request carrying a stale token
gets its own 401, and the naive eviction re-mints once per 401. Each bearer
therefore carries the generation it was minted at, and `evict(generation)`
clears the cache only if that generation is still the current one — so of the
eight, the first eviction wins and the rest find a cache that has already moved
on. The retry that follows is capped at one per call (Q56).

### Request policy

`Idempotence` is read off the HTTP method rather than from a table of endpoints
(Q56), so the ninety-three tools that do not exist yet get the safe default:
`GET`/`HEAD`/`PUT`/`DELETE` may be repeated, `POST`/`PATCH` may not, and a 429
may be repeated whatever the method, because the status says the server declined
to act.

Backoff is 250ms doubling to a 20s ceiling, four attempts, and the server's own
`Retry-After` believed over the schedule when it sends one. The whole call is
bounded by a budget taken from the tool timeout, enforced two ways: the retry
loop stops short of a sleep that would run past the deadline, which is the tidy
exit and leaves the caller holding the failure that caused the wait, and a
`tokio::time::timeout` around the loop is the untidy one, for an attempt still
going when the budget is already spent. Only the second makes the budget a real
bound — without it a slow final attempt runs to the per-attempt timeout, which
is the same number and so hides the difference until someone raises one of
them. `DEFAULT_CONCURRENCY` is 8, held by a semaphore in the shared `Inner`,
so the limit is per client and not per call site.

### The size cap

Refused rather than truncated (Q57), and checked twice: `Content-Length` before
anything is transferred, and the running total as chunks arrive, for a response
that states no length. `TooLarge` carries the cap so the caller is told the
ceiling rather than left to infer it. The cap is `max_result_bytes` — the same
ceiling a tool result is held to, moving with the same switch.

### Reaching a fake

`ClientConfig::base_url` defaults to `https://api.tailscale.com` and is
overridable only through `TAILSCALE_MCP_API_BASE_URL`, with no CLI flag (Q55).
`checked_base_url` asks three things of an address, and all three are about how a
credential travels rather than about where it lands: the transport is `https` or
the host is this machine; there is no path, because a base URL is a host and
nothing more; and there is no userinfo, because a credential in a URL is a
credential that gets printed (Q58). What it deliberately does not do is pin the
host, so `a_base_url_is_an_encrypted_host_and_nothing_more` is named for what it
checks.

An address that fails fails the startup rather than producing a note, since the
only way to reach it is to have already redirected every credential the server
holds. The check runs whenever the tailnet surface is enabled, credential or
not, so an operator hears about a redirected control plane now rather than on
the day they add a credential — the worst day for it, because the surface will
have been quietly absent until then.

`fake.rs` grew three things, each demanded by a criterion that could not
otherwise be observed: `Response::slow()` and `peak_concurrency()`, because an
instant server has no concurrency to peak at, and `Response::chunked()`, because
an always-length-framed server never exercises the second half of the size
check.

`Setup::api_answers` had been starting a `FakeControlPlane` since ticket 07
without its address ever reaching the configuration. The harness now feeds
`base_url()` through the environment it answers, behind whatever the test set
itself, so a test that sets the base URL is not overruled by a fake it also
arranged. The suite still answers nothing it was not given, which is what keeps
`the_suite_answers_from_its_fakes_and_not_from_this_machine` true.

### Acceptance criteria

The first five live in `client.rs`, against the fake:

- *Each mode, and precedence* — `an_api_key_is_the_bearer_token_itself`,
  `an_oauth_client_is_exchanged_for_a_token`,
  `a_federated_identity_signs_with_the_jwt_on_disk`,
  `a_missing_jwt_file_says_which_file_it_was`, and
  `the_credential_with_precedence_is_the_one_that_is_used`.
- *Token life* — minted once and reused, re-minted near expiry, replaced exactly
  once on a rejection, not replaced twice, and not replaced at all for an API
  key, where there is nothing to mint.
- *Retry* — a transient failure retried on a repeatable method and not on an
  unsafe one, a rate limit retried on both, a permanent failure never, the
  attempt count bounded, the budget respected, and the server believed about
  when to come back. `only_the_methods_http_calls_idempotent_may_be_repeated`
  pins the rule itself rather than one instance of it.
- *Concurrency* — `no_more_calls_are_in_flight_than_the_limit_allows`: eight 80ms
  calls through a client limited to two, on a multi-thread runtime, asserting
  the fake's observed peak.
- *Size* — refused over a stated length, refused while being read when there is
  none, and delivered whole under the cap however it is framed.

`control_plane.rs` is the sixth thing, which the criteria do not ask for: six
tests that the client a *session* gets is the one described above. A call made
through `ToolContext::tailnet()` reaches the fake carrying the session's
credential; the tailnet in a path is the one the environment named; the size cap
is the session's; an unreachable control plane is the surface being unavailable;
no credential means no client and a refusal naming what to set; and the surface
switched off leaves no client even where a credential exists.

It is a third test seam where `spec.md` names two, so Q59 records it as
provisional: behaviour here is invisible from above only because there is
nothing above it yet, and each of the six becomes an ordinary tool call once
ticket 16 lands the first tailnet tools. The base-URL pair started here and has
already moved to `server.rs`'s own unit tests, next to the other reasons a
server refuses to start — neither of them makes a request, so neither was ever
about the transport.

### What `/code-review` found

Both halves converged on the same two docs, and in each case one of the two
sides was the one to change:

- **The budget was not a whole-call bound.** The doc, the ticket and
  `ApiError::Timeout`'s own text all promised one; the loop only declined to
  sleep past it. The code was wrong, not the promise, so the timeout above went
  in.
- **`check_base_url`'s doc claimed the host was pinned.** The ticket asks for
  "only over a secure or loopback address", which is what was agreed and what
  the code does. Here the doc was the wrong half; it now says what is enforced
  and says out loud what is not.

The spec half's sharpest finding was the missing `ApiError` → `ToolError`
mapping: without it "fails with its own code **and a hint naming the narrowing
available**" was unfulfilled, because `TooLarge` never became
`result_too_large`. It also found `a_token_near_its_expiry_is_minted_again`
passing an already-expired token, so `REFRESH_SKEW` was never exercised — it now
mints one with half the skew left, which the clock still calls valid — and
`Retry-After` proved only through a hand-built `ApiError`, so
`the_wait_a_server_asks_for_is_read_off_the_wire` sends the same 503 twice
against the same budget, differing in nothing but the header's value.

The standards half found `fixtures_are_redacted.rs` rooted at
`crates/tailscale-mcp/tests`, so roughly a thousand new lines of tailnet-shaped
test data in `tailscale-rest` escaped it. It now walks every crate in the
workspace: a unit test carries the same risk as an integration one, and there is
no reason to hold them to different rules. The cost was marking about twenty
pre-existing key literals fake by the same mechanical rule the check already
applies to fixtures, and one new rule — a bare `tskey-auth` names a *kind* of
key and grants access to nothing, the same reasoning already applied to a bare
`tskey-`, with counter-examples in both directions.

The rest were accumulation, all fixed: `ToolError::conflict` not carrying its
409 where its siblings carry 404 and 429; `Idempotence` public with no consumer;
`ToolContext::tailnet` a public field colliding with the accessor that exists so
that ninety-three tools do not each find their own words for a missing
credential; `--max-result-bytes 0` reaching `StartupError::ControlPlane` by way
of the client, reporting a typo on the command line as a fault in the
control-plane address; `LONG_ABOUT` describing the base-URL override in words
the code did not enforce; and `token.rs` and `credentials.rs` saying "API key"
throughout, which `CONTEXT.md` lists under _Avoid_. The prose is "API access
token" now; `ApiKey` and `API_KEY_ENV` keep the ecosystem's spelling, because
the variable they read is `TAILSCALE_API_KEY` and an operator setting it should
be able to find it.
