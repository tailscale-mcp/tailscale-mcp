<!-- AI-maintained, append-only -->

## Q1 — interactive/design-research — gate-resolution

**Question:** Where should the fact-finding gathered during the design interview live once the session scratchpad is gone?
**Options considered:** leave it in the scratchpad / keep it only in the personal knowledge base / commit it to the repo under `docs/research/`
**Chosen:** Commit it under `docs/research/` with an index, kept separate from `CONTEXT.md` (glossary) and `docs/adr/` (decisions); durable findings are also compiled into the personal knowledge base.
**Decided-by:** agent
**Justification:** The design skills in use put the glossary in `CONTEXT.md` and decisions in `docs/adr/` but say nothing about research inputs; the notes are the evidence behind every fact-bound design question, so they belong with the repo that the decisions are about. Cheapest option to reverse (delete a directory).
**Outcome:** applied
**Ref:** `docs/research/README.md`

## Q2 — interactive/design-research — deviation

**Question:** The research notes were produced on the maintainer's own machine and tailnet. May they be committed as produced?
**Options considered:** commit verbatim / scrub identifying values and keep the notes / keep the notes out of the repo entirely
**Chosen:** Scrub the node name, Tailscale IPs, tailnet suffix, operator login and scratchpad paths into placeholders, and leave the raw CLI probe samples out of the repo because they carry identity throughout.
**Decided-by:** agent
**Justification:** The repository has a public GitHub remote, so anything committed is published; the notes lose nothing of design value without the identifiers. Verbatim `--help` texts contain no identity and are kept.
**Outcome:** applied
**Ref:** `docs/research/README.md`

## Q3 — interactive/repo-setup — gate-resolution

**Question:** The setup answers asked for a local-markdown issue tracker and, in the same pass, for the triage labels to be created; on which tracker should labels be created?
**Options considered:** create the five labels on the GitHub repo anyway / record the label vocabulary in `docs/agents/triage-labels.md` only
**Chosen:** Record the vocabulary only; no GitHub labels created.
**Decided-by:** agent
**Justification:** With a local tracker there is no remote issue list for labels to apply to; the label file is what the triage skill reads. GitHub labels can be created on request in one command.
**Outcome:** applied
**Ref:** `docs/agents/triage-labels.md`

## Q4 — build/ticket-01 — tradeoff

**Question:** Q23 settled "clippy with warnings denied" but not which lints beyond the default, nor whether the toolchain should be pinned for contributors.
**Options considered:** default clippy only / default plus a small set of lints that encode design constraints / a broad pedantic or restriction group
**Chosen:** Default clippy plus four workspace lints: `unsafe_code` forbidden, `missing_debug_implementations` warned, `clippy::print_stdout` denied, `clippy::unwrap_used` denied with `expect_used` warned. `print_stderr` stays allowed. No `rust-toolchain.toml`; the MSRV is declared in the manifests and checked by its own CI job.
**Decided-by:** agent
**Justification:** Two of these are design constraints rather than style: writing to standard output corrupts the stdio transport, and a panic on caller-supplied input turns a tool error into a dropped connection, so both are better caught by the compiler than by review. Logging goes to standard error by design, hence the asymmetry. A pedantic or restriction group would generate noise unrelated to either constraint. Pinning a toolchain file would force every contributor onto one version and hide MSRV breakage until release, whereas a declared MSRV plus a dedicated job tests the thing that actually matters.
**Outcome:** applied
**Ref:** `Cargo.toml` workspace lints; `.scratch/tailscale-mcp-v1/issues/01-workspace-skeleton.md`

## Q5 — build/ticket-01 — gate-resolution

**Question:** ADR-0002 requires that copyleft dependencies be rejected mechanically, but not which licences count as acceptable.
**Options considered:** deny a named list of copyleft licences / allow a named list of permissive licences and deny everything else
**Chosen:** Allow-list: Apache-2.0 (with the LLVM exception), MIT, MIT-0, the two- and three-clause BSD licences, ISC, Unicode-3.0, Zlib, CC0-1.0, Unlicense and BSL-1.0. Everything else fails, including anything unrecognised. OpenSSL and its system crate are banned by name, and unknown registries and git sources are denied.
**Decided-by:** agent
**Justification:** A deny-list passes anything it has not heard of, which is the wrong default for a boundary the project's licence depends on; an allow-list fails closed and the failure is a one-line review rather than a licence incident. The list is what a normal Rust dependency tree actually uses, so the check should be quiet until something genuinely new appears. The OpenSSL ban makes the rustls decision mechanical too, since it is otherwise easy to reintroduce through a transitive default feature.
**Outcome:** applied
**Ref:** `deny.toml`; `docs/adr/0002-own-rest-client-not-rtailscale.md`

## Q6 — build/ticket-04 — interpretation

**Question:** The design fixed the error vocabulary at fourteen codes and named thirteen of them explicitly: the eight agreed at the outset, the platform one, and four added later (`conflict`, `rate_limited`, `result_too_large`, `confirmation_required`). The fourteenth was counted but never named.
**Options considered:** `not_found` for a target that does not exist / a second CLI code splitting parse failures from command failures / drop the count to thirteen
**Chosen:** `not_found`.
**Decided-by:** agent
**Justification:** A device, user, key or webhook that is not there is the single most common tailnet failure an agent will hit, and it is the one where the right next step differs sharply from the generic case: re-list and pick again, rather than retry or escalate. Folding it into `api_error` with a status of 404 makes the agent parse a number to find that out. Dropping to thirteen would contradict a settled count for no gain.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/error.rs`

## Q7 — build/ticket-03 — interpretation

**Question:** `TAILSCALE_MCP_TOOLSETS` was settled as taking `+x,-y` adjustments to a preset. It was not settled what a bare name means, nor what a mixture of the two forms means.
**Options considered:** treat a bare name as `+name` / treat a bare list as an outright replacement of the preset, and refuse a mixture / accept only prefixed entries
**Chosen:** A list where every entry is bare replaces the preset's selection; a list where every entry is prefixed adjusts it; a mixture is a startup error naming the problem.
**Decided-by:** agent
**Justification:** Both pure forms are ones operators reach for, and each is unambiguous on its own. The mixture is the only genuinely ambiguous case — `local-status,-tailnet-org` reads as "just this one" to one person and "add this, drop that" to another — so it is refused rather than resolved to whichever reading we happened to implement. Refusing at startup is consistent with the settled rule that a zero-tool configuration does not start.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/gating.rs`

## Q8 — build/ticket-05 — tradeoff

