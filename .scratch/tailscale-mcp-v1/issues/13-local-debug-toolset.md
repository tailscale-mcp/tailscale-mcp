# 13 — Debug toolset

Status: done
Milestone: 2 — Local surface
Blocked by: 08

The 30 tools of the opt-in debug toolset: 22 readers and 8 runtime knobs. The toolset is off in every preset and must be added explicitly. Readers are read tier; knobs are write tier, so they need both the toolset and the tier.

The unstable and dangerous debug members are excluded entirely and are not reachable here or through the passthrough. The event-watching reader takes a required count.

## Acceptance criteria
- No debug tool appears under any preset unless the toolset is added explicitly.
- The eight knobs are absent unless the write tier is also enabled.
- The excluded debug members are not registered and are refused by the passthrough.
- The event watcher honours its count and cap and always returns.

## As built

Thirty tools in `local_debug.rs`, split 22 readers and 8 knobs exactly as the
ticket asks, but not quite from the members it assumed. `debug prefs` was on the
research table's keep list and prints the node's private keys, so it is excluded
(DECISIONS Q45) and `debug --file=get` takes the 22nd reader's place, listing the
Taildrop inbox that no other tool describes (DECISIONS Q46). `spec.md`'s totals
(186 tools, 30 debug) are unchanged.

The knobs run on the shared mutation lane rather than the exclusive one, which is
what that lane was described for: they change a peer or the node's transient
runtime state, never what `set` and `up` contend over.

`pub const EXCLUDED` lists the 14 members that never become tools, each with the
reason a caller will be shown. It is public because the second half of the third
acceptance criterion — the passthrough refusing them — belongs to ticket 14,
which consumes this table rather than repeating it. What this ticket proves is
the first half: `no_excluded_debug_subcommand_is_reachable_as_a_tool` walks
`EXCLUDED` and asserts each name is neither offered nor callable. The code is
`not_found` rather than `not_permitted`, because a gated tool exists and some
switch would reach it, while these do not exist at all.

`debug reload-config` is in neither list, deliberately: it re-reads a
configuration file this server has never seen, so no honest tier or summary could
be written for it, but it stays a legitimate operation for whoever owns that file
and the passthrough may still run it (DECISIONS Q44).

The watcher takes a count it caps at 100 and a wall-clock bound it clamps to
1..=300s. Reaching the bound first comes back as a timeout carrying what arrived,
following the road `tailscale_funnel` already takes for a foreground command
(DECISIONS Q47); `count: 0` is refused, because the client reads zero as "never
stop". `tailscale_debug_portmap` is bounded the same way against its own default.
`component_logs` gets no duration ceiling: the client has none, and ticket 12's
`MAX_DISABLEMENTS` was the lesson about inventing one.

Two commands have their own stated constraints enforced here rather than left to
fail downstream: `portmap`'s `--gateway-addr` and `--self-addr` must be given
together, and `via`'s two positional forms are told apart before anything runs.

`tests/fixtures_are_redacted.rs` gained `is_via_route`, so that a 4via6 route in
a fixture is not read as a node address: `fd7a:115c:a1e0:b1a::/64` is the block
reserved for 4via6, and nothing the control plane assigns lives in it. The
node-address check is untouched.

`/code-review` then found, and this ticket fixed:

- Three parameters named for what they mean rather than for the flag behind
  them, against `spec.md`'s rule that a CLI flag becomes a parameter of the same
  name in snake_case. `portmap`'s `--duration` and `--type` and
  `component-logs`' `--for` are now `duration_seconds`, `r#type` and
  `for_seconds`: the flag's own name, with `local_status.rs`'s
  `--timeout` → `timeout_seconds` precedent for the two Go durations, and a raw
  identifier for the one that collides with a keyword. `r#type` reaches the
  schema as `"type"` and deserialises from it, which a test now holds down
  rather than leaving to a reader's faith in serde.
- `watch-ipn`'s six `--initial-*` flags withheld along with bare `--initial`.
  Only the bare form dumps Prefs and so prints keys; the six narrow ones each
  ask for a single field and carry none, so the exclusion had no ground under
  `CONTEXT.md` and contradicted the flag rule. They are ordinary parameters now
  (DECISIONS Q45).
- Nine report structs deriving `Debug, Serialize` where all 34 of their siblings
  derive `JsonSchema` as well. Not load-bearing today — the `tools!` macro
  generates no output schema — but a convention with one exception is a trap for
  whoever adds output schemas, so the exception is gone.
- A hand-rolled knob helper folding stdout and stderr itself, next to
  `common::printed` doing the same thing with redaction. Replaced. The same read
  found `stat` and `resolve` putting `cli::run_text`'s raw stdout into `printed`
  fields, which is the one place text reaches a caller without passing the
  redactor; both now go through it, and `via` redacts its one line.
- `MAX_EVENT_COUNT`/`WATCH_CEILING`-style constant names, where every sibling
  pairs `DEFAULT_X` with `MAX_X`. Renamed to match.
- A module header that counted the readers wrong and did not say which flag the
  toolset withholds. Corrected, and `debug metrics --watch` is now named as the
  one flag dropped from an otherwise whole command.

The review's sharpest finding was that the third acceptance criterion was being
proved by asserting on tool *names*, which is the weaker half of what it asks:
`debug prefs` is excluded precisely because a command's name does not tell you
what it does, so a name check would miss a tool that quietly ran one.
`no_debug_tool_runs_an_excluded_subcommand` now drives all 30 tools through a
real session — building each call's arguments from the tool's own schema — and
reads back the argument list the fake `tailscale` was given, refusing any that
starts with an excluded path. The name check stays beside it as the weaker
claim a caller can also check.

`is_via_route`'s doc said the exemption was for routes "built from documentation
values", which is a condition the code cannot check. It now says what the code
does check — no control-plane-assigned address lives in the reserved `b1a`
block — and the two counter-examples the review wanted are tests: a node address
that merely begins the same way still fails, and a real 4via6 route passes.
