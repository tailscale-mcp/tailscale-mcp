<!-- Research note. Study of the three reference implementations (rtailscale, HexSleeves/tailscale-mcp, YawLabs/tailscale-mcp).
     Produced by a research sub-agent on 2026-09-03 during the design interview; facts were verified against the sources named inside. Not a spec. -->

Research is complete. Findings follow, organized per repo, then the rtailscale dependency verdict, the comparison table, and lessons.

# Reference implementation study: three Tailscale MCP servers

Paths are relative to each repo unless absolute. Nothing was written or modified.

---

## 1. `~/github.com/dinglebear-ai/rtailscale`

### (1) Basics
- **Language**: Rust, edition 2024, `rust-version = "1.97.1"` (`Cargo.toml`, `rust-toolchain.toml`); README says MSRV 1.86 (stale).
- **Crate**: package `tailscale-rmcp` v0.1.4, binary named `rtailscale`, `[lib]` + `[[bin]]`, `publish = false`. **Not on crates.io.**
- **License**: AGPL-3.0-only (commit `2026-08-05 chore: adopt AGPL commercial dual licensing (#25)`).
- **Last commit**: `2026-08-11 09:34:21 -0400 fix(plugin): use published rtailscale npm package (#27)`.
- **Stars**: no badge in README.
- **Distribution**: npm launcher `@dinglebear/rtailscale` (`packages/tailscale-rmcp/scripts/install.js` downloads a GitHub Release binary and verifies sha256), Docker `ghcr.io/dinglebear-ai/rtailscale`, `server.json` for the MCP registry, CI via `.github/workflows/{ci,release,release-please,mcp-registry,repository-contract}.yml`.

### (2) Backends
- **REST only.** `src/tailscale.rs` is a `reqwest` client against a hard-coded `https://api.tailscale.com/api/v2`. It never shells out to `tailscale` for tool work; the only `Command::new` calls are in `src/cli.rs` (`which tailscale`, `du`) for the `doctor` command, which expects *its own* binary to be installed as `~/.local/bin/tailscale` and warns about "Binary name conflict" if the real Tailscale CLI is found there. No LocalAPI, no socket, no app-bundle probing.

### (3) Tools / resources / prompts
One tool, `tailscale`, dispatched on an `action` enum (`src/mcp/tools.rs`, `src/mcp/schemas.rs`):

| action | purpose | params | backend | kind |
|---|---|---|---|---|
| `devices` | list devices | – | GET `tailnet/{t}/devices` | read |
| `device` | get one device | `id` | GET `device/{id}` | read |
| `device_routes` | routes for a device | `id` | GET `device/{id}/routes` | read |
| `keys` | list auth keys | – | GET `tailnet/{t}/keys` | read |
| `acl` | policy file | – | GET `tailnet/{t}/acl` (Accept json) | read |
| `dns` | nameservers+searchpaths+preferences, joined with `tokio::try_join!` (`src/app.rs`) | – | 3 GETs | read |
| `users` | list users | – | GET `tailnet/{t}/users` | read |
| `authorize_device` | approve device | `id` | POST `device/{id}/authorized` `{authorized:true}` | write |
| `delete_device` | remove device | `id`, `confirm` | DELETE `device/{id}` | destructive |
| `help` | inline docs | – | none | read |

Annotations on the single tool: `readOnlyHint:false, destructiveHint:true, idempotentHint:false, openWorldHint:true` — the whole tool is flagged destructive because one tool covers everything; a custom `_meta["ai.dinglebear/rtailscale"]` lists `readOnlyActions/mutatingActions/destructiveActions/requiresConfirmation`. `execution.taskSupport: "forbidden"`. One resource `tailscale://schema/mcp-tool`; one prompt `network_status` (`src/mcp/prompts.rs`).

### (4) Transports
`src/main.rs`: `rtailscale mcp` = stdio; `rtailscale serve` (or no args) = Streamable HTTP over axum on `TAILSCALE_MCP_HOST:PORT` (default `0.0.0.0:40040`). `src/mcp/rmcp_server.rs` uses `StreamableHttpServerConfig::default().with_legacy_session_mode(false).with_json_response(true)` with `LocalSessionManager` (stateless JSON responses, no SSE). `src/mcp/routes.rs`: `/mcp`, `/health`, OAuth `.well-known` routes, `RequestBodyLimitLayer::new(65_536)`, CORS from allowed origins.