**Question:** The design requires a timed-out CLI call to be terminated gracefully before it is killed, but the workspace forbids `unsafe`, and neither the standard library nor tokio can send `SIGTERM` without it.
**Options considered:** relax `unsafe_code` to `deny` in the CLI crate and call `libc::kill` / take the `nix` dependency for its safe signal wrapper / skip the graceful step and kill directly
**Chosen:** Depend on `nix` (unix targets only, `signal` feature) and call its safe `kill`.
**Decided-by:** agent
**Justification:** The graceful step is not decoration: `tailscale` unwinds partially-applied preference changes on the way out, so a killed process can leave the node in a state nobody asked for. Relaxing the `unsafe` forbid to write one FFI call would spend a workspace-wide guarantee on a single line. `nix` is permissively licensed, already ubiquitous in the Rust process-handling ecosystem, and target-gated so it does not reach Windows, where there is no `SIGTERM` to send and killing directly is the honest behaviour.
**Outcome:** applied
**Ref:** `crates/tailscale-cli/src/exec.rs`; `Cargo.toml` workspace dependencies

## Q9 — build/ticket-05 — tradeoff

**Question:** Ticket 05 requires process execution to be tested against a stub binary. A stub needs to be a real executable that cargo can locate from a test, but it is not part of what the crate offers.
**Options considered:** ship the stub as a normal binary target / put it in `src/bin/` and exclude the directory from the published package / write a shell script into a temporary directory at test time
**Chosen:** `crates/tailscale-cli/src/bin/tailscale-stub.rs`, auto-discovered by cargo so that `CARGO_BIN_EXE_tailscale-stub` resolves for integration tests, with `exclude = ["src/bin/**"]` in the manifest so it never reaches the registry.
**Decided-by:** agent
**Justification:** The environment variable is the only reliable way to find a built helper across platforms and target directories, and it is set for binary targets alone. A shell script would not run on Windows, where the process behaviour being tested differs most. Excluding the directory rather than declaring the target explicitly means the published crate has neither the file nor a declaration pointing at a missing file, so `cargo package --verify` still builds.
**Outcome:** applied
**Ref:** `crates/tailscale-cli/Cargo.toml`; `crates/tailscale-cli/src/bin/tailscale-stub.rs`

## Q10 — build/ticket-02 — interpretation

**Question:** Confirmation was settled as a `confirm: true` argument on the tools that need it. It was not settled whether that argument belongs in each tool's parameter type or is supplied by the framework.
**Options considered:** a `confirm` field on every parameter struct that needs one / inject the property into the schema and enforce it in the registry, stripping it before the handler runs
**Chosen:** Injected by the registry from the metadata row, and removed from the arguments before the handler sees them.
**Decided-by:** agent
**Justification:** The rule and its enforcement end up in the same place, so a tool marked as requiring confirmation cannot be shipped with a parameter struct that forgot the field — the failure mode that matters, since it fails open. It also keeps roughly twenty parameter structs free of a field that has nothing to do with what they describe, and keeps the refusal message uniform.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/registry.rs`

## Q11 — build/ticket-06 — gate-resolution

**Question:** Ticket 06 requires a supported version floor "from upstream changelogs", but upstream publishes no end-of-life policy and no minimum supported client version.
**Options considered:** track the newest stable and warn below it / pick the oldest release the whole tool surface exists in / pick the newest release that a default-preset command needs / declare no floor at all
**Chosen:** 1.78, the release that introduced `tailscale metrics`, which is the newest command in the default `core` preset. Below the floor the server warns once on standard error and hides nothing; commands newer than the floor carry their own `min_version`.
**Decided-by:** agent
**Justification:** Tailscale's client-version reference documents the three release tracks and states no minimum, and their public position is that they do not break clients people still run, so there is no upstream answer to read off — the floor is a claim about what this server models. Anchoring it to the default preset makes the claim testable: an operator running a supported version never meets a missing command without having opted into a non-default toolset. Anchoring to the newest stable would strand anyone a release or two behind, and anchoring to the oldest release containing every tool would put the floor below 1.32, which we would be asserting without being able to check. Warning rather than hiding follows because the version string is a capability guess: a fork or distribution build can report a version that does not describe what it implements, and refusing a command the binary actually has is a worse failure than letting the CLI answer for itself.
**Outcome:** applied
**Ref:** `docs/research/tailscale-cli.md` §9; `crates/tailscale-mcp/src/version.rs`

## Q12 — build/ticket-07 — tradeoff

**Question:** Ticket 07 requires a fake control-plane HTTP server under the harness. Should it be a mocking crate, a fake at the client's own interface, or a server on a real socket?
**Options considered:** `wiremock` or a similar crate / a trait the REST client is generic over, faked in tests / a small HTTP/1.1 server on a loopback socket, written here
**Chosen:** A hand-written server on a loopback socket, in `tailscale-rest` behind a `testing` feature, exercised by its own tests through `reqwest`.
**Decided-by:** agent
**Justification:** Faking at the client's interface would skip exactly the code most likely to be wrong — the request line, the authorization header, query construction, status and retry handling — so the tests would agree with the client about a shape neither had checked against HTTP. A mocking crate would test the right layer, but ADR-0002 already commits this project to owning its HTTP client rather than inheriting one, and the same reasoning applies to the thing that proves it works: the fake is roughly two hundred lines, has no dependency of its own beyond tokio, and answers exactly the questions a test needs to ask (what arrived, in what order, and what happens on the second attempt). It is verified against a real `reqwest` client, so a mistake in the fake surfaces as a failing test of the fake rather than as a mystery in a client test.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/fake.rs`

## Q13 — build/ticket-07 — gate-resolution

**Question:** Q2 promised that recorded material would be scrubbed before it was committed, but "scrubbed" was a habit rather than a rule. What makes a fixture acceptable, and what enforces it?
**Options considered:** review each fixture by hand / scan for a list of the maintainer's own identifiers / require every identifier to match a placeholder shape, and fail the suite otherwise
**Chosen:** Every identifier in a fixture or a test source must be an obvious placeholder — one tailnet name, one mail domain, addresses from the first hundred of `100.64.0.0/24`, keys marked `example` or `redacted`, hexadecimal keys of a single repeated character, device ids of digits alone. A test walks `crates/tailscale-mcp/tests/` and fails on anything else.
**Decided-by:** agent
**Justification:** A scan for known-bad values only catches the identifiers of whoever wrote the scan, and passes silently for the next contributor's tailnet, which is the case that matters. Requiring a placeholder shape fails closed: a response pasted from a live tailnet does not match, whoever pasted it. The rules are shape rules rather than a value list, so they need no maintenance as fixtures accumulate, and the check carries its own counter-examples so it is itself tested. The one file exempt from the scan is the file that defines it, which is the only place a real-looking identifier is the point.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/tests/fixtures_are_redacted.rs`; `DECISIONS.md` Q2

