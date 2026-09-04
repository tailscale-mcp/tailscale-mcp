#!/usr/bin/env bash
# Render the tap formula for a release.
#
# Fills `packaging/homebrew/tailscale-mcp.rb.in` in from the SHA256SUMS the
# release published, and writes the formula the tap should carry. The template
# names four archives — macOS and Linux, on each architecture — and every one
# of them has to be in the sums file: a formula with a marker left in it is a
# formula that fails at somebody's `brew install`, which is too late to find
# out.
#
# The tap lives in another repository (`tailscale-mcp/homebrew-tap`), so this
# writes a file rather than pushing anything. The release attaches the result,
# and updating the tap is committing it there.
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: scripts/update-formula.sh <version> <SHA256SUMS> [output]

  <version>     the released version, without a leading v
  <SHA256SUMS>  the sums file the release published
  [output]      where to write the formula, or - for standard output;
                defaults to tailscale-mcp.rb in the working directory

To render from a published release:

  curl -sSLO https://github.com/tailscale-mcp/tailscale-mcp/releases/download/v1.0.0/SHA256SUMS
  scripts/update-formula.sh 1.0.0 SHA256SUMS
USAGE
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    usage >&2
    exit 2
fi

version=$1
sums=$2
output=${3:-tailscale-mcp.rb}

if ! printf '%s' "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
    echo "not a version: '$version'" >&2
    exit 1
fi

if [ ! -f "$sums" ]; then
    echo "no sums file at '$sums'" >&2
    exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
template="$root/packaging/homebrew/tailscale-mcp.rb.in"
base="https://github.com/tailscale-mcp/tailscale-mcp/releases/download/v$version"

# The formula installs a pre-built binary, so it covers the platforms Homebrew
# runs on: Windows is not one of them, and the bundles cover that instead.
targets="aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu"

rendered=$(sed -e "s|@VERSION@|$version|g" -e "s|@BASE@|$base|g" "$template")

for target in $targets; do
    archive="tailscale-mcp-$version-$target.tar.gz"
    # `sha256sum` writes the digest, then a space and an optional `*`, then the
    # name; anything else in the file is another archive.
    digest=$(awk -v want="$archive" '{ name = $2; sub(/^\*/, "", name); if (name == want) print $1 }' "$sums")
    if [ -z "$digest" ]; then
        echo "'$sums' has no checksum for $archive" >&2
        exit 1
    fi
    if [ "$(printf '%s\n' "$digest" | wc -l)" -ne 1 ]; then
        echo "'$sums' has more than one checksum for $archive" >&2
        exit 1
    fi
    if ! printf '%s' "$digest" | grep -qE '^[0-9a-f]{64}$'; then
        echo "'$digest' is not a sha256 of $archive" >&2
        exit 1
    fi
    marker=$(printf '%s' "$target" | tr 'a-z-' 'A-Z_')
    rendered=${rendered//"@SHA256_$marker@"/$digest}
done

if printf '%s' "$rendered" | grep -q '@[A-Z0-9_]*@'; then
    echo "the formula still has a marker in it:" >&2
    printf '%s\n' "$rendered" | grep -n '@[A-Z0-9_]*@' >&2
    exit 1
fi

if [ "$output" = - ]; then
    printf '%s\n' "$rendered"
else
    printf '%s\n' "$rendered" > "$output"
    echo "wrote $output"
fi