### (5) Configuration
`src/config.rs`: `config.toml` from CWD then env overrides: `TAILSCALE_API_KEY`, `TAILSCALE_TAILNET` (default `-`), `TAILSCALE_ALLOW_DESTRUCTIVE`, `TAILSCALE_MCP_HOST/PORT/SERVER_NAME/NO_AUTH/TOKEN/ALLOWED_HOSTS/ALLOWED_ORIGINS/PUBLIC_URL/AUTH_ADMIN_EMAIL/AUTH_MODE(bearer|oauth)`; `TAILSCALE_MCP_HOME` data dir. `.env` loaded by `load_dotenv()` which **refuses a symlinked `.env`** (exit 1). No binary-path or timeout config (timeout is a `const` 30s).

### (6) Safety
- Two-signal destructive interlock in `src/app.rs::delete_device`: server-side `TAILSCALE_ALLOW_DESTRUCTIVE=true` **and** caller `confirm=true`, otherwise bail with a specific message.
- In HTTP-auth mode, `src/mcp/rmcp_server.rs` maps actions to scopes: read actions to `tailscale:read`, `authorize_device` to `tailscale:write`, `delete_device` and unknown actions to `DENY_SCOPE = "tailscale:__deny__"`, so deletion is impossible over authenticated HTTP — only via stdio/loopback.
- `validate_bind_security()` in `src/main.rs` refuses a non-loopback bind unless a token, OAuth, or explicit `TAILSCALE_NOAUTH=true` is set.
- Credentials are never accepted as tool arguments. No output redaction.

### (7) Error handling
`src/tailscale.rs::TailscaleApiError { status, code, message, hint, body }` with codes `unauthorized/forbidden/not_found/rate_limited/tailscale_api_error`; wrapped in `anyhow`. `rmcp_server.rs`: validation errors (message contains "is required"/"unknown tailscale action") become protocol `ErrorData::invalid_params`; API errors become `CallToolResult` with `is_error:true` and a structured envelope `{ok:false,data:null,error:{code,message,status,action,upstream:"tailscale",hint?,body?}}`; any other error is masked as `"Tool execution failed for action '{action}'. Check server logs for details."` (code `tool_execution_failed`, status 500). Success is `{ok:true,data,error:null}` as both text and `structured_content`. Quirk: the network-error hint mentions a nonexistent `TAILSCALE_API_URL` env var.

### (8) Long-running / excluded
No CLI exposure at all, so `up/serve/funnel/ping` are moot. 30s HTTP timeout. `src/token_limit.rs` (`MAX_RESPONSE_BYTES = 40_000`, `truncate_response`) is **dead code** — grep finds no caller outside the module.

### (9) Tests
`tests/tool_dispatch.rs`, `tests/destructive_gate.rs`, `tests/cli_parse.rs`, `tests/setup_cli.rs`, unit tests inside `rmcp_server.rs`; all in-process via `#[cfg(feature="test-support")] mcp::testing` helpers. **No HTTP mocking**: the `confirm=true` case in `destructive_gate.rs` performs a live network call with key `"test"` and asserts on the resulting API error. Live smoke test `tests/mcporter/test-tools.sh` against a running server.

### (10) Notable
Clever: scope-per-action deny mapping, bind-security refusal, symlink-`.env` refusal, `doctor`/`setup` subcommands with JSON output, `--json` health checks, request counters (`src/observability.rs`). Broken/odd: dead truncation module, stale MSRV, naming-conflict design (`tailscale` binary name), `--json` flag ignored in `src/cli.rs` (both branches pretty-print), git-pinned `lab-auth` dependency for Google OAuth.

