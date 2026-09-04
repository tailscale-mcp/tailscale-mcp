<!-- Research note. Inventory of the local `tailscale` CLI (1.102.2, standalone macOS app) with every command, flag, output shape, risk class, and blocking behaviour.
     Produced by a research sub-agent on 2026-09-03 during the design interview; commands were probed read-only on the developer's machine. Not a spec. -->

# Tailscale CLI inventory — this Mac (macsys 1.102.2)

## 0. Environment

| Item | Value |
|---|---|
| `tailscale` on PATH | `/usr/local/bin/tailscale` — a 2-line `sh` wrapper that execs `/Applications/Tailscale.app/Contents/MacOS/tailscale` (same inode as `.../MacOS/Tailscale`, 11.5 MB universal Mach-O; installed by Homebrew cask `tailscale-app`) |
| Real CLI/daemon code | `/Applications/Tailscale.app/Contents/Library/SystemExtensions/io.tailscale.ipn.macsys.network-extension.systemextension/Contents/MacOS/io.tailscale.ipn.macsys.network-extension` (56 MB; the Go CLI is linked into the network extension, the outer binary is a thin launcher) |
| `tailscaled` | not on PATH (not the open-source/Homebrew `tailscale` formula) |
| Variant | **Standalone `Tailscale.app`** (`osVariant: "macsys"`, bundle id `io.tailscale.ipn.macsys`, system extension). Not App Store (`macos`), not open-source `tailscaled`. |
| Version | `tailscale version` → `1.102.2`, long `1.102.2-t6cac91817-g6ff0ddc72`, commit `6cac918179d4…`, go1.26.5 (tailscale/go 63ae404c82), cap 142. Daemon (`version --daemon`) also `1.102.2` |
| Transport | On this variant the CLI does not use `--socket`; it talks to the network extension's LocalAPI (localhost TCP + token). Runs as the logged-in GUI user, no `sudo` needed. |

## 1. Help dump

All `--help` output, one file per command (path joined with `_`; root is `tailscale.txt`; hidden commands included):

**`tailscale-cli/help/` (copied from the session scratchpad)** — 130 files (root + 129 commands).

Also there: `../json-docs.json` (output of hidden root flag `tailscale --json-docs`: the visible tree, 78 nodes), `../samples/*.out` (raw captures of every read-only sample below), and the walker scripts `dump-help.sh`, `dump-help-from.sh`, `complete_diff.py`.

How hidden things were found without executing anything: (a) `strings` of the network-extension binary for ffcli's `"HIDDEN: "` marker, (b) `tailscale completion __complete --descs=true --flags=true -- <words>` (the read-only completion engine lists hidden subcommands and most hidden flags), (c) `tailscale <cmd> --flag=value --help` (help short-circuits before Exec; "flag provided but not defined" vs "invalid boolean value" distinguishes missing / bool / string flags). The final leaf scan (completions for every leaf) found no further hidden children.

## 2. Read-only samples — output structure

All ran successfully as the GUI user, no sudo. (Own node: `<this-node>`, 100.x.y.z / fd7a:115c:a1e0::…, tailnet suffix `<tailnet>.ts.net`, 12 peers, home DERP `sfo`.)

