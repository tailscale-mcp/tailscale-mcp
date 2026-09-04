# Configuration

Every setting has an environment variable, and most have a command-line flag as
well. An MCP client launches this server from a JSON file that sets environment
variables and often cannot pass arguments at all, which is why the environment
is the primary form; where both are given, the flag wins.

Nothing here is required. With no configuration the server offers the `core`
preset at the read tier, over stdio, with whichever surfaces it can reach: the
`tailscale` binary for the tools that drive this node, a control-plane
credential for the tools that drive the tailnet. A surface that is missing is
not an error — its tools are not offered, and `tailscale-mcp diagnose` says
which of the two this machine has.

## Settings

| Environment variable | Flag | Default | What it does |
|---|---|---|---|
| `TAILSCALE_MCP_PRESET` | `--preset` | `core` | Which group of toolsets to start from: `minimal`, `core` or `full`. See [tools.md](tools.md) for what each contains. |
| `TAILSCALE_MCP_TOOLSETS` | `--toolsets` | the preset's selection | Toolsets to offer, comma-separated. A bare list replaces the preset's selection; a list where every entry begins with `+` or `-` adjusts it, so `+local-debug` adds one to whatever the preset chose. |
| `TAILSCALE_MCP_ALLOW_WRITE` | `--allow-write` | off | Offer the tools that change configuration. Without it the server is read-only. |
| `TAILSCALE_MCP_ALLOW_DESTRUCTIVE` | `--allow-destructive` | off | Offer the tools that remove or expose something. Implies the write tier. |
| `TAILSCALE_MCP_NO_LOCAL` | `--no-local` | off | Do not offer the tools that drive this node, even where the `tailscale` binary is present. |
| `TAILSCALE_MCP_NO_TAILNET` | `--no-tailnet` | off | Do not offer the tools that drive the tailnet, even where a credential is present. |
| `TAILSCALE_MCP_CLI_PATH` | `--cli-path` | found on `PATH`, then in the macOS application bundles | Where the `tailscale` binary is. |
| `TAILSCALE_MCP_MAX_RESULT_BYTES` | `--max-result-bytes` | `1048576` (1 MiB) | Refuse a tool result larger than this, with `result_too_large`, rather than sending it to a model that cannot hold it. |
| `TAILSCALE_MCP_LOG` | `--log` | `warn,tailscale_mcp=info` | Logging filter, in the `tracing` syntax. Logs go to standard error. The MCP SDK is held to `info` unless the filter names it, because it traces whole messages — secrets included — at `debug`. |
| `TAILSCALE_MCP_HTTP_TOKEN` | none | none | The bearer token an HTTP caller must present. It has no flag on purpose: an argument is readable by every process on this machine. |
| `TAILSCALE_MCP_HTTP_NO_AUTH` | `--http-no-auth` | off | Serve HTTP with no token at all. Binding anywhere but loopback needs either this or a token. |
| `TAILSCALE_MCP_HTTP_STATELESS` | `--http-stateless` | off | Serve HTTP without sessions, where the negotiated protocol version still has them. From protocol version 2026-07-28 there are no sessions and this changes nothing. |
| `TAILSCALE_MCP_API_BASE_URL` | none | `https://api.tailscale.com` | Where the control-plane calls go. It exists so the test suite can reach a fake and so a staging control plane can be reached; an address is accepted only over HTTPS or to this machine, and never with a username or password in it. |
| none | `--http` | off — stdio | Serve MCP over Streamable HTTP at this address instead of over stdio. `127.0.0.1:8449` when the flag is given with no address. |
| none | `--http-allow-host` | loopback and this node's own tailnet names | Also answer for this `Host` header. Repeatable. |
| none | `--http-allow-origin` | none — a request carrying any `Origin` is refused | Answer requests from this browser origin. Repeatable. |
| none | `--help` | — | Print help. `-h` is the summary, `--help` the long form. |
| none | `--version` | — | Print the version and the protocol versions this server speaks. |

## Credentials

The tools that drive the tailnet need a control-plane credential. Three shapes
are accepted, in this order of precedence; the first one that is complete is
the one used. None has a command-line form, because a secret on an argument
list is visible to every process on the machine.

| Environment variable | Flag | Default | What it does |
|---|---|---|---|
| `TAILSCALE_API_KEY` | none | none | An API access token, sent as a bearer token. It belongs to a user and carries that user's permissions, and it expires. |
| `TAILSCALE_OAUTH_CLIENT_ID` | none | none | An OAuth client's id, used with the secret below. The client belongs to the tailnet rather than to a user, does not expire on its own, and mints short-lived tokens limited by its scopes — the better choice for anything long-lived. |
| `TAILSCALE_OAUTH_CLIENT_SECRET` | none | none | The secret that goes with the OAuth client id. |
| `TAILSCALE_OAUTH_SCOPES` | none | whatever the client itself carries | Scopes to ask for when minting a token, comma- or space-separated. |
| `TAILSCALE_OAUTH_JWT_FILE` | none | none | A file holding a JWT for federated identity, read when there is no API access token and no OAuth client. |
| `TAILSCALE_TAILNET` | none | `-`, the tailnet the credential belongs to | The tailnet the control-plane tools act on. |

A secret is never written to a log, never put on an argument list, and never
echoed back in an error. Where a tool mints one — an auth key, an OAuth client
secret, an invite URL — it is returned once, verbatim, and nothing keeps a
copy.

## Subcommands

Given a subcommand this binary answers a question instead of serving.

| Subcommand | What it does |
|---|---|
| `diagnose` | Check the `tailscale` binary, the credential and the control plane, and report each. `--json` for a machine. Exits non-zero when a check fails. |
| `tools` | Print what this preset and tier would offer, without serving. `--json` for a machine. |
| `version` | Print this server's version and the protocol versions it speaks. |
| `setup <client>` | Print a configuration snippet for an MCP client: `claude-code`, `claude-desktop`, `vscode`, `cursor` or `zed`. Writes nothing. |
| `policy check <file>` | Ask the control plane whether it would accept this policy file. Changes nothing. |
| `policy deploy <file>` | Write this file as the tailnet policy, guarded by the version identifier read immediately before. |

## Serving over HTTP

`--http` serves Streamable HTTP instead of stdio. The defaults are the
cautious ones: loopback, a bearer token required, no browser origin allowed,
and only this machine's own names answered for. Widening any of those is a
flag, and binding beyond loopback without a token needs `--http-no-auth`
said out loud.

See [the README's security section](../README.md#security) for what that does
and does not protect against.
