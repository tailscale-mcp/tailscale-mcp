<!-- Generated from the tool metadata table. Do not edit by hand; run
     UPDATE_DOCS=1 cargo test -p tailscale-mcp --test docs_are_current -->

# Tools

186 tools in 20 toolsets. Which of them a session offers depends on the
preset, the tier and which surfaces are reachable; see
[configuration.md](configuration.md).

| Toolset | Surface | Tools | In presets |
|---|---|---|---|
| [`local-status`](#local-status) | local | 25 | minimal, core, full |
| [`local-prefs`](#local-prefs) | local | 8 | core, full |
| [`local-serve`](#local-serve) | local | 10 | core, full |
| [`local-files`](#local-files) | local | 11 | core, full |
| [`local-lock`](#local-lock) | local | 8 | full |
| [`local-debug`](#local-debug) | local | 30 | none — ask for it by name |
| [`local-passthrough`](#local-passthrough) | local | 1 | none — ask for it by name |
| [`tailnet-devices`](#tailnet-devices) | tailnet | 15 | minimal, core, full |
| [`tailnet-invites`](#tailnet-invites) | tailnet | 11 | core, full |
| [`tailnet-logging`](#tailnet-logging) | tailnet | 8 | full |
| [`tailnet-dns`](#tailnet-dns) | tailnet | 11 | minimal, core, full |
| [`tailnet-keys`](#tailnet-keys) | tailnet | 5 | core, full |
| [`tailnet-policy`](#tailnet-policy) | tailnet | 4 | minimal, core, full |
| [`tailnet-posture`](#tailnet-posture) | tailnet | 5 | full |
| [`tailnet-users`](#tailnet-users) | tailnet | 7 | core, full |
| [`tailnet-settings`](#tailnet-settings) | tailnet | 5 | core, full |
| [`tailnet-webhooks`](#tailnet-webhooks) | tailnet | 7 | core, full |
| [`tailnet-services`](#tailnet-services) | tailnet | 7 | core, full |
| [`tailnet-oauth-apps`](#tailnet-oauth-apps) | tailnet | 5 | full |
| [`tailnet-org`](#tailnet-org) | tailnet | 3 | full |

## local-status

25 tools on the local surface, in the minimal, core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_status` | read |  | Report the state of this node and of every peer it can see: addresses, operating systems, connection paths, transfer counters and health warnings. The whole `tailscale status --json` document, unmodified. |
| `tailscale_ip` | read |  | List this node's Tailscale addresses, or resolve a peer or service name to its addresses. |
| `tailscale_netcheck` | read |  | Probe the local network and the DERP relays: whether UDP works, whether the NAT is symmetric, which relay is nearest and how far away each one is. Sends traffic, and takes a second or two. |
| `tailscale_ping` | read |  | Ping a peer at the Tailscale layer and report the path each reply took, which is how you tell a direct connection from a relayed one. |
| `tailscale_whois` | read |  | Identify the machine and user behind a Tailscale address. |
| `tailscale_whoami` | read | needs tailscale 1.90 | Identify this node: its machine record and the user it is logged in as. |
| `tailscale_version` | read |  | Report the version of the `tailscale` binary this server drives, and whether it is new enough for everything this server models. |
| `tailscale_licenses` | read |  | Print where the open-source licences of the components in this client build are published. |
| `tailscale_bugreport` | read |  | Emit a diagnostic marker to Tailscale's log service and return it, so that it can be quoted to Tailscale support. Uploads nothing beyond what the client already logs. |
| `tailscale_appc_routes` | read | needs tailscale 1.90 | List the routes this node has learnt as an app connector, and say whether it is acting as one at all. |
| `tailscale_routecheck` | read | needs tailscale 1.102 | Report the experimental reachability check: which advertised routes this node can reach and through which peer. Often has no report to give until a probe has been asked for. |
| `tailscale_wait` | read | needs tailscale 1.90 | Wait until the node's network interface and addresses are ready, up to a bounded timeout. Returns immediately when they already are. |
| `tailscale_dns_status` | read | needs tailscale 1.72 | Report how DNS is resolving: whether MagicDNS is in use, which nameservers the tailnet supplies, and what the operating system's own resolver is configured with. |
| `tailscale_dns_query` | read | needs tailscale 1.72 | Resolve a name through Tailscale's resolver and report the answer, including which resolver answered. |
| `tailscale_exit_node_list` | read |  | List the exit nodes available to this node, both those in the tailnet and those from Mullvad, and say which one is selected. |
| `tailscale_exit_node_suggest` | read | needs tailscale 1.66 | Ask the client which exit node it would pick, by latency and location. |
| `tailscale_metrics_print` | read | needs tailscale 1.78 | Report the client's user-facing metrics: routes advertised and approved, home relay region, and bytes and packets by path. |
| `tailscale_service_list` | read | needs tailscale 1.90 | List the Tailscale Services this node hosts. |
| `tailscale_syspolicy_list` | read | needs tailscale 1.72 | List the system policies in force on this machine — the settings an MDM profile or a local administrator has fixed — and where each came from. |
| `tailscale_lock_status` | read |  | Report whether tailnet lock is enabled and what this node's tailnet-lock key is. |
| `tailscale_lock_log` | read |  | List the tailnet-lock updates this node knows about, newest first. |
| `tailscale_serve_status` | read |  | Report what this node is currently serving on the tailnet: which ports are listening and what each path forwards to. |
| `tailscale_funnel_status` | read |  | Report what this node is currently exposing to the public internet through Funnel. |
| `tailscale_configure_sysext_status` | read | macos only | Report the state of the macOS system extension the standalone client runs its networking in. |
| `tailscale_switch_list` | read |  | List the Tailscale accounts stored on this machine and say which one is currently active. |

## local-prefs

8 tools on the local surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_prefs_get` | read | needs tailscale 1.90 | Show the node's current preferences: one setting by name, or all of them. Reads only; use `tailscale_prefs_set` to change anything. |
| `tailscale_prefs_set` | write |  | Change only the preferences named here, leaving every other preference as it is. This is the tool to reach for on a node that is already connected: unlike `tailscale_up` it does not restate the whole preference set, so it cannot reset something by omitting it. |
| `tailscale_up` | destructive | needs `confirm`; can sever this server | Connect this node to the tailnet, logging in if it is not already, and apply the preferences given here. This applies a *whole* preference set: anything not restated goes back to its default. On a node that is already connected, use `tailscale_prefs_set` instead unless you mean to reset the rest. Without an authentication key the client prints a login URL and waits, so the call is bounded by `timeout_seconds` and returns the URL when it has one. |
| `tailscale_down` | destructive | needs `confirm`; can sever this server | Disconnect this node from the tailnet. The node stays logged in and can be reconnected with `tailscale_up`. |
| `tailscale_login` | write | needs `confirm`; can sever this server | Log in, adding a new account profile if this node is already logged in to another. Prints a login URL and waits when no authentication key is given, bounded by `timeout_seconds`. |
| `tailscale_logout` | destructive | needs `confirm`; can sever this server | Log out and expire this node's key. The node disconnects, and coming back needs a fresh login rather than a reconnect. |
| `tailscale_switch_profile` | write | needs `confirm`; can sever this server | Switch this machine to one of the other accounts stored on it, which restarts the connection under that account's identity. Use `tailscale_switch_list` to see the accounts and their ids. |
| `tailscale_switch_remove` | destructive | needs `confirm` | Forget one of the accounts stored on this machine. The account itself is untouched; it is logging in again that brings it back. |

## local-serve

10 tools on the local surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_serve_set` | write |  | Serve a local server, a file, a directory or a block of text to the tailnet. Reachable by other nodes on the tailnet only; use `tailscale_funnel_set` to publish to the internet. Runs in the background: the call returns as soon as the handler is in place. |
| `tailscale_serve_off` | write |  | Stop serving one endpoint, leaving every other handler in place. Names the same endpoint the handler was added on; reports `not_found` when there is no handler there. |
| `tailscale_serve_reset` | destructive | needs `confirm` | Remove every serve and funnel handler on this node at once. There is one configuration behind both commands, so this clears both. Use `tailscale_serve_off` to remove a single endpoint. |
| `tailscale_serve_drain` | write | needs tailscale 1.90 | Stop this node accepting new connections for a service, while letting the connections it already has finish. Bring it back with `tailscale_serve_advertise`. |
| `tailscale_serve_clear` | destructive | needs `confirm`; needs tailscale 1.90 | Remove every handler configured for one service on this node. Unlike `tailscale_serve_drain` this discards the configuration rather than stopping at the door, and `tailscale_serve_advertise` will not bring it back. |
| `tailscale_serve_advertise` | write | needs tailscale 1.90 | Offer this node to the tailnet as a host for a service, which is what undoes `tailscale_serve_drain`. Not needed after `tailscale_serve_set`, which advertises the service itself. |
| `tailscale_serve_get_config` | read | needs tailscale 1.90 | Read the service configuration this node is hosting, as a document that `tailscale_serve_set_config` takes back unchanged. Reads only. |
| `tailscale_serve_set_config` | write | needs tailscale 1.90 | Replace the service configuration on this node with the document given here, which overwrites every handler in scope. Writing back a document that `tailscale_serve_get_config` produced changes nothing. |
| `tailscale_funnel_set` | destructive | needs `confirm` | Publish a local server to the **public internet** through Tailscale Funnel. Anyone on the internet who knows the name can reach it, with no tailnet membership and no authentication of any kind. Use `tailscale_serve_set` for a server that only the tailnet should reach. Funnel has to be enabled for the tailnet in the admin console first; on a tailnet where it is not, the client waits rather than failing, and this call comes back as a timeout carrying the URL that enables it. |
| `tailscale_funnel_off` | destructive |  | Stop publishing one endpoint to the internet. The handler stops being reachable from outside the tailnet; use `tailscale_serve_off` to remove it from the tailnet as well. |

## local-files

11 tools on the local surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_file_cp` | write |  | Send files from **the local filesystem** to another node over Taildrop. The files are read from the paths given here; the peer takes delivery with `tailscale_file_get` on its own machine. Transfers are slow, so this call allows several minutes. |
| `tailscale_file_targets` | read |  | List the nodes this one may send files to with `tailscale_file_cp`, with the address and name to use for each. Reads only. |
| `tailscale_file_get` | write |  | Move files waiting in this node's Taildrop inbox into a directory on **the local filesystem**, emptying the inbox. Returns straight away whether or not anything was waiting: it never blocks for a file to arrive and never runs on in a loop. |
| `tailscale_cert` | write |  | Obtain a TLS certificate for a name in this tailnet and **write it to two files on the local filesystem**. Both output paths are required, and neither may be `-`: the private key is never printed into the answer. Issuance talks to a certificate authority, so this call allows a minute or two. |
| `tailscale_metrics_write` | write | needs tailscale 1.78 | Write this node's client metrics, in Prometheus text format, to a file on **the local filesystem** — the form a node exporter's textfile collector reads. Use `tailscale_metrics_print` to read the same numbers without writing anything. |
| `tailscale_configure_kubeconfig` | write |  | Add a context to the **local kubectl configuration file** for a Kubernetes cluster reached through a Tailscale auth proxy running on the named peer. Alpha in the client, and it edits the kubeconfig in place. |
| `tailscale_syspolicy_reload` | write | needs tailscale 1.72 | Reload the local node's MDM and system policy settings even when nothing has changed, and report what the reload produced. Use `tailscale_syspolicy_list` to read the settings without reloading. |
| `tailscale_drive_list` | read |  | List the directories on **the local filesystem** that Taildrive shares with the tailnet, with the local user each is served as. Reads only. |
| `tailscale_drive_share` | write |  | Share a directory on **the local filesystem** with the tailnet over Taildrive, under a name peers will see. Sharing the same name and path again changes nothing. |
| `tailscale_drive_rename` | write |  | Rename an existing Taildrive share. Peers see the new name; the directory on the local filesystem is untouched. |
| `tailscale_drive_unshare` | destructive |  | Stop sharing a directory over Taildrive. Every peer loses access at once; the files stay on disk. Restoring it needs `tailscale_drive_share` and the original path. |

## local-lock

8 tools on the local surface, in the full preset.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_lock_init` | destructive | needs `confirm` | Turn on tailnet lock for the whole tailnet, trusting the tailnet-lock keys given here to sign nodes and to make further lock changes. Mints the disablement secrets that are the only way back, and returns them once: store them before answering, because nothing here keeps a copy. Affects every node in the tailnet, so it needs a confirmation as well as the destructive tier. |
| `tailscale_lock_add` | write |  | Trust more tailnet-lock keys, so that the nodes holding them can sign nodes and change tailnet lock. Adding a key that is already trusted changes nothing. |
| `tailscale_lock_remove` | destructive |  | Stop trusting tailnet-lock keys. Signatures those keys made are re-signed by default so that the nodes they admitted stay admitted; turning that off locks those nodes out. Use `tailscale_lock_revoke_keys` instead if a key was compromised. |
| `tailscale_lock_sign` | write |  | Sign a node key, admitting that node to a tailnet under lock, or sign a pre-approved auth key so that it can bring nodes up. Give the key directly or as `file:<path>` to a file holding it. A signed auth key is returned once and is not stored. |
| `tailscale_lock_disable` | destructive | needs `confirm` | Turn tailnet lock off for the whole tailnet by spending one of the disablement secrets minted when it was initialised. The secret is consumed and becomes public; re-enabling the lock means initialising it again from scratch. Needs a confirmation as well as the destructive tier. |
| `tailscale_lock_disablement_kdf` | read |  | Compute the public disablement value that corresponds to a disablement secret, without disabling anything and without contacting anything. Local arithmetic on the value given, for checking a stored secret against what `tailscale_lock_status` reports. |
| `tailscale_lock_local_disable` | destructive |  | Make this node accept traffic from nodes that tailnet lock has locked out. Affects this node only: the tailnet's lock stays on and every other node keeps enforcing it. |
| `tailscale_lock_revoke_keys` | destructive | needs `confirm` | Retroactively revoke compromised tailnet-lock keys, so that every node they signed loses its authorisation and must be signed again. Several signing nodes have to co-sign: start with `keys`, then re-run with the `recovery_blob` the previous step printed and `cosign` on each further node, then once with `finish`. Needs a confirmation as well as the destructive tier. |

## local-debug

30 tools on the local surface, in no preset — `--toolsets +local-debug` offers it.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_debug_derp_map` | read |  | The DERP map the local node is using: every relay region the control plane has told it about, with the servers in each. Large. |
| `tailscale_debug_netmap` | read |  | The current netmap: every peer the local node knows, with the keys, addresses and endpoints it holds for each. The largest document this server produces, and the one most likely to exceed the result cap. |
| `tailscale_debug_hostinfo` | read |  | What the local node reports about itself to the control plane: its OS, version, hardware and the features it has enabled. |
| `tailscale_debug_control_knobs` | read |  | The control knobs the control plane has set on this node: the per-tailnet switches that change how the daemon behaves without a preference. |
| `tailscale_debug_daemon_goroutines` | read |  | A stack dump of every goroutine in tailscaled, for diagnosing a daemon that is wedged. Tens of kilobytes of Go stack traces. |
| `tailscale_debug_daemon_bus_graph` | read |  | The daemon's internal event bus as a graph: which components publish which events and which subscribe to them. |
| `tailscale_debug_daemon_bus_queues` | read |  | How much is queued on each of the daemon's internal event bus queues, for finding a subscriber that has fallen behind. |
| `tailscale_debug_metrics` | read |  | The daemon's own internal metrics, in Prometheus text format. Distinct from `tailscale_metrics_print`, which reports the node's client metrics. |
| `tailscale_debug_statedir` | read |  | The directory tailscaled keeps its state in on the local filesystem. |
| `tailscale_debug_go_buildinfo` | read |  | The Go build information of the `tailscale` binary: its module versions, build settings and the toolchain that produced it. |
| `tailscale_debug_peer_relay_servers` | read |  | The peers this node could relay through, for reaching a peer that no direct path and no DERP region can carry. |
| `tailscale_debug_peer_relay_sessions` | read |  | The relay sessions currently running through this node on behalf of other peers, and whether it is configured to serve them at all. |
| `tailscale_debug_file_list` | read |  | List the files waiting in this node's Taildrop inbox without downloading any of them. Use `tailscale_file_get` to fetch what this reports. |
| `tailscale_debug_stat` | read |  | The mode and size of files on the local filesystem, as tailscaled's own process sees them, for telling a missing file from an unreadable one. |
| `tailscale_debug_via` | read |  | Convert between a site's IPv4 prefix and the IPv6 `via` route that carries it. Give `site_id` and `prefix` to go one way, or `route` to go back. Arithmetic on the values given: nothing is contacted. |
| `tailscale_debug_watch_ipn` | read |  | Watch the local node's state notifications until `count` of them have arrived or the bound expires, whichever comes first. Reports the events as they were published. A quiet node reaches the bound instead, which is reported as a timeout carrying whatever had arrived. |
| `tailscale_debug_peer_endpoint_changes` | read |  | The history of endpoint changes the local node has recorded for one peer, for diagnosing a connection that keeps being rebuilt. |
| `tailscale_debug_resolve` | read |  | Resolve a hostname through the daemon's own resolver rather than the operating system's, to see what tailscaled would have got. |
| `tailscale_debug_dial_types` | read |  | Try every path type in turn to a host and port — direct, through DERP, through the operating system — and report which of them connected. |
| `tailscale_debug_derp` | read |  | Test the local node's DERP configuration by exercising the relays it would use. A network probe: it makes connections but changes nothing. |
| `tailscale_debug_ts2021` | read |  | Test that the control plane is reachable over the Noise protocol the daemon uses, reporting each dial attempt and its result. |
| `tailscale_debug_portmap` | read |  | Probe the local network's gateway for port-mapping support — PMP, PCP and UPnP — and report what it offered. Runs for `seconds`. |
| `tailscale_debug_component_logs` | write |  | Turn verbose logging on for one of the daemon's components for a while. A zero or negative duration turns it off again. The logs go to the daemon's own log, which this server does not read. |
| `tailscale_debug_restun` | write |  | Force a fresh STUN round, so the node re-learns the addresses peers should reach it on. Useful after a network change the daemon missed. |
| `tailscale_debug_rebind` | write |  | Force the node to rebind its UDP sockets, so it starts using whatever the operating system's routing table now says. |
| `tailscale_debug_rotate_disco_key` | write |  | Rotate this node's disco key, so that peers must re-establish their direct paths to it. Existing sessions are rebuilt, not dropped. |
| `tailscale_debug_derp_unset_on_demand` | write |  | Put DERP connections back to always-on after something set them to on-demand. The repair for a node that has become slow to be reached. |
| `tailscale_debug_pick_new_derp` | write |  | Move the node to a different home DERP region, chosen by the daemon, until the next restart. For testing whether a region is the problem. |
| `tailscale_debug_force_prefer_derp` | write |  | Pin the node's home DERP to one region until the next restart, or pass region 0 to un-pin it and let the daemon choose again. |
| `tailscale_debug_force_netmap_update` | write |  | Push a full netmap update through the daemon without changing it, to exercise everything that reacts to one. |

## local-passthrough

1 tool on the local surface, in no preset — `--toolsets +local-passthrough` offers it.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailscale_run` | read | tier depends on arguments | Run a `tailscale` subcommand that no other tool covers, given as a list of arguments. Prefer a typed tool wherever one exists: they validate what they are given and answer with structured data rather than the text a person would read. What this is allowed to run depends on the command — one a typed tool covers takes that tool's tier and confirmation, an unrecognised one counts as destructive, and a command this server never runs is refused with the reason why. |

## tailnet-devices

15 tools on the tailnet surface, in the minimal, core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_device_list` | read |  | List the devices in the tailnet. Answers with the control plane's own `{"devices": [...]}`, or with a `window` beside it when `limit` or `offset` narrowed the list here. The endpoint has no pagination, so a large tailnet answers whole and may exceed this server's result cap. `filters` is what asks the control plane for less, and so the only one of these that can rescue such a listing; `limit` and `offset` are applied to an answer that has already arrived. `fields: "all"` adds the costlier fields — posture identity, client connectivity — that `default` leaves out, and `fields: "default"` is the smaller answer. |
| `tailnet_device_get` | read |  | Read one device, by any of the names or identifiers it answers to. |
| `tailnet_device_delete` | destructive |  | Remove a device from the tailnet permanently. The machine has to re-authenticate to come back, as a new device with a new address. Only devices belonging to this tailnet can be deleted; a device shared in from another tailnet is refused by the control plane. |
| `tailnet_device_expire` | destructive |  | Expire a device's node key, which disconnects it until someone re-authenticates the machine. The device itself is kept. |
| `tailnet_device_authorize` | write | tier depends on arguments | Authorise a device, or revoke its authorisation. Only meaningful on a tailnet that requires device approval. Authorising is a write; revoking disconnects the device until it is authorised again, and needs the destructive tier. |
| `tailnet_device_rename` | write |  | Rename a device. Its old MagicDNS names stop resolving, so anything addressing it by name has to be updated. |
| `tailnet_device_tags_set` | write |  | Replace a device's tags. Tags must already be defined in the tailnet policy file, and the credential's own tags limit what it may assign. This replaces the whole set: tags not listed are removed, and removing a tag can remove the access a policy rule granted through it. |
| `tailnet_device_key_expiry_set` | write |  | Turn a device's key expiry off, so it stays connected without re-authenticating, or back on. |
| `tailnet_device_ip_set` | write |  | Move a device to a different Tailscale IPv4 address. Existing connections to the old address break. The address has to come from the tailnet's own range. |
| `tailnet_device_routes_get` | read |  | Read the routes a device advertises and the subset that is enabled. |
| `tailnet_device_routes_set` | write |  | Replace the set of advertised routes that are enabled for a device. Only the enabled set can be set here; what a device advertises is the device's own configuration. Enabling a route the device does not advertise has no effect until it does. |
| `tailnet_device_attributes_get` | read |  | Read the custom posture attributes set on a device, and when each expires. |
| `tailnet_device_attribute_set` | write |  | Set one custom posture attribute on one device. The key must begin `custom:`. The value's type — string, number or boolean — is fixed by the first write and a later write of a different type is refused by the control plane. |
| `tailnet_device_attribute_delete` | destructive |  | Delete one custom posture attribute from one device. Only `custom:` attributes can be deleted; the ones Tailscale sets are read-only. |
| `tailnet_device_attributes_update` | write |  | Set custom posture attributes on many devices in one call. A merge: a device the map does not mention is left alone, an attribute it does not mention is left alone, and an attribute given `null` is deleted. Each value is a string, a number, a boolean, `null`, or an object `{"value": ..., "expiry": "<RFC 3339>"}` to set an expiry too. |

## tailnet-invites

11 tools on the tailnet surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_device_invite_list` | read |  | List the outstanding invitations to share one device. |
| `tailnet_device_invite_create` | write |  | Share a device with people outside the tailnet. Takes a list, so several people can be invited at once and each gets its own settings. Each answer carries an `inviteUrl`, which is a credential: anyone holding it can accept. Omitting `email` is how to create one nobody is mailed, to be passed on by hand. Needs a user-owned credential: the invite records who sent it. |
| `tailnet_device_invite_get` | read |  | Read one device invitation by its id. |
| `tailnet_device_invite_delete` | destructive |  | Withdraw a device invitation. Anyone who already accepted keeps the share; this only stops it being accepted again. |
| `tailnet_device_invite_resend` | write |  | Send the invitation email again. Only for an invite that was created with an email address, and rate limited upstream to one a minute. Needs a user-owned credential. |
| `tailnet_device_invite_accept` | write |  | Accept a device invitation, taking the share for this credential's own user. Takes the invite URL or the bare id at the end of it. Needs a user-owned credential: a share belongs to a person. |
| `tailnet_user_invite_list` | read |  | List the outstanding invitations to join the tailnet. |
| `tailnet_user_invite_create` | write |  | Invite people to join the tailnet, each with the role they will get. Takes a list, so several people can be invited at once. Each answer carries an `inviteUrl`, which is a credential: anyone holding it can accept. Omitting `email` is how to create one nobody is mailed. Needs a user-owned credential: the invite records who sent it. |
| `tailnet_user_invite_get` | read |  | Read one tailnet invitation by its id. |
| `tailnet_user_invite_delete` | destructive |  | Withdraw a tailnet invitation. Needs a user-owned credential. |
| `tailnet_user_invite_resend` | write |  | Send the invitation email again. Only for an invite that was created with an email address, and rate limited upstream to one a minute. Needs a user-owned credential. |

## tailnet-logging

8 tools on the tailnet surface, in the full preset.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_audit_log_list` | read |  | Read the configuration audit log over a window: who changed what, when and from where. `start` and `end` are RFC3339 timestamps and both are required — the endpoint does not paginate, so the window is what bounds the answer. The optional filters are ANDed across kinds and ORed within one: give two `event` values to see either. |
| `tailnet_network_log_list` | read |  | Read network flow logs over a window: which node reached which, over what protocol, and how much went each way. Requires network flow logging to be switched on for the tailnet — `tailnet_settings_get` reports whether it is. `start` and `end` are RFC3339 timestamps and both are required. |
| `tailnet_log_stream_get` | read |  | Read where a log type is being streamed to. The credential fields are write-only and never come back: `token`, `s3_secret_access_key` and `gcs_credentials` are absent from this answer even when they are configured. Do not read this, edit it and write it back — the write would clear them. |
| `tailnet_log_stream_status_get` | read |  | Read whether a log stream is actually being delivered: last activity, last error, and the rates and counts behind them. |
| `tailnet_log_stream_replace` | write |  | Replace where a log type is streamed to. The whole endpoint, not a merge: a field this does not carry is gone. Send the credential — `token`, or `s3_secret_access_key`, or `gcs_credentials` — every time, because a read never returns it and a write without it removes it. |
| `tailnet_log_stream_delete` | destructive |  | Stop streaming a log type. The logs are still recorded and still readable here; only the delivery stops. |
| `tailnet_aws_external_id_create` | write |  | Mint an AWS external id for role-based S3 log streaming. The answer carries the id and the Tailscale AWS account id that will assume the role; both go into the IAM trust policy. `reusable: true` returns the same id to later reusable calls until it is linked to an account, which is how a caller that may retry avoids stranding ids. |
| `tailnet_aws_trust_policy_validate` | read |  | Check that an IAM role's trust policy actually lets Tailscale assume it with a given external id. Changes nothing; run it before configuring the stream. |

## tailnet-dns

11 tools on the tailnet surface, in the minimal, core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_dns_nameservers_get` | read |  | Read the tailnet's global DNS nameservers. Answers `{"dns": [...]}`. |
| `tailnet_dns_nameservers_replace` | write |  | Replace the tailnet's global nameservers with exactly this list. A full replace: anything not in `dns` is removed. An empty list removes every global nameserver, which also turns MagicDNS off — the answer says which state MagicDNS was left in. |
| `tailnet_dns_preferences_get` | read |  | Read whether MagicDNS is on for the tailnet. |
| `tailnet_dns_preferences_set` | write |  | Turn MagicDNS on or off for the tailnet. Turning it on needs at least one global nameserver; without one the control plane refuses the call. |
| `tailnet_dns_search_paths_get` | read |  | Read the search domains appended to a bare hostname. |
| `tailnet_dns_search_paths_replace` | write |  | Replace the search domains with exactly this list. A full replace: a domain not in `search_paths` is removed, and an empty list removes all of them. |
| `tailnet_dns_split_get` | read |  | Read the split-DNS map: which nameservers answer for which domain. |
| `tailnet_dns_split_update` | write |  | Change the split-DNS entries named here and leave the rest alone. A merge, and the only one of these that is: a domain the map does not mention keeps its nameservers. A domain mapped to `null` has its nameservers cleared. |
| `tailnet_dns_split_replace` | write |  | Replace the whole split-DNS map with exactly this one. A full replace: a domain not named here loses its nameservers, and an empty map clears every domain. |
| `tailnet_dns_configuration_get` | read |  | Read the whole DNS configuration in one document: nameservers, split DNS, search paths and preferences. The newer shape, and the one to read before a `_configuration_replace`. Its split DNS is a map of domain to resolver objects, where `tailnet_dns_split_get` gives bare addresses for the same thing. |
| `tailnet_dns_configuration_replace` | write |  | Replace the whole DNS configuration. A full replace of everything the document holds — nameservers, split DNS, search paths and preferences together. Read `tailnet_dns_configuration_get` first and send that back with the one change made, or anything omitted is cleared. |

## tailnet-keys

5 tools on the tailnet surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_key_list` | read |  | List the tailnet's keys: auth keys, API access tokens, OAuth clients and federated identities. No secret is included — a key's secret is returned only by the call that created it. `all` decides how wide the listing is, and defaults to true, which is every key the credential's scopes let it see. Set it to false to see only the keys belonging to the credential's own user. |
| `tailnet_key_get` | read |  | Read one key by its id. A revoked or expired key answers with `invalid: true` rather than a 404. |
| `tailnet_key_create` | write |  | Mint an auth key, an OAuth client or a federated identity. **The secret is in the answer and nowhere else.** There is no way to read it again; a caller that loses it has to create another key and revoke this one. `key_type: "auth"` takes `capabilities` and `expiry_seconds`; `"client"` and `"federated"` take `scopes` instead, and `tags` is required when the scopes include `devices:core` or `auth_keys`. An API access token cannot be created here. |
| `tailnet_key_update` | write |  | Reconfigure an OAuth client or a federated identity. Auth keys and API access tokens cannot be changed: revoke and mint another. The secret is neither regenerated nor returned. |
| `tailnet_key_delete` | destructive |  | Revoke a key. Anything authenticating with it stops working at once, and devices registered with an auth key are unaffected. |

## tailnet-policy

4 tools on the tailnet surface, in the minimal, core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_policy_get` | read |  | Read the tailnet policy file, with the version identifier a write has to quote back. Answers `{"etag": ..., "format": ..., "policy": ...}`. `format: "hujson"` — the default — gives the document as written, comments and all; `format: "json"` gives it parsed, with the comments gone. `details: true` asks instead for the control plane's own report on the document, with the policy base64-encoded beside its warnings and errors. |
| `tailnet_policy_set` | destructive |  | Replace the whole tailnet policy file. The highest-impact call on this surface: the policy is what decides who may reach what, and this replaces all of it. Read it first with `tailnet_policy_get`, change what came back, and send it with the `etag` that read gave you — the write is refused without either that or `over_default: true`, which is only for a tailnet whose policy is still the untouched default. A stale `etag` is a conflict: somebody else changed the policy since you read it. Validate first with `tailnet_policy_validate`, which changes nothing. Not idempotent, unusually for a replace: the guard makes the second call fail. Once the write lands, the `etag` it was made with is stale and the policy is no longer the untouched default. |
| `tailnet_policy_preview` | read |  | Show which rules of a candidate policy would match a user or an address, without saving anything. `subject_type: "user"` with an email address in `preview_for`, or `subject_type: "ipport"` with something like `10.0.0.1:80`. Answers the matching rules and the line each is written on. |
| `tailnet_policy_validate` | read |  | Check a policy, or run access tests, without saving anything. Two things in one endpoint, told apart by what is sent: give `tests` and they run against the policy in force; give `policy` and that document is parsed, checked, and its own `tests` run. An empty answer is a pass, which this reports as `{"passed": true}` so that a pass is not an empty result. |

## tailnet-posture

5 tools on the tailnet surface, in the full preset.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_posture_integration_list` | read |  | List the posture integrations configured for the tailnet, with the status of each one's last sync. No secret is included. |
| `tailnet_posture_integration_get` | read |  | Read one posture integration by its id. |
| `tailnet_posture_integration_create` | write |  | Configure a link to a device posture provider. A tailnet may have only one integration per provider: if one already exists the control plane refuses this call, and `tailnet_posture_integration_update` is what changes the existing one. The client secret is sent to the control plane and never comes back. |
| `tailnet_posture_integration_update` | write |  | Change an existing posture integration. Anything not given is left as it is, including the client secret, and the provider cannot be changed. |
| `tailnet_posture_integration_delete` | destructive |  | Remove a posture integration. Tailscale stops collecting posture from that provider, and any policy rule depending on those attributes stops matching. |

## tailnet-users

7 tools on the tailnet surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_user_list` | read |  | List the tailnet's users. Answers with `{"users": [...]}`. `role` and `type` narrow the listing at the control plane. Both accept `all`, which is the same as leaving them out. |
| `tailnet_user_get` | read |  | Read one user by their id. |
| `tailnet_user_role_set` | write |  | Change a user's role. A credential owned by a user cannot change that user's own role, which the control plane refuses rather than this server. |
| `tailnet_user_approve` | write |  | Approve a user waiting for approval. Does nothing if the tailnet does not require approval or the user is already approved, which the control plane treats as success. |
| `tailnet_user_suspend` | write |  | Suspend a user: their devices stop connecting and they cannot sign in. Reversible with `tailnet_user_restore`. |
| `tailnet_user_restore` | write |  | Restore a suspended user, and their devices with them. |
| `tailnet_user_delete` | destructive |  | Delete a user from the tailnet, along with every device they own. Not reversible: the user has to be invited again and their devices re-registered. |

## tailnet-settings

5 tools on the tailnet surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_contacts_get` | read |  | Read the addresses the tailnet's notices go to: `account`, `support` and `security`, each with its verification state. |
| `tailnet_contact_update` | write |  | Change one contact address. Not immediate: the new address is mailed a verification link and sits in `fallbackEmail` until it is followed, while the old one keeps receiving. |
| `tailnet_contact_verification_resend` | write |  | Send the verification link again for a contact address waiting on one. Only works while a verification is pending; there is nothing to resend for an address already in use. |
| `tailnet_settings_get` | read |  | Read the tailnet-wide settings: device and user approval, key durations, automatic updates, network flow logging and the rest. A setting reads as `null` where the tailnet's plan does not carry the feature, which is not the same answer as `false`. |
| `tailnet_settings_update` | write |  | Change tailnet-wide settings. A merge: a setting the document does not mention is left alone. The field names are Tailscale's own, as `tailnet_settings_get` reports them — `devicesApprovalOn`, `usersApprovalOn`, `devicesKeyDurationDays` and so on. |

## tailnet-webhooks

7 tools on the tailnet surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_webhook_list` | read |  | List the tailnet's webhook endpoints. No secret is included — a webhook's secret comes back only when it is created or rotated. |
| `tailnet_webhook_create` | write |  | Create a webhook endpoint. **The signing secret is in the answer and nowhere else.** It signs the `Tailscale-Webhook-Signature` header; a receiver that checks signatures needs it, and there is no way to read it again — only to rotate it, which breaks the old one. `provider_type` shapes the payload for a receiver that expects its own format. Omit it for Tailscale's own shape. |
| `tailnet_webhook_get` | read |  | Read one webhook endpoint by its id. |
| `tailnet_webhook_subscriptions_replace` | write |  | Replace which events an endpoint is sent. The whole list, not an addition: an event not in `subscriptions` stops being delivered. The endpoint's URL and provider cannot be changed — delete it and create another. |
| `tailnet_webhook_delete` | destructive |  | Delete a webhook endpoint. Deliveries stop at once. |
| `tailnet_webhook_test` | write |  | Queue a `test` event at the endpoint, to check it is reachable. Changes nothing in the tailnet and does send a real delivery. The control plane accepts it and delivers asynchronously, so a success here means queued rather than received. |
| `tailnet_webhook_secret_rotate` | destructive |  | Replace an endpoint's signing secret, and answer with the new one. **The old secret stops verifying immediately.** Every receiver checking signatures rejects every delivery until it has the new secret, which is in this answer and nowhere else. |

## tailnet-services

7 tools on the tailnet surface, in the core, full presets.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_service_list` | read |  | List the tailnet's services. Answers with `{"vipServices": [...]}`. Tailscale calls these VIP services in its own client and its answers, and services in its published description; these tools use the published name and send whichever path the control plane serves. |
| `tailnet_service_get` | read |  | Read one service by its name, which is `svc:` followed by the name. |
| `tailnet_service_replace` | write |  | Create a service, or replace one that already exists. One endpoint for both: a service that is not there is created, and one that is gets this document in place of its own — anything the document leaves out is discarded. Read `tailnet_service_get` first and send back what it answered with the one change made. On a create, the `name` in the document has to match `service_name`; on a replace, a different `name` renames the service. |
| `tailnet_service_delete` | destructive |  | Delete a service. Anything addressing it by name stops resolving. |
| `tailnet_service_devices_list` | read |  | List the devices standing behind a service, and whether each is approved to serve it. |
| `tailnet_service_approval_get` | read |  | Read whether one device may host a service, and whether an auto-approver decided it rather than a person. |
| `tailnet_service_approval_set` | write | tier depends on arguments | Approve one device to host a service, or withdraw that approval. Approving is a write. Withdrawing takes the device out of the service — traffic stops being sent to it — and needs the destructive tier. |

## tailnet-oauth-apps

5 tools on the tailnet surface, in the full preset.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_oauth_app_list` | read |  | List the OAuth apps registered on this tailnet, with their redirect URIs and scopes. Not the OAuth clients — those are `tailnet_key_list`. |
| `tailnet_oauth_app_create` | write |  | Register an OAuth app. `name` is 3 to 50 characters of letters, digits, `.`, `-` and `_`. Every redirect URI must use `https`, except `localhost`, `127.0.0.1` and `::1`, which may use any scheme; a bare IP address host is refused by the control plane. |
| `tailnet_oauth_app_get` | read |  | Read one OAuth app. |
| `tailnet_oauth_app_update` | write |  | Replace an OAuth app's registration. Every field is written, not merged: `name`, `redirect_uris` and `scopes` are required, and an omitted `description` or `allowed_node_attributes` clears what was there. |
| `tailnet_oauth_app_delete` | destructive |  | Delete an OAuth app. Anything holding an authorisation from it stops working. |

## tailnet-org

3 tools on the tailnet surface, in the full preset.

| Tool | Tier | Notes | What it does |
|---|---|---|---|
| `tailnet_organization_tailnet_list` | read |  | List every tailnet in an organisation, including the original one and any created through the tailnet-creation API. Alpha upstream. This is the one paginated endpoint on this surface: by default it follows the cursor and answers with every tailnet. Pass `cursor` to take one page at a time instead, using the `cursor` the previous answer carried. |
| `tailnet_organization_tailnet_create` | write |  | Create a tailnet in an organisation. Alpha upstream. The answer carries OAuth client credentials for the new tailnet; they are shown once and cannot be read back, so keep them from this answer or the tailnet is unreachable except through the admin console. |
| `tailnet_organization_tailnet_delete` | destructive | needs `confirm` | Delete a tailnet, with all of its users, devices and configuration. Alpha upstream. Irreversible, and irreversible for everyone in that tailnet rather than only for this caller — which is why it needs an explicit `confirm: true` as well as the destructive tier. Requires an access token for the tailnet being deleted, or an OAuth client with the `all` scope from the tailnet that created it. |
