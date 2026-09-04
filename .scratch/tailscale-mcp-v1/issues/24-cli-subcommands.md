# 24 — Diagnosis, tool listing, version and setup subcommands

Status: ready-for-agent
Milestone: 4 — HTTP mode and subcommands
Blocked by: 23

Four subcommands on the binary. Diagnosis checks the CLI's presence and version, the configured credentials and control-plane reachability, and reports each with a remedy. Tool listing prints what a given preset and tier combination would expose, in a human table or machine-readable form, read from the metadata table. Version prints the server's own version and the SDK's protocol support. Setup prints a client configuration snippet for the named client without writing any file.

## Acceptance criteria
- Diagnosis reports each check independently and exits non-zero when a check fails.
- Tool listing counts match the agreed table for every preset and tier combination.
- Setup prints a snippet that, pasted into the named client, produces a working server; it writes nothing.
- No subcommand requires credentials except the ones that check them.
