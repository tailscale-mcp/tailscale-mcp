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

## Q44 — build/ticket-13 — interpretation

**Question:** Ticket 13 asks for "8 runtime knobs", and nine `tailscale debug` subcommands are write-class and otherwise eligible. Which is the ninth, and what happens to it?
**Options considered:** offer all nine and treat the ticket's count as approximate / drop one of the eight the research table classes as a knob / exclude `debug reload-config` and offer the other eight
**Chosen:** Exclude `debug reload-config` from the toolset, and from the excluded list as well — it is deliberately in neither.
**Decided-by:** agent
**Justification:** `CONTEXT.md` defines a knob as something that "changes the local node's transient runtime state … without changing any preference". The other eight fit exactly: a fresh STUN round, a new socket, a different home relay, all undone by the next restart. `reload-config` makes tailscaled re-read a configuration file that this server did not write, cannot see and cannot describe, so its effect is whatever that file now says — it may change every preference at once. `spec.md` line 144 requires each tool's annotations to state the truth about it, and no honest tier or summary could be written for a command whose scope is a file we have never read. It is still a legitimate operation for whoever does own that file, so it is not in `EXCLUDED` either: the passthrough may run it, with the operator's own judgement standing behind it. Being absent from both lists is the statement, and the module says so where a reader will look for it. The consequence is worth naming: ticket 14 treats a subcommand it does not recognise as destructive, so this is a write-class command that a caller reaches only with the destructive tier enabled. That is the right way round — a command whose scope we cannot describe should cost the caller the widest permission — but it is a consequence of this decision rather than an accident.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_debug.rs`

## Q45 — build/ticket-13 — deviation

**Question:** `docs/research/tailscale-cli.md` §6 lists `debug prefs` among the debug members to keep, on the strength of its help text. Run against the real client it prints `PrivateNodeKey`, `OldPrivateNodeKey` and the tailnet-lock private key. Does it still become a tool?
**Options considered:** offer it as the research table says / offer it and teach the redactor the `privkey:` and `nlpriv:` shapes / exclude it and offer the 22nd reader elsewhere
**Chosen:** Exclude it, as a command that prints a secret.
**Decided-by:** agent
**Justification:** `spec.md` line 152 already excludes "commands that print a secret", and `CONTEXT.md` gives the same rule for an excluded command; this is that rule applied to a case the research table got wrong because it read the help rather than the output. Teaching the redactor two more prefixes was drafted and dropped: the redactor is a safety net for secrets we never had, and leaning on it to make a private-key dump safe is the wrong direction of reliance — a shape it does not yet know would go straight through. The exclusion costs a caller nothing, because every non-secret field in that dump is already reported by a tool that does not carry the keys. The same dump is reachable through `debug watch-ipn --initial`, so that flag is not offered either. Its six `--initial-*` siblings are a different matter and are offered as ordinary parameters: each asks for one narrow current value — the client version, the Taildrive shares, health, outgoing files, status, the suggested exit node — and none of them carries a key. An earlier draft withheld all seven on the ground that the narrow six duplicate dedicated readers, which is not one of the grounds `CONTEXT.md` gives for leaving a flag out; `spec.md` line 140 says every CLI flag becomes a parameter of the same name, and duplication is the caller's business, not ours. The scan that found this was run over every other kept reader; none of them prints anything key-shaped.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_debug.rs`

## Q46 — build/ticket-13 — interpretation

**Question:** Excluding `debug prefs` leaves 21 readers where the ticket asks for 22. What fills the place?
**Options considered:** report 21 and amend the ticket / promote one of the excluded members / offer `debug --file=get` in the files toolset, where the inbox it lists belongs / offer `debug --file=get` here, as a reader on the parent command rather than a subcommand
**Chosen:** `tailscale_debug_file_list`, running `debug --file=get`.
**Decided-by:** agent
**Justification:** It lists what is waiting in the node's Taildrop inbox without downloading any of it, which no other tool does: `tailscale_file_get` fetches, and fetching is the decision a caller wants to make *after* seeing the list. It is a `debug` reader on the same terms as the rest, it changes nothing, and it keeps the ticket's 22 and `spec.md`'s 186 both true. Putting it in the files toolset instead was the closer call, and was rejected on two grounds: the toolset boundary follows the client's own command tree everywhere else, and moving it would change `spec.md`'s 62/30 split without changing what the server can do. The cost is real and worth stating — asking "what is waiting for me" now means enabling a toolset whose premise is that its output may change shape between releases. The parent's other flags stay out on their own grounds: `--file=<name>` and `--file=delete:<name>` act on the inbox that the files toolset owns, and `--cpu-profile` and `--mem-profile` write a binary profile to a path or to standard output.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_debug.rs`

## Q47 — build/ticket-13 — interpretation

**Question:** Ticket 13 requires that the event watcher "always returns". `debug watch-ipn` has `--count` but no timeout flag of its own, so on a quiet node the count may never be reached. What does the tool answer with when its wall-clock bound expires first?
**Options considered:** report the bound as a timeout carrying what had arrived / add a mode to the backend that turns an expired bound into an ordinary end, so a partial result comes back as success
**Chosen:** A timeout, carrying whatever the watcher had printed.
**Decided-by:** agent
**Justification:** This is the road the server already takes for a foreground command it bounds itself: a foreground `tailscale_funnel` "comes back as a timeout carrying the URL that enables it", and `ToolError::timeout`'s own hint reads "The command was waiting on something. Act on what it printed, then call again." Following it costs no new machinery and gives the watcher the same shape as its neighbours. The alternative was drafted: `ExecError::Timeout` already holds what the child printed, so a partial success was reachable — but only by widening `Invocation` and `Output` for one tool, and the text it holds merges both streams and is capped for use in a message, so the notifications would come back worse than the error already reports them. "Always returns" is satisfied either way: the call ends at a bound this server set, rather than running until something interrupts it. What makes the common case return promptly is the count, which the client itself honours, so the bound is the safety net rather than the mechanism.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/local_debug.rs`

## Q48 — build/ticket-14 — interpretation

**Question:** `spec.md` line 148 says the passthrough "inherits the tier of the typed tool covering the same subcommand". Where does that correspondence live: computed from the tools themselves, or written down?
**Options considered:** give `ToolMeta` a `command_path` field and derive the mapping from the registry at startup / have each handler declare its argv shape / write the correspondence out as a table in `passthrough.rs` and pin it with a test that drives every typed tool
**Chosen:** A written table, `passthrough::COVERED`, held true by a test that re-derives it from the tools.
**Decided-by:** agent
**Justification:** Deriving it was the first design and does not work, because a tool's command path is not a property of its metadata but of what its handler builds at call time from the arguments it was given: `tailscale_serve_path` and `tailscale_serve_off` both run `serve` with different flags, `tailscale_debug_file_list` runs `debug --file=get` and has no subcommand path at all, and several tools choose between paths on a parameter. A `command_path` field would therefore be a second, hand-written claim about the handler, sitting further from the code that makes it true than a table does. So the choice is not between derived and written but between two places to write it, and the table wins on being one list a reader can check against the client's own command tree. What makes it trustworthy is `the_covered_table_follows_the_tools_it_claims_to_follow`, which drives every one of the other 92 typed tools through a real session, reads back the argument list each one actually gave the client, classifies it with the passthrough's own matcher, and checks both directions: no row is weaker than a tool that runs its command, and no row is stronger than every tool that runs it. A tool that changes what it runs fails that test rather than quietly widening what the passthrough allows. The two exemptions are stated in the test: the passthrough itself, which has no fixed command, and `tailscale_debug_file_list`, whose flag-on-the-parent form the matcher refuses outright — the test asserts that refusal rather than skipping it.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/passthrough.rs`, `crates/tailscale-mcp/tests/contract.rs`

