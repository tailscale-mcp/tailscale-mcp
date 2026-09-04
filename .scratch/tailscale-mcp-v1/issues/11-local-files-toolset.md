# 11 — Files, certificates and host configuration toolset

Status: done
Milestone: 2 — Local surface
Blocked by: 09

The 9 tools covering file transfer to and from peers, transfer targets, certificate issuance, metrics writing, and the two host configuration commands that survived exclusion. Host filesystem paths are permitted at the write tier in this release, with the allow-list mechanism designed in but not enabled.

Certificate issuance requires explicit certificate and key paths and never writes material to standard output. File transfer and certificate issuance carry the longer timeouts agreed for them.

## Acceptance criteria
- Certificate issuance without explicit paths is rejected; no path writes key material to standard output.
- File transfer accepts a literal path and reports progress or completion within its timeout.
- Receiving files writes into the requested directory and never waits or loops.
- Tool descriptions state that these tools read and write host files.

## As built

Eleven tools, not nine: design-round Q24 keeps the four Taildrive commands as
typed tools, and `local-files` is where they belong. Recorded in DECISIONS Q28,
with the toolset placement of `tailscale_drive_list` in Q30. The "9" in the
description was never reachable from the category list beside it, which sums to
seven; `spec.md`'s totals were corrected to the built counts in DECISIONS Q35.

The allow-list is `PathPolicy` on `ToolContext`, shipping as `Unrestricted` and
already consulted by every path-taking parameter (DECISIONS Q34).

Review found and fixed three things after the first cut: the Taildrive share
table lost rows silently (Q36), the four long-running tools held the exclusive
process lock and stalled every concurrent read (Q33), and the descriptions had
drifted to a term the glossary avoids, which was missing an entry (Q37).