### rtailscale library questions
- **lib vs bin**: both (`src/lib.rs` exports `app, cli, config, logging, mcp, observability, setup, tailscale, token_limit`), but the lib exists to support the bin and tests, not as a public API.
- **crates.io**: no (`publish = false`).
- **Modules**: `tailscale.rs` REST client; no LocalAPI client; no CLI wrapper.
- **Endpoints covered (12)**: devices list, device get, device routes, keys list, ACL get, DNS nameservers/searchpaths/preferences, users list, `probe()` (`devices?limit=1`), device authorize, device delete. No create/update for anything, no ACL POST/validate, no ETag/If-Match, no pagination, no webhooks/settings/posture/invites/services/log-streaming.
- **HTTP client / runtime**: `reqwest 0.12.28` (rustls) + `tokio 1.52.3`; `rmcp =3.1.0`; `axum 0.8.9`.
- **Models**: none — every method returns `anyhow::Result<serde_json::Value>`.
- **Auth**: API key bearer only. No OAuth client-credentials, no refresh. (`lab-auth` is for authenticating *MCP clients* to the HTTP server, not for Tailscale.)
- **Errors**: `TailscaleApiError` (Display + StdError) inside `anyhow::Error`; consumers must `downcast_ref`.
- **Quality**: clean, well-commented, but thin and coupled (`Arc<Counters>` in the client struct, hard-coded base URL, `pub tailnet`).
- **Recommendation: NO, do not depend on it.** Reasons: not published; AGPL-3.0-only would infect the new server; untyped `Value` API; API-key-only auth with no OAuth refresh; 12 read-heavy endpoints with no ETag, retry, or pagination; coupling to counters/config and a git-pinned private auth crate. Copy its error envelope, two-signal gate, bind-security check, and `doctor` idea instead.

---

## 2. `~/github.com/HexSleeves/tailscale-mcp`

### (1) Basics
TypeScript on Node >=20 (bun for build/test), `@modelcontextprotocol/sdk` 1.29.0, zod 4, express 5. npm `@hexsleeves/tailscale-mcp-server` 1.3.4 (`server.json` stale at 1.3.0); Docker Hub `hexsleeves/tailscale-mcp-server` + GHCR. MIT. Last commit `2026-07-27 10:37:09 -0500 chore(deps): update dependency @hono/node-server to v2.0.12 (#152)`. No star badge.

### (2) Backends
Both REST (`src/tailscale/api-client.ts`, native `fetch`, no timeout, no retry) and CLI (`src/tailscale/cli-client.ts`, `spawn(this.cliPath, args, {stdio:["ignore","pipe","pipe"], shell:false})`, **no timeout**). Binary located via `TAILSCALE_CLI_PATH` env (default `"tailscale"`, PATH lookup); no macOS app-bundle probing, no LocalAPI. `src/tailscale/service.ts` does API-first with CLI `status --json` fallback for `listDevices`, and CLI-first with API fallback for `getNetworkStatus`.

### (3) Tools (19 registered via `registerTool`; CHANGELOG claims 15)
| tool | purpose | key params | backend | risk / annotation |
|---|---|---|---|---|
| `list_devices` | list devices | `includeRoutes` | API (`?fields=all`) w/ CLI fallback | read, `readOnlyHint` |
| `device_action` | authorize/deauthorize/delete/expire-key | `deviceId, action` | API | write (delete/deauthorize=admin), `destructiveHint` |
| `manage_routes` | enable/disable subnet routes (read-modify-write on `enabledRoutes`) | `deviceId, routes, action` | API | write |
| `get_network_status` | this node's status | – | CLI `status --json` | read |
| `connect_network` | `tailscale up` with `--login-server/--accept-routes/--accept-dns/--hostname/--advertise-routes/--authkey` | options | CLI | admin, no annotation; **auth key on argv** |
| `disconnect_network` | `tailscale down` | – | CLI | admin, `destructiveHint` |
| `ping_peer` | `ping --c=N --timeout=1s target` (count 1–100, default 4) | `target, count` | CLI | read |
| `get_version` | `tailscale version` | – | CLI | read |
| `get_tailnet_info` | `GET /tailnet/{t}` (**not a real endpoint**) | – | API | read |
| `manage_file_sharing` | get/enable/disable via `/settings` | `action` | API | read/write |
| `manage_exit_nodes` | list (from advertised 0.0.0.0/0 or ::/0) / set / clear / advertise / stop_advertising | `action, nodeId` | CLI `set --exit-node` + API routes | admin |
| `manage_webhooks` | list/create/delete/test | `action, endpointUrl, subscriptions, id` | API | read/write |
| `manage_device_tags` | get/set/add/remove | `deviceId, action, tags` | API POST `/device/{id}/tags` | read/write |
| `get_version_info` | constant string stub | – | none | no risk check |
| `manage_acl` | get/validate/update (**no If-Match**) | `action, acl` | API | get=read, validate+update=write |
| `manage_dns` | get/set nameservers, preferences, searchpaths | `action, ...` | API | read/write |
| `manage_keys` | list/create/delete | `action, ...` | API | list=read, create/delete=admin, `destructiveHint` |
| `manage_policy_file` | get/update; `test_access` returns "not implemented" (`src/mcp/tools/acl.ts:178`) | | API | |
| `manage_network_lock` | **stub**: "Network lock management requires endpoint-specific implementation in this rebuild." (`acl.ts:219`) | | none | |