| Command | Structure |
|---|---|
| `status --json` | Object: `Version`, `TUN`, `BackendState` ("Running"), `HaveNodeKey`, `AuthURL`, `TailscaleIPs[]`, `Self{}`, `Health[]`, `MagicDNSSuffix`, `CurrentTailnet{Name, MagicDNSSuffix, MagicDNSEnabled}`, `CertDomains[]`, `ExtraRecords`, `Peer{ "nodekey:…": PeerStatus }` (`null` with `--peers=false`), `User{ "<id>": {ID, LoginName, DisplayName, ProfilePicURL} }`, `ClientVersion{}`. `Self`/peer object keys: `ID, NodeID, PublicKey, HostName, DNSName, OS, UserID, TailscaleIPs[], AllowedIPs[], Addrs[], CurAddr, Relay, PeerRelay, RxBytes, TxBytes, Created, LastWrite, LastSeen, LastHandshake, Online, ExitNode, ExitNodeOption, Active, PeerAPIURL[], TaildropTarget, NoFileSharingReason, InNetworkMap, InMagicSock, InEngine`; Self only: `Capabilities[], CapMap{}`; peers optionally `KeyExpiry, ShareeNode, AltSharerUserID, Tags[]`. |
| `status` (text) | One row per node: `IP  hostname[.tailnet.ts.net]  user@  OS  state` where state is `-`, `idle, tx N rx N`, `active; relay "sfo", tx N rx N` / `direct <ip:port>`, or `offline, last seen Nd ago`. `--header` adds a header row. `--active` filters. Non-Running states print a message and exit non-zero [K]. |
| `ip` / `ip -4` / `ip -6` | One IP per line (`100.x.y.z` then `fd7a:…`); `-4`/`-6`/`-1` select one; `ip <peer>` resolves a peer. No JSON. |
| `dns status --json` | `{TailscaleDNS, CurrentTailnet{MagicDNSEnabled, MagicDNSSuffix, SelfDNSName}, SplitDNSRoutes{domain:[{Addr}]}, SearchDomains[], CertDomains[], ExitNodeFilteredSet[], SystemDNS{Nameservers[], SearchDomains[], MatchDomains[]}}`; `--all` adds forwarder debug info. Text: sections `'Use Tailscale DNS' status`, `MagicDNS configuration`, `System DNS configuration`. |
| `dns query --json <name> [type]` | `{Name, QueryType, Resolvers[{Addr}], ResponseCode, Answers[{Name, TTL, Class, Type, Body}]}`. |
| `netcheck --format=json` | stderr: timestamped `portmap:` log lines + `# Warning: this JSON format is not yet considered a stable interface`; stdout JSON: `Now, UDP, IPv6, IPv4, IPv6CanSend, IPv4CanSend, OSHasIPv6, ICMPv4, MappingVariesByDestIP, UPnP, PMP, PCP, PreferredDERP, RegionLatency{id:ns}, RegionV4Latency, RegionV6Latency, GlobalV4Counters, GlobalV6Counters, GlobalV4, GlobalV6, CaptivePortal`. Text: `Report:` block (`* Time`, `* UDP`, `* IPv4: yes, ip:port`, `* IPv6`, `* MappingVariesByDestIP`, `* PortMapping: UPnP, NAT-PMP, PCP`, `* Nearest DERP`, `* DERP latency:` list `- sfo: 22ms (San Francisco)`). Took ~0.3–1 s. |
| `serve status [--json]` / `funnel status --json` | `{}` here; text `No serve config`. Populated form (`serve get-config --all=true` **printed** `{"version":"0.0.1"}`) is the `ipn.ServeConfig` JSON (`TCP{port:{HTTPS,HTTP,TCPForward,TerminateTLS}}, Web{"host:port":{Handlers{path:{Proxy,Path,Text}}}}, AllowFunnel{}, Services{}, Foreground{}`) [K]. |
| `exit-node list` | exit 1 + `no exit nodes found` (none on this tailnet); otherwise table `IP HOSTNAME COUNTRY CITY STATUS` [K]. `exit-node suggest` → `No exit node suggestion is available.` |
| `lock status --json` | `{SchemaVersion, Enabled:false, PublicKey:"tlpub:…", NodeKey:"nodekey:…"}`; text `Tailnet lock is NOT enabled.` + `This node's tailnet-lock key: tlpub:…`. |
| `whois --json <ip>` / `whoami --json` | `{Node{ID, StableID, Name, User, Key, DiscoKey, Addresses[], AllowedIPs[], Endpoints[], Hostinfo{Hostname, Services[]}, Created, MachineAuthorized, Capabilities[], CapMap{}, ComputedName, ComputedNameWithHost}, UserProfile{ID, LoginName, DisplayName, ProfilePicURL}, CapMap{}}`. Text: `Machine:` block (Name, ID, Addresses) + `User:` block (Name, ID). |
| `metrics print` | Prometheus text (48 lines): `tailscaled_advertised_routes`, `tailscaled_approved_routes`, `tailscaled_home_derp_region_id`, `tailscaled_inbound/outbound_{bytes,packets}_total{path="derp|direct_ipv4|direct_ipv6|peer_relay_ipv4|peer_relay_ipv6"}`, `..._dropped_packets_total{reason}`. No JSON. |
| `get [name|all]` | Table `NAME VALUE` of the 18 `set`-able prefs; `--json` flat object `{accept-dns:true, …}`; `--set-flags` one line: `--accept-dns --accept-routes --advertise-connector=false … --operator=<user> --relay-server-port= … --webclient=false`. |
| `debug prefs` | JSON `ipn.Prefs`: `ControlURL, RouteAll, ExitNodeID, ExitNodeIP, InternalExitNodePrior, ExitNodeAllowLANAccess, CorpDNS, RunSSH, RunWebClient, WantRunning, LoggedOut, ShieldsUp, AdvertiseTags, Hostname, NotepadURLs, AdvertiseRoutes, AdvertiseServices, Sync, NoSNAT, NoStatefulFiltering, NetfilterMode, OperatorUser, AutoUpdate{Check,Apply}, AppConnector{Advertise}, PostureChecking, NetfilterKind, RemoteConfig, DriveShares, Config{PrivateNodeKey (zeroed), OldPrivateNodeKey, UserProfile, NetworkLockKey (zeroed), NodeID}`. |
| `debug hostinfo` | First line on stderr `TPM: error opening: TPM not supported on this platform`, then JSON `{IPNVersion, OS, OSVersion, Package, Hostname, Machine, GoArch, GoArchVar, GoVersion}`. |
| `version --json` | `{majorMinorPatch, short, long, gitCommit, osVariant, extraGitCommit, gitCommitTime, tailscaleGoGitHash, cap}`. `--daemon` text adds `Daemon: …` line. |
| `service list [--json]` | Text sentence (no services) / `[]`. |
| `syspolicy list [--json]` | Table `Name Origin Value Error`; JSON `{Summary{Scope, Origin{Name, Scope}}, Settings{name:{Value, Origin{Name, Scope}}}}`. |
| `switch --list [--json]` | Table `ID Tailnet Account`; JSON `[{id, nickname, tailnet, account, selected}]`. |
| `appc-routes` | `not a connector` (exit 0). |
| `wait --timeout 3s` | Silent, exit 0 (already Running). |
| `configure sysext status` | `System extension state: OK …` (single line). |
| `licenses` | Prose + URL `https://tailscale.com/licenses/apple`. |
| `drive list` | exit 1: `Taildrive CLI commands are not supported when using the macOS GUI app. Please use the Tailscale menu bar icon…` — whole `drive` tree is a stub on this variant. |
| `routecheck [--json]` | exit 1 `routecheck: report pending` (no report until `--probe`; not run). |

## 3. Inventory

Legend — **Flags**: `--name <T=default> meaning`; T: b=bool, s=string, d=duration, i=int; `(H)` = hidden. **JSON**: `--json`, `--format=json`, `native` (always JSON), `none`. **Class**: R=READ, W=WRITE, D=DESTRUCTIVE, N=no daemon/pure local, X=arbitrary. **Behavior**: B=bounded non-interactive, I=interactive/prompts, S=streaming, L=long-running server/loop, K=blocks waiting. **Priv**: `L:any` any local user / `L:op` root or `--operator` user / `L:root`; `M:gui` the logged-in GUI-app user, no sudo (sudo may fail to find the LocalAPI token) / `M:admin` macOS admin auth / `M:n/a` unsupported on this variant. Privilege for Linux is knowledge-based (LocalAPI read vs write gating) — marked [K]. **Platform**: `all` = every tailscaled platform including this build.

### 3.1 Root

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `tailscale` | root; prints usage | `<subcommand>` | `--socket <s=/var/run/tailscaled.socket>` path to tailscaled socket (ignored on macsys, transport is LocalAPI TCP+token); `(H) --json-docs <b>` dump visible command tree as JSON `{Name, Desc, Subcommands[], Flags[{Name, Desc}]}` | `--json-docs` | N | B | any | all |