## Q49 — build/ticket-14 — interpretation

**Question:** The ticket says the passthrough is "enabled by one switch that is equivalent to adding its toolset". Is that switch a dedicated `--allow-passthrough` flag, or the toolset itself?
**Options considered:** add `--allow-passthrough` as its own flag / accept both / the toolset only, named as `--toolsets +local-passthrough`
**Chosen:** The toolset only.
**Decided-by:** agent
**Justification:** "Equivalent to adding its toolset" is most simply satisfied by *being* the toolset. The flag surface has no per-toolset flags anywhere else — `--allow-write` and `--allow-destructive` are tiers, which is the other axis — and ticket 13's debug toolset is opt-in on exactly these terms with no flag of its own. A dedicated flag would be a second spelling of one thing, and two spellings need a rule for what happens when they disagree, which is cost with no gain. What the ticket is really asking for is that the toolset be reachable from no preset, and that is what `Preset::Full` excluding `LocalDebug | LocalPassthrough` does; `the_passthrough_is_reached_by_naming_it_and_no_other_way` holds both halves down. The tier switches still apply on top, so running anything destructive through it needs two switches rather than one.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/gating.rs`, `crates/tailscale-mcp/src/tools/passthrough.rs`

## Q50 — build/ticket-14 — interpretation

**Question:** Every tool's MCP annotations are computed from its tier, and the gate uses the same tier to decide what a session may offer. The passthrough has no fixed tier. What does its row say, and what do its annotations say?
**Options considered:** register it at the destructive tier, so the gate is the whole check / register it at the read tier and let the handler do the whole check / register a floor and mark the row as a floor, so the annotations can tell the truth about both
**Chosen:** A read-tier floor plus a new `varying_tier` axis on `ToolMeta`; the annotations state the worst case.
**Decided-by:** agent
**Justification:** The first option makes the acceptance criterion unreachable — "with the read tier only, a status subcommand runs" cannot happen if the gate withholds the tool from a read-tier session. The second leaves the row saying `read` about a tool that can run `logout`, which is a lie told to the gate and to every reader of the table. The third states both facts separately: the row's tier is the floor the gate applies, and `varying_tier` says that floor is not the whole truth, so `annotations()` reports `read_only: false, destructive: true` regardless of the floor. That is the honest reading of the MCP annotations, which are a client's hint about what a call might do rather than a description of one call; a client that trusted `read_only` here would be trusting it about arguments it has not seen. `varying_tier` sits alongside `severing`, `confirm` and `idempotent` as one more axis that is independent of the tier, which is the shape the macro already had. The handler then makes the same decision the gate makes, against the command it was given, which is why `ToolContext` gained `max_tier`: exactly one tool needs to know it, and it needs to know it for a reason no other tool has.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/meta.rs`, `crates/tailscale-mcp/src/context.rs`, `crates/tailscale-mcp/src/tools/passthrough.rs`

## Q51 — build/ticket-14 — deviation

**Question:** `cli::command_failure` and the timeout path put `Invocation::display()` — the argument list verbatim — into the error message and the log line. With the passthrough, those arguments are the caller's. Where is that redacted?
**Options considered:** redact in the passthrough handler before calling `cli::run` / give `Invocation` a redacted display / redact in `cli::run` and `cli::run_tolerant`, where a command line first turns into text
**Chosen:** In `cli.rs`, at the one place a command line becomes a message or a log line.
**Decided-by:** agent
**Justification:** The rule is that a secret never reaches an argument list, and for every typed tool that rule is kept by the tool: the server assembles those arguments itself and an auth key only ever reaches the client through a 0600 file. The passthrough is the one tool that cannot promise it, because the argument list is the caller's. Redacting in the handler was the smaller change and was rejected as the wrong shape — it leaves the unsafe path in place for whoever adds the next tool that takes an argument list, and it is a promise made in the caller rather than enforced where it matters. Redacting in `Invocation` was rejected because the display is also what the exclusive-lane bookkeeping and the tests read, and a type whose `Display` silently differs from its contents is a trap. `cli::run`/`run_tolerant` computing `displayed(ctx, &invocation)` once, up front, costs one redactor pass per command and closes the path for good; the argument list the client is actually given is untouched, which a test asserts alongside the redacted report.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/cli.rs`

## Q52 — build/ticket-14 — deviation

**Question:** `spec.md` line 152 says the excluded commands come to "roughly 34 command paths in total". The list as built is 23: nine documented commands and the fourteen hidden `debug` members from ticket 13. Is the shortfall a gap?
**Options considered:** find eleven more paths to exclude / count sub-paths and flag variants to reach the number / report 23 and leave the estimate as the estimate it was
**Chosen:** Report 23.
**Decided-by:** agent
**Justification:** "Roughly 34" was written during design from the research table's own count, before the tools existed to say which commands a passthrough would otherwise reach; it is an estimate in a sentence whose load-bearing half is the four grounds for exclusion, and each of the 23 is one of those grounds applied to a real command. Padding the list to the number would mean excluding commands with no ground under `CONTEXT.md`'s definition, which is the opposite of what the estimate was for — and every excluded command is a capability removed from a caller who has already opted into two switches to get here. Counting differently was the other way to reach 34 and is worse: it would make the number true by changing what it counts. The four grounds are each represented — interactive (`ssh`, `nc`), foreground (`web`, `systray`), host-altering (`update`, the two `configure sysext` and two `configure mac-vpn` paths), and printing a secret (`debug prefs`, from Q45) — and `every_excluded_command_is_refused` walks all 23 and asserts the refusal, its reason, that no switch is suggested, and that nothing reached the client. `spec.md` keeps its "roughly".
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/passthrough.rs`, `crates/tailscale-mcp/src/tools/local_debug.rs`

## Q53 — build/ticket-14 — deviation

**Question:** `classify` read the subcommand as the leading run of non-flag words. `/code-review` found two ways past that: `tailscale serve --bg reset` runs `serve reset` but reads as `serve`, and `tailscale DEBUG PREFS` runs `debug prefs` but matches no row at all. How is the command read, given that this server does not parse the client's flags?
**Options considered:** parse the flags, using the client's own flag table / read every non-flag word instead of the leading run / refuse any argument list with a flag before a bare word / take both readings and keep the stricter, with the words case-folded before matching
**Chosen:** Both readings, stricter wins, case-folded and trimmed.
**Decided-by:** agent
**Justification:** The first finding is an escalation — a write-tier session wiping the serve configuration with no confirmation, and `serve clear` and `switch remove` the same way — and the second is worse, reaching `debug prefs` and its `PrivateNodeKey` through the destructive tier, because ffcli matches subcommands with `EqualFold` at every depth. Verified against the real client 1.102.2: `tailscale VERSION`, `tailscale DEBUG --help` and `tailscale SERVE STATUS` all resolve, so the tables must be compared case-folded or they are decoration.

Parsing the flags would be right and is not available: it means carrying the client's flag table for every subcommand of every version, and being wrong about one flag's arity is the same bug again with more code behind it. Reading every non-flag word fails in the opposite direction — `tailscale funnel --set-path status 8080` would read as `funnel status`, a reader, when `status` is only a flag's value — so it trades an escalation for a de-escalation, which is the worse trade. Refusing every flag-before-a-word was rejected because it refuses `["serve", "--https=443", "off"]`, an ordinary and correct call.

Taking both readings and keeping the stricter needs no flag knowledge and can only be wrong in one direction: a caller is charged a tier or a confirmation they did not need, never spared one they did. An exclusion binds on either reading. A reading that matches nothing and stops partway into an excluded path — `["debug", "--file=get"]` — is still refused outright rather than run as unknown, because unknown means runnable at the destructive tier.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/passthrough.rs`

