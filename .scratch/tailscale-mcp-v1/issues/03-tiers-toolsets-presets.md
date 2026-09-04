# 03 — Tiers, toolsets and presets

Status: ready-for-agent
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