### 3.2 Connection, identity, preferences

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior / bounded variant | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `up` | Connect, log in if needed, apply prefs (all prefs must be re-stated or it errors unless `--reset`) | none | see **F1** (26 visible + 2 hidden) | `--json` (AuthURL/QR/backend state) | W (D with `--force-reauth`/`--reset`) | K until Running (`--timeout` bounds); I: prints login URL and waits for browser; prompts on risks (`--accept-risk`). Non-interactive: `--auth-key` + `--timeout` + `--accept-risk=all` | L:op / M:gui | all |
| `down` | Disconnect (WantRunning=false) | none | `--accept-risk <s>` skip confirmation for `lose-ssh,mac-app-connector,all`; `--reason <s>` reason if policy requires | none | W | B; I prompt if SSH-in-use risk → `--accept-risk` | L:op / M:gui | all |
| `login` | Log in (adds a new profile if already logged in) | none | see **F1** (23 visible + 2 hidden) | none | W | K/I like `up`; bounded with `--auth-key --timeout` | L:op / M:gui | all |
| `logout` | Bring network down and expire node key | none | `--reason <s>` reason if policy requires | none | D | B | L:op / M:gui | all |
| `set` | Change only the prefs named | none | see **F1** (19 visible + 3 hidden) | none | W | B; I prompt on risk → `--accept-risk` | L:op / M:gui | all |
| `get` | Show current pref value(s) | `[setting-name \| all]` | `--json <b=false>`; `--set-flags <b=false>` print as `tailscale set` flags | `--json` | R | B | L:any / M:gui | all |
| `switch` | Switch active account/profile (parent has Exec) | `<id>` (ID, tailnet, account or display name) | `--list <b=false>` list accounts; `--json <b=false>` list as JSON | `--json` (list only) | W (switching restarts the connection); `--list` R | B | L:op / M:gui | all |
| `switch remove` | Remove a stored account from this machine | `<id>` | none | none | D | B | L:op / M:gui | all |
| `whoami` | Machine + user identity of this node (whois on own IP) | none | `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `(H) id-token` | Fetch an OIDC id-token for this machine | `<aud>` | none | prints raw JWT | R (network to control; secret material) | B | L:any [K] / M:gui | all |
| `wait` | Wait until interface/IPs are ready | none | `--timeout <d=0s>` 0 = forever | none | R | K; bounded with `--timeout` | L:any / M:gui | all |

**F1 — shared pref flags of `up` / `login` / `set`** (✓ = present). In `up`/`login` unspecified bools take the default shown; in `set` only flags you mention change.

| Flag | Type | Default (up/login) | up | login | set | Meaning |
|---|---|---|---|---|---|---|
| `--accept-dns` | b | true | ✓ | ✓ | ✓ | accept DNS config from admin panel |
| `--accept-risk` | s | "" | ✓ | – | ✓ | skip confirmation for `lose-ssh,mac-app-connector,all` |
| `--accept-routes` | b | false | ✓ | ✓ | ✓ | accept subnet routes advertised by peers |
| `--advertise-connector` | b | false | ✓ | ✓ | ✓ | advertise as app connector |
| `--advertise-exit-node` | b | false | ✓ | ✓ | ✓ | offer to be exit node |
| `--advertise-routes` | s | "" | ✓ | ✓ | ✓ | comma-separated CIDRs; "" = none |
| `--advertise-tags` | s | "" | ✓ | ✓ | – | comma-separated ACL tags (`tag:` prefix optional) |
| `--audience` | s | "" | ✓ | ✓ | – | audience for WIF id-token request |
| `--auth-key` | s | "" | ✓ | ✓ | – | auth key, or `file:<path>` |
| `--client-id` | s | "" | ✓ | ✓ | – | OAuth/WIF client id |
| `--client-secret` | s | "" | ✓ | ✓ | – | OAuth client secret, or `file:<path>` |
| `--exit-node` | s | "" | ✓ | ✓ | ✓ | exit node IP, base name, or `auto:any`; "" = none |
| `--exit-node-allow-lan-access` | b | false | ✓ | ✓ | ✓ | allow LAN access while using exit node |
| `--force-reauth` | b | false | ✓ | – | – | force reauth (may drop connection) |
| `--hostname` | s | "" | ✓ | ✓ | ✓ | override OS hostname |
| `--id-token` | s | "" | ✓ | ✓ | – | IdP token for WIF, or `file:<path>` |
| `--json` | b | false | ✓ | – | – | JSON output (unstable format) |
| `--login-server` | s | `https://controlplane.tailscale.com` | ✓ | ✓ | – | control server URL |
| `--nickname` | s | "" | – | ✓ | ✓ | short name for the account/profile |
| `--operator` | s | "" | ✓ | ✓ | ✓ | Unix user allowed to operate without sudo (Unix incl. macOS) |
| `--qr` | b | false | ✓ | ✓ | – | show QR for login URL |
| `--qr-format` | s | auto | ✓ | ✓ | – | `auto,ascii,large,small` |
| `--report-posture` | b | false | ✓ | ✓ | ✓ | allow posture collection |
| `--reset` | b | false | ✓ | – | – | reset unspecified settings to defaults |
| `--shields-up` | b | false | ✓ | ✓ | ✓ | block incoming connections |
| `--ssh` | b | false | ✓ | ✓ | ✓ | run Tailscale SSH server |
| `--timeout` | d | 0s | ✓ | ✓ | – | max wait for Running; 0 blocks forever |
| `--auto-update` | b | – | – | – | ✓ | auto-apply updates (maps to app updater on macsys) |
| `--relay-server-port` | s (int or "") | – | – | – | ✓ | UDP port for peer-relay server; 0 random; "" disable |
| `--relay-server-static-endpoints` | s | – | – | – | ✓ | comma-separated `ip:port` static relay endpoints |
| `--update-check` | b | – | – | – | ✓ | notify about updates |
| `--webclient` | b | – | – | – | ✓ | expose web UI on port 5252 over Tailscale |
| `(H) --host-routes` | b | true | ✓ | ✓ | – | legacy; only `true` accepted ("unsupported value; only 'true' is allowed") |
| `(H) --posture-checking` | b | false | ✓ | ✓ | ✓ | hidden alias of `--report-posture` |
| `(H) --sync` | b | – | – | – | ✓ | toggles the `Sync` pref (undocumented/experimental) |
| `(H) --remote-config` | b | – | – | – | ✓ | toggles the `RemoteConfig` pref (lets control plane manage node config; experimental, treat as dangerous) |
| Linux-only, absent here [K] | | | | | | `--snat-subnet-routes` (b, true), `--stateful-filtering` (b, false), `--netfilter-mode` (on/nodivert/off); Windows-only `--unattended` (b) |