## Q54 — build/ticket-14 — deviation

**Question:** `exec_error` turned `ExecError::Io` into a message with `error.to_string()`, whose `Display` embeds the argument list verbatim rather than the redacted one Q51 computes. Q51 said the leak was closed. Where is it actually closed?
**Options considered:** rebuild the message from the redacted display in that one arm / apply the session redactor to that one arm / apply the session redactor to every arm of `exec_error`
**Chosen:** Every arm.
**Decided-by:** agent
**Justification:** Q51 closed the two paths a command line was known to take — the failure message and the timeout — and `ExecError::Io` is a third, built inside `exec.rs` from `invocation.display()` before anything redacts. `ToolError::new` redacts by shape, which catches a `tskey-` but not a value the session registered, so the arm was leaking exactly the class of secret the redactor exists for. Rebuilding the message from `display` was rejected as a second spelling of `ExecError`'s own `Display`, which is the thing that has to stay in one place. Redacting the single arm fixes today and leaves the next variant to be judged by whoever adds it. Redacting every arm makes `exec_error` the seam it already is — the one place an `ExecError` becomes caller-visible text — at the cost of a redundant pass over two messages that carry only a binary path.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/cli.rs`

## Q55 — build/ticket-15 — interpretation

**Question:** Ticket 15 says the base URL "is pinned, with an override accepted only for tests and only over a secure or loopback address". A test cannot present a certificate for `api.tailscale.com`, so the override has to exist for the suite to reach a fake. How is a setting whose stated purpose is "only for tests" offered without becoming a way to send a credential somewhere it should not go?
**Options considered:** a `#[cfg(test)]` seam with no runtime setting at all / a hidden environment variable, undocumented / a documented environment variable with no CLI flag, refusing any address that is neither https nor loopback / a CLI flag alongside it
**Chosen:** `TAILSCALE_MCP_API_BASE_URL`, documented, environment-only, checked at startup, and a hard failure when it is neither.
**Decided-by:** agent
**Justification:** The `#[cfg(test)]` seam was the first choice and does not work: the integration tests build the server the way the binary builds it, through `server::build` and a real `Config`, which is the property that makes them worth having. A compile-time seam would mean the suite testing a different construction than the one that ships.

Undocumented was the intent until `the_documented_variables_are_the_ones_that_are_read` failed, and the test was right. A setting that redirects every credential the server holds is worse hidden than explained: hidden, it is still readable in the source by anyone who might misuse it and invisible to the operator who would want to know it exists. Documented, it comes with the sentence saying what it refuses.

No CLI flag, because a flag is the surface an operator reaches for and this is not a thing an operator should reach for; every other setting has both spellings, and this one deliberately does not. `check_base_url` requires `https` or a loopback host — `localhost`, or an address that `is_loopback()`, brackets trimmed for the IPv6 form — and refuses a URL carrying a path, since the base URL is a host and nothing more.

Failing the startup rather than noting it is the last part. A note would leave the server running with the tailnet surface silently missing, and the only way to reach the failure at all is to have already pointed the server somewhere a credential must not go — which is not a thing to carry on from. `StartupError::ControlPlane` says the address and the reason.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/client.rs`, `crates/tailscale-mcp/src/config.rs`, `crates/tailscale-mcp/src/server.rs`

## Q56 — build/ticket-15 — interpretation

**Question:** The ticket asks for "retries only where retrying is safe … no retry on the unsafe methods". What decides that a call is safe to repeat?
**Options considered:** a per-endpoint table naming which calls may be retried / an `idempotent` flag each call site passes / the HTTP method the call uses, per HTTP's own definition of idempotence
**Chosen:** The method: `GET`, `HEAD`, `PUT` and `DELETE` repeatable; `POST` and `PATCH` once; a 429 the one exception, for any method.
**Decided-by:** agent
**Justification:** The rule has to hold for the ninety-three tailnet tools that do not exist yet, so the safe default matters more than the precision. A per-endpoint table is more accurate where it is filled in and silent where it is not, and its silence defaults the wrong way — the endpoint nobody classified is the one that gets retried. A flag at each call site is the same problem moved to whoever writes the call. The method is already on every request, is what HTTP itself defines idempotence against, and is what the control plane's own API shape follows: minting an auth key is a `POST`, and a retried mint is a second key that nobody holds and nobody sees.

The 429 exception is not a hole in that reasoning but the same reasoning: the status means the server declined to act, so the request was not performed and repeating it repeats nothing. It is the only status that says so for a method HTTP does not call idempotent.

A refused token is handled separately and before this, because it is not really a retry: a 401 means nothing was done either, whatever the method, so the attempt evicts the token and goes round once to mint another. Once per call, since a second 401 on a fresh token is the credential being wrong rather than the token being stale, and an API key is never re-minted because there is nothing to mint.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/client.rs`, `crates/tailscale-rest/src/error.rs`

## Q57 — build/ticket-15 — deviation

**Question:** A response over the size cap: truncate it and say so, or refuse it? The local surface's tools narrow a large result rather than failing, and `result_too_large` exists as an error code.
**Options considered:** truncate and mark the result / parse first and narrow the parsed value / refuse the transfer with `TooLarge`, before parsing
**Chosen:** Refuse, checking `Content-Length` before the transfer and the accumulated length while reading.
**Decided-by:** agent
**Justification:** Truncating a JSON document produces something that does not parse, which is a confusing failure rather than a smaller answer. Worse is the case where it does parse: half a device list is a wrong answer that nothing downstream can see is wrong, and a caller asking "which devices are stale" would get a confident, incomplete one. Parsing first and narrowing after was rejected because it buys nothing — the cost this cap exists to bound is the transfer and the parse, both of which have already happened by then.

So the cap is enforced twice, in the two places the size is knowable: `content_length()` before anything is transferred, and the running total as chunks arrive, for a server that frames its response as chunked and states no length. Both fail with `TooLarge` carrying the cap, so the caller is told the ceiling they hit rather than left to guess. The cap is the session's `max_result_bytes`, so it is the same ceiling a tool result is held to and moves with the same switch.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/client.rs`

## Q58 — build/ticket-15 — deviation

**Question:** Q55 settled the base-URL override as "https or loopback, no path, checked at startup". Reviewing the code against that answer raised two things it did not cover: whether an address carrying a username or password is one of the accepted shapes, and whether the check runs when there is no credential for it to protect.
**Options considered:** leave userinfo to the https requirement, since the transport is encrypted either way / refuse userinfo outright / check the address only when a client is actually built / check it whenever the tailnet surface is enabled
**Chosen:** Refuse userinfo, and check the address whenever the tailnet surface is enabled, credential or not. `check_base_url` is renamed `checked_base_url` and made public, since the server crate is now the caller.
**Decided-by:** agent
**Supersedes:** Q55 — same question, two shapes it did not decide.
**Justification:** `https://user:pass@host` is encrypted on the wire and still wrong here, for a reason https does not address: a URL is a thing that gets logged, printed in a diagnostic, and pasted into an issue, so userinfo is a secret written in the one place secrets travel in the clear. This server sends its credential as a header and has no use for the form at all, which makes refusing it free. The refusal message deliberately does not echo the URL back, for the same reason.