## Q14 — build/ticket-08 — interpreted-ambiguity

**Question:** Ticket 08 asks for "25 read-only local tools" in the `LocalStatus` toolset, but the local CLI's read-only commands are scattered across the command tree — `lock status`, `serve status`, `funnel status`, `configure sysext status` and `switch --list` all read, while their siblings all write. Does `LocalStatus` mean the `status`-shaped commands, or every local command that only reads?
**Options considered:** group by command tree, so `lock status` joins `LocalLock` / group by risk, so every read-only local command joins `LocalStatus` / list the read-only ones in both
**Chosen:** Group by risk. `LocalStatus` holds all 25 read-only local commands regardless of where they sit in the command tree, and the write and destructive members of `lock`, `serve` and the rest go to their own toolsets.
**Decided-by:** agent
**Justification:** Toolsets are what a caller opts into, and the thing a caller is deciding is how much this session is allowed to touch, not which upstream subcommand a capability happens to live under. The presets make the consequence concrete: `LocalStatus` is in all three, `LocalLock` only in `full`, so grouping by command tree would mean a `minimal` session could not ask whether tailnet lock is enabled without also being handed the commands that sign and revoke keys. Reading the lock state is the same risk as reading the node's status, and the preset should be able to say so. The cost is that the toolset does not map one-to-one onto the CLI's own organisation, which the tool descriptions carry instead.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_status.rs`

## Q15 — build/ticket-08 — tradeoff

**Question:** `tailscale configure sysext` exists only on macOS. Should a tool for a command that does not exist on this platform be hidden from the tool list, or listed and refused when called?
**Options considered:** omit it from the table on other platforms / list it everywhere and refuse the call with `unsupported_platform` / list it everywhere and let the CLI fail
**Chosen:** List it everywhere; refuse the call before spawning, with `unsupported_platform` naming the operating system we are on. `ToolMeta::platforms` records the restriction and `ToolMeta::runs_here` applies it.
**Decided-by:** agent
**Justification:** A table that changes shape by platform makes every artefact derived from it platform-specific: the documentation, the `tools` subcommand's output, and the contract tests would each describe a different server depending on where they ran, and the suite could no longer assert that every tool has a contract. Refusing at call time keeps one table and gives the caller the better answer of the two — "this command is macOS-only and you are on Linux" rather than a tool that silently is not there, which reads to a model as a capability it imagined. Letting the CLI fail was rejected because the failure would arrive as `cli_failed` with whatever the binary happened to say, which is neither stable nor accurate: the binary on this platform does not have the subcommand, so its complaint is about parsing, not about platforms.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/meta.rs`; `crates/tailscale-mcp/src/server.rs`

## Q16 — build/ticket-08 — interpreted-ambiguity

**Question:** Ticket 08 requires that "commands that exit non-zero in a normal condition are reported as success with their status". Which commands are those, and how is the distinction drawn without swallowing real failures?
**Options considered:** treat any non-zero exit from a read-only command as an answer / list the commands that exit non-zero normally and run those through a tolerant runner / parse each command's stderr for its own known phrases
**Chosen:** A named tolerant runner, `cli::run_tolerant`, used by `status`, `exit-node list`, `exit-node suggest`, `routecheck` and `wait`. It still fails on a missing binary, an unrecognised subcommand and an operator refusal; everything else it returns as output, with the stderr carried on the answer as a note.
**Decided-by:** agent
**Justification:** The distinction is a property of the command, not of the exit code: `exit-node list` exits non-zero when there are no exit nodes, which is the answer the command exists to give, while `netcheck` exiting non-zero means the probe did not run. Only the command knows which it is, so the list is per-command and the research notes record why each one is on it. Blanket tolerance for read-only commands was rejected because it would report a broken `dns status` as an empty one. Phrase-matching each command's stderr was rejected because the phrases are not part of any interface and change between releases, whereas the three conditions kept as failures are ones we already recognise elsewhere for the same reasons. Notably `is_not_found` is deliberately not applied in tolerant mode: "no exit nodes found" is a result, not a missing thing.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/cli.rs`; `docs/research/tailscale-cli.md` §8

## Q17 — build/ticket-09 — interpreted-ambiguity

**Question:** Ticket 09 says "down, logout, re-authenticate and reset refuse without a confirmation". The CLI has its own gate for the same danger — `--accept-risk` — which it demands on any command that would cut the connection it is being driven over. Which of the two gates does the server present, and does passing one imply passing the other?
**Options considered:** surface `--accept-risk` as a parameter and let the caller set it / take the confirmation as the caller's answer and pass the CLI's risk flag on their behalf / require both, separately
**Chosen:** One gate, ours. A tool that can sever its own connection carries `self_severing`, which implies `requires_confirmation`; the caller answers `confirm: true`, and the handler passes `--accept-risk=all` to the commands that have such a flag. `tailscale_prefs_set` is not confirmable and never passes a risk flag, so a `set` that would cut the connection fails with the CLI's own complaint.
**Decided-by:** agent
**Justification:** Two gates for one danger is a gate a caller learns to answer twice without reading, and the CLI's flag is the worse of the two to expose: it is spelled as a list of risk names that changes between releases, it is absent on some of the commands that need confirming, and a model filling in a schema field called `accept_risk` has no way to tell it apart from any other option. Our confirmation is the one with a stable meaning — the registry strips it before the handler runs and refuses the call outright when it is missing — so it is the one the caller sees. `prefs_set` is deliberately outside the arrangement: the ticket does not name it, and quietly accepting a risk on the routine way to change one preference would defeat the point of having asked at all. The cost is that a `set` which happens to sever fails rather than prompting; that failure carries the CLI's message, which names the flag, and the caller can reach for `tailscale_up` instead.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_prefs.rs`; `crates/tailscale-mcp/src/registry.rs`

## Q18 — build/ticket-09 — deviation

**Question:** Q14 settled that `LocalStatus` holds every read-only local command. `tailscale get` reads, so by that rule it belongs there — but it reads back exactly what `tailscale set` writes. Which toolset does `tailscale_prefs_get` live in?
**Options considered:** `LocalStatus`, per Q14 as written / `LocalPrefs`, with the writer it mirrors / both
**Chosen:** `LocalPrefs`, at the read tier. Q14 is refined rather than overturned: a read command that exists only to show what a write command in its own toolset has set stays with that writer.
**Decided-by:** agent
**Justification:** Q14's reasoning was that a caller opts into a toolset to decide how much a session may touch, and that reading the lock state is the same risk as reading the node's status. `get` is the case that reasoning does not reach: it is not a view of the node that stands on its own, it is the other half of `set`, sharing `set`'s vocabulary of preference names and useful mainly for reading a value back before or after changing it. Splitting the pair across two toolsets would mean a session that can change preferences cannot see them, which is the worse failure of the two — and the tier still protects, because a read-tier session that enables `LocalPrefs` is offered the getter and nothing else. `LocalStatus` keeps its 25 commands, every one of which answers a question about the node rather than about one command's own settings.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_prefs.rs`; DECISIONS Q14

