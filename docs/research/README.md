# Research notes

Fact-finding gathered during the design interview on 2026-09-03, before any code existed. These are
inputs to the design, not the design itself: the glossary lives in `CONTEXT.md`, decisions in
`docs/adr/`. Paths written as `<scratchpad>/…` inside the notes point at a session scratchpad that no
longer exists; everything worth keeping from it was copied here.

| File | What it is |
|---|---|
| `control-plane-api.md` | Every operation of the control-plane REST API v2 (93 operations, 60 paths), classified read / write / destructive, with scopes, quirks, and drift against the official Go client |
| `tailscale-openapi.yaml` | The upstream OpenAPI 3.1 schema as served by `https://api.tailscale.com/api/v2?outputOpenapiSchema=true` on 2026-09-03 (ETag `30c73c46…ae0eb505`), kept verbatim for model generation and drift tests |
| `tailscale-cli.md` | Every command of the local `tailscale` CLI (1.102.2, 129 command nodes, hidden ones included) with flags, output shapes, risk class, blocking behaviour, privilege needs, and recommended exclusions |
| `tailscale-cli/help/` | Verbatim `--help` output of every CLI command, one file per command path, plus `tailscale-cli/json-docs.json`, the visible command tree from the hidden `--json-docs` root flag |
| `rmcp-sdk.md` | The official Rust MCP SDK (`rmcp` 3.2.0): feature flags, macros, transports, structured output, error model, verified probe code |
| `reference-implementations.md` | What rtailscale, HexSleeves/tailscale-mcp, and YawLabs/tailscale-mcp each cover, how they are built, and what to copy or avoid |
| `ecosystem.md` | Licences, alternatives, authorization patterns (Tailscale grants), and community concerns around Tailscale MCP servers |