Checking the address without a credential is the difference between an operator hearing about a misconfiguration now and hearing about it on the day they add a credential — which is the worst day for it, because the surface will have been quietly absent until then and the change that revealed the fault will look like its cause. The cost is that a server with no control-plane credential can now fail to start over a setting it was not going to use, which is the right way round: the setting is only ever set deliberately.

Q55's "hard failure when it is neither" is unchanged, but one route to it has gone. `--max-result-bytes 0` used to reach `StartupError::ControlPlane` by way of the client refusing a zero cap, which reported a typo on the command line as a fault in the control-plane address. The flag is now judged in `Config::resolve_with` alongside the environment variable it duplicates, so the two spellings of one setting give the same answer and neither reaches the client.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/client.rs`, `crates/tailscale-mcp/src/config.rs`, `crates/tailscale-mcp/src/server.rs`

## Q59 — build/ticket-15 — deviation

**Question:** `spec.md` names two test seams and says "the exceptions are the two places behaviour is invisible from above, and both are named below". `tests/control_plane.rs` is a third: it drives the control-plane client through a session's `ToolContext` rather than through a tool call. Does the rule allow it?
**Options considered:** drop the file and prove the wiring in ticket 16 through the first real tailnet tools / keep it permanently as a third named seam / keep it as a provisional seam that ticket 16 is expected to absorb
**Chosen:** Keep it, provisionally, at six tests, and re-express what they prove through tool calls when ticket 16 lands the first tailnet tools.
**Decided-by:** agent
**Justification:** The rule's own words are the argument for the file: behaviour here is invisible from above because there is nothing above it yet. Ticket 15 builds the transport and no tool that uses it, so the alternative to this seam is a ticket whose acceptance criteria — the credential on the wire, the tailnet a path means, the size cap, an unreachable control plane — are asserted by nothing at all until a later ticket happens to cover them, which is how a foundation ships broken. Each of the six becomes an ordinary tool call the moment a tailnet tool exists.

Keeping it permanently was rejected because the rule is right and this file is the shape it warns about: a test that knows how the server is wired will keep passing after the wiring stops reaching any caller.

Two tests written here have already moved out. The base-URL pair is startup behaviour rather than transport behaviour — neither one makes a request — and `server.rs`'s own unit tests are where a `StartupError` is already asserted, beside `neither_surface_is_a_startup_error` and `a_selection_naming_only_hidden_surfaces_does_not_start`. They needed a server built by hand, which was the whole reason the file had a second helper, and they read better next to the other reasons a server refuses to start.

Recorded rather than done quietly, because a third seam that nobody wrote down is a fourth one's precedent.
**Outcome:** assumed
**Ref:** `crates/tailscale-mcp/tests/control_plane.rs`, `.scratch/tailscale-mcp-v1/spec.md`

## Q60 — build/ticket-16 — interpretation

**Question:** ADR-0003 says "enums are strict only for closed sets and free strings elsewhere". Which of the vendored description's enums are closed?
**Options considered:** model each `enum` as a Rust enum and add `#[serde(other)]` catch-alls / judge each of the twenty-two individually / model all of them as documented strings, with the known values named beside the field
**Chosen:** None of the twenty-two is closed. Every one becomes a `String`, with a `&[&str]` constant naming the values the description knows, and the drift test compares those constants against the document.
**Decided-by:** agent
**Justification:** Going through them one at a time is what produced the answer, and the answer was unanimous, which is itself the finding: this API's enums are lists of what exists today. `ConfigurationAuditLog.type` is `['CONFIG']`, a single-member set that only makes sense as the first of several. `LogstreamEndpointConfiguration.destinationType` names eight log vendors and `compressionFormat` three codecs, both of which are markets, not specifications. `Key.keyType` already disagrees with the live API — `docs/research/control-plane-api.md` records the create and response shapes differing — so a strict enum there would fail on a body the server actually sends. `Webhook.subscriptions` is the event catalogue, which grows whenever a feature does. Nothing in the set is a protocol constant.

A `#[serde(other)]` catch-all would parse an unknown value without failing, and that is the trap: it turns a value the control plane meant something by into an `Unknown` variant that discards the string, so a caller is told less than the wire said. ADR-0004 requires Tailscale's shapes back verbatim, and a variant that cannot round-trip is not verbatim.

What a strict enum genuinely buys is documentation, and that is bought here instead: the values live in a named constant next to the field, which is what a tool's parameter description quotes, and the drift test asserts the constant still equals the document's list. So a refreshed description that adds `brotli` fails the build in the same way a new property does, and the failure names the constant to edit — which is the "recorded where the test can explain them rather than silently pass" the ticket asks for.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/models/`, `crates/tailscale-rest/tests/schema_drift.rs`

## Q61 — build/ticket-16 — interpretation

**Question:** The drift test must compare the vendored description against the models. What is it comparing the description *to* — how does a test find out which JSON fields a struct has, and how does it read a 6,700-line YAML document?
**Options considered:** `schemars::schema_for!` on each model / a hand-written table of schema names and field names / a macro that declares the struct and its field list together / a hand-rolled YAML reader vs a parser crate
**Chosen:** A `model!` macro that emits the struct and a `ModelShape` naming the same JSON strings, and `serde_norway` as a test-only dependency for the document.
**Decided-by:** agent
**Justification:** A hand-written table is a second copy of the field names that nothing keeps in step, and its failure mode is the one this test exists to prevent: a field deleted from a struct and forgotten in the table leaves a green test. The macro makes that impossible, because there is one place the JSON name is written and both the `#[serde(rename)]` and the shape come from it.

`schemars::schema_for!` would derive the list from the type, which is stronger still, but it puts a schema-generation dependency in a published library crate for the sake of one test, and it reports what schemars believes about serde's attributes rather than what serde does.

The document is read with a parser rather than by hand for the same reason. An indentation reader that misparses a block silently sees fewer properties than there are, and a drift test that under-reads its input passes. `serde_norway` is a dev-dependency of `tailscale-rest` alone, so nothing it brings reaches a release build.

The shapes are keyed by path rather than by name, because eleven of the description's object types are inline and have no name: `Device.clientConnectivity`, `KeyCapabilities.devices.create`, `ConfigurationAuditLog.actor` and the rest. The test walks the document and builds the same paths, so an inline object is checked exactly as a named schema is, and a new one appearing is a failure rather than a thing the walk steps over.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/models/mod.rs`, `crates/tailscale-rest/tests/schema_drift.rs`

## Q62 — build/ticket-16 — deviation

**Question:** Nine of the description's properties are secrets: a minted auth key, an OAuth client secret, a webhook signing secret, a log stream's bearer token and cloud credentials. The models derive `Debug`. What type holds them?
**Options considered:** `String`, like every other string / `Secret`, which redacts itself / omit them from the models and read them off the raw body
**Chosen:** `Secret`, with `Serialize`/`Deserialize` added to it so it can appear in a model.
**Decided-by:** agent
**Justification:** `String` is the default and it is wrong here for a reason that has nothing to do with these nine call sites: `missing_debug_implementations` means every model derives `Debug`, and a derived `Debug` reaching a `tracing` field is exactly how the key a user just minted ends up in a log file. `secret.rs` says as much in its own module doc — "a derived `Debug` on a `String` field is how tokens end up in logs" — and these are the same fields it was written for.

Omitting them was rejected because the caller needs them: minting an auth key that does not return the key is not a feature. The value is returned verbatim, once, exactly as the control plane sent it; what `Secret` changes is only what happens when something prints the struct instead of reading the field.

