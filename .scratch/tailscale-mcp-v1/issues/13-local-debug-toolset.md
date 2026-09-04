# 13 — Debug toolset

Status: ready-for-agent
Milestone: 2 — Local surface
Blocked by: 08

The 30 tools of the opt-in debug toolset: 22 readers and 8 runtime knobs. The toolset is off in every preset and must be added explicitly. Readers are read tier; knobs are write tier, so they need both the toolset and the tier.

The unstable and dangerous debug members are excluded entirely and are not reachable here or through the passthrough. The event-watching reader takes a required count.

## Acceptance criteria
- No debug tool appears under any preset unless the toolset is added explicitly.
- The eight knobs are absent unless the write tier is also enabled.
- The excluded debug members are not registered and are refused by the passthrough.
- The event watcher honours its count and cap and always returns.
