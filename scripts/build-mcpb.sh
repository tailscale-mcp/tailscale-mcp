#!/usr/bin/env bash
# Assemble an MCP bundle (`.mcpb`) from a release archive.
#
# A bundle is a zip holding `manifest.json` and the server it describes, which
# a client such as Claude Desktop installs by double-click. The manifest is
# checked in at `packaging/mcpb/manifest.json` and the binary comes out of the
# release archive the build job produced, so this joins the two rather than
# building anything: one bundle per platform, each carrying the binary for it.
#
# The one edit made to the manifest is narrowing `compatibility.platforms` to
# the platform the binary is for. The checked-in manifest lists all three
# because it describes the server rather than any one bundle; a bundle that
# claimed all three would install on a machine it cannot run on.
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: scripts/build-mcpb.sh <target> <archive> <output.mcpb>

  <target>       the Rust target the archive was built for, such as
                 aarch64-apple-darwin
  <archive>      the release archive holding the binary, as .tar.gz
  <output.mcpb>  where to write the bundle
USAGE
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac

if [ "$#" -ne 3 ]; then
    usage >&2
    exit 2
fi

target=$1
archive=$2
output=$3

# Rust names a target by machine and system; a bundle names a platform the way
# Node does, which is what the manifest schema accepts.
case "$target" in
    *-apple-darwin) platform=darwin ;;
    *-pc-windows-msvc) platform=win32 ;;
    *-unknown-linux-*) platform=linux ;;
    *)
        echo "no bundle platform for target '$target'" >&2
        exit 1
        ;;
esac

# The entry point is written without `.exe`: a host appends it on Windows, so
# the manifest is the same everywhere and only the file in the zip differs.
binary=tailscale-mcp
if [ "$platform" = win32 ]; then
    binary=tailscale-mcp.exe
fi

if [ ! -f "$archive" ]; then
    echo "no archive at '$archive'" >&2
    exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
manifest="$root/packaging/mcpb/manifest.json"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

tar xzf "$archive" -C "$staging"

# The archive holds one directory named for the version and target; the bundle
# does not, so its contents are lifted out.
unpacked=$(find "$staging" -mindepth 1 -maxdepth 1 -type d)
if [ -z "$unpacked" ] || [ "$(printf '%s\n' "$unpacked" | wc -l)" -ne 1 ]; then
    echo "expected one directory in '$archive', found:" >&2
    printf '%s\n' "${unpacked:-nothing}" >&2
    exit 1
fi

if [ ! -f "$unpacked/$binary" ]; then
    echo "no '$binary' in '$archive'" >&2
    exit 1
fi

bundle="$staging/bundle"
mkdir -p "$bundle/server"
cp "$unpacked/$binary" "$bundle/server/$binary"
chmod +x "$bundle/server/$binary"
for extra in README.md LICENSE; do
    if [ -f "$unpacked/$extra" ]; then
        cp "$unpacked/$extra" "$bundle/$extra"
    fi
done

MANIFEST=$manifest BUNDLE=$bundle PLATFORM=$platform OUTPUT=$output python3 - <<'PY'
import json
import os
import zipfile
from pathlib import Path

bundle = Path(os.environ["BUNDLE"])
output = Path(os.environ["OUTPUT"])

manifest = json.loads(Path(os.environ["MANIFEST"]).read_text())
manifest.setdefault("compatibility", {})["platforms"] = [os.environ["PLATFORM"]]
(bundle / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

output.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as zf:
    for path in sorted(bundle.rglob("*")):
        if path.is_file():
            # `write` carries the mode across, which is what keeps the server
            # executable once a host unpacks the bundle.
            zf.write(path, path.relative_to(bundle).as_posix())
PY

echo "wrote $output"