The cost is that `Secret` now derives `Serialize`, so a model serialised back out writes the value in the clear — which is correct, since that is the tool result the caller asked for, and is the one place the value is meant to travel. `Display` and `Debug` still redact, so the accident stays impossible and the deliberate path stays open.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/secret.rs`, `crates/tailscale-rest/src/models/key.rs`

## Q63 — build/ticket-16 — deviation

**Question:** Q59 kept `tests/control_plane.rs` provisionally, on the understanding that "each of the six becomes an ordinary tool call the moment a tailnet tool exists" and that ticket 16 would be that moment. Ticket 16 turns out to land models and a drift test and no tool at all. Does the file move now or wait?
**Options considered:** absorb the six into tool calls now / delete the file now and let ticket 17 re-derive what it was proving / keep it unchanged and re-aim the absorption at ticket 17
**Chosen:** Keep it unchanged; the absorption moves to ticket 17, which builds the first tailnet tools.
**Decided-by:** agent
**Justification:** Q59's reasoning was never about ticket 16 in particular — it was that a transport asserted by nothing until some later ticket happens to cover it is how a foundation ships broken. That argument is unchanged: ticket 16 added `send_answer` and forty-five models, and still nothing above the client calls it, so the six tests are the only thing holding the credential, the tailnet, the size cap and an unreachable control plane to their contracts. Absorbing them now is not possible, and deleting them would leave ticket 16's own addition — `Answer<T>` reading the parsed and raw halves off one parse — with a transport underneath it that no test reaches.

Recorded rather than left implicit, because Q59 named a ticket and that ticket has now passed. A provisional seam whose expiry quietly slips one ticket at a time is a permanent seam, which is the thing Q59 rejected. Ticket 17 is the last ticket this can slip to: it builds the device tools, so a tool call that carries the credential to a fake control plane exists there by construction.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/tests/control_plane.rs`, `.scratch/tailscale-mcp-v1/issues/17-devices-and-posture.md`

## Q64 — build/ticket-16 — deviation

**Question:** The drift test as first written walked `components/schemas` and nothing else. The description also carries forty-six objects and ten enumerations inline in `paths` and `components/parameters`, which that walk could not see. `spec.md` says the test asserts "every property is modelled". Does the walk widen, and if it does, are those forty-six modelled here?
**Options considered:** leave the walk at the named schemas and narrow the claim / widen the walk and model all forty-six in this ticket / widen the walk and record the ones this ticket does not model, with the ticket that will
**Chosen:** Widen the walk to the whole document, and carry a `DEFERRED` table naming each unmodelled object and the ticket it belongs to.
**Decided-by:** agent
**Justification:** Narrowing the claim was rejected because the claim is the test's whole value. A tripwire that watches a third of the document and says so is still a tripwire nobody can rely on: the properties most likely to move on a refresh are request bodies, and those are exactly what `components/schemas` does not hold.

Modelling all forty-six here was rejected as the other half of the same mistake. They are request bodies a tool builds from its own parameters and envelopes a listing arrives in — `{"devices": […]}` — which belong to the tools that send and receive them, and so to tickets 17 through 20. Ticket 16 would have swallowed four tickets' worth of shapes to satisfy a test.

The table is what makes the deferral honest rather than silent. A path in it must still be in the document and must not also be modelled, so it cannot rot; an object the description grows that is neither modelled nor listed fails the walk. The list can only shrink, and it shrinks by a ticket landing rather than by anyone editing the table.

Widening also found what the narrow walk was hiding: ten enumerations with no constant — including `keyType`, which the description gives three different lists for, so a create tool quoting the response list would have offered values the control plane rejects — and an `anyOf` branch with properties in the bulk device-attributes body. Both are the under-reading failure Q61 named, arrived at by a different route.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/tests/schema_drift.rs`, `crates/tailscale-rest/src/models/key.rs`

## Q65 — build/ticket-16 — interpretation

**Question:** Q60 says "None of the twenty-two is closed" and gives its reasoning in those terms. The description has thirty-three enumerations, not twenty-two — the missing eleven being the ten inline ones of Q64 plus one this crate had miscounted. Does Q60 stand?
**Options considered:** amend Q60 / record the count here and leave Q60's reasoning intact / re-take the decision across all thirty-three
**Chosen:** Q60's decision stands unchanged; only its count was wrong, and the count is corrected here.
**Decided-by:** agent
**Supersedes:** Q60, as to the number only.
**Justification:** The eleven that Q60 never saw were examined before this was written, and every one is the same kind of thing it judged: `?event` is a catalogue of a hundred and thirty-eight audit events that grows with the product, `?fields` and the role and type filters are lists with an `all` member, and the three `keyType` lists differ from each other, which is Q60's own argument for documented strings made twice over. There was nothing to re-decide.

Recorded rather than fixed quietly because the journal is append-only and a number in it that the code contradicts is worse than a number that is merely wrong: a later reader checking the models against Q60 would find eleven enumerations unaccounted for and no way to tell whether they were an oversight or a deliberate exclusion.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/src/models/mod.rs`, `crates/tailscale-rest/tests/schema_drift.rs`

## Q66 — build/ticket-17 — interpretation

**Question:** `spec.md` says tailnet tools are named `tailnet_<resource>_<verb>` "with a fixed verb vocabulary" and nowhere says what the vocabulary is. Ticket 17 names the first twenty. What is it, and where does the verb go?
**Options considered:** verb first, `tailnet_device_set_tags` / verb last, `tailnet_device_tags_set` / no fixed list, each toolset naming its own operations
**Chosen:** Verb last, from a closed list of nineteen declared whole in `meta::TAILNET_VERBS` and enforced over the whole table.
**Decided-by:** agent
**Justification:** Verb last is what the spec's own `<resource>_<verb>` says, and it sorts usefully: `tailnet_device_routes_get` and `tailnet_device_routes_set` are adjacent in a listing, where verb-first would put them at opposite ends of the device tools. A model scanning an alphabetical tool list sees a sub-resource and everything that can be done to it.

The list is declared whole rather than grown as tools land, which is the part worth recording because it looks like speculative generality and is the opposite. Ninety-three tools arrive across five tickets; a vocabulary that gains a word whenever a name does not fit is not a vocabulary, and the failure it prevents is the same operation being `delete` in one toolset and `remove` in the next, which a caller then has to learn twice. The entries with no tool yet are the constraint on tickets 18 through 20, not dead weight.

No fixed list was rejected for the same reason: it is the state the spec asked not to be in.

The verbs that are not CRUD are the API's own words — `authorize`, `expire`, `approve`, `resend` — rather than translations of them, because a caller reading Tailscale's documentation and a caller reading this tool list should not have to map between two vocabularies.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/meta.rs`, `crates/tailscale-mcp/src/tools/mod.rs`

## Q67 — build/ticket-17 — deviation

**Question:** Eight of the twenty endpoints answer with an empty body. ADR-0004 says Tailscale's bodies come back in Tailscale's shape, and an empty body's shape is `null`. Does the tool answer `null`?
**Options considered:** answer `null`, verbatim / answer a small report saying what was done / answer the resource as it now stands, by reading it back
**Chosen:** A small report — `{"done": "deleted", "device_id": "…"}` — on the eight that answer with nothing.
**Decided-by:** agent
**Justification:** ADR-0004 is about not renaming or reshaping what the control plane sends, and nothing is being reshaped here: there is no body to keep faithful to. What `null` costs is real — a caller cannot tell it from a tool that lost its answer, and an agent that cannot tell success from breakage will retry a deletion.

Reading the resource back was rejected as a second call the caller did not ask for, which can fail on its own and would make a delete answer with a 404 it had itself caused.

The report is deliberately not shaped like a resource: `done` is a phrase for a reader rather than a status code to branch on, and the only other fields are the identifiers the call was given, so nothing here can be mistaken for something the control plane said.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_devices.rs`