## Q19 — build/ticket-09 — tradeoff

**Question:** `tailscale up` changes preferences, which is the write tier, and it is also how a disconnected node is brought back, which sounds like the least destructive thing in the toolset. Ticket 09 does not assign it a tier. Write or destructive?
**Options considered:** write, since connecting is restorative / destructive, since `up` applies a whole preference set / write normally and destructive only when `--force-reauth` or `--reset` is passed
**Chosen:** Destructive, always. Its description sends a caller who only wants to change one setting on a connected node to `tailscale_prefs_set` instead.
**Decided-by:** agent
**Justification:** `up` does not merge: it applies a complete preference set, and anything the caller did not restate goes back to its default. That is the same loss `--reset` makes explicit, silently, and it is exactly the mistake a model is likely to make — calling `up` with the one field it wants to change and quietly clearing advertised routes, the exit node and the hostname. On top of that, `--force-reauth` and `--reset` drop the connection the server is being reached over. A tier that depended on which optional fields were filled in would be a tier a caller cannot read off the tool list, and the annotations a client renders — the destructive hint among them — are computed from the tier, so it has to be fixed. The cost is that reconnecting a node needs `--allow-destructive`; the mitigation is that the ordinary reason to reach for `up` on a running node is served by `prefs_set` at the write tier.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_prefs.rs`

## Q20 — build/ticket-09 — interpreted-ambiguity

**Question:** Four preferences exist only on one operating system — `snat_subnet_routes`, `stateful_filtering` and `netfilter_mode` on Linux, `unattended` on Windows. Q15 settled what to do with a whole *tool* that does not exist here. What about a *field* that does not?
**Options considered:** drop the field from the schema on other platforms / keep the schema everywhere and refuse the call before spawning / pass it and let the CLI complain
**Chosen:** Keep the schema everywhere; refuse before spawning, with `unsupported_platform` naming both the field and the operating system. `only_on` applies the check, and every one of the four is offered identically on `set`, `up` and `login`.
**Decided-by:** agent
**Justification:** The same argument as Q15, one level down: a schema that changes shape by platform makes the generated documentation and the tool list platform-specific, and a model reading a schema on one machine cannot then be shown a different one on another. Refusing before spawning is also the only way to keep the promise ticket 09 asks for by name — "a Linux-only flag on macOS produces the platform code without spawning" — which passing the flag through could not, because the client would answer with a parse error and that is `cli_failed`, not a statement about platforms. The uncertainty accepted is which flags belong to which platform: that comes from the research notes and cannot be checked on this machine, so ticket 26's end-to-end tests are where a wrong entry will show up.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_prefs.rs`; `docs/research/tailscale-cli.md` §3.2

## Q21 — build/ticket-09 — tradeoff

**Question:** Sixteen preferences are accepted by three tools — `prefs_set`, `up` and `login`. The obvious way to share them is one struct embedded with `#[serde(flatten)]`. Should they be shared that way?
**Options considered:** `#[serde(flatten)]` on a shared struct / a declarative macro that expands the fields into each struct / repeat the fields by hand three times
**Chosen:** A `prefs_params!` macro that emits the sixteen shared fields plus each tool's own, so every parameter struct is a flat object.
**Decided-by:** agent
**Justification:** A flattened struct does not produce a flat schema: `schemars` renders it as an `allOf` composition with no top-level `properties` map. The registry injects the `confirm` field into that map when a tool requires confirmation, and five of these three tools' relatives do — so flattening would have silently produced tools whose confirmation field never appeared in their schema, and the failure would have surfaced as a caller unable to confirm rather than as a compile error. The macro keeps one definition of the shared fields, and with them the two methods that turn those fields into arguments, at the price of a layer of `macro_rules!` between the reader and the field list. Repeating the fields by hand was rejected for the reason it always is: three copies of sixteen fields drift.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_prefs.rs`; `crates/tailscale-mcp/src/registry.rs`

## Q22 — build/ticket-10 — tradeoff

**Question:** `tailscale serve` and `tailscale funnel` run in the foreground by default and stop on an interactive prompt. Ticket 10 asks that no tool leave a foreground process running. What is passed on the caller's behalf?
**Options considered:** `--bg` only, and let a prompt time out / `--bg` and `--yes` on every set and clear / expose both as parameters and let the caller choose
**Chosen:** `--bg=true --yes=true` on every call that sets or clears a handler, with neither exposed as a parameter.
**Decided-by:** agent
**Justification:** A foreground `serve` holds its terminal until it is interrupted, which no tool call can do, so `--bg` is not a choice — the alternative is a call that never returns. `--yes` is the same argument one step further in: the prompt it answers is drawn on a terminal nobody is watching, so leaving it unanswered turns the call into a timeout rather than into a question the caller gets asked. The question it asks is nevertheless a real one, and this server asks it in the place a caller can see: funnel publishes to the public internet, so both funnel tools sit at the destructive tier and the one that exposes something requires `confirm: true`. This is the same division as Q17 — our gate is the one with a stable meaning, the client's flag is passed on a call that already carried our answer — except that here `--yes` is passed on the serve tools too, where the tier rather than a confirmation is what the caller had to opt into. Exposing either flag as a parameter was rejected because every value other than the one passed is a hang.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_serve.rs`; DECISIONS Q17

## Q23 — build/ticket-10 — deviation

