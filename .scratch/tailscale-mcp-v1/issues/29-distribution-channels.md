# 29 — Distribution channels

Status: ready-for-agent
Milestone: 5 — Packaging
Blocked by: 28

The five channels beyond the release binaries: the scoped npm launcher that downloads and verifies a release binary, the container image, the Homebrew tap, the registry listing, and the plugin manifest for the client that supports one. Names are settled and recorded in the research notes.

Publishing under the npm scope needs a token with write access to it, which the maintainer must supply at this point.

## Acceptance criteria
- The launcher installs and runs on both first-class platforms and verifies the checksum before executing.
- The container image runs the server with no arguments and honours the environment variables.
- The tap formula installs a working binary.
- The registry listing validates against the registry's schema, and the plugin manifest loads in its client.
