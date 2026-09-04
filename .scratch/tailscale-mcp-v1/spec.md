# tailscale-mcp v1

Status: ready-for-agent

Derived from the design interview of 2026-09-03/04 (52 questions, all settled). The glossary in `CONTEXT.md` is authoritative for vocabulary; ADRs 0001 to 0005 are authoritative for the decisions they record. `docs/research/` holds the evidence behind every count and capability named here.

## Problem Statement

An agent asked to do something with Tailscale today has two bad options. It can shell out to the `tailscale` CLI, which means inventing the argv, guessing which commands block forever, and parsing text that changes between releases; or it can call the control-plane REST API, which means handling authentication, ninety-odd operations and a schema that drifts from its own published description. Neither path tells the agent which operations are safe to try, and both make it trivially easy to run the one command that severs the connection the agent is working over.

The three existing Tailscale MCP servers each solve part of this. One covers the control plane well but ignores the local node; another covers a handful of both; the third covers most of the API but exposes it through a tool surface that no risk model gates. None of them lets an operator say "this agent may read anything and change nothing", and none of them can be trusted not to run `tailscale down` on the host that is serving the conversation.

## Solution

One MCP server, written in Rust on the official SDK, exposing both of Tailscale's interfaces as typed tools under one risk model.

An operator starts it with no arguments and gets a read-only server: every tool that only reads is available, everything that writes is hidden until they pass a flag. Tools are grouped into toolsets and three presets so that a working set of about fifty tools is offered by default rather than a catalogue of a hundred and eighty. Operations that could sever the server from the tailnet or from its client refuse to run unless the caller states its intent in the call itself, which no flag can pre-authorise.

The agent sees per-verb tools with real parameters and honest annotations. It gets Tailscale's own JSON back, unmodified, so anything it learned from Tailscale's documentation still applies. When something fails it gets a fixed error code and a hint, not a wall of stderr.

Coverage is the point: every local CLI command that can safely be a tool, and every documented control-plane operation. The server is a strict superset of all three existing implementations except four features deliberately dropped and documented as such.

## User Stories

### As an agent operating Tailscale

1. As an agent, I want the local node's status as structured data, so that I can reason about connectivity without parsing a table.
2. As an agent, I want to list peers with their online state and addresses, so that I can pick a target for a diagnostic.
3. As an agent, I want to ping a peer with a bounded number of probes, so that the call always returns.
4. As an agent, I want a network condition report, so that I can tell a NAT problem from a DNS problem.
5. As an agent, I want to know which exit nodes are available and which is selected, so that I can advise a change.
6. As an agent, I want to read the local node's preferences, so that I can explain the current configuration before changing it.
7. As an agent, I want to change one preference at a time, so that I never disturb settings I was not asked about.
8. As an agent, I want the tool that changes preferences to be distinct from the tool that connects or logs in, so that I do not accidentally reset the node.
9. As an agent, I want to bring the node up with an auth key I was given, so that I can join a machine to a tailnet unattended.
10. As an agent, I want to hand a secret to the server as a value or as a file reference, so that I can use whichever the caller has.
11. As an agent, I want to publish a local port to the tailnet, so that a colleague can reach a development server.
12. As an agent, I want to see the current serve configuration, so that I can describe what is exposed before changing it.
13. As an agent, I want exposing something to the public internet to be classified more severely than exposing it to the tailnet, so that I cannot do it by accident.
14. As an agent, I want to send and receive files with a peer, so that I can move an artefact without a third-party service.
15. As an agent, I want to fetch a TLS certificate to explicit paths, so that a private key is never written where I might read it back.
16. As an agent, I want to inspect and sign tailnet lock state, so that I can help an operator through a key rotation.
17. As an agent, I want the deep diagnostic commands available only when an operator has switched them on, so that my default tool list stays legible.
18. As an agent, I want to run a `tailscale` subcommand the server has no typed tool for, when the operator has enabled that, so that a coverage gap does not block the task.
19. As an agent, I want the escape hatch to obey the same risk model as the typed tools, so that enabling it on a read-only server still cannot change anything.
20. As an agent, I want to list the tailnet's devices, so that I can find the one an operator is describing by name.
21. As an agent, I want to narrow a device listing server-side, so that a large tailnet does not overflow my context.
22. As an agent, I want to authorize, tag, rename, expire and delete a device, so that I can complete an onboarding or offboarding.
23. As an agent, I want to read and set a device's advertised routes, so that I can approve a subnet router.
24. As an agent, I want to read the policy file with its version identifier, so that I can propose a change against a known base.
25. As an agent, I want to validate a proposed policy before applying it, so that I never deploy something that fails to parse.
26. As an agent, I want to preview which rules apply to a user or an address, so that I can explain the effect of a change.
27. As an agent, I want a policy write to fail when the file changed underneath me, so that I cannot silently overwrite someone else's edit.
28. As an agent, I want to manage auth keys, so that I can mint a tagged key for a new node and revoke it afterwards.
29. As an agent, I want a newly minted secret returned once and never logged, so that using the server does not leak it.
30. As an agent, I want to manage DNS nameservers, search paths, split DNS and preferences, so that I can complete a DNS change end to end.
31. As an agent, I want to manage users, their roles and their suspension state, so that I can handle a joiner or a leaver.
32. As an agent, I want to send, resend and revoke invitations for users and devices, so that I can onboard someone without an auth key.
33. As an agent, I want to read audit and network logs over a time window, so that I can investigate an incident.
34. As an agent, I want to configure log streaming, so that I can wire a tailnet into an existing log pipeline.
35. As an agent, I want to manage webhooks and rotate their secrets, so that I can maintain an integration.
36. As an agent, I want to manage device posture integrations and attributes, so that I can enforce a device compliance rule.
37. As an agent, I want to manage services and their host approvals, so that I can run an application across several nodes.
38. As an agent, I want to manage OAuth clients, so that I can provision credentials for automation.
39. As an agent, I want request bodies to keep Tailscale's own field names, so that an example from Tailscale's documentation works unmodified.
40. As an agent, I want the response to be Tailscale's own JSON, so that what I read matches what the documentation describes.
41. As an agent, I want a machine-readable error code and a hint, so that I can decide whether to retry, narrow, or ask a human.
42. As an agent, I want a distinct code when a result is too large, naming the narrowing available, so that I can retry successfully rather than lose the result.
43. As an agent, I want a distinct code when an operation needs a version, a platform or a privilege the host does not have, so that I do not retry something that cannot work.
44. As an agent, I want to be told when an operation requires a user-owned credential and the server is using a tailnet-owned one, so that I can explain the failure.
45. As an agent, I want annotations that truthfully say whether a tool reads, writes, is repeatable, and touches the outside world, so that my own planning can rely on them.
46. As an agent, I want a canned workflow for reviewing a policy change, so that I follow read, validate and preview before writing.
47. As an agent, I want a canned workflow for diagnosing a connection to a peer, so that I run the diagnostics in a sensible order.
48. As an agent, I want the node's status and the tailnet's policy available as readable resources, so that I can consult them without spending a tool call.

