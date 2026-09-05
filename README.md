# tailscale-mcp

An MCP server for [Tailscale](https://tailscale.com). The node it runs on is
driven through the `tailscale` command-line interface; the tailnet behind it is
driven through the control-plane REST API.

[186 tools](docs/tools.md), one per verb, with real parameters and honest
annotations. Tailscale's own JSON comes back unmodified, so anything you
learned from Tailscale's documentation still applies. When something fails you
get a fixed error code and a hint, not a wall of standard error.

Both surfaces are optional and neither is fatal. No `tailscale` binary means the
tools that drive this node are not offered; no control-plane credential means the
tools that drive the tailnet are not. `tailscale-mcp diagnose` says which of the
two this machine has.

## Install

| Channel | How |
|---|---|
| npm | `npx -y @tailscale-mcp/tailscale-mcp` — downloads the release binary for your machine and refuses to run it unless the release's own `SHA256SUMS` vouches for it |
| Container | `docker run -i --rm -e TAILSCALE_API_KEY ghcr.io/tailscale-mcp/tailscale-mcp` |
| Homebrew | `brew install tailscale-mcp/tap/tailscale-mcp` |
| Bundle | Download the `.mcpb` for your platform from the [releases](https://github.com/tailscale-mcp/tailscale-mcp/releases) and open it — for MCP clients that install bundles, such as Claude Desktop |
| From source | `cargo install tailscale-mcp` |

Release archives are on the
[releases page](https://github.com/tailscale-mcp/tailscale-mcp/releases) with a
`SHA256SUMS` beside them: macOS and Linux on `x86_64` and `arm64`, and Windows
on `x86_64`.

## Point a client at it

```sh
tailscale-mcp setup claude-code
```

prints the snippet for your client — `claude-code`, `claude-desktop`, `vscode`,
`cursor` or `zed` — and says where it goes. It writes nothing: the snippet is
yours to paste, and it leaves the credential out, for the reason in the next
section.

Installed through npm there is nothing to install first, and the client can
carry the settings:

```jsonc
{
  "mcpServers": {
    "tailscale": {
      "command": "npx",
      "args": ["-y", "@tailscale-mcp/tailscale-mcp"],
      "env": {
        "TAILSCALE_MCP_ALLOW_WRITE": "true"
      }
    }
  }
}
```

That much is enough to start it. Without a credential it offers the tools that
drive this node and hides the ones that act on the tailnet, and
`tailscale-mcp diagnose` says which of the two you have.

## Give it a credential

`setup` prints no credential of its own, because the file it prints into is one
people paste into issues and chats without rereading. Adding it is a separate,
deliberate step — the [credentials
table](docs/configuration.md#credentials) has every accepted shape.

An OAuth client is the one to reach for, and the reason is what a client
configuration *is*: a file somebody writes once and then forgets, holding a
secret for as long as the tool is installed.

An API access token suits that badly on three counts. It belongs to a person
and carries everything that person can do, so what leaks with the file is their
whole account rather than the tools you turned on. It expires — which in a file
nobody has looked at since they wrote it does not present as an expired
credential, but as a server that has stopped working for no reason. And it is
itself the bearer token, so the thing at rest is the thing that opens the door.

An OAuth client inverts each of those. It belongs to the tailnet, so it outlives
whoever set it up and is revoked without touching that person's access. Its
scopes narrow it to what the toolsets you enabled actually call. It does not
expire on its own, and what sits in the file is not a key but the means of
minting one — the token it hands this server lasts an hour, so a copy taken from
a backup or a screen share is worth very little by the time it is used.

So the `env` block above becomes:

```jsonc
"env": {
  "TAILSCALE_OAUTH_CLIENT_ID": "k123456CNTRL",
  "TAILSCALE_OAUTH_CLIENT_SECRET": "tskey-client-…",
  "TAILSCALE_MCP_ALLOW_WRITE": "true"
}
```

An API access token goes in `TAILSCALE_API_KEY` instead, and is what the
control plane hands you first — reasonable for trying this out, and worth
replacing with the above once it is something you keep.

## Tiers and presets

Two independent dials decide what a session offers.

**The tier** is how dangerous a tool is allowed to be. Read tools change nothing
and are always offered. Write tools change configuration that can be changed
back. Destructive tools remove something or expose something in a way that is
not simply undone — deleting a device, revoking a key, publishing a service to
the internet. Read is always on; write and destructive are off until they are
turned on, and a tool above the permitted tier is *not listed*, not refused
when called.

**The preset** is how much surface area is offered at all. `minimal` is what an
agent needs to answer questions and fix the common things; `core` adds the rest
of everyday administration; `full` adds the tailnet-wide and irreversible
corners. Two toolsets are in no preset and must be asked for by name:
`local-debug`, which is Tailscale's own diagnostic surface, and
`local-passthrough`, which is one tool that runs an arbitrary `tailscale`
subcommand.

| Preset | Read | With `--allow-write` | With `--allow-destructive` | Toolsets |
|---|---|---|---|---|
| `minimal` | 37 | 51 | 55 | 4 |
| `core` (default) | 57 | 106 | 126 | 13 |
| `full` | 68 | 126 | 155 | 18 |

Adding `--toolsets +local-debug,+local-passthrough` to `full` reaches all 186.

Some destructive tools ask for one more thing: a `confirm: true` argument. Those
are the ones that affect the whole tailnet, or that can cut this server off from
the node or tailnet it is driving — logging the node out, deleting its own
device, deploying a policy that locks the caller out. The server does not decide
they are wrong; it makes the caller say it meant them.

- **[docs/tools.md](docs/tools.md)** — every tool, its tier and what it does.
  Generated from the code.
- **[docs/configuration.md](docs/configuration.md)** — every environment
  variable and flag, with defaults.
- **[docs/errors.md](docs/errors.md)** — every error code and what to do about
  it.

## Resources and prompts

Nine resources — eight fixed and one template addressed by device identifier —
give a client the local node's status, preferences, netcheck report and tailnet
lock state, and the tailnet's policy file, devices, DNS configuration and
settings. They are read-only, they appear only when their surface is on, and
there are no subscriptions.

Three prompts steer a sequence of tool calls: `diagnose_connectivity`,
`review_policy_change` and `audit_tailnet_access`.

## Transports

Stdio by default. `--http` serves Streamable HTTP instead, on
`127.0.0.1:8449`, behind a bearer token in `TAILSCALE_MCP_HTTP_TOKEN`, with
host and origin allow-lists, a body limit, a per-address rate limit and an open
health endpoint. Binding anywhere but loopback needs either that token or
`--http-no-auth` said out loud.

## Security

**What the tiers do.** A tool above the permitted tier is not in the tool list,
so a caller cannot invoke it by guessing its name, and a model cannot be talked
into one that was never offered. The default is read-only. `confirm` on top of
the destructive tier is a second signal for the operations that are worth one.
Secrets never reach an argument list, a log line or an error message; a minted
key or invite URL is returned once, verbatim, and nothing here keeps a copy.
Every tool result and every error goes through the same redaction on the way
out.

**What the tiers do not do.** They are not an authorization system, and they are
not a sandbox.

- A tier is a property of the *server*, not of the caller. Everyone talking to
  one server gets the same tools. If two callers should have different powers,
  run two servers.
- Permitting a tier permits every tool in it. There is no per-tool switch;
  `--toolsets` is the coarser dial for narrowing what a session can reach.
- The credential is the real boundary. A read-only OAuth client makes the
  tailnet write tools fail at the control plane whatever this server offers,
  and that is the boundary to lean on. Scope the credential, and use the tiers
  to stop honest mistakes.
- The local tools run as whoever runs this server. Tailscale's own operator
  check applies — commands refused to a non-operator are refused here, with
  `needs_operator` — but a server run as the operator can do what the operator
  can do.
- Read is not harmless. The read tier returns real network topology: node names,
  addresses, users, policy. Anything that can read the tool output can read
  that.

**The server never escalates privileges.** It does not use `sudo`, does not ask
for elevation, and does not alter its own permissions or install anything. It
runs the `tailscale` binary as the user it was started as and sends the
control-plane credential it was given. Where Tailscale refuses because of who is
asking, that refusal is passed through rather than worked around. Four tools do
write files: three of them — a certificate, a Taildrop delivery, a metrics file
— to the path the call names, and `tailscale_configure_kubeconfig` to the
kubeconfig the client itself would edit. All four write as the user running the
server, and so does `tailscale_run`, which is in no preset and, once asked for
by name, can reach those same writes and anything else the `tailscale` binary
does.

Over HTTP the defaults are the cautious ones — loopback, a token required, no
browser origin allowed — and each of those is widened by a flag, deliberately.

## Compared with the other Tailscale MCP servers

This server is a strict superset of the three that came before it
([rtailscale](https://github.com/dinglebear-ai/rtailscale),
[HexSleeves/tailscale-mcp](https://github.com/HexSleeves/tailscale-mcp),
[YawLabs/tailscale-mcp](https://github.com/YawLabs/tailscale-mcp)), with four
exceptions listed below the table.

| Capability | rtailscale | HexSleeves | YawLabs | this server |
|---|---|---|---|---|
| Devices: list, get | yes | list only | yes | yes, within `tailnet-devices` below |
| Devices: authorize, delete, expire, rename, set IP, key expiry | authorize, delete | authorize, delete, expire | yes | yes, [15 tools](docs/tools.md#tailnet-devices) in all |
| Device routes, tags | routes get | yes | yes | yes, within `tailnet-devices` below |
| Posture attributes | — | — | yes | yes, within `tailnet-devices` below |
| Policy file: get, set, preview, validate | get | get, set, validate | yes, with `If-Match` | yes, with the version identifier on every write |
| DNS: nameservers, preferences, search paths, split DNS | get | yes | 11 tools | [11 tools](docs/tools.md#tailnet-dns) |
| Auth keys | list | yes | yes | [5 tools](docs/tools.md#tailnet-keys) |
| OAuth clients | — | — | 4 tools | [5 tools](docs/tools.md#tailnet-oauth-apps) |
| Users | list | — | 7 tools | [7 tools](docs/tools.md#tailnet-users) |
| Tailnet settings, contacts | — | partly | 5 tools | [5 tools](docs/tools.md#tailnet-settings) |
| Webhooks | — | yes | 7 tools | [7 tools](docs/tools.md#tailnet-webhooks) |
| Posture integrations | — | — | 5 tools | [5 tools](docs/tools.md#tailnet-posture) |
| Audit and network flow logs, log streaming | — | — | 9 tools | [8 tools](docs/tools.md#tailnet-logging) |
| Invites: device and user | — | — | 11 tools | [11 tools](docs/tools.md#tailnet-invites) |
| Services | — | — | 7 tools | [7 tools](docs/tools.md#tailnet-services) |
| Organization tailnets | — | — | 3 tools | [3 tools](docs/tools.md#tailnet-org) |
| Local status, IP, whois, whoami, version | — | status, version | 4 tools | [25 tools](docs/tools.md#local-status) |
| Ping, netcheck, routecheck, DNS query, exit nodes | — | ping, exit nodes | ping, netcheck | yes, within `local-status` above |
| Preferences, up, down, login, logout, profiles | — | up, down | — | [8 tools](docs/tools.md#local-prefs) |
| Serve and funnel | — | — | — | [10 tools](docs/tools.md#local-serve) |
| Taildrive, file transfer, certificates, kubeconfig | — | — | — | [11 tools](docs/tools.md#local-files) |
| Tailnet lock | — | stub | — | [8 tools](docs/tools.md#local-lock), plus status and log |
| Tailscale's own debug surface | — | — | — | [30 tools](docs/tools.md#local-debug) |
| Arbitrary `tailscale` subcommand | — | — | — | [1 tool](docs/tools.md#local-passthrough), off by default |
| Resources | 1 | 4 | 4 | 9 |
| Prompts | 1 | 2 | 0 | 3 |
| Transports | stdio, HTTP | stdio, HTTP | stdio | stdio, HTTP |
| Tool count | 1, with 10 actions | 19, one of them a stub | 102 | [186 tools](docs/tools.md) |

One row is lower than YawLabs' and no capability is missing behind it: two of
their tools are aggregates over endpoints this server exposes one at a time —
one that reads both log types' stream configurations in a single call, and one
that authorizes a list of devices in a single call. Both are reachable here as
repeated calls to the per-item tool. They are counted in their column and not
in this one, which is what makes the logging row read 9 against 8.

Four things the others have and this does not, deliberately:

1. **No configuration file.** rtailscale reads a `config.toml`. Everything here
   is an environment variable or a flag, because that is what an MCP client
   configuration can set, and a third source of truth is a third place for a
   setting to hide.
2. **No tool-schema resource.** rtailscale serves its own tool schema as a
   resource. MCP already has `tools/list`; a second copy is one that can
   disagree with the first.
3. **No OAuth resource-server mode for browser clients**, in this release.
   rtailscale can act as an OAuth resource server so a browser client can
   authenticate to it. The HTTP transport here takes a bearer token. This is a
   deferral, not a refusal.
4. **No extra-enum environment knobs.** Several settings in the others accept
   values outside the documented set, or are read from variables that are not
   documented at all. Every setting here is in
   [docs/configuration.md](docs/configuration.md), and a value outside the set
   is a startup error naming the alternatives.

## Development

```sh
cargo test --workspace --all-targets     # the whole suite, offline
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The suite needs no `tailscale` binary, no credential and no network: the
`tailscale` binary is faked and the control plane is a fake HTTP server. The end-to-end
tests that do want a real node and tailnet are gated on environment variables
and report themselves skipped without them.

`docs/tools.md` is generated. After adding or changing a tool:

```sh
UPDATE_DOCS=1 cargo test -p tailscale-mcp --test docs_are_current
```

Architecture decisions are in [`docs/adr/`](docs/adr/), the vocabulary this
codebase holds itself to is in [`CONTEXT.md`](CONTEXT.md), every judgement call
made while building it is in [`DECISIONS.md`](DECISIONS.md), and how a release
is made is in [`RELEASING.md`](RELEASING.md).

## Licence

Apache-2.0.
