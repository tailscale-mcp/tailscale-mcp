---
status: accepted
---

# Tool parameters use snake_case for what is ours and Tailscale's own shape for what is theirs

A tool's parameters mix two vocabularies: identifiers and options the server invents (`device_id`, `timeout_seconds`, `confirm`) and request bodies whose shape belongs to Tailscale's API (a key's capabilities, a service definition, a policy file). MCP servers name their own parameters in snake_case, while Tailscale's bodies are camelCase and documented that way. We keep both as they are: our parameters are snake_case, and any body or nested object that is Tailscale's is accepted in Tailscale's shape, unrenamed, and returned verbatim. Converting case in either direction would break copy-and-paste from Tailscale's documentation and would make results inconsistent with the inputs that produced them.

## Considered options
- **snake_case everywhere, converting bodies on the way in and out.** A rename table to maintain for every schema, and pasted examples stop working.
- **camelCase everywhere.** Alien among MCP servers, and the local surface's flags are already another convention.
- **Mixed, by ownership.** Chosen.

## Consequences
- A parameter description says when an object is "in Tailscale's shape", and points at Tailscale's documentation for it.
- A reader will see `device_id` beside `keyExpiryDisabled` in one schema; this record is why.
- Renaming a parameter later is a breaking change for every saved prompt and client configuration, so the convention is fixed at the first release.
