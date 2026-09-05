# 26 — End-to-end tests against a real node and tailnet

Status: done
Milestone: 4 — HTTP mode and subcommands
Blocked by: 25

Tests that exercise the real CLI on the host and the real control plane, gated behind environment variables so they are skipped by default and never run in continuous integration. Read-only by default; any test that writes must be opt-in separately and must clean up after itself.

## Acceptance criteria
- With the gates unset, the suite skips these tests and reports why.
- With a read-only credential set, the tailnet read paths pass against a real tailnet.
- With the CLI present, the local read paths pass on the developer machine.
- No test writes to a real tailnet unless a second, separate gate is set.

## As built

`crates/tailscale-mcp/tests/end_to_end.rs`, behind three variables:
`TAILSCALE_MCP_E2E_LOCAL` for the local read paths, `TAILSCALE_MCP_E2E_TAILNET`
for the tailnet ones, and `TAILSCALE_MCP_E2E_WRITE` for the single write.
Variables rather than a cargo feature, because a build inherits a feature and a
shell does not, and these must never run in continuous integration.

With the gates unset every test returns early and says which variable it would
have needed. Because a skipped test and a passing test look identical in
`cargo test`'s output, one test always runs and reports which gates are open —
saying, when none are, that every test below did nothing (Q99).

They go through the in-process client and a fully constructed server, the same
seam the rest of the suite uses; what differs is that the backends underneath
are the real ones.

The local paths read status, version, preferences, addresses and DNS status.
Verified against the Tailscale 1.102.2 on this machine: all five pass.

The tailnet paths list devices, read the policy file, the DNS configuration, the
tailnet settings and the keys, then read back one device by the identifier the
listing gave — which is the read that would catch an identifier this server
builds wrongly. All of them are reads, so a read-only credential is enough.
**Run against a real tailnet on 2026-09-05**, with an API access token: all six
reads pass, including the read-back by the listed identifier. The first run
failed, and on something no other test could have caught — the DNS read named
`tailnet_dns_get`, which is not a tool and never has been. The toolset offers
`tailnet_dns_configuration_get` and four narrower readers instead. A gated test
says nothing until somebody opens the gate, which is why the ticket counted the
run as outstanding rather than the code as written.

The write gate covers one test, which sets a custom posture attribute on a
device and deletes it again, reading back both times.

**Attempted on 2026-09-05 and still unproven**, for a reason no credential
fixes: custom posture attributes are a paid Tailscale feature, and the control
plane answers a write with 403 "feature not available on current billing plan".
Getting that far took two corrections the test had carried since it was written
— the attribute tools take `attribute_key` rather than `key`, and the device has
to be one the tailnet owns rather than whichever the listing puts first, since a
device shared in from elsewhere is `isExternal`: listed, readable, and refused
on write as "no manageable device matching this ID found".

So the argument shapes and the device selection are now right, and everything up
to the control plane's billing check is exercised. The write itself has never
succeeded against a real tailnet and should not be described as though it has.
Proving it needs a tailnet on a plan that includes device posture. It is the smallest thing
that can be written and removed without affecting anything, and the test asserts
the tailnet gate is open too, so the write gate cannot be a way round it.