Only 10 of 19 tools carry annotations. Resources: `tailscale://tailnet/summary`, `tailscale://devices`, `tailscale://devices/{deviceId}` (ResourceTemplate), `tailscale://acl/current` (`application/hujson`). Prompts: `diagnose_tailnet_connectivity`, `review_acl_change`.

### (4) Transports
stdio (`src/mcp/transports/stdio.ts`) and Streamable HTTP (`src/mcp/transports/http.ts`): express, `express.json({limit:"1mb"})`, in-memory 120 req/min/IP rate limit, `GET /health`, bearer auth middleware on `/mcp`, a fresh `McpServer` + `StreamableHTTPServerTransport({sessionIdGenerator: undefined})` per POST (stateless), GET/DELETE `/mcp` return 405 (no SSE stream). Flags `--http/--stdio/--port/--host`.

### (5) Configuration (`src/config/env.ts`, zod)
`MCP_TRANSPORT`, `MCP_HTTP_BIND_HOST` (127.0.0.1), `MCP_HTTP_PORT` (3000), `MCP_HTTP_BEARER_TOKEN` (min 32 chars, required for HTTP), `MCP_ALLOWED_HOSTS`, `TAILSCALE_TAILNET` (`-`), `TAILSCALE_API_BASE_URL` (https enforced except loopback), `TAILSCALE_API_KEY`, `TAILSCALE_OAUTH_CLIENT_ID/SECRET` (partial pair is an error), `TAILSCALE_CLI_PATH`, `TAILSCALE_ALLOWED_TOOL_RISK` (read|write|admin, default **read**), `LOG_LEVEL`, `MCP_SERVER_LOG_FILE`. OAuth token provider (`src/tailscale/oauth-token-provider.ts`) posts to `/api/v2/oauth/token`, caches with 60s early refresh, no concurrent-refresh dedupe.

### (6) Safety
Risk tiers enforced at call time by `requireRisk(config, "read"|"write"|"admin")` (`src/security/scopes.ts`) which throws `AppError(... "risk_level_denied", 403)`. Timing-safe bearer compare and host allow-list (`localhost`, loopback, `*.ts.net`, extras) in `src/security/auth.ts`. Regex redaction (`src/observability/redaction.ts`) of `tskey-*`, `Bearer ...`, `--authkey ...`, and JSON keys `apiKey|authKey|accessToken|clientSecret|authorization`, applied to logs and CLI stderr. Input hygiene in `src/utils.ts` (`DANGEROUS_CHARS`, `validateTarget` via ipaddr.js/hostname regex, `validateRoutes` CIDR). `manage_acl validate` is gated as write although it is read-only.

### (7) Error handling
`src/observability/errors.ts::toMcpError` returns `{isError:true, content:[{type:"text", text: error.message}]}` and **deliberately omits `structuredContent`** because clients validate it against `outputSchema` even on errors (comment at top of file — a real footgun worth remembering). API errors: `ApiResult{success:false, error: body.message|body.error|"HTTP <status>", statusCode}`; fetch exceptions get `statusCode: 0`. CLI: non-zero exit becomes `{success:false, error: redactedStderr || "tailscale exited with code N"}`.

### (8) Long-running / excluded
CLI has no timeout at all (a hung `tailscale up` blocks forever). `ping` uses `--c=N --timeout=1s`. `up` passes `--authkey` on argv (visible in `ps`). `serve/funnel/ssh/lock/logout` not exposed; no comment explains why. Stubs (`manage_network_lock`, `test_access`, `get_version_info`) ship as tools.