## Q68 — build/ticket-17 — deviation

**Question:** Two defects surfaced while building this ticket that belong to earlier ones: `redact` scanned by byte index and panicked on a multi-byte character, and `instructions::render` decided a surface was present from the toolsets selected rather than from whether the backend was there. Fix them here or ticket them?
**Options considered:** fix both here / fix the panic here and ticket the instructions / ticket both
**Chosen:** Fix both here, each with a test that fails without the fix.
**Decided-by:** agent
**Justification:** Neither was reachable before this ticket, which is why neither was found earlier. The panic needs a message carrying a character outside ASCII, and this server's own prose only started carrying em dashes into error hints with the refusals written here. The instructions bug needs a session that selects a tailnet toolset while the tailnet surface is absent, which needs a tailnet toolset to exist.

The panic is the more serious: `redact` is on the path of every message and hint a caller sees, and a panic in a handler takes the session down rather than failing the call — which is the thing `report`'s own documentation says it exists to avoid.

The instructions bug is quieter and worse in one way. A session with no credential hid every tailnet tool and then told the model the tailnet surface was available, which is precisely the failure the module's own doc comment describes itself as preventing: "a model that has been told the tailnet surface is off will stop proposing tailnet tools". `Gate::offers` now asks both questions — selected, and present — and is the one place either is asked.

Ticketing them was rejected because a found panic left in the tree is a panic somebody meets, and because both fixes are three lines and a test each.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/error.rs`, `crates/tailscale-mcp/src/instructions.rs`, `crates/tailscale-mcp/src/gating.rs`

## Q69 — build/ticket-17 — deviation

**Question:** Ticket 17's AC2 asks that `limit`/`offset` slice the listing "without changing the response shape". A slice with nothing said about it is indistinguishable from a short tailnet. Does the windowed answer stay exactly `{"devices": [...]}`?
**Options considered:** the same shape, silently sliced / add a `window` object beside `devices` / return the slice only when it is the whole list, and refuse otherwise
**Chosen:** An unwindowed call answers the control plane's own body, byte for byte. A call that gives `limit` or `offset` gets `{"devices": [...], "window": {total, returned, offset, limit}}`.
**Decided-by:** agent
**Justification:** The criterion's purpose is that `devices` still means what it meant — a caller that reads `answer["devices"]` and expects a list of devices is not broken by asking for a window. That holds. What the criterion cannot have meant is that the server may drop 900 of 950 devices and say nothing, because the endpoint has no pagination and sends no counts: without `total` there is no signal anywhere that the answer is partial, and an agent reading a 50-device answer from a 950-device tailnet would conclude the tailnet has 50 devices.

The shape is only added when it is earned. `(None, 0)` — no limit, no offset — takes the untouched body straight through, so ADR-0004's "returned verbatim" is unweakened for every caller that did not ask for a window, which is the default and the common case.

`limit` is echoed alongside the counts because `returned: 0` is otherwise ambiguous: a limit of zero and an offset past the end are different mistakes and a caller should be able to tell which they made.

Refusing an over-large window was rejected as inventing a failure the caller can already see from `total`.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_devices.rs`, `crates/tailscale-mcp/tests/tailnet_surface.rs`

## Q70 — build/ticket-17 — interpretation

**Question:** Ticket 17 says "device deletion, key expiry and de-authorisation are destructive", but the tool inventory classifies `tailnet_device_authorize` as WRITE. One tool, two tiers, and the difference is an argument: `authorized: true` connects a device, `authorized: false` disconnects it.
**Options considered:** register it as Destructive / register it as Write and let the argument through / split it into two tools, one per direction
**Chosen:** Registered as Write with `varying: true`, and the handler refuses `authorized: false` unless the session's tier is at least Destructive.
**Decided-by:** agent
**Justification:** Both source documents are right about the case they describe, and the tier a tool is registered at is a single value applied before the handler runs — so a tool whose danger is in its arguments cannot be classified honestly by registration alone. Registering it Destructive would put authorising a device, which is how a tailnet with device approval admits a new machine at all, behind `--allow-destructive`, and a routine admission would need the flag that also permits deleting the tailnet's devices. Registering it Write and letting `false` through would make the ticket's sentence false.

So the row carries the floor and the handler makes the same decision the gate would have. This is not a new mechanism: the passthrough already does exactly this, for exactly this reason, and `varying: true` is the existing flag that marks a row whose tier is a floor rather than the whole truth.

Splitting into `_authorize` and `_deauthorize` was rejected because the API is one endpoint with one boolean, and two tools would be this server inventing a distinction a caller would then have to learn.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_devices.rs`, `.scratch/tailscale-mcp-v1/issues/17-devices-posture.md`

## Q71 — build/ticket-18 — deviation

**Question:** The drift walk read one media type per request body and per response — whichever the parsed map yielded first. The policy endpoints describe the same body twice, under `application/json` and `application/hujson`. Widen the walk again, or leave it?
**Options considered:** leave it / walk every media type at one path each / walk every media type, naming the path when there is more than one
**Chosen:** Every media type is walked. One media type keeps the plain path; several are named `… body (application/json)`.
**Decided-by:** agent
**Justification:** The same failure as Q64, one level down. `acl/validate` carries its real request shape under `application/json` and a bare string under `application/hujson`, and the map yielded the string first — so the walk read the string, recorded nothing, and a five-property test case sat unmodelled while the test stayed green. A tripwire that reads part of its input and passes is worse than no tripwire, because it is believed.

Naming only the ambiguous paths is what keeps the widening from churning the whole table: nearly every body in the description carries one media type, and a suffix on all of them would add noise to sixty paths to disambiguate four. The names that do appear say exactly which schema failed, which is what a failure here is for.

`a_body_carrying_two_media_types_is_read_at_both` asserts both halves — that `acl/validate` yields two schemas and that `POST /keys` still yields one at the plain path — so a later simplification back to `find_map` fails rather than silently under-reading again.
**Outcome:** applied
**Ref:** `crates/tailscale-rest/tests/schema_drift.rs`

## Q72 — build/ticket-18 — interpretation

**Question:** Six of the eleven DNS endpoints overwrite a whole list or document and one merges. The API calls them all `set…`. What are the tools called?
**Options considered:** follow the API's `set` throughout / `_replace` for a full overwrite, `_update` for a merge, `_set` for a single value / one tool per resource with a `mode` parameter
**Chosen:** `_replace` overwrites, `_update` merges, `_set` carries a single value. `tailnet_dns_split_update` is the only `_update` here.
**Decided-by:** agent
**Justification:** The verbs come from the closed list Q66 fixed, and all three are in it, so this is a choice about which of them each endpoint gets rather than about growing the vocabulary.

An agent calling `tailnet_dns_nameservers_set` with the one nameserver it wants to add would remove every other nameserver in the tailnet. That is the most expensive mistake this toolset can produce, it is silent, and the name is the only place a caller sees the difference before making the call. `split-dns` makes the case plainest: `PATCH` and `PUT` are the same resource and differ only in this, so two names that differ only in this is the honest mapping.

Following the API's own `set` was rejected because the API distinguishes the two by HTTP verb, which a tool call does not show; ADR-0004 is about not renaming Tailscale's *data*, and a tool name is this server's.

A `mode` parameter was rejected as a defaulted argument deciding whether a tailnet keeps its DNS.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_dns.rs`

