# 11 — Files, certificates and host configuration toolset

Status: ready-for-agent
Milestone: 2 — Local surface
Blocked by: 09

The 9 tools covering file transfer to and from peers, transfer targets, certificate issuance, metrics writing, and the two host configuration commands that survived exclusion. Host filesystem paths are permitted at the write tier in this release, with the allow-list mechanism designed in but not enabled.

Certificate issuance requires explicit certificate and key paths and never writes material to standard output. File transfer and certificate issuance carry the longer timeouts agreed for them.

## Acceptance criteria
- Certificate issuance without explicit paths is rejected; no path writes key material to standard output.
- File transfer accepts a literal path and reports progress or completion within its timeout.
- Receiving files writes into the requested directory and never waits or loops.
- Tool descriptions state that these tools read and write host files.
