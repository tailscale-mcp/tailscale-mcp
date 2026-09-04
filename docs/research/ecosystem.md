<!-- Research note. Community and ecosystem context: licences, alternatives, authorization patterns, vault notes.
     Produced by a research sub-agent on 2026-09-03 during the design interview; facts were verified against the sources named inside. Not a spec. -->

## Task 1 — `/last30days` "Rust alternatives to tailscale-mcp"

**Files read:**
- `~/github.com/soulmachine/llm-wiki/raw/agent-notes/Last30Days/rust-alternatives-to-tailscale-mcp-raw-v3.md` (845 lines, 57 KB, mtime 2026-09-03 14:24 — the only artifact from this run)
- `~/github.com/soulmachine/llm-wiki/CLAUDE.md` (ownership rules; `raw/` human-owned except `raw/agent-notes/`, which is agent-owned)

**No summary file exists.** The Last30Days directory contains only `*-raw-v3.md` files; there is no companion summary for this run (or any other). I verified with `find … -newermt 2026-09-03` across the whole vault and the `tailscale-mcp` working dir — nothing else was written.

**Caveat on quality:** the social crawl badly missed the topic. 74 items across 4 sources, but 52 of the X items and most Reddit/HN items are `score:0` with `Why: fallback-local-score (entity-miss demotion)` — Grok Bot ads, Hermes masterclasses, Tailscale marketing. Essentially **all the substance is in the `## WebSearch Supplemental Results` section, lines 833–845.**

### (a) Tailscale MCP servers and Tailscale Rust crates

| Project | URL | Lang | Stars / status | Backend | Notes |
|---|---|---|---|---|---|
| `dinglebear-ai/rtailscale` | github.com/dinglebear-ai/rtailscale | Rust | created 2026-05-13, commits through 2026-08-11; AGPL-3.0-only + commercial dual license since 2026-08-05 | **REST API v2** | Called "the only actively maintained Rust MCP server for Tailscale." **Single `tailscale` tool with an action enum** — reads `devices, device, device_routes, keys, acl, dns, users, help`; one write `authorize_device`; one destructive `delete_device` double-gated on `TAILSCALE_ALLOW_DESTRUCTIVE=true` **and** caller-supplied `confirm=true`. stdio + HTTP MCP on port 40040. Installs via `npx -y @dinglebear/rtailscale`, bash installer, Docker Compose, cargo, or a Claude Code plugin. |
| `pnocera/tailscale-mcp` ("Tailscale MCP Server v2") | github.com/pnocera/tailscale-mcp | Rust | 0 stars; created and last pushed the same day, 2025-08-13 | not stated | "Exists, but is not a maintained option." |
| `YawLabs/tailscale-mcp` | via glama.ai | TypeScript | — | **REST admin API + local CLI** | **89 admin-API tools plus 4 local-CLI diagnostics**, 700+ tests, full Tailscale v2 API. Explicitly framed as "the coverage benchmark any Rust rewrite is measured against." |
| `jaxxstorm/tailscale-mcp` (+ `jaxxstorm/tailscale-mcp-proxy`) | github.com/jaxxstorm/tailscale-mcp | Go | — | REST API, embedded via `tsnet` | Lee Briggs's server, from the Tailscale blog clipped in the vault. Reads **Tailscale grants** (`app` capability rules) to decide which tools a caller may invoke; proxy forwards `X-Tailscale-User`. |
| `tailscale/tailscale-rs` | github.com/tailscale/tailscale-rs | Rust | **1,130 stars, 66 open issues**, last pushed 2026-09-03 | n/a (crate `tailscale`, docs.rs/tailscale) | Official experimental/preview pure-Rust Tailscale implementation. **Not an MCP server** — "the building block a native Rust server would sit on if it wanted to avoid shelling out to the CLI." Only GitHub-source item in the whole run. |
| `jtdowney/tailscale-localapi` | github.com/jtdowney/tailscale-localapi | Rust | **10 stars** | **LocalAPI client** | "The alternative integration surface to the REST API for a server that runs on a tailnet node." |
| `HexSleeves/tailscale-mcp` | — | — | — | — | **Named only** in the Resolved Entities line 10 as a seed entity; the run gathered zero evidence about it. Gap. |
| `tailscale/tailcat` | github.com/tailscale/tailcat | Go | HN 687 pts / 134 cmt (2026-08-26); blog post 73 pts | data plane, no tailscaled, no control server | Shipped 2026-08-26. "Widens what a Rust MCP server can target beyond the admin REST API." |
| Adjacent: `maisem/tailmix` (multi-tailnet on one host, HN 9 pts), `tailscale/tailvisor` (VM sandbox with tailnet identity, HN 4 pts), `almeidapaulopt/tsdproxy` (HN 3 pts) | | Go | | | Context only. |

