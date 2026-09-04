---
status: accepted
---

# The first release is 1.0.0, not 0.1.0

A pre-1.0 version number would have signalled that the tool names, parameters and error codes might still move, and that is the conventional hedge for a first release. We ship 1.0.0 instead. The interface is not exploratory: every tool name, parameter name and error code was decided before any code was written, the parameter convention is fixed by ADR-0004, and the surface is derived from two interfaces that are themselves stable and versioned. An MCP server's tool schemas are a contract with saved prompts and client configurations, so a version number that invites breaking changes invites them into other people's setups. Semver still permits additive change: new tools, toolsets and optional parameters are minor releases.

## Considered options
- **0.1.0.** Room to rename things after real use, at the cost of telling every early adopter the surface is provisional.
- **1.0.0.** Chosen.

## Consequences
- Renaming a tool, a parameter or an error code now requires a major release, so the naming conventions have to be right the first time.
- Coverage gaps are filled in minor releases, because adding a tool breaks nothing.
- Tailscale's own additive API changes stay minor releases too, which is what the unknown-field retention in ADR-0003 is for.
