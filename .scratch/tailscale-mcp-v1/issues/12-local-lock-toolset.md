# 12 — Tailnet lock toolset

Status: done
Milestone: 2 — Local surface
Blocked by: 09

The 8 tailnet lock tools: status, log with a bounded limit, signing, and the key and node operations. Initialisation, disabling and key revocation change the tailnet's trust root, are irreversible for everyone, and require a confirmation in addition to the destructive tier.

## Acceptance criteria
- Status and log are read tier; the log honours its default and cap.
- Initialise, disable and revoke keys refuse without a confirmation even when the destructive tier is enabled.
- Signing a node succeeds against the fake backend and reports the resulting state.

## As built

Eight tools in `local_lock.rs`, none of them status or log: `tailscale_lock_status`
and `tailscale_lock_log` were built in ticket 08 and stay in `local-status`,
where willingness to read lock state is the same willingness that reads the peer
list. The ticket's "8" is reachable exactly from the remaining subcommands —
init, add, remove, sign, disable, disablement-kdf, local-disable, revoke-keys —
so `spec.md`'s totals (186 tools, 62 typed local) are unchanged. The first
acceptance criterion was a regression check rather than new work; the log's cap
was already tested and its default now is too.

Three tools confirm as well as being destructive, which is the set `spec.md`
names; `remove` and `local-disable` are destructive without a confirmation
because they are node-scale rather than tailnet-scale (DECISIONS Q40).
`disablement-kdf` is read tier and idempotent: it is local arithmetic that
contacts nothing.

Four things the client's real behaviour decided rather than the ticket:

- `lock init` without `--confirm` exits 0 having done nothing, so the flag is
  always passed (DECISIONS Q39). Probed against a tailnet with lock off, which
  stayed off.
- `lock disable` and `lock disablement-kdf` have no `file:` form, so this module
  honours one itself — keeping the secret out of the conversation, not off the
  argument list (DECISIONS Q38).
- A signed auth key is the product of `lock sign` and would be removed entirely
  by the shape-based redaction, so it comes back whole in its own field, once
  (DECISIONS Q41).
- `revoke-keys` has two usage forms behind one positional, split here into
  `keys` and `recovery_blob` with the invalid combinations refused (DECISIONS
  Q42).

`real_path`, the secret-file helper and the printed-token scan moved into
`tools/common.rs` as `real_path`, `secret_value` and `tokens_with_prefix`,
because this toolset needed all three and none should have two spellings;
`find_url` is now defined over the last of them rather than beside it.
`CONTEXT.md` gained the five tailnet-lock terms the module's descriptions and
reports lean on, including the disablement secret and disablement value that the
two commands disagree about.

`/code-review` then found, and this ticket fixed:

- `gen_disablements` and `gen_disablement_for_support` were shortened to
  `disablements` and `support_disablement`, against `spec.md`'s rule that a CLI
  flag becomes a parameter of the same name in snake_case — a 1.0.0
  compatibility promise. Restored to the flags' own names.
- A `MAX_DISABLEMENTS = 10` ceiling the client, ticket and spec all lack.
  Removed; how many places a caller can store a secret is not this server's to
  guess. The `gen_disablements == 0` refusal stays: the client has that rule
  itself, and applying it here costs a message rather than a spawn.
- "coordination server" in the module header and in a caller-facing outcome
  string, which `CONTEXT.md` lists under _Avoid_ for **Control plane** — in the
  same change that added the glossary entry.
- `DisablementReport.disablement`, unqualified where the glossary has two
  halves that are not interchangeable. Now `disablement_value`, with the two
  prefix constants spelled out to match.
- Report structs were private where every sibling module makes them `pub`; and
  `LockReport` was being built with `keys: Vec::new()` by the two tools that can
  never name a key, advertising a field they cannot fill. Those two now answer
  with a `StateReport`.
- `secret_from` sat one import line from `common::secret_value` doing the
  opposite thing. Renamed `disablement_secret`, which is what it reads, and its
  unreadable-file error now distinguishes permission-denied from missing rather
  than calling everything `not_found`.

The review also read `spec.md`'s "returned once" as forbidding `lock init` from
keeping the printed text its secrets were parsed out of. Kept, and DECISIONS Q43
records why: a missed auth key costs a second `lock sign`, while a missed
disablement secret cannot be recovered by any call at all.
