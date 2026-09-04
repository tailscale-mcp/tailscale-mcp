# 14 — Passthrough tool

Status: done
Milestone: 2 — Local surface
Blocked by: 13

The single escape hatch that runs an arbitrary `tailscale` subcommand, off by default and enabled by one switch that is equivalent to adding its toolset. It takes an argument array and never a shell string.

It has no fixed tier: it inherits the tier of the typed tool covering the same subcommand, an unknown subcommand counts as destructive, and an excluded command is refused with the permission code. Arguments are logged with secrets redacted.

## Acceptance criteria
- With the read tier only, a status subcommand runs and a down subcommand is refused.
- An unknown subcommand is treated as destructive and refused unless that tier is enabled.
- Every excluded command is refused, verified by a test enumerating the exclusion list.
  The debug half of that list is `tools::local_debug::EXCLUDED` — 14 entries, each with a
  caller-facing reason — which ticket 13 built public for this tool to consume rather than
  repeat. `debug reload-config` is deliberately absent from it and stays runnable
  (DECISIONS Q44).
- No shell is involved: a string containing shell metacharacters is passed through as a literal argument.

## As built

One tool, `tailscale_run`, in `passthrough.rs`. It takes `args` as a list and
never a string, so nothing it is given reaches a shell; the ninety-third tool,
which brings the local surface to 62 typed + 30 debug + 1.

The switch is the toolset and nothing else (DECISIONS Q49). `Preset::Full`
already excluded `LocalPassthrough` alongside `LocalDebug`, so `--toolsets
+local-passthrough` is the whole of it — a dedicated `--allow-passthrough`
would have been a second spelling of one thing, and the flag surface has no
per-toolset flags anywhere else.

### The tier problem, and `varying_tier`

The ticket's first two acceptance criteria pull in opposite directions: a
read-tier session must be able to run `status` through this tool, and the same
tool must be able to refuse `down`. The gate reads a tool's tier before any
handler is reached, so a destructive row would put the tool out of a read-tier
session's reach entirely, and a read row would tell the gate — and every reader
of the table — that a tool which can run `logout` is a reader.

`ToolMeta` gained a `varying_tier` axis for this (DECISIONS Q50), independent of
the tier in the same way `severing`, `confirm` and `idempotent` already are. The
row carries `Tier::Read` as a floor, `varying_tier` says the floor is not the
truth, and `annotations()` reports the worst case: `read_only: false,
destructive: true`. A client reading those cannot learn that this one depends on
its arguments, which is the honest answer — it has not seen the arguments
either. `ToolContext` gained `max_tier` so the one tool that must make the
gate's decision itself can make it, against the command it was given.

### Judging a command

`classify` reads the command out of the argument list *twice* — the leading run
of non-flag words, and every non-flag word — matches each deepest-first up to
three words so `lock remove` is not read as `lock`, and keeps whichever reading
is stricter (DECISIONS Q53). Both readings are lowercased and trimmed first,
because the client's own subcommand lookup ignores case at every depth. Three
answers come back:

- **Covered** — `COVERED`, 87 rows naming a command a typed tool already runs,
  with that tool's tier and confirmation. `tailscale down` is destructive here
  because `tailscale_down` is destructive there, and wants the same `confirm`.
- **Excluded** — refused with `not_permitted`, the caller-facing reason, and
  deliberately **no hint**. Every other permission refusal names the switch that
  would allow it; there is no such switch here, and that absence is the whole
  difference between this refusal and the gate's.
- **Unknown** — destructive. A command nobody has judged is judged at the top.

Neither reading is sound on its own, and each fails in the direction the other
does not: `tailscale serve --bg reset` runs `serve reset`, which the leading
reading calls `serve`, and `tailscale funnel --set-path status 8080` runs
`funnel`, which the every-word reading calls `funnel status`. Deciding between
them is the flag parsing this tool refuses to do, so both are taken and the
stricter wins; being wrong then costs a caller a tier or a confirmation they did
not need, which is the direction to be wrong in. An exclusion binds on either
reading.

What survives both is a path that stops partway into something excluded:
`["debug", "--file=get"]` has no word after `debug`, and running it as unknown
would put a `debug`-anything within reach of the destructive tier. That is
refused outright rather than guessed at. `["serve", "--https=443", "off"]`
matches `serve` on both readings and never comes near the guard.

`timeout_seconds` is a parameter the ticket did not ask for. It is here because
a passthrough command has no fixed shape and two of the commands it can reach —
`cert` and `netcheck` — routinely outrun `exec::DEFAULT_TIMEOUT`, so the
alternative to a bounded parameter is a caller who cannot run them at all. It
takes the house form every other waiting tool uses: `common::bounded_wait`
against a stated default and ceiling, never an unbounded wait.

`COVERED` is a written table rather than a derived one, because a tool's command
path is not in its metadata — it is what the handler builds at call time
(DECISIONS Q48). What holds it true is
`the_covered_table_follows_the_tools_it_claims_to_follow`, which drives all 92
other typed tools through a real session, reads back the argument list each one
actually gave the client, classifies it with the passthrough's own matcher, and
checks both directions: no row is weaker than a tool that runs its command, and
no row is stronger than every tool that runs it. The table was written from that
same read-back rather than from memory. Two exemptions, both asserted rather
than skipped: `tailscale_run` itself, which has no fixed command, and
`tailscale_debug_file_list`, which runs `debug --file=get` — a flag on the
parent, so there is no subcommand path to put in a row, and the prefix guard
refuses it, which is the safe outcome.

