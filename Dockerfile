# The container image.
#
# Two stages. The first builds a static binary against musl, so the second can
# be a base with no shell, no package manager and no libc of its own: the
# server reaches the control plane over TLS with the root certificates compiled
# into it, and needs nothing from the operating system but a temporary
# directory.
#
# The `tailscale` command-line interface is deliberately absent. A container is
# not a Tailscale node, so the local tools have nothing to drive; the server
# detects that at startup, offers the tailnet tools alone, and says so. Mount a
# `tailscale` binary and its socket in if you want the other half.

FROM rust:1-alpine AS build

# `ring` compiles C, which on Alpine means musl's headers have to be here.
RUN apk add --no-cache musl-dev

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Alpine's Rust targets musl and links it statically, so this comes out as one
# file that depends on nothing. `--locked` so the image is built from the
# dependency tree the repository committed.
RUN cargo build --release --locked -p tailscale-mcp

# `static` rather than `base`: nothing dynamic to link against, and nothing to
# exec if something ever gets in. `nonroot` runs as 65532.
FROM gcr.io/distroless/static-debian12:nonroot

LABEL org.opencontainers.image.title="tailscale-mcp"
LABEL org.opencontainers.image.description="MCP server for Tailscale: the local node through the CLI, the tailnet through the control-plane API"
LABEL org.opencontainers.image.source="https://github.com/tailscale-mcp/tailscale-mcp"
LABEL org.opencontainers.image.licenses="Apache-2.0"
# How the MCP registry knows this image is the one `server.json` offers: it
# pulls the image and looks for its own name here. The npm package carries the
# same claim in its `mcpName`, and `registry_listing_is_valid.rs` holds all
# three to the same string.
LABEL io.modelcontextprotocol.server.name="io.github.tailscale-mcp/tailscale-mcp"

COPY --from=build /src/target/release/tailscale-mcp /usr/local/bin/tailscale-mcp
# Apache-2.0 asks that a copy of the licence go with the work wherever it is
# distributed, and an image is a distribution. Every other channel carries it
# too: the archives, the bundles and the npm package.
COPY LICENSE /LICENSE

# No arguments: every setting has an environment variable, which is what a
# container runtime and an MCP client configuration both have to hand. The
# server speaks MCP over the standard streams unless `--http` says otherwise.
ENTRYPOINT ["/usr/local/bin/tailscale-mcp"]
