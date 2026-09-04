# 03 — Tiers, toolsets and presets

Status: done
Milestone: 1 — Skeleton and core
Blocked by: 02

Gate the tool list by tier and toolset. Read is the default tier; write and destructive are each enabled by their own flag. Tools outside the permitted tiers are hidden from the listing rather than listed and refused.

Toolsets are the agreed groups across both surfaces. Three presets select them, with `core` the default, and an environment variable applies additive and subtractive modifiers to a preset. A configuration that would expose no tools is a startup error. The server's instructions field explains the tiers and presets so a client's model knows why a tool it expects is absent.

## Acceptance criteria
- With no flags, the listing contains only read-tier tools from the default preset, and the counts match the agreed table.
- Enabling a tier or a toolset changes the listing accordingly; a subtractive modifier removes a toolset from a preset.
- A tool hidden by tier is absent from the listing and also refuses to run if called directly.
- Starting with every toolset removed exits with a diagnostic rather than serving zero tools.
- The instructions field is present and mentions the tier flags.

## As built

Built with tickets 01–06 in one commit, `9d56516 Skeleton, core plumbing and
server bootstrap (tickets 01-06)`, because the six are one another's
prerequisites: a tool cannot be declared without a tier to declare, a tier is
unobservable without an error model, and none of it runs without a way to
execute a process. That commit's message is the record of what was built; no
ticket in the group wrote an "As built" section, and the practice started at
ticket 11. The status is corrected here rather than the prose invented after
the fact.