### 3.3 Status and diagnostics

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior / bounded variant | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `status` | State of tailscaled and peers | none | `--active <b=false>` only peers with active sessions; `--browser <b=true>` open browser (web mode); `--header <b=false>` column headers; `--json <b=false>`; `--listen <s=127.0.0.1:8384>` web-mode listen addr; `--peers <b=true>`; `--self <b=true>`; `--web <b=false>` run HTML status server | `--json` | R | B; `--web` = L server | L:any / M:gui | all |
| `ip` | Show Tailscale IPs (self or a peer/service) | `[peer or service hostname or ip]` | `--1 <b=false>` one IP; `--4 <b=false>` IPv4 only; `--6 <b=false>` IPv6 only; `--assert <s>` assert one IP matches | none | R | B | L:any / M:gui | all |
| `netcheck` | Analyze local network / DERP latency (works without daemon) | none | `--bind-address <s>`; `--bind-port <i=0>`; `--every <d=0s>` repeat interval; `--format <s="">` `""`, `json`, `json-line`; `--verbose <b=false>` | `--format=json` (+ stderr warning that format is unstable) | R (sends probes) | B (~1–5 s); `--every` = L | L:any (no daemon needed) / M:gui | all |
| `ping` | Ping at Tailscale layer, show path | `<hostname-or-IP>` | `--c <i=10>` max pings, 0 = infinite; `--icmp <b=false>`; `--peerapi <b=false>`; `--size <i=0>` disco payload; `--timeout <d=5s>` per ping; `--tsmp <b=false>`; `--until-direct <b=true>` stop once direct; `--verbose <b=false>` | none (text per ping) | R | bounded by `--c`×`--timeout` (default ≤ ~50 s); `--c=0` = L | L:any / M:gui | all |
| `whois` | Machine/user for a Tailscale IP | `ip[:port]` | `--json <b=false>`; `--proto <s="">` `tcp`/`udp` | `--json` | R | B | L:any / M:gui | all |
| `version` | Print version | none | `--daemon <b=false>` also daemon version; `--json <b=false>`; `--track <s>` `stable`/`release-candidate`/`unstable`; `--upstream <b=false>` fetch latest from pkgs.tailscale.com | `--json` | N (R with `--daemon`; network with `--upstream`) | B | L:any / M:gui | all |
| `licenses` | Print OSS license URL | none | none | none | N | B | any | all |
| `metrics` | container (prints usage) | – | – | – | – | – | – | all |
| `metrics print` | Print user-facing metrics (Prometheus text) | none | none | none | R | B | L:any / M:gui | all |
| `metrics write` | Write metrics to a file | `<path>` | none | none | R (writes local file) | B | L:any / M:gui | all |
| `dns` | container (prints usage) | – | – | – | – | – | – | all |
| `dns status` | Diagnose DNS forwarder/MagicDNS/system DNS | none | `--all <b=false>` advanced debug info; `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `dns query` | Resolve a name through the Tailscale resolver | `<name> [type]` (A, AAAA, …) | `--json <b=false>` | `--json` | R (network query) | B | L:any / M:gui | all |
| `exit-node` | container (Exec prints help) | none | none | – | – | – | – | all |
| `exit-node list` | List exit nodes (Mullvad + tailnet) | none | `--filter <s>` country filter | none (table) | R | B; exit 1 when none | L:any / M:gui | all |
| `exit-node suggest` | Suggest best exit node | none | `(H) --force-probe <b>` force a fresh latency probe | none | R (probe) | B | L:any / M:gui | all |
| `appc-routes` | App-connector learned routes | none | `--all <b=false>` learned+policy routes; `--map <b=false>` domain→routes map; `--n <b=false>` count only | none | R | B | L:any / M:gui | all |
| `(H) routecheck` | Experimental reachability report | none | `--format <s>` `""`/`json`/`json-line`; `--json <b=false>`; `--probe <b=false>` run a new probe now | `--json` / `--format=json` | R (`--probe` triggers probing) | B; exit 1 "report pending" when none | L:any [K] / M:gui | all (1.102 experimental) |
| `bugreport` | Print a shareable diagnostic marker (logged to Tailscale log service) | `[note]` | `--diagnose <b=false>` extra checks; `--record <b=false>` pause, then write a second marker | none | R (emits log marker) | B; `--record` = I (waits for Enter) | L:any / M:gui | all |

### 3.4 Serve, Funnel, Services, certs, web UI

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior / bounded variant | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `serve` | Share a target (port, URL, file, dir, text) on the tailnet (parent has Exec) | `<target>` | `--accept-app-caps <s>` app caps to forward (comma list); `--bg <b=false>` background (defaults true with `--service`); `--http <s>` port; `--https <s>` port (default mode); `--proxy-protocol <s>` 1 or 2; `--service <s>` serve for a Tailscale Service VIP; `--set-path <s>`; `--tcp <s>` port; `--tls-terminated-tcp <s>` port; `--tun <b=false>` forward all traffic (services only); `--yes <b=false>` no prompts | none | W | Foreground = L (config removed on exit); `--bg --yes` = B | L:op / M:gui | all |
| `serve status` | Show serve/funnel config | none | `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `serve reset` | Remove all serve+funnel config | none | none | none | D | B | L:op / M:gui | all |
| `serve drain` | Stop advertising a service (drain) | `<service>` | none | none | W | B | L:op / M:gui | all |
| `serve clear` | Delete a service's serve config | `<service>` | none | none | D | B | L:op / M:gui | all |
| `serve advertise` | (Re-)advertise a service | `<service>` | none | none | W | B | L:op / M:gui | all |
| `serve get-config` | Dump serve config. **Corrected 2026-09-04 against 1.102.2: prints the JSON to stdout and ignores the `<file>` positional; nothing is written.** One of `--all`/`--service` is required or it refuses. | `<file>` (ignored) | `--all <b=false>` all services; `--service <s>` one service (needs the `svc:` prefix) | prints JSON | R | B | L:any / M:gui | all |
| `serve set-config` | Apply serve config from a file | `<file>` | `--all <b=false>`; `--service <s>` | reads JSON file | W (replaces) | B | L:op / M:gui | all |
| `funnel` | Expose target to the public internet (parent has Exec) | `<target>` | `--bg <b=false>`; `--https <s>`; `--proxy-protocol <s>`; `--set-path <s>`; `--tcp <s>`; `--tls-terminated-tcp <s>`; `--yes <b=false>` | none | W (public exposure) | L foreground / B with `--bg --yes` | L:op / M:gui | all |
| `funnel status` | Show funnel config | none | `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `funnel reset` | Remove all serve+funnel config | none | none | none | D | B | L:op / M:gui | all |
| `service` | container (usage) | – | – | – | – | – | – | all |
| `service list` | List Tailscale Services this node hosts | none | `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `cert` | Obtain TLS cert via ACME (writes files) | `<domain>` | `--cert-file <s>` (`-` = stdout; default DOMAIN.crt); `--key-file <s>` (default DOMAIN.key); `--min-validity <d=0s>`; `--serve-demo <b=false>` serve on :443 instead of writing | none | W (network ACME + local files; key material) | B but slow (seconds–minutes); `--serve-demo` = L | L:op (cert perm) / M:gui | all |
| `web` | Run web UI server for controlling the daemon | none | `--cgi <b=false>`; `--listen <s=localhost:8088>`; `--origin <s>`; `--prefix <s>`; `--readonly <b=false>` | none | W (UI can mutate) | L | L:op / M:gui | all |

