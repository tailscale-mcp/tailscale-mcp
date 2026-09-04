# 09 — Local preferences toolset

Status: done
Milestone: 2 — Local surface
Blocked by: 08

The 8 tools that read and change the local node's preferences, plus connecting, logging in, logging out and switching profiles. The preference-setting tool changes only what it is given. The connect tool mirrors the same parameters plus authentication, timeout and reset, and its description directs callers to the preference tool on an already-connected node.

Platform-specific flags stay in the schema with the platform named and are rejected before spawning. Down, logout, re-authentication and reset are self-severing and require a confirmation; the CLI's own risk acceptance is passed only on a confirmed call.

## Acceptance criteria
- Setting one preference leaves the others untouched, verified against the backend's recorded argument list.
- A Linux-only flag on macOS produces the platform code without spawning.
- Down, logout, re-authenticate and reset refuse without a confirmation and succeed with one.
- An auth key supplied as a literal or as a file reference both work, and neither appears in the argument list.
