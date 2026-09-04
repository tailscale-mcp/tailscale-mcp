#!/usr/bin/env bash
# Check that the container image runs the server and honours its environment.
#
# The image is the one distribution nothing else exercises: the test suite runs
# the binary directly, and a Dockerfile that builds is not a Dockerfile that
# runs — a wrong entrypoint, a missing user, or a base image with nothing to
# execute all build clean. So this starts the image the way a client does, with
# no arguments and stdio for a transport, and asks it something.
#
# The credential below is not one: it is the shape of an API access token, so
# that the tailnet surface is offered and the server takes the path a
# configured client takes. Nothing is sent anywhere — `initialize` reaches no
# control plane — and this is why the check lives here rather than in the
# workflow, where a variable named for a credential would trip the guard in
# `tests/ci_needs_no_credential.rs`.
set -euo pipefail

image=${1:-tailscale-mcp:check}
fake_key=tskey-api-example-redacted

if ! docker image inspect "$image" > /dev/null 2>&1; then
    echo "no image '$image'; build it with: docker build -t $image ." >&2
    exit 1
fi

fail() {
    echo "FAIL: $1" >&2
    exit 1
}

# 1. It runs as somebody other than root.
user=$(docker inspect --format '{{.Config.User}}' "$image")
case "$user" in
    "" | root | 0) fail "the image runs as root ('$user')" ;;
    *) echo "ok: runs as $user" ;;
esac

# 2. With no arguments it speaks MCP over stdio. Closing stdin after the frame
#    is what ends the run: the server serves until its transport is gone.
frame='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"check","version":"0"}}}'
answer=$(printf '%s\n' "$frame" | docker run --rm -i -e TAILSCALE_API_KEY="$fake_key" "$image" 2> /dev/null)
case "$answer" in
    *'"serverInfo"'*'"tailscale-mcp"'*) echo "ok: answers initialize over stdio" ;;
    *) fail "initialize was not answered: $answer" ;;
esac

# 3. The environment reaches it: a preset it cannot honour is refused, and one
#    it can is used. A client configures this image only through variables, so
#    an image that ignored them would offer the wrong tools with no sign of it.
if docker run --rm -e TAILSCALE_MCP_PRESET=nonsense "$image" tools > /dev/null 2>&1; then
    fail "a preset that does not exist was accepted"
fi
echo "ok: refuses a preset that does not exist"

offered=$(docker run --rm -e TAILSCALE_MCP_PRESET=minimal -e TAILSCALE_API_KEY="$fake_key" "$image" tools 2> /dev/null | head -1)
case "$offered" in
    "preset minimal"*) echo "ok: $offered" ;;
    *) fail "the preset was ignored: $offered" ;;
esac

full=$(docker run --rm -e TAILSCALE_MCP_PRESET=full -e TAILSCALE_MCP_ALLOW_DESTRUCTIVE=true -e TAILSCALE_API_KEY="$fake_key" "$image" tools 2> /dev/null | head -1)
case "$full" in
    "preset full, tier destructive"*) echo "ok: $full" ;;
    *) fail "the tier was ignored: $full" ;;
esac

echo "the image runs the server and honours its environment"