### 3.5 Files and Taildrive

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `file` | container (usage) | – | – | – | – | – | – | all |
| `file cp` | Send files (Taildrop) | `<files...> <target>:` (`-` = stdin) | `--name <s>` alternate filename; `--targets <b=false>` list valid targets; `--update-interval <d=250ms>` progress repaint, ≤0 disables; `--verbose <b=false>` | none | W (sends data) ; `--targets` R | L during transfer; bounded by size | L:op / M:gui | all |
| `file get` | Move received files from inbox to a directory | `<target-directory>` | `--conflict <s=skip>` `skip`/`overwrite`/`rename`; `--loop <b=false>` keep receiving; `--verbose <b=false>`; `--wait <b=false>` wait if inbox empty | none | W (local FS; `overwrite` destructive) | B; `--wait` = K; `--loop` = L | L:op / M: GUI variants write received files directly, CLI inbox mode may be unsupported [K, untested] | all |
| `(H) drive` | Taildrive (share/rename/unshare/list on Linux/Windows) | `[...any]` | none here | none | stub: exit 1 | B | M:n/a | stub on macOS GUI; real subcommands on tailscaled platforms [K] |

### 3.6 Tailnet lock

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `lock` | container; bare `lock` = `lock status` | – | – | – | R | B | L:any / M:gui | all |
| `lock status` | Tailnet lock state and this node's key | none | `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `lock log` | List lock changes (AUMs) | none | `--json <b=false>`; `--limit <i=50>` | `--json` | R | B | L:any / M:gui | all |
| `lock disablement-kdf` | Compute disablement value from secret (offline) | `<hex-secret>` | none | none | N | B | any | all |
| `lock init` | Enable tailnet lock | `<trusted-key>...` | `--confirm <b=false>` no prompt; `--gen-disablement-for-support <b=false>`; `--gen-disablements <i=1>` | none | D (tailnet-wide) | I unless `--confirm` | L:op / M:gui | all |
| `lock add` | Add trusted signing keys | `<public-key>...` | none | none | W | B | L:op / M:gui | all |
| `lock remove` | Remove trusted signing keys | `<public-key>...` | `--re-sign <b=true>` re-sign affected signatures | none | D | B | L:op / M:gui | all |
| `lock sign` | Sign a node key or pre-approved auth key | `<node-key> [<rotation-key>]` or `<auth-key>` | none | none | W | B | L:op / M:gui | all |
| `lock disable` | Consume disablement secret; turn off lock tailnet-wide | `<disablement-secret>` | none | none | D | B | L:op / M:gui | all |
| `lock local-disable` | Disable lock on this node only | none | none | none | D (local) | B | L:op / M:gui | all |
| `lock revoke-keys` | Revoke compromised lock keys (multi-step recovery) | `<tailnet-lock-key>...` or `<recovery-blob>` | `--cosign <b=false>`; `--finish <b=false>`; `--fork-from <s>` parent AUM hash | none | D | multi-step, B per step | L:op / M:gui | all |

### 3.7 Host configuration, policy, update

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `configure` | container (usage) | – | – | – | – | – | – | all |
| `configure kubeconfig` | [ALPHA] add Tailscale auth-proxy cluster to kubeconfig | `<hostname-or-fqdn>` | `--http <b=false>` use HTTP to auth proxy | none | W (edits `~/.kube/config`) | B | L:any / M:gui | all |
| `configure sysext` | container | – | – | – | – | – | – | macOS (standalone) |
| `configure sysext activate` | Activate the system extension | none | none | none | W | B (macOS may prompt in System Settings) | M:admin | macOS standalone |
| `configure sysext deactivate` | Deactivate system extension (drops VPN) | none | none | none | D | B (+OS prompt) | M:admin | macOS standalone |
| `configure sysext status` | Show system-extension state | none | none | none | R | B | M:gui | macOS standalone |
| `configure mac-vpn` | container | – | – | – | – | – | – | macOS (App Store + standalone) |
| `configure mac-vpn install` | Install VPN configuration | none | none | none | W | B (+OS prompt) | M:admin | macOS |
| `configure mac-vpn uninstall` | Remove VPN configuration | none | none | none | D | B (+OS prompt) | M:admin | macOS |
| `(H) configure flash-appliance` | Download Tailscale appliance image and write it to a block device | none | `--add-ssh-authorized-keys <s>`; `--disk <s>` target device; `--gaf <s>` local image file; `--track <s=stable>`; `--variant <s>` `pi-arm64`/`vm-amd64`/`vm-arm64` (empty → interactive); `--yes <b=false>` skip destructive-write prompt | none | D (raw disk write) | L (download) + I unless `--yes` and `--variant` | root | macOS/Linux (experimental) |
| `(H) configure pve-appliance` | Create appliance VM on a Proxmox host | none | `--add-ssh-authorized-keys <s>`; `--bridge <s=vmbr0>`; `--cores <i=2>`; `--disk-size <s=4G>`; `--gaf <s>`; `--memory <i=1024>` MiB; `--name <s>` (default `tsapp-<vmid>`); `--start <b=true>`; `--storage <s>` required; `--track <s=stable>`; `--variant <s=vm-amd64>`; `--vmid <i=0>`; `--yes <b=false>` | none | W/D (creates VM) | L + I unless `--yes` | root on PVE host | Linux (Proxmox) |
| `syspolicy` | container (usage) | – | – | – | – | – | – | all |
| `syspolicy list` | List effective MDM/system policies | none | `--json <b=false>` | `--json` | R | B | L:any / M:gui | all |
| `syspolicy reload` | Force re-read of policies | none | `--json <b=false>` | `--json` | W | B | L:op / M:gui | all |
| `update` | Self-update the client | none | `--dry-run <b=false>` print what would happen; `--yes <b=false>` no prompts | none | D (replaces binaries, restarts) | I unless `--yes`; `--dry-run` = B network check | L:root / M: not supported on GUI variants (Sparkle/App Store instead) [K, untested] | Linux/Win/FreeBSD/some macOS |