### As an operator running the server

49. As an operator, I want the server to be read-only until I say otherwise, so that connecting an agent to it is not a risk decision.
50. As an operator, I want separate switches for writing and for destroying, so that I can grant the middle ground.
51. As an operator, I want tools I have not permitted to be absent from the list rather than present and refusing, so that the agent does not plan around them.
52. As an operator, I want a default selection of tools rather than all of them, so that the tool list stays useful in a client with a context budget.
53. As an operator, I want named presets and the ability to add or remove one toolset from a preset, so that tuning is one environment variable.
54. As an operator, I want to see exactly which tools a configuration would expose, without starting a client, so that I can check before deploying.
55. As an operator, I want the server to refuse to start with no tools enabled, so that a typo fails loudly.
56. As an operator, I want the server to work with only the CLI present, or only credentials present, so that I can run it on a node or in a container.
57. As an operator, I want to disable a surface explicitly, so that a server on a node need not be given tailnet credentials.
58. As an operator, I want the credential names to match those the Terraform provider and existing tooling already use, so that my existing configuration works.
59. As an operator, I want to authenticate with an API access token, an OAuth client, or a federated identity, so that I can follow my organisation's practice.
60. As an operator, I want to narrow a minted token by scope, so that the server holds no more permission than it needs.
61. As an operator, I want stdio by default, so that the common client configuration is one line.
62. As an operator, I want an HTTP mode with a bearer token, so that I can share one server with several clients.
63. As an operator, I want the HTTP mode to refuse a non-loopback bind without a token, so that I cannot expose it unauthenticated by accident.
64. As an operator, I want the server to accept requests addressed to its own tailnet name without my configuring anything, so that binding to a Tailscale address just works.
65. As an operator, I want browser origins rejected by default, so that a web page cannot drive my server.
66. As an operator, I want a health endpoint, so that a supervisor can restart the server.
67. As an operator, I want each request attributed to the tailnet identity that made it, so that the log says who did what.
68. As an operator, I want secrets never written to the log, so that the log is not itself a secret.
69. As an operator, I want a one-command check of the CLI, its version, my credentials and reachability, so that I can diagnose a broken setup.
70. As an operator, I want a printed client configuration snippet, so that I can paste it into my client.
71. As an operator, I want to validate and deploy a policy file from the command line, so that the same code path serves my CI.
72. As an operator, I want the server never to escalate privileges, so that its authority is exactly the account it runs as.
73. As an operator, I want a clear message naming the operator user or root when a command needs privileges the server lacks, so that I can fix it.
74. As an operator, I want install through Homebrew, a container image, an npm launcher or a release binary, so that I use the channel I already have.

