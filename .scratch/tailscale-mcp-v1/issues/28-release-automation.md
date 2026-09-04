# 28 — Release automation

Status: ready-for-agent
Milestone: 5 — Packaging
Blocked by: 27

Binary release building and publishing for the supported platforms, and automated version and changelog management driven by commit messages in the conventional form. All three crates publish together at the same version. The first release is 1.0.0 per ADR-0005.

## Acceptance criteria
- A tagged release produces binaries for the supported platforms with checksums.
- The changelog is generated from commit history and the version bump follows the commit types.
- A dry run publishes nothing and reports what it would publish.
- The three crates publish in dependency order.