### 3.8 Remote sessions

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `ssh` | SSH to a Tailscale node (execs system `ssh` with Tailscale-aware config) | `[user@]<host> [args...]` | none | none | X | I session | L:any / M:gui | all (needs `ssh` binary) |
| `nc` | Connect stdin/stdout to a TCP port over the tailnet | `<hostname-or-IP> <port>` | none | none | X | S | L:any / M:gui | all |

### 3.9 Shell completion, systray

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv | Platform |
|---|---|---|---|---|---|---|---|---|
| `completion` | container (usage) | `<shell>` | – | – | – | – | – | all |
| `completion bash` / `zsh` / `fish` / `powershell` | Emit shell completion script | none | `--descs <b=true>` include descriptions; `--flags <b=true>` suggest flags | none | N | B | any | all |
| `(H) completion __complete` | Completion engine (lists subcommands/flags incl. hidden; may query daemon for hostnames) | `-- <args to complete...>` | `--descs <b=true>`; `--flags <b=true>` | none | N/R | B | any | all |
| `(H) systray` | Linux systray app | none | none | none | stub: error | B | – | Linux only; "not included in this client build" here |

### 3.10 `debug` (hidden; "not a stable interface")

The parent has its own Exec: `tailscale debug --file=get` lists Taildrop inbox files (R), `--file=delete:NAME` deletes one (D), `--file=NAME` writes the file to stdout (R); `--cpu-profile <s>` (`-` = stdout) with `--profile-seconds <i=15>` and `--mem-profile <s>` pull profiles from the daemon (R, heavy, bounded). Priv for `debug*`: Linux — LocalAPI debug endpoints are write-gated (root/operator); the pure readers below marked `L:any` need only read access [K]; macOS — all `M:gui`.

| Command | Purpose | Positionals | Flags | JSON | Class | Behavior | Priv (Linux) |
|---|---|---|---|---|---|---|---|
| `debug derp-map` | Print DERP map | none | none | native | R | B | any |
| `debug component-logs` | Enable/disable component debug logs | `[magicsock\|sockstats\|syspolicy]` | `--for <d=1h0m0s>` ≤0 disables | none | W | B | op |
| `debug daemon-goroutines` | Dump daemon goroutines | none | none | none | R | B | op |
| `debug daemon-logs` | Watch daemon logs | none | `--time <b=false>`; `--verbose <i=0>` | none | R | S | op |
| `debug daemon-bus-events` | Watch event bus | none | none | native (stream) | R | S | op |
| `debug daemon-bus-graph` | Event-bus graph | none | `--format <s=json>` json/dot | native | R | B | op |
| `debug daemon-bus-queues` | Bus queue depths | none | none | none | R | B | op |
| `debug metrics` | Daemon internal metrics | none | `--watch <b=false>` JSON deltas | `--watch` | R | B; `--watch` = S | any |
| `debug env` | Print CLI process environment | none | none | none | N (may leak env secrets) | B | any |
| `debug stat` | Stat files | `<files...>` | none | none | N | B | any |
| `debug hostinfo` | Print hostinfo | none | none | native (+stderr TPM line) | R | B | any |
| `debug local-creds` | Print how to reach LocalAPI (includes token/curl line) | none | none | none | R (secret) | B | any |
| `debug localapi` | Call any LocalAPI endpoint | `[<method>] <path> [<body\|"-">]` | `--v <b=false>` dump headers | depends | X | B | any/op per endpoint |
| `debug restun` | Force magicsock re-STUN | none | none | none | W | B | op |
| `debug rebind` | Force magicsock rebind | none | none | none | W | B | op |
| `debug rotate-disco-key` | Rotate disco key | none | none | none | W | B | op |
| `debug derp-set-on-demand` | DERP on-demand mode (breaks reachability) | none | none | none | D | B | op |
| `debug derp-unset-on-demand` | Undo above | none | none | none | W | B | op |
| `debug break-tcp-conns` | Break daemon's TCP conns | none | none | none | D | B | op |
| `debug break-derp-conns` | Break DERP conns | none | none | none | D | B | op |
| `debug pick-new-derp` | Temporarily switch home DERP | none | none | none | W | B | op |
| `debug force-prefer-derp` | Prefer region id (0 clears) | `<region-id>` (per short help) | none | none | W | B | op |
| `debug force-netmap-update` | No-op full netmap update (load test) | none | none | none | W | B | op |
| `debug reload-config` | Reload tailscaled config file | none | none | none | W | B | op |
| `debug control-knobs` | Show control knobs | none | none | native | R | B | any |
| `debug prefs` | Print prefs | none | `--pretty <b=false>` | native | R | B | any |
| `debug watch-ipn` | Subscribe to IPN notify bus | none | `--count <i=0>` exit after N (0 forever); `--engine-updates <b=false>`; `--health-actions <b=false>`; `--initial <b=false>`; `--initial-client-version <b=false>`; `--initial-drive-shares <b=false>`; `--initial-health <b=false>`; `--initial-outgoing-files <b=false>`; `--initial-status <b=false>`; `--initial-suggested-exit-node <b=false>`; `--peer-changes <b=true>`; `--peer-patches <b=true>`; `--peer-wireguard-state <b=false>` | native (stream) | R | S; bounded with `--count=N` | any |
| `debug netmap` | Print current netmap | none | none | native (large) | R | B | any |
| `debug via` | Convert site CIDR ⇄ IPv6 via route | `<site-id> <v4-cidr>` or `<v6-route>` | none | none | N | B | any |
| `debug ts2021` | Test control-plane Noise connectivity | none | `--ace <s>`; `--dial-plan <s>` JSON file; `--host <s=controlplane.tailscale.com>`; `--verbose <b=false>`; `--version <i=142>` | none | R (network) | B | any |
| `debug set-expire` | Set node-key expiry (testing) | none | `--in <d=0s>` | none | D | B | op |
| `debug dev-store-set` | Write a state-store key/value | `<key> <value>` [K] | `--danger <b=false>` required | none | D | B | op |
| `debug derp` | Test a DERP configuration | none (per help) | none | none | R (network) | B | any |
| `debug capture` | Stream pcap of tunnel traffic | none | `--o <s>` path or `-`; empty launches Wireshark | none | R (sensitive traffic) | S / L | op |
| `debug portmap` | Port-mapping (UPnP/PMP/PCP) probe | none | `--duration <d=5s>`; `--gateway-addr <s>`; `--log-http <b=false>`; `--self-addr <s>`; `--type <s>` `""`/pmp/pcp/upnp | none | R (network) | B | op |
| `debug peer-endpoint-changes` | Endpoint-change history for a peer | `<hostname-or-IP>` | none | native | R | B | any |
| `debug dial-types` | Try dialing a host via each path type | `<hostname-or-IP> <port>` | `--network <s=tcp>` | none | R (network) | B | op |
| `debug resolve` | DNS lookup via Go resolver | `<hostname>` | `--net <s=ip>` ip/ip4/ip6 | none | R (network) | B | any |
| `debug go-buildinfo` | Go build info | none | none | none | N | B | any |
| `debug peer-relay-servers` | Candidate peer-relay servers | none | none | none | R | B | any |
| `debug test-risk` | Fake risky action (prompt test) | none | `--accept-risk <s>` | none | N | I unless flag | any |
| `debug statedir` | Print state directory | none | none | none | R | B | any |
| `debug peer-relay-sessions` | Active relay sessions through this node | none | none | none | R | B | any |
| `debug clear-netmap-cache` | Delete cached netmaps | none | none | none | D | B | op |