Also relevant from Tailscale itself: **Aperture went GA 2026-08-26** with "built-in tokens, **Tailscale MCPs**, Projects" (@Tailscale, 667 likes); **Tailscale PAM** beta (Border0 acquisition); an explicit "embed Tailscale into apps, create tailnets through APIs" push (2026-08-31).

### (b) What people ask for / complain about

- **Scope creep is the #1 safety worry.** @chrisveleris, 2026-08-30: *"I gave my own app an MCP server so agents could drive it, and the thing I underestimated was scope. Something that can turn lights on can turn everything off at 3am after misreading one word. What did you end up restricting it from?"*
- **"Why not just shell out?"** — the direct challenge to the whole premise. @adolandev, 2026-09-03: *"Totally valid if your agent already shells out to Tailscale. This isn't a new network path. It's a UI for the mesh you already have: who's up, copy IP/MagicDNS, SSH in an overlay, send a file, publish this instance on the tailnet. Same CLI, fewer context switches. If you live in the agent's terminal anyway, you probably don't need it."* This is the sharpest framing of what the tool surface should be.
- **Credential handling**: r/mcp 2026-08-29 (15 pts, 12 cmt) *"I built a tiny local MCP server that lets AI agents use credentials without exposing them"*; @Voxyz_ai (186 likes): *"I can also keep the real keys on my self-hosted MCP server and give the Bot its own token."*
- **Deployment friction**: r/mcp 2026-09-01 (42 pts, **62 cmt**) *"How is anyone actually running MCP in an enterprise without every user having a Docker container on their laptop?"* — direct argument for a single static binary.
- **Server-selection fatigue**: r/mcp *"Which MCP servers do you use the most?"* (93 pts, **111 cmt**), *"Compiled a list of MCP servers Q3 2026"* (20 pts).
- **Tailscale's own security framing**: "Tailscale Mitigates the Lethal Trifecta" (tailscale.com/blog/aperture-lethal-trifecta), and Aperture GA pitched as *"Give AI agents the models, tools, and infrastructure they need without handing over API keys or unrestricted access."*
- **No explicit tool-count / context-bloat complaint appears in this run.** The closest signal is structural: `rtailscale`'s 1-tool-with-action-enum vs `YawLabs`'s 93 tools. Treat tool-count as an inferred concern, not an evidenced one.
- One adjacent tailnet-MCP pattern: @ThatcherThorn, *"session-bridge turns every device on my tailnet into an MCP server… Discovery: `tailscale status` every 15s, then ssh into anything online but silent."*

### (c) Rust MCP ecosystem facts (with evidence lines)

- Line 837: *"the official Rust MCP SDK lives at `modelcontextprotocol/rust-sdk` (crate `rmcp`), **3,868 stars**, last pushed 2026-09-02. Derives JSON Schema for tool parameters from Rust types at compile time, so a hand-rolled Tailscale server gets schema generation for free."*
- Line 838: *"the `rmcp` crate has crossed **4.7 million downloads** as of early 2026, making it the de facto Rust MCP substrate rather than a niche choice."*
- Line 839 (rustify.rs): *"2026 guide to building MCP servers in Rust; cites **sub-5ms cold start** versus 300-800ms for Python and **5-15 MB static binaries** versus 50-200 MB interpreted environments."*
- Line 840 (systemprompt.io): *"`rmcp` walkthrough reporting that a Rust MCP server **loads in under a second where an equivalent TypeScript server takes over four seconds**, the practical argument for a compiled MCP server on an always-on host."*
- **Negative finding:** `mcp-sdk`, `rust-mcp-sdk`, and `mcp-core` do **not** appear anywhere in the file (grepped). The run surfaced no alternative to `rmcp`.
- Line 845 points back into the vault: Lee Briggs's post *"argues MCP servers should be reachable over the tailnet rather than the public internet, the deployment shape a single static Rust binary suits well. Clipped locally at `raw/clipped/tailscale.com/webpages/`."*