### (9) Tests
`bun:test`; `src/__test__/mcp/helpers.ts` has a `CapturingServer` that records `registerTool` handlers and lets tests `callTool(name, input)` against `makeFakeService()`; `risk-gating.test.ts` asserts write/admin tools return `isError` at read level; `api-client.test.ts` monkeypatches `globalThis.fetch`. `src/__test__/README.md` is stale (Jest), `test:integration` script targets nonexistent files, `setup.integration.ts` shells `which tailscale`.

### (10) Notable
Copy: `outputSchema`/error `structuredContent` lesson, redaction of CLI stderr, risk tiers, bearer+host checks, stateless per-request HTTP. Avoid: unbounded child processes, auth keys on argv, fake endpoints, stubs registered as tools, dead `axios` dependency (`src/utils.ts:2`), stale metadata, annotation gaps.

---

## 3. `~/github.com/YawLabs/tailscale-mcp`

### (1) Basics
TypeScript, Node >=20.11.0; sdk ^1.29.0 and zod ^4 bundled by esbuild into a **zero-runtime-dependency** `dist/index.js`; Node SEA / `oam compile` standalone binaries (`sea-config.json`, `build.mjs`). npm `@yawlabs/tailscale-mcp` 0.18.0; `server.json` for the registry; no Docker image. MIT. Last commit `2026-08-31 17:35:25 -0700 v0.18.0`. README shows a dynamic shields.io stars badge (no static count). **No CI** (`.github/workflows` absent; `release.sh` runs lint+test+publish locally). CHANGELOG is detailed (Keep-a-Changelog).

### (2) Backends
REST (`src/api.ts`, native `fetch`, `BASE_URL = https://api.tailscale.com/api/v2`) plus **opt-in** CLI (`src/local-cli.ts`, `execFile(binary, args, {timeout: 30_000, maxBuffer: 10 MiB})`, never rejects). Binary = `process.env.TAILSCALE_BINARY || "tailscale"` (PATH only; no app-bundle probing, no LocalAPI). API key sent as HTTP **Basic** (`key:`), OAuth as Bearer.

### (3) Tools (96 API + 6 CLI = 102; verified by grep) — every tool has `title`, `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`
- **status** (1): `tailscale_status` (devices?fields=id + settings; read).
- **devices** (17, `src/tools/devices.ts`): `list_devices` (`fields`, server-side `filters`), `get_device`, `authorize_device`, `deauthorize_device` (destructive), `delete_device` (destructive), `rename_device`, `expire_device` (destructive), `get_device_routes`, `set_device_routes` (destructive; CIDR validated with `net.isIPv4/6`), `get_device_posture_attributes`, `set_device_posture_attribute`, `delete_device_posture_attribute` (destructive), `set_device_tags` (destructive), `set_device_ip`, `update_device_key`, `set_devices_authorized` (batch, parallel, per-id errors), `batch_update_posture_attributes`.
- **acl** (4, `src/tools/acl.ts`): `get_acl` (HuJSON raw + ETag appended as an idempotent `// ETag:` comment footer), `update_acl` (required non-empty `etag` → `If-Match`; destructive), `validate_acl` (normalizes empty/`{}` to "ACL policy is valid."), `preview_acl`.
- **dns** (11): get/set nameservers, search paths, split DNS, `update_split_dns` (merge), preferences, `get/set_dns_configuration` (unified). All `set_*` replace-all writes are destructive.
- **keys** (9): `list_keys` (`all=true`), `get_key`, `create_key` (auth/client/federated; description sanitized to API rules; tags validated `tag:`), `delete_key`, `update_key`, `create/get/list/delete_oauth_app`.
- **users** (7): list (type/role filters), get, approve, suspend, restore, update_role, delete.
- **tailnet** (5): get/update settings, get/set contacts, resend contact verification.
- **webhooks** (7): list, get, create (subscriptions validated against a static catalog + `TAILSCALE_EXTRA_WEBHOOK_EVENTS`), update, delete, rotate_secret, test.
- **posture** (5): list/get/create/update/delete integrations (provider list + `TAILSCALE_EXTRA_POSTURE_PROVIDERS`).
- **audit** (2): `get_audit_log`, `get_network_flow_logs`.
- **invites** (11): device invites list/create/get/delete/accept/resend; user invites list/create/get/delete/resend.
- **services** (7): list/get/update/delete, list hosts, get/set device approval.
- **log-streaming** (7): list/get/set/delete configs, status, AWS external id, validate AWS trust policy.
- **org-tailnets** (3, `src/tools/tailnets.ts`): `list_org_tailnets` (paginated), `create_org_tailnet`, `delete_tailnet` (requires `confirmTailnet` to equal the resolved target; refuses when target is `-`).
- **local-cli** (6, `src/tools/local-cli.ts`, all read-only): `tailscale_local_status` (`status --json`), `tailscale_ping` (`ping -c N target`, target validated by `net.isIP`/hostname, count 1–20), `tailscale_netcheck` (`netcheck --format=json`), `tailscale_local_version`, `tailscale_local_whoami` (>=1.102.1), `tailscale_local_service_list` (>=1.102.1).

