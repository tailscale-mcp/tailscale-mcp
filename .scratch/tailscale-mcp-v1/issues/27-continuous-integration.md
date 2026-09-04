# 27 — Continuous integration

Status: ready-for-agent
Milestone: 5 — Packaging
Blocked by: 07

Test on the two first-class platforms and build on the third. Separate jobs for the minimum supported toolchain, linting, formatting and the dependency licence check. End-to-end tests never run here, and no job requires a credential.

## Acceptance criteria
- The matrix runs on both first-class platforms and builds on the best-effort one.
- The minimum-toolchain job fails if a dependency raises the requirement.
- Linting, formatting and licence checks fail the build on violation.
- A pull request from a fork runs the full suite without secrets.
