# 06 — Server bootstrap and version probe

Status: done
Milestone: 1 — Skeleton and core
Blocked by: 03, 04, 05

Wire the SDK handler: server information, instructions, the tool router built from the metadata table, and the stdio transport as the default. Detect the two surfaces at startup and honour the flags that disable each.

Probe the CLI version at startup and warn when it is below the supported floor, without hiding any tool; an unsupported subcommand or flag is reported at call time with the version code and the minimum version. Nothing is ever written to standard output on the stdio transport.

## Acceptance criteria
- A client connects over stdio, receives the server information and instructions, and lists tools.
- With the CLI absent, the local surface is disabled and the tailnet tools still work, and the reverse holds with credentials absent.
- The version probe warns on standard error, and the warning does not corrupt the stdio protocol.
- An unknown subcommand or flag produces the version code carrying the minimum version.
- Establish and record the supported version floor from upstream changelogs as part of this ticket.