## 4. Hidden items summary

- Hidden top-level commands (5): `debug`, `drive`, `id-token`, `routecheck`, `systray`.
- Hidden nested commands (3): `configure flash-appliance`, `configure pve-appliance`, `completion __complete`.
- Hidden flags: root `--json-docs`; `up`/`login` `--host-routes`, `--posture-checking`; `set` `--sync`, `--remote-config`, `--posture-checking`; `exit-node suggest` `--force-probe`. Confirmed absent on this build: `--netfilter-mode`, `--snat-subnet-routes`, `--stateful-filtering`, `--unattended`, and any `drive` subcommands.

## 5. Counts

- Command nodes (excluding root): **129** = 77 visible (34 top-level) + 52 hidden (debug + 44 children, drive, id-token, routecheck, systray, flash-appliance, pve-appliance, `__complete`).
- **Leaf commands: 114** — 63 visible + 51 hidden. Containers: 15 (`switch`, `configure`, `configure sysext`, `configure mac-vpn`, `syspolicy`, `dns`, `metrics`, `funnel`, `serve`, `service`, `file`, `lock`, `exit-node`, `completion`, `debug`); of these, `switch`, `serve`, `funnel`, `lock` (= status) and `debug` (`--file`/profiles) are also runnable in their own right, so **119 distinct runnable command paths**.

## 6. Recommended exclusions from an MCP server

| Exclude | Reason |
|---|---|
| `ssh`, `nc` | interactive/streaming sessions; arbitrary remote execution |
| `web`, `status --web`, `cert --serve-demo`, `serve`/`funnel` foreground (no `--bg`) | long-running servers; foreground serve is ephemeral and never returns |
| `completion *`, `systray`, `licenses` | no operational value (or stub) |
| `update`, `configure sysext activate/deactivate`, `configure mac-vpn *`, `configure flash-appliance`, `configure pve-appliance` | host/OS mutation, admin/root prompts, raw disk writes, VM creation |
| `drive *` | unsupported on this macOS variant (exit 1) |
| `debug` entirely except `prefs`, `hostinfo`, `derp-map`, `netmap`, `control-knobs`, `metrics` (no `--watch`), `peer-relay-*`, `go-buildinfo`, `statedir`, `via`, `watch-ipn --count=N` | unstable interface; `localapi`/`dev-store-set`/`set-expire`/`break-*`/`derp-set-on-demand`/`clear-netmap-cache` are destructive; `capture`, `daemon-logs`, `daemon-bus-events`, `metrics --watch` stream; `env`/`local-creds` leak secrets; `--file=delete:` destroys inbox files |
| `id-token`, `up --auth-key`/`--client-secret`/`--id-token` args, `cert` key output to stdout | secret material would pass through the model/transport |
| `logout`, `switch remove`, `serve reset`/`funnel reset`, `serve clear`, `lock init/disable/local-disable/revoke-keys/remove`, `up --force-reauth`/`--reset`, `set --remote-config` | destructive or identity-changing; if kept, gate behind explicit confirmation |
| `file get --wait/--loop`, `ping --c=0`, `netcheck --every`, `wait` without `--timeout`, `up`/`login` without `--timeout` | unbounded blocking; expose only bounded variants (`ping --c N`, `wait --timeout`, `up --timeout --auth-key --accept-risk=all`) |
| `bugreport --record`, `lock init` without `--confirm`, `funnel`/`serve` without `--yes`, `update` without `--yes`, `down`/`set`/`up` without `--accept-risk` | interactive prompts hang a non-TTY caller; always pass the non-interactive flag |
| `funnel <target>` | publishes to the public internet — keep only with explicit confirmation |

## 7. Differences vs recent releases (knowledge-based, [K] — versions approximate)

