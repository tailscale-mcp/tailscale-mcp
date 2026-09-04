# 18 — DNS and policy toolsets

Status: ready-for-agent
Milestone: 3 — Tailnet surface
Blocked by: 16

The 11 DNS tools and the 4 policy tools. The policy read returns the document together with its version identifier, which is the single documented exception to forwarding the response verbatim, because the identifier is a header. Both document formats are supported, and the detail form returns the upstream warnings and errors.

A policy write must carry the version identifier or an explicit statement that it is writing over the default, and a version mismatch produces the conflict code with a hint to re-read. Validation accepts either a proposed document or a set of tests, and treats an empty success response as a pass. Preview takes a subject and a subject type.

## Acceptance criteria
- Reading returns identifier, format and document; writing without either guard is rejected before the request is sent.
- A mismatched identifier produces the conflict code.
- Validation of a set of tests against the current policy and of a hypothetical document are distinguished correctly.
- All DNS tools round-trip against the fake server, and the split configuration update and replace forms behave differently as the API defines them.
