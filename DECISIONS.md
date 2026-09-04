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