Resources (4, `src/server-wiring.ts`): `tailscale://tailnet/status`, `.../devices`, `.../acl` (hujson; failures emitted as `//` comment lines so the body stays parseable), `.../dns`. No prompts.

### (4) Transports
stdio only (`StdioServerTransport`). Also a CLI mode: `deploy-acl <file>` / `validate-acl <file>` / `version` (`src/index.ts`, `src/cli.ts`).

### (5) Configuration (env only, all reads verified in `src/`)
`TAILSCALE_API_KEY` | `TAILSCALE_OAUTH_CLIENT_ID` + `_SECRET` | `TAILSCALE_OAUTH_TAILNET` (API-only tailnets; `?tailnet=` on the token request) | `TAILSCALE_TAILNET` (`-`) | `TAILSCALE_BINARY` | `TAILSCALE_LOCAL_CLI` (`1`/`true`) | `TAILSCALE_PROFILE` (minimal=status,devices,audit / core=+acl,dns,keys,users / full) | `TAILSCALE_TOOLS` (group list; overrides profile) | `TAILSCALE_READONLY` | `TAILSCALE_DEBUG` | `TAILSCALE_MAX_CONCURRENT` | `TAILSCALE_REQUEST_BUDGET_MS` (90000) | `TAILSCALE_RETRY_BASE_DELAY_MS` (1000) | `TAILSCALE_EXTRA_WEBHOOK_EVENTS` | `TAILSCALE_EXTRA_POSTURE_PROVIDERS`. Launcher: `TAILSCALE_MCP_RUNTIME`, `TAILSCALE_MCP_SANDBOX`. Startup banner on stderr reports tool count, profile, overrides, and a tailnet-mismatch warning.

### (6) Safety
Filtering happens at registration (`src/filter.ts`): `TAILSCALE_READONLY` drops every tool without `readOnlyHint:true`; groups/profiles intersect; all-unknown group lists fall back rather than yielding a zero-tool server; group name `org-tailnets` chosen to avoid a one-character typo from `tailnets`. Per-call guards only where irreversible and ambiguous (`confirmTailnet`, `etag` non-empty). `src/local-cli.ts` header: *"Scope is deliberately narrow: read-only diagnostics that don't require root. We don't expose `tailscale up/down/set/lock` -- those need elevation and have non-trivial argument-injection surface if driven by an LLM."* `apiRequest` refuses absolute URLs not on `https://api.tailscale.com/` (SSRF/credential-exfil guard). No output redaction; `create_key`/`create_org_tailnet` descriptions warn that responses contain secrets. Authorization headers are never debug-logged.

### (7) Error handling
`ApiResponse {ok, status, data?, error?, rawBody?, etag?}`. `apiRequest` never throws once auth resolved: transport errors, timeouts (`AbortSignal.timeout`), body-read failures all land in the envelope; 401/403 get a multi-line `formatAuthError` (mode-aware, Windows env hint, console links); other errors unwrap `{message}`/`{error}` via `extractErrorMessage`; empty-body failures floor to `HTTP <status>`. A 401 evicts the cached OAuth token (only if the header still matches that token). `wrapToolHandler` (`src/server-wiring.ts`) renders `Error: ...` text with `isError:true`, or `rawBody ?? JSON.stringify(data)`. CLI: `CliResult {ok, data?, rawBody?, error?, exitCode?}` distinguishing ENOENT (with `TAILSCALE_BINARY` hint), maxBuffer overflow, timeout kill, and non-zero exit (`stderr || err.message`).

