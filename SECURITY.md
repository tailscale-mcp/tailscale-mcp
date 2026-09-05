# Reporting a vulnerability

**Please report privately, not as an issue.** Use
[GitHub's private advisory form](https://github.com/tailscale-mcp/tailscale-mcp/security/advisories/new),
which is enabled on this repository and visible only to the maintainers.

A public issue here is a public disclosure to everyone already running the
published packages — npm, crates.io, the container image, the Homebrew tap and
the MCP registry all serve the same binaries — before there is a version to
move to. The private form avoids that; a fix and an advisory can then go out
together.

Please include what you did, what happened, and what you expected. A failing
test case against the in-process harness is the fastest possible report, but is
not a requirement.

## What is in scope

This server holds a control-plane credential and can change a tailnet, so the
things worth reporting are the ones that break the boundaries it claims:

- A credential reaching somewhere it should not — a log line, an error message,
  a tool result, a crash dump. `tskey-` values are meant to be redacted
  wherever they appear.
- Anything written to standard output that is not the protocol. The transport
  is stdio, so a stray `print` is not a cosmetic bug: it corrupts the session.
- A tool acting outside the tier it is annotated with: a read-only tool that
  writes, or a destructive one offered without `--allow-destructive`.
- A way to bypass the checksum the npm launcher verifies before running a
  downloaded binary, or anything else that would let a client run something the
  release's own `SHA256SUMS` does not vouch for.
- A path that severs the node's connectivity without the refusal that is
  supposed to catch it.

## What is not

- **Vulnerabilities in Tailscale itself** — report those to
  [Tailscale](https://tailscale.com/security), not here. This project drives
  the `tailscale` CLI and the public API and can only ask for what they offer.
- **A valid credential doing what it is entitled to.** A caller holding a
  tailnet-write token can change the tailnet; that is the feature. The
  interesting question is only ever whether the tiers, presets and switches
  that gate it can be got around.
- **Anything already requiring control of the machine.** Somebody who can run
  processes as you can read the credential from the environment regardless.

## Supported versions

The newest release. Fixes go out as a new version rather than as patches to an
old one; there is no long-term-support line.

## What to expect

An acknowledgement within a few days. This is a small project without a
security team, so please say if you have a disclosure deadline in mind and it
will be worked to rather than argued with.
