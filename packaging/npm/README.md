# @tailscale-mcp/tailscale-mcp

An MCP server for [Tailscale](https://tailscale.com). The local node is driven
through the `tailscale` command-line interface; the tailnet is driven through
the control-plane REST API.

This package carries no binary. On its first run it downloads the release
archive for your machine, checks it against the release's own `SHA256SUMS`, and
refuses to run anything that does not match. The verified binary is cached, so
the download happens once.

```jsonc
{
  "mcpServers": {
    "tailscale": {
      "command": "npx",
      "args": ["-y", "@tailscale-mcp/tailscale-mcp"],
      "env": {
        "TAILSCALE_API_KEY": "tskey-api-…"
      }
    }
  }
}
```

Without a control-plane credential only the tools that drive this node are
offered; without the `tailscale` command-line interface installed, only
the tools that drive the tailnet are. Tools are read-only until a tier is permitted:
`TAILSCALE_MCP_ALLOW_WRITE` adds the ones that change configuration, and
`TAILSCALE_MCP_ALLOW_DESTRUCTIVE` the ones that remove or expose something.

`npx @tailscale-mcp/tailscale-mcp diagnose` reports what this machine has, and
`… setup claude-code` prints a configuration snippet.

Binaries are published for macOS, Linux and Windows on `x86_64` and `arm64`.
The server is also distributed as a container image, a Homebrew formula and an
MCP bundle; see the
[repository](https://github.com/tailscale-mcp/tailscale-mcp) for all of them.

Apache-2.0.