---

## Task 2 — graphify + vault grep

`graphify` is installed at `~/.local/bin/graphify`; both queries ran clean against a 5,615-node graph.

- **`graphify query "Tailscale MCP server design"`** → 75 nodes (truncated to 41 by the ~2000-token budget). Surfaced a dedicated community **"Private MCP over Tailscale"**, all sourced to the Briggs clip: `Network-layer privacy over transport-layer authentication`, `Tailscale grants (app capability rules)`, `Per-user, per-tool MCP authorization`, `Critique: OAuth authenticates an MCP server but does not unexpose it`, `tsnet (embedding Tailscale in a Go server)`, `stdio-to-remote MCP proxy`, `MCP SSE transport (deprecated)`, `Tailnet (private overlay network)`, `jaxxstorm/tailscale-mcp (Tailscale API MCP server)`, `Objection: a vendor in the middle of an open standard`. Plus `Per-machine ACL Tags` from the human Tailscale ops note.
- **`graphify query "rmcp Rust MCP SDK"`** → 24 nodes, **no `rmcp` node exists**. It fell back to generic `Rust` (rustup) and the `MCP` server-catalogue nodes. The vault has no Rust-MCP-SDK knowledge at all.

### `grep -ril "tailscale" wiki/ raw/`

Zero hits in `wiki/`. Nine relevant hits in `raw/` (excluding `.obsidian/workspace.json`):

- **`raw/clipped/tailscale.com/webpages/Making a Model Context Protocol server more robust, and much more private.md`** — **the single most useful file for this project.** Briggs (Tailscale Director of SE): MCP's stdio→SSE→Streamable HTTP evolution solved reach but not exposure; OAuth authenticates but does not *unexpose*; run the MCP server on the tailnet, use **Tailscale grants (`app: {"jaxxstorm.com/cap/mcp": [{"tools": ["*"], "resources": ["*"]}]}`)** for per-user, per-tool authorization by forwarding `X-Tailscale-User` from a proxy. Includes a working `tailscale-mcp` invocation (`TS_AUTHKEY=… ./tailscale-mcp --tailnet=… --api-key=…`), the proxy client config, and the caveat that **`tsnet` is Go-only** ("there is no official SDK" for Go MCP as of writing) — a Rust server would need `libtailscale` (C) or `tailscale-rs`. An **agent-added "Capture notes" section dated 2026-09-03** corrects the post: MCP **2025-06-18** made authorization normative (OAuth 2.1, server as resource server, RFC 9728 MUST for servers, RFC 8707 + PKCE MUST for clients) — but notes MCP's OAuth model *still* does not express "which tools may this identity call," so the grants idea is not superseded.
- **`raw/notes/Engineering/DevOps/Tailscale.md`** — the human's hands-on ops note; the best local source of **Tailscale API/CLI facts** a server must model: `tailscale up --auth-key/--advertise-tags/--ssh`, `tailscale status --json`, `tailscale debug prefs`, `tagOwners` + ACL/grants JSON shape, `POST /api/v2/device/<ID>/tags`, `POST /api/v2/device/<ID>/key {"keyExpiryDisabled": true}`, `GET /api/v2/tailnet/<tailnet>/devices`, auth-key basic-auth (`-u "tskey-api-xxxxx:"`), why `autogroup:self` breaks on tagged devices, macOS client variants (App Store build cannot run Tailscale SSH; CLI launcher at `/usr/local/bin/tailscale`).
- `raw/notes/Engineering/AI/LLM/MCP.md` — catalogue of MCP servers the human runs, with the **config-shape convention** (`~/.claude.json` `mcpServers` entries, stdio-via-docker vs `type: "http"` vs `type: "sse"`, `claude mcp add` / `add-json -s user`). Contains the design precedent worth citing: *"Unlike other Kubernetes MCP server implementations, this **IS NOT** just a wrapper around `kubectl`… It is a **Go-based native implementation** that interacts directly with the Kubernetes API server"* — the exact CLI-vs-API choice facing a Rust Tailscale server.
- `raw/agent-notes/agent-auth-middleware-analysis.md:54` — argues there is no good open-source personal auth proxy (unified callback URL + token vault + audit log) self-hosted on a Mac mini/VPS over Tailscale; relevant to how the server holds a Tailscale API key.
- `raw/agent-notes/macos-ssh-jumpbox-fleet.md` — unidirectional SSH jumpbox topology on a Tailscale Mac fleet; note that macOS cannot bind `sshd` to the Tailscale interface via `sshd_config` the way Linux can.
- `raw/agent-notes/macos-provision-omz-plus-cli-stack.md:63`, `raw/agent-notes/nickel/unison-bidirectional-sync.md:404`, `raw/clipped/onevcat.com/webpages/一个半月高强度 Claude Code 使用后感受.md:213` — incidental Tailscale-as-transport mentions, no design content.
- `raw/agent-notes/Last30Days/{rust-alternatives-to-tailscale-mcp,grok-bot,opensource-alternatives-to-rustdesk,omnara-happy-coder-mobile-clients-for-claude-code}-raw-v3.md` — only the first is on-topic.

