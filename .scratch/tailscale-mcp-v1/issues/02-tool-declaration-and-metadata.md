# 02 — Tool declaration and the metadata table

Status: done
Milestone: 1 — Skeleton and core
Blocked by: 01

One declaration per tool that expands to its parameter type, its JSON schema, its argv or request builder, and a row in a metadata table. The table is the single source read by the tool-listing subcommand, the contract tests and the README's tool table, so a tool cannot exist without being classified.

A row carries: tool name, surface, toolset, tier, annotations, whether it is self-severing, whether it requires confirmation, and its minimum Tailscale version where one applies. Parameter naming follows ADR-0004: server-owned parameters in snake_case, Tailscale's own bodies unrenamed.

## Acceptance criteria
- Declaring a tool without a tier or toolset fails to compile.
- The metadata table is enumerable at runtime and is what the router is built from, not a parallel list.
- A test asserts that every registered tool has exactly one metadata row and that names are unique, match the permitted character set, and are within the length limit.
- Schema generation produces the documented parameter names for both naming conventions in one tool.

## As built

Built with tickets 01–06 in one commit, `9d56516 Skeleton, core plumbing and
server bootstrap (tickets 01-06)`, because the six are one another's
prerequisites: a tool cannot be declared without a tier to declare, a tier is
unobservable without an error model, and none of it runs without a way to
execute a process. That commit's message is the record of what was built; no
ticket in the group wrote an "As built" section, and the practice started at
ticket 11. The status is corrected here rather than the prose invented after
the fact.
