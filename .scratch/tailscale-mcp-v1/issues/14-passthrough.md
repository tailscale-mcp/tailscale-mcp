# 14 — Passthrough tool

Status: ready-for-agent
Milestone: 2 — Local surface
Blocked by: 13

The single escape hatch that runs an arbitrary `tailscale` subcommand, off by default and enabled by one switch that is equivalent to adding its toolset. It takes an argument array and never a shell string.

It has no fixed tier: it inherits the tier of the typed tool covering the same subcommand, an unknown subcommand counts as destructive, and an excluded command is refused with the permission code. Arguments are logged with secrets redacted.

## Acceptance criteria
- With the read tier only, a status subcommand runs and a down subcommand is refused.
- An unknown subcommand is treated as destructive and refused unless that tier is enabled.
- Every excluded command is refused, verified by a test enumerating the exclusion list.
- No shell is involved: a string containing shell metacharacters is passed through as a literal argument.
