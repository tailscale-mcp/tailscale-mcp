# tailscale-mcp

An MCP server that lets an AI agent operate Tailscale: the local node it runs on, through the `tailscale` CLI, and the whole tailnet, through the control-plane REST API.

## Language

### Network

**Tailnet**:
A private Tailscale network, as seen and administered by the control plane.
_Avoid_: network, VPN, mesh

**Control plane**:
Tailscale's coordination service, which the REST API administers.
_Avoid_: admin API, coordination server, Tailscale backend

**LocalAPI**:
The private HTTP interface of tailscaled on the local machine, which the `tailscale` CLI uses under the hood.
_Avoid_: daemon API, socket API

**Node**:
Any machine that is a member of a tailnet.
_Avoid_: machine, host, endpoint

**Local node**:
The node this server is running on, and the only node the local surface can act on.
_Avoid_: self, this machine, localhost

**Peer**:
Any node other than the local node, as the local node sees it.
_Avoid_: remote node, other machine

**Local filesystem**:
The files and directories of the machine the server runs on, as opposed to that machine's membership of the tailnet, which is the local node. The only thing a caller-supplied path can ever refer to.
_Avoid_: host filesystem, disk, local disk, this machine's files

**Device**:
A node as represented by the control-plane REST API. Used only when naming or talking to that API.
_Avoid_: device for anything seen from the local node

**Tailnet lock**:
The tailnet's own admission rule, enforced by its nodes independently of the control plane: a node is accepted only once a key the tailnet already trusts has signed it.
_Avoid_: TKA, key authority, network lock

**Tailnet-lock key**:
The public key, beginning `tlpub:`, that identifies one node as trusted to sign nodes and to change tailnet lock. Public by design and meant to be copied between nodes, unlike everything else in this vocabulary that ends in "key".
_Avoid_: lock key, signing key, TKA key

**Signing node**:
A node whose tailnet-lock key tailnet lock trusts, and therefore one that can admit other nodes.
_Avoid_: trusted node, authority

### Credentials

**API access token**:
A user-owned, expiring bearer credential for the control plane, carrying the permissions of the user who created it.
_Avoid_: API key, token (unqualified)

**Trust credential**:
A control-plane credential that belongs to the tailnet rather than to a user and does not expire on its own: an OAuth client or a federated identity. It mints short-lived tokens limited by its scopes.
_Avoid_: service account, OAuth key, machine token

**Scope**:
A named permission on a credential that unlocks one family of control-plane operations, in a read-only or a read-write form.
_Avoid_: permission, role (a role belongs to a user)

**Auth key**:
A pre-authentication key that lets a node join the tailnet without a browser login.
_Avoid_: pre-auth key, join key, token

**Secret**:
Any value that grants access if disclosed: an API access token, a trust credential's secret, an auth key, or a key or webhook secret a tool has just minted.
_Avoid_: credential (for the value itself), key (unqualified)

**Disablement secret**:
A secret minted when tailnet lock is initialised, and the only way to turn it off again. Spending one consumes it and makes it public. Written `disablement-secret:<hex>`.
_Avoid_: disablement key, recovery key, kill switch

**Disablement value**:
The public half of a disablement secret, derived from it and reported by tailnet lock so that a stored secret can be checked without being spent. Written `disablement:<hex>`, and distinct from the secret it comes from — the two are not interchangeable, and the commands that take them disagree about which.
_Avoid_: disablement (unqualified), disablement hash

### Server

**Surface**:
One of the two backends the server exposes: the local surface (the CLI, acting on the local node) and the tailnet surface (the REST API, acting on the tailnet).
_Avoid_: mode, backend, side

**Tool**:
One MCP tool, corresponding to exactly one operation on one surface. Local tools are named `tailscale_...`, tailnet tools `tailnet_...`.
_Avoid_: command, action, endpoint (when meaning the MCP-facing unit)

**Resource**:
A read-only view of one surface that a client reads by URI, such as the local node's status or the tailnet's policy file.
_Avoid_: document, file, feed

**Prompt**:
A canned, parameterised workflow the server offers to the client, which steers a sequence of tool calls.
_Avoid_: template, recipe, macro

**Tier**:
A tool's risk class: read, write, or destructive.
_Avoid_: level, permission, mode

**Toolset**:
A named group of tools switched on or off together.
_Avoid_: category, module, group

**Passthrough**:
The opt-in tool that runs an arbitrary `tailscale` subcommand.
_Avoid_: raw mode, shell, exec

**Self-severing**:
An operation that can cut the server off from the tailnet or from the client it serves: taking the local node down, logging it out or re-authenticating it, or, on the control plane, deleting, expiring, de-authorizing or re-tagging the local node's own device.
_Avoid_: dangerous, self-affecting, suicidal

**Confirmation**:
The explicit statement of intent a caller must include for a self-severing operation to run.
_Avoid_: approval (the client's own prompt to its user), acknowledgement

**Preference**:
A persistent setting of the local node, such as whether it accepts routes or acts as an exit node, changed one at a time with `set` and re-stated in full by `up`.
_Avoid_: pref (in prose), option, flag (a flag is how the CLI takes a preference)

**Knob**:
A debug operation that changes the local node's transient runtime state, such as forcing a fresh relay choice or a new NAT probe, without changing any preference.
_Avoid_: setting, toggle, hack

**Bounded form**:
The variant of a CLI command that would otherwise run until interrupted, limited by a count or a time limit so that every tool call returns.
_Avoid_: one-shot, non-blocking

**Excluded command**:
A `tailscale` CLI command that never becomes a tool and that the passthrough refuses to run, because it is interactive, runs in the foreground indefinitely, alters the host outside Tailscale, or prints a secret.
_Avoid_: blacklisted, banned, unsupported (that word is for version and platform gaps)

**Preset**:
A named selection of toolsets that the server starts with when the operator has not listed toolsets one by one.
_Avoid_: profile, mode, level

**Schema drift**:
A difference between the control plane's published API description and what the live API accepts or returns.
_Avoid_: breaking change, bug, mismatch (unqualified)