### `grep -ril "rmcp\|model context protocol" wiki/`

**Zero hits.** `wiki/` holds only 6 pages (`index.md`, `log.md`, `habena.md`, `ccb-vs-herdr.md`, `agents-md-loading-order.md`, `ai-startup-name-affixes.md`). Loosening to `"mcp"` gives three incidental hits: `wiki/habena.md:35` (an unrelated npm package `habena` published 2026-06-10 is an MCP guardrails proxy — a naming collision, not design content) and `wiki/log.md` lines 17/33/53/55 (compilation-log mentions of MCP support in coding agents and cua-driver's MCP stdio registration). **The wiki has no compiled page on MCP, Tailscale, or Rust MCP servers** — this research is uncompiled.

One more source worth knowing about: **`DECISIONS.md` lines 155–200 (Q17/Q18/Q19)** records how the Briggs clip was ingested — it got its own graph community "Private MCP over Tailscale" (cid 453, 24 nodes, 63 internal vs 15 external edges), and the stale-auth-spec correction was encoded as a first-class `rationale_for` node rather than prose alone.

**Nothing was modified.**

## Task 3 — Distribution names, settled 2026-09-04

Every channel name for the first release, and the evidence for each.

| Channel | Name | Evidence |
| --- | --- | --- |
| crates.io, server binary | `tailscale-mcp` | free; `cargo info` (crates.io treats `-` and `_` as one name) |
| crates.io, REST client | `tailscale-rest` | free; `tailscale-api` and `tailscale-control` are taken |
| crates.io, CLI wrapper | `tailscale-cli` | free |
| npm launcher | `@tailscale-mcp/tailscale-mcp` | org `tailscale-mcp` created by the maintainer 2026-09-04; the scoped package returns 404, so the name is free |
| Docker | `ghcr.io/tailscale-mcp/tailscale-mcp` | GitHub org `tailscale-mcp` holds this repo |
| Homebrew | `tailscale-mcp/tap/tailscale-mcp` | tap to be created at release |
| MCP registry | `io.github.tailscale-mcp/tailscale-mcp` | derived from the GitHub org |

Unscoped `tailscale-mcp` on npm belongs to an unrelated project (itunified-io/mcp-tailscale) and `tailscale-mcp-server` is taken, which is why the launcher is scoped. The fallback name `tailscale-mcp-rs` is no longer needed.

**Publishing credentials, for the packaging milestone:** the npm token on the maintainer's machine authenticates as `soulmachine` but returns `403 You may not perform that action with these credentials` for `npm org ls tailscale-mcp`, i.e. it lacks org scope. Publishing under `@tailscale-mcp` will need a token with write access to that scope, which is a release-time task, not a build-time one.