- New in the 1.9x–1.102 line: `get` (prefs read-back with `--set-flags`), `service list` and Tailscale Services (`serve --service/--tun/--accept-app-caps`, `serve drain/clear/advertise/get-config/set-config`), `whoami`, `wait`, `appc-routes`, hidden experimental `routecheck`, hidden `configure flash-appliance`/`pve-appliance`, `serve`/`funnel --proxy-protocol`, workload-identity flags `--client-id/--client-secret/--audience/--id-token` on `up`/`login`, `set --relay-server-static-endpoints`, `debug peer-relay-*`, `debug daemon-bus-*`, `--advertise-tags` now auto-prefixes `tag:`.
- 1.86–1.90: peer relay (`set --relay-server-port`), `exit-node list` country filter improvements, `syspolicy` JSON.
- 1.78: `metrics print/write` (user metrics). 1.72: `dns status/query`, `syspolicy list/reload`. 1.66–1.68: `exit-node suggest`, `debug watch-ipn` initial-* flags. 1.58: `--posture-checking` renamed to `--report-posture` (old name kept as hidden alias). 1.56: `drive` (alpha), `set --webclient`. 1.50: `serve`/`funnel` "v2" syntax (`tailscale serve <target>`); the legacy `serve https:443 / http / tcp / tls-terminated-tcp` subcommands are gone. 1.36: `configure kubeconfig`. 1.34: `switch` multi-account. 1.32: `lock` GA.
- Removed/absent vs older or other-platform builds: `--host-routes` (now hidden no-op), `configure synology`/`synology-cert`/`jetkvm` (Linux/NAS only), `drive share/rename/unshare/list` (tailscaled platforms only), `up --unattended` (Windows), Linux netfilter/SNAT flags.

## 8. Wrapper gotchas observed

- Always pass flags as `--flag=value` and **before** positionals (Go flag parsing stops at the first positional; `serve get-config <file> --all` fails, `serve get-config --all <file>` works).
- Bare `tailscale lock` runs `lock status`; bare `exit-node`/`dns`/`file`/`metrics`/`configure`/`syspolicy`/`service` print usage.
- stderr noise: `netcheck` prints timestamped `portmap:` lines plus a JSON-instability warning; `debug hostinfo` prints a `TPM:` line; parse stdout only.
- Non-zero exits that are not failures: `exit-node list` (none found), `routecheck` (report pending), `drive *` (unsupported here), `status` in non-Running states.
- `status --json` peer map is keyed by `nodekey:…`; `Peer` is `null` when `--peers=false`.
- `serve`/`funnel` run in the **foreground** by default: `--bg=true` is required or the command never returns. `--yes=true` skips the interactive prompt.
- `funnel` on a tailnet where Funnel is not enabled **does not fail**: it prints `Funnel is not enabled on your tailnet. To enable, visit: <url>` and then polls for ever, `--yes` and `--bg` notwithstanding. Only a timeout ends it, so what the child printed before it was killed is the whole of the answer (DECISIONS Q25).
- `serve --https=<port> off` reports `failed to remove web serve: handler does not exist` when there is no handler there, which classifies as `not_found`.
- Service names must carry the `svc:` prefix; the client rejects a bare name with `invalid service name` rather than adding it, unlike `--advertise-tags`.
- `file cp` requires the final argument to end in a colon — without one it refuses with `final argument to 'tailscale file cp' must end in colon` — and a target it cannot resolve fails with `error looking up IP of "<name>": lookup <name>: no such host`, which classifies as `not_found`. A file named `-` means standard input.
- `file cp --targets` prints a tab-separated table with no header: address, name, and for a node that is not up a third column such as `offline; last seen 66h38m0s ago`. Exit 0 even when the list is empty.
- `--update-interval=0` on `file cp` disables the repainting progress line, which is what a captured pipe wants; the flag parses without a duration unit only for zero.
- `file get` on an empty inbox prints nothing and exits 0; on a directory that is not there it exits 1 with `"<path>" is not a directory`, which is neither "no such" nor "not found" and so classifies as `cli_failed`. `--wait` and `--loop` are the two flags that would hold the call open.
- `cert` with neither `--cert-file` nor `--key-file` writes `DOMAIN.crt` and `DOMAIN.key` into the working directory; either flag set to `-` writes that half to stdout, private key included. `cert` with no domain at all exits **0** after printing usage and the node's own FQDN.
- `syspolicy reload --json=true` prints a `{"Summary": …, "Settings": …}` object on stdout, so it is read with the same document helper as the `status` family.
- `drive list` prints a padded three-column table (`name path as`) with a dashes row under the header and no `--json`. The Go side pads each column to the longest value in it, header included, so the header states the column offsets for every row beneath it — which is the only reliable way to read the table back. Splitting on whitespace cannot: a shared path may contain spaces, and the `as` column is blank on any platform where `drive.AllowShareAs()` is false, leaving a row with nothing in its last column (DECISIONS Q36). No `drive` subcommand takes flags.
- The macOS GUI packaging carries every `drive` subcommand and refuses all of them with `Taildrive CLI commands are not supported when using the macOS GUI app.` on stderr, exit 1 — a fact about the packaging, not the operating system (DECISIONS Q31).

## 9. Supported version floor

Established for ticket 06, from the upstream release documentation
(<https://tailscale.com/docs/reference/tailscale-client-versions>) and the
per-command dating in §7.

**Upstream publishes no end-of-life policy and no supported-version floor.**
The client version reference documents three tracks — stable (even minor
numbers, released roughly every four weeks), unstable (odd minor numbers) and
release candidates (patch releases of the current stable) — but states no
minimum. Tailscale's public position is backward compatibility with clients
people are still running, and searching their documentation for a deprecation
or EOL statement about client versions returns nothing. Two plausible-looking
knowledge-base URLs (`/kb/1195/cli-supported-versions`,
`/kb/1523/device-supported-versions`) do not exist and resolve to unrelated
articles.

The floor is therefore ours to pick, and it is a statement about what this
server models rather than about what Tailscale supports.

**Floor: 1.78.** It is the newest release that introduced a command belonging
to the default (`core`) preset — `metrics print` and `metrics write`, added in
1.78. Everything the core preset exposes exists at or below the floor, so an
operator on a supported version never meets a missing command in the default
configuration.

Consequences, and what they are not:

- Below the floor the server still starts, and still offers every tool. It
  warns once on standard error naming the version it found and the floor. It
  does **not** hide anything: the version string is a guess about capability,
  and hiding a tool on a wrong guess is worse than letting the CLI refuse the
  command with its own message.
- Commands newer than the floor carry an explicit `min_version` on their
  metadata row, from the dating in §7: Tailscale Services, `whoami`, `wait`,
  `get`, `appc-routes` and the peer-relay flags at 1.9x–1.102; `dns status`,
  `dns query`, `syspolicy list` and `syspolicy reload` at 1.72.
- Two changes predate the floor and so need no gate: the `--posture-checking`
  → `--report-posture` rename (1.58, old name kept as a hidden alias) and the
  serve/funnel v2 syntax (1.50), which this server writes exclusively.
- An unstable build (odd minor) is newer than the stable release with the next
  even minor, so it is never warned about.
