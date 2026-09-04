# 08 — Local status toolset

Status: ready-for-agent
Milestone: 2 — Local surface
Blocked by: 07

The 25 read-tier tools of the status toolset: node and peer status, network condition report, addresses, DNS query and status, ping, route check, exit node listing and suggestion, WHOIS, lock status where read-only, version, and the remaining inspection commands. Structured output is used wherever the CLI offers it; the rest are parsed from text with the parser isolated per command.

Bounded forms apply here: probe counts and per-probe timeouts carry the agreed defaults and caps.

## Acceptance criteria
- All 25 tools are listed at the read tier under the default preset.
- Where the CLI offers structured output it is forwarded unmodified; where it does not, the parser has a fixture-driven test.
- Ping honours its default and cap, and a bounded call always returns.
- Commands that exit non-zero in a normal condition are reported as success with their status, not as failures.
