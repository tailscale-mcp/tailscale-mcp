#!/usr/bin/env bash
# Fetch the MCP registry's publisher into the working directory as
# `./mcp-publisher`.
#
# Two jobs need it — one to check the listing before anything is created, one
# to publish it afterwards — and the version and the digest are the sort of
# thing that gets updated in one place and forgotten in the other. So they live
# here, once.
#
# Pinned by version and by digest rather than fetched from the
# `releases/latest/download` URL the registry's own instructions use (Q100): a
# binary that publishes on our behalf is the last one to take from a moving
# target.
set -euo pipefail

version="1.8.1"
sha256="a06c9096dcb9727c13555b6be26c7effa707b01f06a4c561ba7a3635443cf2cc"
archive="mcp-publisher_linux_amd64.tar.gz"

curl -fsSL -o "$archive" \
  "https://github.com/modelcontextprotocol/registry/releases/download/v$version/$archive"
echo "$sha256  $archive" | sha256sum -c -
tar xzf "$archive" mcp-publisher
rm -f "$archive"