**Question:** Ticket 10's acceptance criteria say "serve tools are available at the write tier; funnel tools only at the destructive tier". `serve reset` removes every handler on the node and `serve clear` removes every handler for a service. Do they stay at the write tier to satisfy the criterion as written?
**Options considered:** every serve tool at write, as the criterion reads / `reset` and `clear` at destructive, against the letter of it / a fourth tier for bulk removal
**Chosen:** `tailscale_serve_reset` and `tailscale_serve_clear` at the destructive tier with `confirm: true`; the other six serve tools at write or read, so serve remains fully usable without `--allow-destructive`.
**Decided-by:** agent
**Justification:** The criterion's load-bearing word is *only*, on the funnel side: what it is protecting is that a write-tier session cannot publish to the internet, and that exposing a server on the tailnet does not require the destructive flag. Both hold. Against that, the tier definitions are explicit — destructive is "removes something ... in a way that is not simply undone" — and `reset` discards a configuration that the caller cannot reconstruct from anything the tool returned. Classifying it as write to satisfy a phrase would make the tier mean less everywhere else, which is the one thing the tier model cannot afford. A fourth tier was rejected for the same reason: three tiers a caller can hold in mind are worth more than a taxonomy. The cost is that clearing everything needs a flag that setting things up did not; a caller who wants to undo one handler has `tailscale_serve_off` at the write tier.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_serve.rs`; `.scratch/tailscale-mcp-v1/issues/10-local-serve-toolset.md`

## Q24 — build/ticket-10 — interpretation

**Question:** The command inventory records `serve get-config <file>` as writing a file, alongside `set-config <file>` which reads one. Should `tailscale_serve_get_config` create a temporary file for the client to write into?
**Options considered:** reserve a private path and read it back / print to standard output and parse that / probe the installed client and follow what it does
**Chosen:** Probed, and the note is wrong: `serve get-config` prints the document to standard output and ignores the positional argument entirely. Only `set-config` needs a file, so only `set-config` makes one.
**Decided-by:** agent
**Justification:** Verified against the installed client at 1.102.2: `serve get-config --all=true` prints JSON with no file named, prints the same JSON when one is named, and creates nothing at that path. A `PrivateFile::reserved` variant had been added to the CLI crate for the file that turned out not to be needed, and a stub reply that wrote files with it; both were removed rather than left in for a case that does not exist. The general lesson is recorded here because the inventory is the input to several tickets still to come: where a note describes an interface rather than a version, the installed client is the authority and is cheap to ask.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_serve.rs`; `docs/research/tailscale-cli.md`

## Q25 — build/ticket-10 — tradeoff

**Question:** On a tailnet where Funnel has not been enabled in the admin console, `tailscale funnel` does not fail: it prints the URL that enables it and then waits for ever, even with `--yes` and `--bg`. A tool call against such a tailnet hits the timeout and, as the exec layer stood, returned nothing but the elapsed seconds. What should the caller get?
**Options considered:** leave it as a bare timeout / give funnel a longer timeout / keep what the child printed before it was killed, and report it with the timeout
**Chosen:** Keep it. `ExecError::Timeout` now carries a `printed` field holding what the child had written to either stream when it was killed, capped and cut on a character boundary; `ToolError::timeout` appends it and changes its hint to say the command was waiting on something and to act on what it printed.
**Decided-by:** agent
**Justification:** The bare timeout is the least actionable error this server can produce: it says a command took too long and nothing about why, and here the why was already on the wire — a one-line explanation and a URL that fixes it permanently. A longer timeout only moves the same non-answer further away. The change is in the exec layer rather than in the funnel handler because nothing about it is funnel-specific: any `tailscale` command that stops to talk to a person has something worth reading in what it printed, and `up` and `login` are the next two. The cost is that the buffers now outlive the futures that fill them, which made the read paths slightly more involved, and that a killed child's partial output could in principle carry a secret — so it goes through the redactor on the way out, like every other captured stream.
**Outcome:** applied
**Ref:** `crates/tailscale-cli/src/exec.rs`; `crates/tailscale-mcp/src/error.rs`; `crates/tailscale-mcp/src/cli.rs`

## Q26 — build/ticket-10 — interpreted-ambiguity

**Question:** Ticket 10 puts funnel at the destructive tier for publishing to the internet. `tailscale_funnel_off` un-publishes. Does it need `confirm: true` as well?
**Options considered:** confirm both, since both are destructive / confirm only the one that exposes / drop `funnel_off` to the write tier
**Chosen:** Both at the destructive tier; only `tailscale_funnel_set` requires confirmation.
**Decided-by:** agent
**Justification:** The tier and the confirmation answer different questions, and there is deliberately no rule in this server that makes one imply the other. The tier says which flag an operator had to pass to have the tool offered at all, and keeping both funnel tools behind the same flag means a session that can reach funnel reaches all of it — an operator who has granted the ability to publish should not find that turning it off again requires a different grant. The confirmation asks the caller to state an intent in the call, and the intent worth stating is exposure: `funnel_set` makes something reachable by anyone on the internet, `funnel_off` makes it unreachable. Requiring an answer to a question whose wrong answer is safe is how a caller learns to answer without reading.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_serve.rs`; `crates/tailscale-mcp/src/meta.rs`

## Q27 — build/ticket-10 — interpretation