### As a maintainer

75. As a maintainer, I want one declaration per tool that yields its schema, its argv or request, and its metadata, so that a tool cannot be registered without metadata.
76. As a maintainer, I want one table that the tests, the printed tool list and the README all read, so that they cannot disagree.
77. As a maintainer, I want a test per tool asserting its tier, toolset and annotations, so that a mistake in classification fails the build.
78. As a maintainer, I want tests that drive the server as a client sees it, so that I test the contract rather than the internals.
79. As a maintainer, I want the process spawning tested against a stub binary, so that timeouts and signals are covered without a real daemon.
80. As a maintainer, I want a test that fails when Tailscale's published schema gains a field no model carries, so that drift surfaces on a refresh rather than in production.
81. As a maintainer, I want tests against a fake HTTP server rather than a real tailnet, so that the suite runs offline.
82. As a maintainer, I want end-to-end tests that only run when credentials are present, so that contributors are not blocked.
83. As a maintainer, I want fixtures redacted of identity, so that the repository never publishes a tailnet.
84. As a maintainer, I want copyleft dependencies rejected mechanically, so that the licence stays clean.
85. As a maintainer, I want the changelog and version bumps derived from commit messages, so that releasing is not a manual chore.

### As a Rust developer using the crates

86. As a developer, I want the control-plane client as its own crate, so that I can use it without the MCP server.
87. As a developer, I want typed models that keep fields they do not know, so that an additive API change does not break my build.
88. As a developer, I want the raw body and the response headers alongside the parsed model, so that I can reach a header the model omits.
89. As a developer, I want the CLI wrapper as its own crate behind a trait, so that I can fake the local node in my own tests.

## Implementation Decisions

### Shape

A Cargo workspace of three publishable crates: a control-plane REST client, a `tailscale` CLI wrapper, and the server binary that depends on both. Rust edition 2024, MSRV pinned to the SDK's. First release is 1.0.0 (ADR-0005), so tool names, parameter names and error codes are a compatibility promise from day one; additive coverage is a minor release.

### Surfaces

Two surfaces, detected at startup and each disableable. The local surface drives the `tailscale` CLI, never tailscaled's LocalAPI (ADR-0001), behind a trait so a LocalAPI implementation can be added later. The tailnet surface uses a REST client written in-house rather than a dependency on an AGPL implementation (ADR-0002).

### Tools

186 tools: 62 typed local tools, a 30-tool debug toolset, one passthrough, and 93 tailnet tools, one per documented control-plane operation. Local tools are named `tailscale_<verb>`, tailnet tools `tailnet_<resource>_<verb>` with a fixed verb vocabulary. Each tool is declared once, in a form that expands to its parameter type, its schema, its argv or request builder, and a row in a metadata table; that table is the single source for the tool-listing subcommand, the contract tests and the README's tool table.

Parameters the server owns are snake_case; anything that is Tailscale's own request body keeps Tailscale's shape and field names (ADR-0004). Small bodies are flattened into parameters, nested schemas are taken as one object. Every CLI flag becomes an optional parameter of the same name in snake_case; flags for other platforms remain in the schema, are documented as such, and fail before spawning.

### Risk model

Three tiers: read, write, destructive. Read is the default; write and destructive each need a flag. Tools outside the permitted tiers are hidden from the listing, not listed and refused, and the server's instructions explain that they exist. Annotations state the truth about each tool.

Self-severing operations additionally require the caller to pass a confirmation in the call, which no flag can pre-authorise: taking the local node down, logging out, re-authenticating, and control-plane operations targeting the local node's own device, identified by matching against cached local status. Four tailnet-scale irreversible operations require the same: deleting a tailnet, and initialising, disabling or revoking keys for tailnet lock. The CLI's own risk acceptance is passed only on a call that carried a confirmation, so its checks become the gate rather than being bypassed.

The passthrough is one switch, inherits the tier of the typed tool covering the same subcommand, treats an unknown subcommand as destructive, and refuses excluded commands.

### Excluded commands

Interactive sessions, foreground servers, commands that mutate the host outside Tailscale, commands that print a secret, the unstable debug members, and the self-update path never become tools, and the passthrough refuses them. Roughly 34 command paths in total. Four hidden flags are dropped from every schema.

### Execution