### (8) Long-running / excluded
`execFile` timeout 30s + 10 MiB buffer; `ping -c N` capped at 20 (no `--timeout`); `--json` used for `status`/`netcheck`, deliberately **not** for `whoami`. Excluded with reasons (see above): `up/down/set/lock/serve/funnel`. REST: 30s per attempt, 3× 429 retry honoring `Retry-After` (int or date), exponential backoff with jitter capped 30s, total budget 90s, retries only on GET/PUT/DELETE, optional concurrency semaphore that also covers the OAuth refresh.

### (9) Tests
`node --test` on compiled `dist/*.test.js`, 17k lines, 1651 tests per CHANGELOG. `handlers.test.ts` installs a default 599 fetch stub so no test can reach the network; `api.test.ts` uses `__resetOAuthTokenCacheForTests`/`__resetConcurrencyStateForTests`; `local-cli.test.ts` injects `__setExecFileForTests`; `release-metadata.test.ts` asserts README tool counts and `server.json`/`package.json` versions against the live registry; `integration.test.ts` is gated by `RUN_INTEGRATION_TESTS=1` and **fails** (not skips) if credentials are missing, and warns it mints real keys.

### (10) Notable
Most mature of the three. Clever: ETag-as-HuJSON-comment, `{}`-is-valid normalization, profiles/groups/readonly intersection, escape-hatch env vars for enum drift, `confirmTailnet` typo guard with honest documentation of what it does and does not prove, budget-aware retry. Caveats: `local-cli` group is not in any PROFILE preset, so `TAILSCALE_PROFILE=core` silently drops CLI tools even with `TAILSCALE_LOCAL_CLI=1`; no CI; API-key auth uses Basic rather than Bearer (works, but differs from docs); no secret redaction of responses.

---

## (a) Coverage comparison

| Capability | rtailscale | HexSleeves | YawLabs |
|---|---|---|---|
| List devices | `tailscale devices` | `list_devices` | `tailscale_list_devices` |
| Get device | `tailscale device` | — | `tailscale_get_device` |
| Authorize / deauthorize | `authorize_device` (auth only) | `device_action` | `authorize_device`, `deauthorize_device`, `set_devices_authorized` |
| Delete device | `delete_device` (gated) | `device_action delete` | `tailscale_delete_device` |
| Expire key / rename / set IP | — | `device_action expire-key` | `expire_device`, `rename_device`, `set_device_ip`, `update_device_key` |
| Device routes get/set | `device_routes` (get) | `manage_routes` | `get/set_device_routes` |
| Device tags | — | `manage_device_tags` | `set_device_tags` |
| Posture attributes | — | — | 4 tools |
| Exit nodes | — | `manage_exit_nodes` (CLI+API) | — (via routes) |
| ACL get | `acl` | `manage_acl get` | `get_acl` (+ETag) |
| ACL update | — | `manage_acl update` (no ETag) | `update_acl` (If-Match) |
| ACL validate / preview | — | validate | validate, preview |
| DNS get / set | `dns` (get) | `manage_dns` | 11 tools incl. split DNS |
| Auth keys list/create/delete | `keys` (list) | `manage_keys` | 5 key tools + 4 OAuth-app tools |
| Users | `users` | — | 7 tools |
| Tailnet settings / contacts | — | `get_tailnet_info` (fake), `manage_file_sharing` | 5 tools |
| Webhooks | — | `manage_webhooks` | 7 tools |
| Posture integrations | — | — | 5 tools |
| Audit / flow logs | — | — | 2 tools |
| Invites | — | — | 11 tools |
| Services | — | — | 7 tools |
| Log streaming | — | — | 7 tools |
| Org tailnets | — | — | 3 tools |
| Network lock | — | `manage_network_lock` (stub) | — |
| Local status (CLI) | — | `get_network_status` | `tailscale_local_status` |
| Ping (CLI) | — | `ping_peer` | `tailscale_ping` |
| Netcheck (CLI) | — | — | `tailscale_netcheck` |
| Version (CLI) | — | `get_version` | `tailscale_local_version` |
| whoami / service list (CLI) | — | — | 2 tools |
| `up` / `down` (CLI) | — | `connect_network`, `disconnect_network` | — (deliberately) |
| serve / funnel / ssh / lock / cert / file | — | — | — |
| Resources | 1 | 4 | 4 |
| Prompts | 1 | 2 | 0 |
| Transports | stdio + Streamable HTTP | stdio + Streamable HTTP | stdio |
| Tool count | 1 (10 actions) | 19 (3 stubs) | 102 |