**Question:** `serve get-config` and `serve set-config` each want `--service=svc:<name>` or `--all`, and refuse when given neither. `--all` on the writing side replaces the configuration of every service the node hosts. How should the scope reach the tool?
**Options considered:** one optional `service`, where omitting it means all / two parameters, `service` and `all`, exactly one of which must be given / a single string that is either `all` or a service name
**Chosen:** Two parameters, refusing both "neither" and "both" with `invalid_args` before anything runs; a service name without the `svc:` prefix is given one.
**Decided-by:** agent
**Justification:** Making "all services" the meaning of an omitted parameter puts the widest possible write one forgotten field away, on the tool that overwrites handlers — the client refuses that call for the same reason, and reproducing the safety in the schema is cheaper than explaining it in a description. Two booleans-worth of state does admit two invalid combinations, but both are refused before spawning with a message that names the choice, which a single overloaded string could not do as clearly. The prefix is added because the client rejects a bare `web` with a flag-parsing error rather than adding it, and that error reaches a caller as `cli_failed` with nothing to act on; unlike `--advertise-tags`, which the client prefixes itself, there is nothing here to defer to.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_serve.rs`

## Q28 — build/ticket-11 — deviation

**Question:** Ticket 11 describes "the 9 tools" of the files and certificates toolset and then lists seven categories. Q24 of the design round, accepted by the user, additionally keeps `drive share`, `drive unshare`, `drive rename` and `drive list` as typed tools rather than leaving them to the passthrough. How many tools does `local-files` hold?
**Options considered:** hold to nine and leave Taildrive to the passthrough tool / hold to nine and give Taildrive a toolset of its own / build eleven, taking the four Taildrive commands into `local-files`
**Chosen:** Eleven, with the four Taildrive commands folded into this toolset.
**Decided-by:** agent
**Justification:** The count in the ticket is a summary of the categories it lists, and those categories were written before Q24 settled that Taildrive gets typed tools; the accepted answer is the later and more specific of the two, so it wins. Leaving Taildrive to `tailscale_passthrough` would put the only tools that share a host directory with the whole tailnet behind the one tool that has no schema, no tier and no description — the opposite of what Q24 asked for. A toolset of its own would be four tools that gate on exactly the same thing as the other seven, which is that they read and write paths on the host, and 20 toolsets is already at the limit of what a caller can hold in mind. The cost is that the ticket's number is wrong and this entry is the only record of why.
**Outcome:** applied
**Ref:** `.scratch/tailscale-mcp-v1/issues/11-local-files-toolset.md`; `crates/tailscale-mcp/src/tools/local_files.rs`

## Q29 — build/ticket-11 — interpreted-ambiguity

**Question:** Ticket 11 says file transfer and certificate issuance "carry the longer timeouts agreed for them". The design round records that agreement twice and not identically: the Q29 table gives one row, "`file_cp`, `cert` call timeout | 300 s | 600 s", while the recap of the same round says "file_cp and cert default timeouts longer (300 s / 120 s)". Which defaults apply?
**Options considered:** 300 s for both, per the table / 300 s and 120 s, per the recap / one shared default chosen fresh
**Chosen:** `tailscale_file_cp` defaults to 300 s and `tailscale_cert` to 120 s, both bounded at the 600 s the two readings agree on, and both settable per call.
**Decided-by:** agent
**Justification:** The two records disagree only about `cert`, and the recap is the more specific of the two: the table groups the pair on one row for brevity, which is exactly the kind of compression a recap would undo rather than invent. The reading also matches what the two commands do. A Taildrop transfer is bounded by the size of the file and the path it takes, so minutes are ordinary; an ACME exchange is a handful of round trips to a certificate authority, and one that has not finished in two minutes has gone wrong rather than slow — waiting five more only delays the report. The cap is 600 s either way, so a caller who disagrees can say so in the call.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_files.rs`

## Q30 — build/ticket-11 — deviation

**Question:** Q14 groups tools by risk, which puts every read-only local command in `local-status`. `tailscale drive list` is read-only. `local-status` closed with ticket 08 behind a test asserting exactly 25 tools. Where does it go?
**Options considered:** `local-status`, reopening ticket 08 to 26 tools / `local-files`, alongside the three Taildrive writers
**Chosen:** `local-files`.
**Decided-by:** agent
**Justification:** Grouping by risk is what decides the tier, and `drive_list` keeps the read tier wherever it lives, so nothing about permission changes. What the toolset decides is what a session is offered together, and a caller that cannot share or unshare has almost no use for the list of shares: the four commands are one feature and are chosen as one. Reopening a closed ticket to move a tool across a boundary that gates nothing would cost a re-review of ticket 08 and buy a tidier rule. `tailscale_file_targets` sits in the same position for the same reason and was never a candidate for `local-status`.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_files.rs`; `crates/tailscale-mcp/src/tools/local_status.rs`

## Q31 — build/ticket-11 — tradeoff

**Question:** The macOS GUI packaging of the client carries the `drive` subcommands and refuses them: "Taildrive CLI commands are not supported when using the macOS GUI app", exit 1. Classified by the ordinary rules that is `cli_failed`, which tells a caller its request was wrong. How should the four Taildrive tools report it?
**Options considered:** leave it as `cli_failed` / declare `platforms: ["linux", "windows"]` on the four tools so they are hidden on a Mac / detect the client's own wording at runtime and report `unsupported_platform`
**Chosen:** Detect the wording and report `unsupported_platform`, with a hint naming where Taildrive is configured instead.
**Decided-by:** agent
**Justification:** A platform gate would be wrong twice over. It is not the operating system that decides this but the packaging: a Mac running the `tailscaled` variant supports every one of these commands, and hiding the tools there would remove working functionality from the surface for a reason that does not apply. It would also have to name the platforms that *do* support Taildrive, which is a list this server would then have to maintain against a client it does not ship. Reading the client's own sentence keeps the knowledge in one place and degrades safely: an unrecognised message falls through to the ordinary classification. The cost is a string match on English text that a future release could reword, which would return the tools to `cli_failed` — a worse error, not a wrong one.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_files.rs`

## Q32 — build/ticket-11 — interpretation

**Question:** Three of these commands have a default or a spelling that a tool call cannot live with: `cert` writes to standard output when a path is `-`, `file cp` reads standard input for a file named `-` and refuses a target without a trailing colon, and `file get` has `--wait` and `--loop` that would hold the call open. What does the server settle, and what does it leave to the client?
**Options considered:** pass the caller's values through and let the client refuse / refuse the impossible ones before spawning and normalise the rest
**Chosen:** Refuse `-` for both certificate paths, for every `file cp` source and for the `file get` directory; add the trailing colon to a transfer target that lacks one; pass `--wait=false --loop=false` explicitly, and `--update-interval=0` on every transfer. `--serve-demo` is not offered at all.
**Decided-by:** agent
**Justification:** `-` is the one value that turns a write to disk into a write to the answer, and for the key file that would put private key material into the response, the transcript and any log that keeps it — which is the acceptance criterion the ticket states, and it has to be enforced before the command runs rather than after. The trailing colon is a syntax detail of the command line with no meaning to the caller, so making it the caller's problem buys a round trip and an error message about argument formatting. `--wait` and `--loop` already default to off, and naming them costs two arguments and makes the promise in the tool description — that receiving never blocks and never loops — visible in what was actually run and assertable in a test, rather than resting on a default a future release could flip. `--update-interval=0` turns off a progress line that exists to be repainted in a terminal; captured into a pipe it would be most of what the caller read back. `--serve-demo` holds port 443 open instead of writing the files, which is a foreground server and not something a tool call can leave behind.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_files.rs`

## Q33 — build/ticket-11 — deviation