## Q73 — build/ticket-18 — deviation

**Question:** `POST /tailnet/{tailnet}/acl` accepts a write with no `If-Match` header and replaces whatever is there. Ticket 18 asks that a write "must carry the version identifier or an explicit statement that it is writing over the default". Refuse before sending, or send and let the control plane decide?
**Options considered:** send whatever the caller gave / refuse before sending when neither guard is present / always read the policy first and use the version that read returned
**Chosen:** Refused before the request is built. `etag` or `over_default: true`, exactly one of them; neither is an `invalid_args` refusal that names both, and both together is refused rather than ranked.
**Decided-by:** agent
**Justification:** The control plane cannot make this decision, because to it an absent `If-Match` is a valid request meaning "replace whatever is there" — the failure mode is a success. The policy file decides who may reach what across the whole tailnet, and the loss is somebody else's change, made between the read and the write, that the caller never saw and cannot recover.

Reading the policy inside the write and using that version was rejected as the same overwrite wearing a seatbelt: it would quote a version the caller never looked at, which defeats the header's entire purpose.

Both guards together is refused rather than resolved because they say different things — one writes over the version you read, the other only over an untouched default — and picking one would be this server deciding which of two contradictory instructions a caller meant.

A stale version comes back as `conflict` with a hint naming `tailnet_policy_get`, which is the whole remedy. The mapping is in `From<ApiError>` rather than in the tool because 412 belongs to no other endpoint in the description, so a general mapping is a true one.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_policy.rs`, `crates/tailscale-mcp/src/error.rs`

## Q75 — build/ticket-18 — deviation

**Question:** `spec.md` and ticket 18 both call the policy *read* "the single documented exception to forwarding the response verbatim, because the identifier is a header". `tailnet_policy_set` answers with the same `{etag, format, policy}` report. Is that a second exception, and is it allowed?
**Options considered:** answer the body alone / answer the report, and record the exception / answer nothing but a `Done`, as the empty-body writes do
**Chosen:** The report, and this entry.
**Decided-by:** agent
**Justification:** The write cannot forward its body verbatim for the same mechanical reason the read cannot. `POST /acl` answers with the policy file, which is HuJSON — text, not JSON — so there is no body to forward into structured content. It has to be put somewhere and named, and naming it `policy` beside the format it is in is the same shape the read already gives. Two shapes for one document would be the worse outcome.

The `ETag` is the second half. The control plane sends a new one with the write, and it is what a caller's *next* write has to quote; dropped, a caller making two edits has to read in between for a value it was already handed.

Answering a `Done` was rejected because the write does return the document, and Q67's reasoning is about endpoints that return nothing.

So the exception is not new in kind — it is the same exception, on the second endpoint that carries the same document. `spec.md`'s sentence is left as written, because it accurately describes the one case it was written about; this entry is where the second is recorded.

Separately, and found by the same review: the write is no longer declared idempotent. A guarded replace cannot be repeated — the second call fails, because the `etag` it quoted is now stale and the policy is no longer the untouched default — and `idempotent: true` was a claim the tool could not keep.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_policy.rs`

## Q74 — build/ticket-19 — interpretation

**Question:** `GET /tailnet/{tailnet}/keys` takes `all`, which the description marks required while its own text calls it optional. The two readings give different listings: without it a user-owned credential sees only its own user's keys. What does the tool send?
**Options considered:** send it only when given / always send it, defaulting to true / always send it, defaulting to false
**Chosen:** Always sent; `all` defaults to true.
**Decided-by:** agent
**Justification:** "Required" and "optional" cannot both be honoured, and the parameter is not one a caller should have to know about to get a sensible answer. Sending it always removes the ambiguity from the wire: whatever the control plane does with an absent `all` stops mattering, because it is never absent.

True is the default because a listing that silently omits keys is the worse failure. An operator asking what keys exist and being shown a subset — with nothing in the answer saying it is a subset — would conclude the others do not exist. `false` remains available for the narrower question, which is the one a caller has to ask for deliberately.

The Go client sends `all=true` unconditionally, which is corroboration rather than the reason.

`schema_drift.rs`'s `a_key_listing_requires_a_parameter_it_calls_optional` already holds the description to this disagreement, so a refresh that settles it fails the test excusing it.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_keys.rs`

## Q76 — build/ticket-19 — deviation

**Question:** Six invitation endpoints accept only a credential owned by a person; a token minted from an OAuth client or a federated identity is refused however wide its scopes are. Ticket 19 asks that "under a tailnet-owned credential their failure carries a hint naming the requirement". How, when this server cannot tell which kind of credential it holds?
**Options considered:** check the credential's kind before calling / add the requirement to every failure from those tools / add it only to a refusal that could be it
**Chosen:** Added on the way back, to a refusal carrying 400, 401 or 403 and no more specific code.
**Decided-by:** accepted from spec
**Justification:** Checking first is impossible in the general case and misleading in the specific one. A bearer token does not say what minted it; the server knows what it was *configured* with, but an operator may have supplied a token obtained elsewhere, and refusing a call on a guess about the credential's provenance would block calls that would have worked.

Which refusals get the hint is the part worth being careful about. A 404 is a missing invitation and a 429 is the documented one-a-minute rate limit; hanging an explanation about credential ownership off either would send a caller to look at the wrong thing. So the hint goes only where the control plane has refused on permission, which is where the explanation could be the answer.

The hint states the requirement rather than asserting the cause: the credential may also simply lack scopes, and the sentence is written so that a caller with the right kind of credential is not misled.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_invites.rs`

## Q77 — build/ticket-19 — deviation

**Question:** `suspendUser` and `restoreUser` end in verbs that Q66's closed nineteen-word vocabulary does not contain, and the test enforcing it fails. Rename the tools, or widen the vocabulary?
**Options considered:** rename to `tailnet_user_disable`/`_enable` / fold both into one tool with a boolean / add `suspend` and `restore` to the vocabulary
**Chosen:** Added, making the list twenty-one words.
**Decided-by:** agent
**Supersedes:** Q66, as to the membership of the list only
**Justification:** Q66 said "a name that does not fit is a name to reconsider rather than a word to add", and this is that reconsideration reaching the other answer. Suspend and restore are Tailscale's own words: the operations are `suspendUser` and `restoreUser`, and the admin console's buttons say Suspend and Restore. `disable`/`enable` are in the vocabulary but would be this server renaming something Tailscale has already named, which is what ADR-0004 exists to prevent on the data and is no better on the verbs.

Folding them into one tool with a `suspended` boolean was rejected because the API has two endpoints and no such boolean; inventing one would be a defaulted argument deciding whether somebody keeps their access.

What Q66 was actually protecting — that the vocabulary is closed, enforced over the whole table, and verb-last — is untouched. The check did its job: it made this an explicit decision rather than a name that quietly did not fit. The list was assembled from the endpoint inventory and these two were missed; adding them is a correction of that reading, and the enforcement test is what found it.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/meta.rs`, `crates/tailscale-mcp/src/tools/tailnet_users.rs`

## Q78 — build/ticket-19 — deviation

