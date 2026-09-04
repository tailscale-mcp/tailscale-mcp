# 24 — Diagnosis, tool listing, version and setup subcommands

Status: done
Milestone: 4 — HTTP mode and subcommands
Blocked by: 23

Four subcommands on the binary. Diagnosis checks the CLI's presence and version, the configured credentials and control-plane reachability, and reports each with a remedy. Tool listing prints what a given preset and tier combination would expose, in a human table or machine-readable form, read from the metadata table. Version prints the server's own version and the SDK's protocol support. Setup prints a client configuration snippet for the named client without writing any file.

## Acceptance criteria
- Diagnosis reports each check independently and exits non-zero when a check fails.
- Tool listing counts match the agreed table for every preset and tier combination.
- Setup prints a snippet that, pasted into the named client, produces a working server; it writes nothing.
- No subcommand requires credentials except the ones that check them.

## As built

`crates/tailscale-mcp/src/subcommands.rs`, reached from `main` before any
server is built. Each returns a `Report` — the text and whether it was bad news
— which `main` turns into an exit code and nothing else, so a test can call the
function rather than spawn the binary.

`diagnose` makes three checks independently: the CLI and its version, the
credential, and whether the control plane answers. A missing binary does not
stop the credential being checked, because somebody running a diagnosis wants
the whole list. Each check is passed, skipped or failed, and only failed changes
the exit code — a check the operator switched off did not pass, and saying it
did would send them looking in the wrong place (Q93). Every failure carries a
remedy; a test asserts that, because a failed check without one is a complaint
rather than a diagnosis.

`tools` reads the metadata table and applies `Gate::unchecked`, so the answer is
what the selection offers rather than what this machine happens to have — the
same everywhere. Two forms, as the ticket asks: `--json` for a machine, a table
otherwise. The nine preset-and-tier counts are derived rather than agreed
separately, and pinned in a test that reconciles the total against the four
numbers `spec.md` did fix (Q92).

`version` prints this crate's version, rmcp's, and every protocol version the
SDK knows, with the preferred one named. It answers before the configuration is
resolved: a bad environment variable should not stop a server saying what it is,
which is the one question worth asking of one that will not start. The rmcp
version is written down and held to the workspace manifest by a test, because a
dependency's version cannot be read at runtime (Q97).

`setup` prints and writes nothing: a client's configuration file has the
operator's own edits in it. Each client gets the shape that client accepts —
`mcpServers`, `servers` or `context_servers` — and Claude Code additionally gets
the `add-json` form, which takes the server object rather than the wrapper
(Q98). The snippet names only the settings the operator actually changed, since
spelling out a default is how a snippet goes stale, and a test holds the carried
set and the deliberately-excluded set to `ENV_VARS` between them, which is what
caught `--cli-path`, `--max-result-bytes` and `--log` going missing (Q96). The
credential is deliberately not in the snippet, and the comment says where to put
it instead.

Only `diagnose` needs a credential or touches the network. `tools`, `version`
and `setup` read compiled-in data, which is the criterion "no subcommand
requires credentials except the ones that check them".

The settings a subcommand might want are `global = true`, so they may be given
after the subcommand as well as before it.

Three files rather than one: `subcommands/mod.rs` holds what shares the metadata
table, `setup.rs` what changes when an editor does, `policy.rs` what changes when
a policy tool does. The clients are one enum in `config`, so `ValueEnum` derives
the parsing and the list of variants, and a sixth client cannot be added without
a test asking what shape it takes (Q95).

Eleven tests in `crates/tailscale-mcp/tests/subcommands.rs`. They construct
backends explicitly rather than calling `Backends::discover`, which reads the
process environment — before that they reached the real control plane on a
machine with `TAILSCALE_API_KEY` set.
