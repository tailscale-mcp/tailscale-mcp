# 05 — Process execution model

Status: ready-for-agent
Milestone: 1 — Skeleton and core
Blocked by: 01

The CLI wrapper crate's core: spawn the `tailscale` binary with an argument array and no shell, stdin closed, a minimal environment, and a timeout that terminates gracefully before killing. Binary discovery is the path, then the macOS application bundle location, then an environment override.

Reads run concurrently; local write and destructive operations serialise behind a single lock. Commands that would otherwise run until interrupted exist only in bounded forms. Secrets are never placed on the command line: a literal secret is written to a private temporary file for the life of the call and passed to the CLI by file reference.

## Acceptance criteria
- Tested against a stub binary rather than a real daemon: argument construction, environment scrubbing, timeout, graceful termination followed by kill, and the exit-code and output capture.
- Two concurrent reads overlap; a write and a destructive operation do not.
- A secret passed as a literal never appears in the spawned argument list; the temporary file is private and is removed when the call ends, including on timeout.
- The binary override is honoured and a missing binary produces the backend-unavailable code.