## (b) Design lessons

**Copy**
- Full annotation set on every tool (`title`, `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) and make `TAILSCALE_READONLY`-style filtering key off `readOnlyHint` at registration time (YawLabs `src/filter.ts`). Rule for `destructiveHint`: "can this call wipe configuration the caller never named?" (replace-all writes yes, scalar/merge writes no).
- Tool groups + profiles + explicit list, intersected; refuse to produce a zero-tool server; print a startup banner naming the active filter and overrides.
- Opt-in CLI surface limited to root-free read-only diagnostics, with a source comment stating what is excluded and why (YawLabs `src/local-cli.ts`). For Rust: `tokio::process::Command` with a kill-on-timeout, output cap, `ENOENT` → "set TAILSCALE_BINARY" hint, JSON parse failure → return raw text.
- Never take credentials as tool args; env only (rtailscale docs, all three).
- Two-signal destructive gate for irreversible calls (rtailscale `TAILSCALE_ALLOW_DESTRUCTIVE` + `confirm=true`) and typo guards that echo the target (`confirmTailnet`).
- ACL: GET returns raw HuJSON + ETag; update requires non-empty ETag → `If-Match`; 412 reported as concurrent edit, no auto-retry; validate treats empty/`{}` as success.
- REST client: per-attempt timeout, total budget, 429 retry honoring `Retry-After` only for idempotent methods, optional concurrency cap, evict OAuth token on 401 (not 403), dedupe concurrent refreshes, refuse absolute URLs off `api.tailscale.com`, unwrap `{message}`/`{error}` bodies, floor empty bodies to `HTTP <status>`.
- Error results: `isError:true` with actionable text; if you declare `outputSchema`, **omit** `structuredContent` on errors (HexSleeves `src/observability/errors.ts`). rtailscale's `{ok,data,error{code,message,status,hint}}` envelope is a good structured shape for success.
- Redact `tskey-*`, `Bearer`, `--authkey` in logs and CLI stderr (HexSleeves `src/observability/redaction.ts`); describe secret-bearing responses as sensitive in tool descriptions.
- HTTP transport hardening if you offer one: refuse non-loopback bind without a token, bearer with timing-safe compare, host/origin allow-list, body-size limit, `/health`, stateless JSON-response mode.
- Tests: default network stub that fails loudly, injectable `execFile`/`Command` seam, opt-in live suite that fails rather than skips when credentials are missing, metadata tests that pin README counts and `server.json` versions to the registry.
- `doctor` subcommand (rtailscale) that checks binary, credentials, upstream reachability, and prints JSON.

**Avoid**
- Spawning the CLI without a timeout or output cap; passing auth keys on argv (HexSleeves).
- Registering stubs or invented endpoints as tools (`manage_network_lock`, `get_tailnet_info`).
- One mega-tool with an `action` enum: forces `destructiveHint:true` on reads and loses per-tool annotations (rtailscale).
- Untyped `serde_json::Value` everywhere with no models, and hard-coding the base URL (rtailscale).
- Shipping dead code/deps (rtailscale `token_limit.rs`, HexSleeves `axios`), stale `server.json` versions, stale test READMEs, drifted MSRV claims.
- Hidden precedence traps: an opt-in group that no profile includes (YawLabs `local-cli` vs `TAILSCALE_PROFILE`).
- Tests that hit the live API by accident (rtailscale `destructive_gate.rs` confirm=true case).
- A binary named `tailscale` that shadows the real CLI (rtailscale `doctor`).
- Gating read-only operations (ACL validate) as writes (HexSleeves).