**Question:** Four invitation endpoints answer with a bare JSON array at the top level, where every other listing on this surface arrives wrapped. A tool result's structured content is an object. What do those four answer with?
**Options considered:** forward the array (impossible) / wrap in `{"invites": […]}` / wrap in a name of this server's own, like `{"items": …}`
**Chosen:** `{"invites": […]}`, and only when the body really is an array.
**Decided-by:** agent
**Justification:** ADR-0004 asks that Tailscale's bodies come back in Tailscale's shape, and here there is no way to do that: the protocol cannot carry a top-level array in structured content. So the choice is which envelope, not whether.

`invites` is the API's own naming convention rather than an invention — `{"devices": …}`, `{"keys": …}`, `{"users": …}`, `{"oauthApps": …}` are how it wraps every listing it does wrap, and these four look like the ones it forgot. A caller who knows the API will guess this name first.

The wrap is conditional on the body being an array, so a control plane that starts wrapping them itself is followed rather than double-wrapped. That is one line and a test, and it means a refresh of the API does not silently produce `{"invites": {"invites": …}}`.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/tools/tailnet_invites.rs`

## Q79 — build/ticket-19 — deviation

**Question:** Ticket 19 asks that a minted key's secret appear "in no log line at any level". Nothing in this workspace logs a response body — but `rmcp` traces whole JSON-RPC messages, results included, at `TRACE` and `DEBUG`. Is that this server's problem?
**Options considered:** narrow the claim to this server's own code / cap the SDK's verbosity in the resolved filter / stop using the SDK's logging
**Chosen:** The resolved log filter carries `rmcp=info` on the end, unless the operator has named `rmcp` themselves.
**Decided-by:** agent
**Justification:** It is this server's problem, because this server builds the subscriber. The default filter never reached the SDK's message tracing, so the exposure needed an operator to raise the volume — and an operator who sets `--log=debug` to follow this server's own work is asking about this server, not asking to have every auth key, OAuth client secret and invite URL written to standard error. The two requests are different and were being served by one switch.

The cap is skipped when the filter already mentions `rmcp`, so `--log=info,rmcp=trace` still gets the wire. That keeps the escape hatch for someone debugging the transport, while making it a thing they asked for by name rather than a side effect.

This was found by writing the test the criterion asked for rather than by reading the code and concluding it was fine — which is the whole difference between the two, and why the test installs the filter an operator actually gets rather than a bare `TRACE` nobody runs.
**Outcome:** applied
**Ref:** `crates/tailscale-mcp/src/config.rs`, `crates/tailscale-mcp/tests/minted_secrets.rs`

## Q80 — build/ticket-19 — interpretation

**Question:** Three write parameters were declared required here that the description does not require: a user invitation's `role` (`"Optionally specifies a user role"`, `"default": "member"`), a key create's `key_type` (`"Defaults to \"auth\" if omitted."`) and a key update's `key_type` (no `required` list on the body at all). Does this server require what the control plane does not, and does it supply the documented default itself?

**Options considered:** keep them required, so a caller always states a consequential value / make them optional and send the description's default when absent / make them optional and send nothing when absent

**Chosen:** Optional, and an unstated field is not sent.

**Decided-by:** agent

**Justification:** Neither half of the alternative survives its own reasoning.

Requiring them would be this server refusing calls the API accepts. The usual argument for over-strictness — that a silent default is a decision made by omission — does not hold here, because both defaults are documented and neither is the dangerous direction: `member` is the least-privileged role, and `auth` is the key type the endpoint exists to mint. Q73's guard on the policy write is the shape that does justify over-strictness, and it earned it by having a failure mode that looks like success; there is none here.

Sending the default ourselves would be worse than requiring it. A default written into this server is a copy of a value the control plane owns, and a copy goes stale silently: if `member` ever stops being the default, every omitted `role` would keep meaning `member` because we said so, and nothing would fail. Not sending the field leaves the decision where the description puts it, and the tool descriptions state what omission gets so a caller is not left guessing.

This is the same rule ticket 18 already applied to MagicDNS — "the control plane owns them and states them better than a guess would" — and Q74 is not an exception to it but its converse: `all` is always sent precisely because the description contradicts itself about what an absent one means, so there is no owned default to defer to.

**Outcome:** applied

**Ref:** `crates/tailscale-mcp/src/tools/tailnet_keys.rs`, `crates/tailscale-mcp/src/tools/tailnet_invites.rs`

## Q81 — build/ticket-20 — interpretation

**Question:** The services endpoints have two path spellings. The vendored description documents `/tailnet/{tailnet}/services`; Tailscale's own Go client calls `/tailnet/{tailnet}/vip-services`. The drift test has recorded the disagreement since ticket 16 and neither source settles it. Ticket 20 asks that "service naming follows the path the live API serves, not only the published description". Which does the server send?

**Options considered:** send the documented one and let a wrong guess be a 404 / send the Go client's one on the grounds that shipped code beats a description / send the documented one and retry the other on a 404 / make the base path a configuration option

**Chosen:** Send the documented spelling; on a 404, send the same call again at the other one.

**Decided-by:** agent

**Justification:** The criterion says to follow the path the *live* API serves, and asking it is the only way to find out from here. Both static answers are guesses, and a guess that is wrong turns every one of these seven tools into a 404 with nothing in the message about why.

Retrying is safe here specifically, not in general: all seven are addressed by a path that either exists or does not, none of them acts before answering a 404, and a request that reached a base path the control plane does not serve did nothing. Where the service itself is genuinely missing, the second call answers 404 too and that is what the caller gets — the retry costs one extra request on a call that was going to fail anyway.

A configuration option was the alternative worth taking seriously, and it fails on who would set it: an operator would have to know which spelling their control plane serves, which is exactly the thing nobody can find out except by trying.

The order is documented-first because the description is what this repo is held to elsewhere — the drift test, the known-value lists, the model shapes — and a build that quietly preferred an undocumented path would be inconsistent with all of it.

**Outcome:** applied

**Ref:** `crates/tailscale-mcp/src/tools/tailnet_services.rs`

## Q82 — build/ticket-20 — interpretation

**Question:** `GET /organizations/{organization}/tailnets` is the only paginated endpoint in the whole API, with a `cursor` and a `limit` capped at 100. Ticket 20 asks that "the paginated listing follows its cursor and respects the API's maximum page size", and separately that it is "the only one whose pagination is exposed". Following and exposing are different tools. Which is it?

**Options considered:** expose `cursor` and `limit` and let the caller page / follow to the end and hide pagination entirely / follow by default, and take one page when the caller passes a cursor

**Chosen:** Follow by default; a `cursor` argument takes exactly one page instead.

**Decided-by:** agent

**Justification:** The ticket asks for both and they are not in conflict: following is the default because "what tailnets does this organisation have" is the question an agent actually asks, and an answer that silently stopped at the first hundred would be read as the whole organisation. Exposing the cursor is what makes the other question askable.

The walk is bounded at ten pages — a thousand tailnets, which no organisation has. The bound is not about size but about control: a control plane that keeps answering with a cursor would otherwise hold a tool call open until the session's timeout, and this way the answer arrives with the cursor and a sentence saying it stopped early. An answer that quietly ended is the failure this is guarding against, and it is the same failure the `window` object exists for in `tailnet_device_list` (Q69).

`limit` above 100 is refused rather than clamped. A caller asking for 500 and being given 100 without being told has been handed a short page it will read as a complete one — the same failure again, in the shape the API's own maximum makes easy.

The client-side window of Q69 is the opposite case and stays as it is: `tailnet_device_list` has no pagination to follow, so slicing here is all that is available. Where an endpoint really paginates, its own mechanism is used and nothing is sliced.

**Outcome:** applied

**Ref:** `crates/tailscale-mcp/src/tools/tailnet_org.rs`