**Question:** `file cp`, `file get`, `cert` and `metrics write` were built on `Invocation::mutate`, which takes the exclusive lane. With a 600-second transfer bound that means one `tailscale_file_cp` call holds the write half of the process lock for ten minutes and every read tool blocks behind it. Is the exclusive lane right for these?
**Options considered:** keep them exclusive because they are write-tier / give them the shared lane because they do not mutate node configuration / shorten the transfer bound so the stall is tolerable
**Chosen:** Add `Invocation::mutate_shared` and move those four onto it. `configure_kubeconfig`, `syspolicy_reload` and the three Taildrive mutations stay exclusive.
**Decided-by:** agent
**Justification:** The lock's own documentation says what it is for: "two `tailscale set` calls racing produce a result neither caller asked for". It protects the local node's configuration, and none of these four touches that. Sending a file to a peer, emptying the Taildrop inbox, fetching a certificate and dumping metrics to a path all change the world, but none of them races `set` or `up`, so serialising them buys no safety at all and costs the entire read surface for the duration of the longest call this server can make. Shortening the bound was rejected because the bound is correct — a large transfer genuinely takes minutes — and it would trade a real capability for a symptom. What this settles is that tier and concurrency are independent axes: the tier says what a caller is allowed to do, the lane says what races. The three that keep the exclusive lane do so because a kubeconfig edited twice at once, a policy reload and the Taildrive share list are each genuinely shared mutable state.
**Outcome:** applied
**Ref:** `crates/tailscale-cli/src/backend.rs`, `crates/tailscale-mcp/src/tools/local_files.rs`

## Q34 — build/ticket-11 — deviation

**Question:** The ticket asks for the allow-list mechanism "designed in but not enabled". The first cut had only a paragraph of module documentation saying so. Is prose enough, and if not, where does the mechanism live?
**Options considered:** leave the prose and build the mechanism when it is switched on / add a `PathPolicy` to `ToolContext` that every path is already checked against
**Chosen:** `PathPolicy` on `ToolContext`, defaulting to `Unrestricted`, consulted by `real_path` and therefore by all six path-taking parameters.
**Decided-by:** agent
**Justification:** "Designed in but not enabled" is not satisfied by a comment: the point of building the seam ahead of the need is that switching it on is later a matter of populating one value rather than of finding every place that should have asked, and a comment leaves exactly that search to be done. The cost is one field on the context and seven struct literals. `permits` refuses any path containing a `..` component rather than resolving it, because resolving would have to touch the filesystem and the path these tools take is usually one that does not exist yet — a root check that skipped this would be walked straight out of, so the seam would have been wrong the day it was switched on.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/context.rs`, `crates/tailscale-mcp/src/tools/local_files.rs`

## Q35 — build/ticket-11 — interpreted-ambiguity

**Question:** Ticket 11 says "the 9 tools", but the categories it then lists sum to seven, and eleven were built. `spec.md` separately totalled 184 tools on the assumption of nine here. Which number is wrong, and what gets corrected?
**Options considered:** cut two tools to reach nine / leave the totals and note the discrepancy / correct the totals to what is actually built
**Chosen:** Correct `spec.md` to 186 tools and 62 typed local tools, and treat the ticket's "9" as superseded by its own category list plus the four Taildrive tools of Q28.
**Decided-by:** agent
**Justification:** Nine was never reachable from the ticket's own text — file transfer both ways, targets, certificates, metrics and the two configuration commands is seven, so the number and the list contradicted each other before any code existed. Cutting to nine would mean dropping working tools to satisfy an arithmetic that was already wrong, and the superset rule points the other way. Leaving the totals stale is worse than either, because `spec.md` states the contract-row count as a check that a tool cannot be added without being classified, and a target nobody can hit stops being a check. The corrected figures are the built counts: 25 status, 8 prefs, 10 serve, 11 files, 8 lock.
**Outcome:** applied
**Ref:** `.scratch/tailscale-mcp-v1/spec.md`

## Q36 — build/ticket-11 — deviation

**Question:** `parse_shares` split each row of the `drive list` table on the literal two-space string and then required exactly three columns, dropping silently any row that did not yield three. What replaces it?
**Options considered:** accept two or three columns / split on runs of two or more spaces / cut each row at the offsets the header establishes
**Chosen:** Cut every row at the character offsets taken from the header row.
**Decided-by:** agent
**Justification:** The split lost three real cases without a trace: a share whose `as` column is blank, which is every share on a platform that cannot share as another user; a path containing two consecutive spaces; and a share that happens to be called `name`, which the header heuristic ate. Silence is what makes this bad — the caller is told the tailnet has no shares rather than told the listing could not be read. Accepting two or three columns fixes only the first case; splitting on runs fixes only the first and third, because no split can tell a column gap from a gap inside a path. The header states the column widths for every row beneath it, so cutting at its offsets is the parse the client actually wrote, and offsets are counted in characters rather than bytes because Go pads these columns by rune count.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_files.rs`

## Q37 — build/ticket-11 — interpretation

**Question:** `CONTEXT.md` lists "host" as a term to avoid under **Node**, but this toolset needs to name the thing a caller-supplied path refers to, and "this machine" is also on an avoid list. Twenty descriptions had drifted to "this host's filesystem".
**Options considered:** keep "host" here as a term of art / reuse "local node" / add the missing glossary term
**Chosen:** Add **Local filesystem** to `CONTEXT.md` and rewrite every description onto it.
**Decided-by:** agent
**Justification:** Three spellings of one concept had appeared because the glossary had no word for it, which is the signal the domain docs describe rather than a lapse to correct in place. "Local node" is genuinely a different concept — that machine's membership of the tailnet, not its files — and using it here would blur the one distinction these tools exist to make clear, since a path is the only thing in this toolset that never refers to the tailnet at all. Keeping "host" would mean documenting an exception to an avoid list in the module that most needed the term.
**Outcome:** applied
**Ref:** `CONTEXT.md`, `crates/tailscale-mcp/src/tools/local_files.rs`

## Q38 — build/ticket-12 — deviation

**Question:** `lock sign` reads a key from `file:<path>` so the key never reaches an argument list, but `lock disable` and `lock disablement-kdf` take their disablement secret as a bare positional and have no `file:` form at all. What does this server accept at those two parameters?
**Options considered:** accept only a literal, since that is all the client accepts / refuse a literal and require a file, reading it here / accept both, honouring `file:` on the client's behalf
**Chosen:** Accept both, and resolve `file:<path>` in this module by reading the file before the client runs.
**Decided-by:** agent
**Justification:** `file:` has to mean one thing at every key parameter in the module or it becomes a trap: a caller that learned it at `tailscale_lock_sign` would otherwise pass a path to `tailscale_lock_disable` and have it spent as if it were the secret. Honouring it here buys the exposure a caller actually controls — the secret need not be pasted into the conversation, the transcript or the model's context — and it is honest about the exposure it cannot buy: the client's own interface puts that positional on the argument list, where `ps` can read it, and no wrapper can change that. Requiring a file would have been the stricter rule but would refuse the ordinary case of a caller who has just been handed a secret by `tailscale_lock_init` and wants to spend it. The limit is stated in the module documentation rather than left for someone to discover. It is also a deviation from `spec.md`'s "Secrets are never placed on the command line": that rule is kept everywhere this server has the choice, and these two commands are where it does not — the client offers no file form, so the alternative to putting the secret on the argument list is not having the tools.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_lock.rs`