Commands are spawned with an argument array and no shell, stdin closed, a minimal environment, and a timeout that terminates gracefully before killing. Reads run concurrently; local writes and destructive operations serialise behind one lock. Secrets are never placed on the command line: a literal is written to a private temporary file for the life of the call and passed by file reference. Blocking commands exist only in bounded forms, with per-tool caps.

### Control plane

Authentication by API access token, OAuth client, or federated identity, in that precedence, under the environment variable names the existing ecosystem already uses. Minted tokens are cached and refreshed before expiry, and evicted once on rejection. Requests retry only where retrying is safe, honour the server's backoff, and run under a concurrency limit. Results above a size cap fail with a code naming the narrowing available rather than truncating.

Models are hand-written, retain unknown fields, and are checked against the vendored schema by a drift test (ADR-0003). Enums are strict only for genuinely closed sets. Responses reach the agent verbatim, with structured content attached and no declared output schema. The policy file is the one documented exception, returning its version identifier alongside the document, and policy writes must carry that identifier.

### Errors

Tool-level results with a fixed code and message, optionally an exit code, captured output, a status and a hint. Fourteen codes. Protocol errors are reserved for malformed requests. Secrets are redacted from every path that could carry them.

### Server

Stdio by default; Streamable HTTP behind a bearer token with host and origin allow-lists, a body limit, a per-address rate limit, an open health endpoint, and caller identity resolved from the tailnet where possible. The allow-list includes the local node's own tailnet names, read from status at startup. Subcommands cover diagnosis, tool listing, version, policy validation and deployment, and printing a client configuration snippet. Nine resources and three prompts.

### Superset

A strict superset of the three reference implementations, with four documented exceptions: no configuration file, no tool-schema resource, no OAuth resource-server mode for browser clients in this release, and no extra-enum environment knobs. The README carries the comparison table.

## Testing Decisions

A good test here drives the server the way a client does and asserts on what the client can observe: the tool list, a call's result, an error's code. It does not reach into the router, assert on an argv string, or name a private function. The exceptions are the two places behaviour is invisible from above, and both are named below.

### The primary seam

An in-process MCP client connected to a fully constructed server, with both backends faked underneath. Everything the design decided is observable at this one seam: which tools a preset and tier combination lists, whether a confirmation is required, what an error's code is, whether a result carries structured content, what a resource returns, what a prompt expands to. Tests preferentially live here.

Underneath it, two fakes: a local backend that returns canned output and exit codes instead of spawning, and a fake HTTP server standing in for the control plane, reached through the base-URL override that exists for this purpose.

### Two supporting seams

Process execution is tested directly against a stub binary, because a faked backend skips exactly the behaviour worth testing: argument construction, environment scrubbing, timeout, graceful termination then kill, and the serialisation lock.

Schema drift is tested by parsing the vendored API description and asserting every property is modelled. This is not a behavioural test; it is a tripwire on a refresh.

### Coverage

One table-driven contract test row per tool asserting tier, toolset, annotations, a success case and an error case: 186 rows, so a tool cannot be added without classifying it. Fixtures are recorded from real responses and redacted of identity. End-to-end tests against a real node and a real tailnet exist but run only when the environment supplies credentials, and never in CI.

## Out of Scope

- Driving tailscaled's LocalAPI directly (ADR-0001).
- Acting on any node other than the one the server runs on, except through the control plane.
- Interactive commands, foreground servers, host mutation outside Tailscale, and the self-update path.
- A configuration file, a tool-schema resource, extra-enum environment knobs, and an OAuth resource-server mode for browser-based clients.
- Grants-derived authorisation, where the caller's tailnet identity decides which tools they may call. The per-request hook is designed for it; the policy is not in this release.
- Declared output schemas, deferred until the drift test has survived several Tailscale releases.
- Subscriptions on resources; pagination beyond what the API offers, other than one client-side window on device listing.
- A documentation site; Windows as a first-class platform.

## Further Notes

Milestones follow the agreed build order: workspace and server core, the local surface, the tailnet surface, HTTP mode and the subcommands, then packaging. The local surface comes first because it is testable end to end on a developer machine with no credentials, so it proves the execution model, the tiers, the presets and the contract harness before ninety-three tailnet tools reuse them.

Two things only the maintainer can supply, neither blocking the first milestone: a read-only control-plane credential for the tailnet end-to-end tests, and an npm token with write access to the `@tailscale-mcp` scope at packaging time. The npm organisation exists; the launcher is `@tailscale-mcp/tailscale-mcp`.

One fact remains to be established during the first milestone: the minimum Tailscale version the server supports, from upstream changelogs, which sets the floor the startup probe warns against and the table the unsupported-version error cites.