`completion` is the one command with no row and no exclusion on purpose: no
typed tool covers it, so any tier written for it would be the first row nothing
checks. `debug reload-config` stays runnable as unknown-destructive, which is
what DECISIONS Q44 asked for.

### Redaction

`cli::command_failure` and the timeout path were putting `Invocation::display()`
— the argument list verbatim — into the message and the log line. For a typed
tool that is safe by construction: the server builds those arguments and an auth
key only ever reaches the client through a 0600 file. These arguments are the
caller's. `cli::run` and `run_tolerant` now compute a redacted display once, up
front (DECISIONS Q51), so the leak is closed for whoever adds the next tool that
takes an argument list, not just for this one. The argument list the client is
actually given is untouched, which
`a_secret_on_the_argument_list_is_kept_out_of_the_report` asserts alongside the
redacted report.

### Acceptance criteria

- `at_the_read_tier_the_command_decides_what_may_run` — `status` runs, `down` is
  refused with `not_permitted` and a hint naming `--allow-destructive`, and
  nothing reached the client.
- `an_unknown_subcommand_is_judged_at_the_top` — refused at the read and write
  tiers, runs at the destructive tier, and the report says `covered: false` so a
  caller can tell a judgement from a refusal to guess.
- `every_excluded_command_is_refused` — walks `passthrough::excluded()` and
  asserts, for each, the code, the reason, that no switch is suggested, and that
  nothing reached the client. It pins the count, and beside it `debug
  reload-config` is asserted still runnable, which is what makes the list a list
  rather than a blanket refusal.
- `nothing_a_caller_writes_is_parsed_by_a_shell` — one argument carrying a
  semicolon, `rm -rf /`, an ampersand, both substitution forms, a pipe, a
  comment marker and both quote characters goes end-to-end through a real
  session and comes back out of `cli_calls()` as that one literal argument.

The count is **23**, not `spec.md`'s "roughly 34": nine documented commands and
ticket 13's fourteen hidden `debug` members. That estimate was written during
design, before the tools existed to say what a passthrough would otherwise
reach; padding the list would mean excluding commands with no ground under
`CONTEXT.md`'s definition (DECISIONS Q52). All four grounds are represented —
interactive (`ssh`, `nc`), foreground (`web`, `systray`), host-altering
(`update`, two `configure sysext` and two `configure mac-vpn` paths), and
printing a secret (`debug prefs`, from Q45).

### What `/code-review` found

Two of its findings were ways past the judgement, both fixed in `classify` and
recorded as DECISIONS Q53:

- **The exclusion list was case-sensitive and the client is not.** ffcli matches
  subcommands with `EqualFold` at every depth, verified against the real client
  1.102.2, so `["DEBUG", "PREFS"]` matched no row, became unknown-destructive,
  and would have run the one command Q45 excluded for printing
  `PrivateNodeKey`.
- **A flag before a *covered* subcommand escalated.** `["serve", "--bg",
  "reset"]` read as `serve` — write tier, no confirmation — while the same
  table calls `serve reset` destructive-and-confirmed. `serve clear` was
  identical and `switch remove` escalated the tier the same way.

A third was that Q51's redaction was not in fact closed: `ExecError::Io`'s own
`Display` embeds the verbatim argument list, and `exec_error` was passing it
through `ToolError::new`, which redacts by shape but not by the session's
registered secrets. Every arm of `exec_error` now goes through the session
redactor (DECISIONS Q54).

`a_command_cannot_be_disguised_past_its_own_judgement` holds all three closed
end-to-end, and the unit tests around `classify` hold each reading's failure
mode by name.

The standards half found the usual accumulation, all fixed: a cited test name
that did not match the test, an `EXCLUDED` doc claiming all four of
`CONTEXT.md`'s grounds while listing three, a `MAX_DEPTH` doc naming one path as
the longest where there are five, a `confirm` doc promising a message the
refusal does not give, an `ssh` reason using "another machine" where
`CONTEXT.md`'s **Peer** entry says not to, and three branches that could not be
reached — `Known::terms`'s excluded arm, the confirmation refusal's fallback
wording, and `tier.flag()`'s. `terms` is gone; `run` now makes all three
judgements in one exhaustive match, and the tier check uses a let-chain, since
`Tier::Read` outranks nothing and is the only tier without a flag. `MAX_DEPTH`
and this module's `EXCLUDED` were `pub` with no consumer outside it and are now
private, with `excluded()` the way in — which is the same rule that made
`local_debug::EXCLUDED` public in the first place.

Its sharpest finding was elsewhere: a read-tier session is told "only tools that
change nothing are offered", which stops being true the moment this toolset is
named, and directly contradicts the `destructive: true` annotation the same
session can read off `tailscale_run`. `instructions.rs` now explains the tool
wherever it is offered — what its annotation means, that it is still held to the
permitted tier command by command, and that a typed tool is preferable.

### Along the way

Adding `max_tier` to `ToolContext` broke eight test literals. Six were identical,
so `testing::context` replaced them; the two that genuinely differ were patched.
`Excluded` moved from `local_debug.rs` to `common.rs`, because the passthrough
refuses from both lists through one iterator and they have to be one type.
`CONTEXT.md` gained **Covered command**, the counterpart to the **Excluded
command** it already defined.