## Q39 — build/ticket-12 — interpretation

**Question:** `lock init` has a `--confirm` flag that suppresses an interactive prompt. Should the tool pass it, pass it only when the caller confirmed, or leave it off?
**Options considered:** leave it off and let the client prompt / pass it only on a confirmed call / always pass `--confirm=true`
**Chosen:** Always pass `--confirm=true`.
**Decided-by:** agent
**Justification:** Probed on the installed client 1.102.2: without the flag, `tailscale lock init` prints its warning, reads end-of-file from a standard input that a tool call does not have, and **exits 0 having initialised nothing**. A silent no-op reported as success is the worst answer available — worse than an error, because the caller goes on believing the tailnet is locked. Making the flag conditional on the confirmation would reproduce that failure for exactly the callers who did not confirm, when the registry has already refused those calls before the handler runs: by the time this argument is built, the question has been asked and answered somewhere a person could answer it. The flag is stated rather than omitted for the same reason every other boolean here is stated. This is `spec.md`'s own rule rather than a departure from it — "The CLI's own risk acceptance is passed only on a call that carried a confirmation, so its checks become the gate rather than being bypassed" — and the probing is what shows why the rule matters here: the check being bypassed is not a prompt but the initialisation itself.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_lock.rs`, `.scratch/tailscale-mcp-v1/spec.md`

## Q40 — build/ticket-12 — interpretation

**Question:** `spec.md` names initialising, disabling and revoking keys as the tailnet-lock operations that need a confirmation on top of the destructive tier. Where does that leave `lock remove` and `lock local-disable`, which are also irreversible?
**Options considered:** confirm everything destructive in the toolset / confirm the three the spec names and put the other two at the destructive tier / put `local-disable` at the write tier
**Chosen:** Destructive tier for all five; a confirmation on the three the spec names and on no others.
**Decided-by:** agent
**Justification:** The spec's own phrase for the confirmed set is "tailnet-scale irreversible", and that is the line the two unnamed commands fall on the other side of. `lock local-disable` changes only what *this* node will accept and leaves every other node enforcing the lock, so it is exactly node-scale; it is destructive because a locked-out peer becomes reachable and nothing here puts that back. `lock remove` re-signs by default, so its ordinary outcome takes nothing away, and its taking-away form is reached by asking for it. Confirming everything would spend the mechanism on the cases that do not need it, which is how a confirmation becomes something a caller passes without reading.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_lock.rs`, `.scratch/tailscale-mcp-v1/spec.md`

## Q41 — build/ticket-12 — deviation

**Question:** `lock sign` on a pre-approved auth key prints the signed key, which begins `tskey-` and is therefore removed by the shape-based redaction every other answer goes through. Redacted, the tool returns nothing; unredacted, it puts key material in the answer.
**Options considered:** return it redacted and document the tool as unusable for auth keys / add a general redaction bypass to `Redactor` / write it to a caller-named file the way `tailscale_cert` does / read the signed key off standard output and carry it in one field, unredacted
**Chosen:** Carry the signed key in its own `signed_auth_key` field, whole, and withhold the standard output it came from.
**Decided-by:** agent
**Justification:** The design already settled that a minted secret comes back verbatim in the answer to the call that minted it — that is how `tailscale_lock_init` returns its disablement secrets, which survive only because `disablement-secret:` happens not to be a shape the redactor knows. (Whether the answer *also* keeps the text the secret was read out of is a separate question, settled differently for the two tools by Q43.) A signed auth key is the same thing: the product of the call, not something leaking out of it, and redacting the product leaves the caller with a tool that reports success and hands back nothing. A general `Redactor` bypass was drafted and dropped: the value here is a single token, so the literal-secret pass is vacuous on it, and the narrow read means no new API exists for a later toolset to reach for. Writing it to a file was the `tailscale_cert` precedent but answers a different question — a certificate is consumed by a server process on that machine, whereas this key exists to be handed to whoever brings the next node up. Which of the two things the client did is read off what it printed rather than off what was asked for, because a caller that named a `file:` path never told us which kind of key was in it.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_lock.rs`

## Q42 — build/ticket-12 — interpretation

**Question:** `lock revoke-keys` has two usage forms behind one positional: the keys to revoke on the first call, and the recovery blob from the previous step on every call after. Does the tool copy that shape?
**Options considered:** one `keys` list carrying whichever the caller means / two parameters, `keys` and `recovery_blob`, with the invalid combinations refused
**Chosen:** Two parameters, and refuse naming both, naming neither, or continuing without a blob.
**Decided-by:** agent
**Justification:** The two forms hold values that look nothing alike and mean nothing alike, and the client tells them apart by trying to parse the argument — which is why `revoke-keys --cosign --finish tlpub:…` fails with `parsing hex: invalid byte 't'` rather than with anything a caller could act on. A model choosing between three named booleans and two named values has the shape of the process in front of it; a model filling one list has to have read the help text. The combinations refused here are the ones the CLI's own instructions rule out — `cosign` on each further signing node, `finish` once the co-signatures outnumber the keys — so nothing reachable through the client is unreachable through the tool.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_lock.rs`

## Q43 — build/ticket-12 — interpretation

**Question:** Both `lock init` and `lock sign` print a secret this server then reads a field out of. `spec.md` asks for "a newly minted secret returned once and never logged". Does the answer also carry the printed text the secret was parsed from, or is the parsed field the only copy?
**Options considered:** withhold the printed text in both, so each secret appears in exactly one field / keep it in both, so the parse is never the only copy / decide per tool, on what a lost secret costs
**Chosen:** Per tool: `tailscale_lock_sign` withholds the standard output it read the auth key from; `tailscale_lock_init` keeps it alongside `disablement_secrets`.
**Decided-by:** agent
**Justification:** "Returned once" is a rule about the calls that follow, not about how many fields of one answer carry the value: nothing here stores a secret, nothing logs one, and no later call can produce one again, which is the whole of the exposure the story is about. Within a single answer the question is a different one — what happens if the parse is wrong — and it has different answers for the two tools because the loss is not the same size. A missed auth key costs a second `tailscale_lock_sign`, so the tidier answer is free and worth having. A missed disablement secret cannot be recovered by any call at all: `lock init` mints them once, and a tailnet whose disablement secrets are lost can never turn its lock off again. Trading that against a parse written against one release's output is not a trade worth making, so `tailscale_lock_init` keeps the client's own text and says in the field's own documentation why. The asymmetry is the point rather than an oversight, so it is stated in the module header too.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_lock.rs`
