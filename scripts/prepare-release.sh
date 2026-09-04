#!/usr/bin/env bash
# Prepare a release.
#
# Works out the next version from the commit messages since the last tag,
# writes it into the workspace manifest, and regenerates the changelog. It does
# not commit, tag or publish: the diff is meant to be read before any of that
# happens, and pushing the tag is what starts `.github/workflows/release.yml`.
#
# The version comes from the commits and not from an argument, so that the rule
# is the one written in `cliff.toml` — a breaking change is a major release, a
# feature a minor one, anything else a patch — rather than whatever the person
# running this remembered. `--version` is there for the case where that rule is
# wrong, and saying so out loud is the point of having to pass it.
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: scripts/prepare-release.sh [--dry-run] [--version <x.y.z>]

  --dry-run, -n    say what it would do, change nothing
  --version        use this version instead of the one the commits imply
  --help, -h       this

Run it, read the diff, commit it, then tag.
USAGE
}

dry_run=false
wanted=""
while [ $# -gt 0 ]; do
    case "$1" in
        -n | --dry-run) dry_run=true ;;
        --version)
            shift
            wanted="${1:?--version needs a version}"
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

cd "$(git rev-parse --show-toplevel)"

if ! command -v git-cliff > /dev/null; then
    echo "git-cliff is not installed: cargo install git-cliff --locked" >&2
    exit 1
fi

# The same reading of the same line as `.github/workflows/release.yml` and
# `tests/release_is_one_version.rs`: `[workspace.package]`'s is the first
# `version = ` in the file.
current=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
next="${wanted:-$(git-cliff --bumped-version | tail -1)}"
next="${next#v}"

# Everything below writes `$next` into a manifest, a changelog and a tag name.
# An unchecked value there is a broken release at best: perl would take `1.0@x`
# as an array interpolation and write `1.0`, and a quote would produce a
# manifest that does not parse.
if ! printf '%s' "$next" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
    echo "not a version this can write: '$next'" >&2
    exit 1
fi

echo "current: $current"
echo "next:    $next"

scratch=$(mktemp -d)
editing=false
finished=false
cleanup() {
    # A failure part-way through would otherwise leave a manifest naming one
    # version, a lockfile naming another and a changelog naming neither.
    if $editing && ! $finished; then
        echo "putting back what was already changed" >&2
        cp "$scratch/Cargo.toml" "$scratch/Cargo.lock" "$scratch/CHANGELOG.md" .
    fi
    rm -rf "$scratch"
}
trap cleanup EXIT

changelog="$scratch/CHANGELOG.next.md"
git-cliff --tag "v$next" --output "$changelog"

if $dry_run; then
    echo
    if [ "$current" != "$next" ]; then
        echo "would set the workspace version and both internal dependencies to $next"
    else
        echo "the version is already $next; only the changelog would change"
    fi
    echo "would write CHANGELOG.md:"
    diff -u CHANGELOG.md "$changelog" || true
    echo
    echo "would publish, in this order:"
    cargo publish --workspace --dry-run --locked --allow-dirty 2>&1 |
        sed -n 's/^ *Uploading /  /p'
    finished=true
    exit 0
fi

cp Cargo.toml Cargo.lock CHANGELOG.md "$scratch/"
editing=true

if [ "$current" != "$next" ]; then
    # Three places carry the version: the workspace package, and the two
    # internal dependencies, which name a version as well as a path so that
    # the published crates depend on each other by version.
    perl -pi -e "s/^version = \"\Q$current\E\"/version = \"$next\"/" Cargo.toml
    perl -pi -e "s/^(tailscale-(?:rest|cli) = \{ version = )\"\Q$current\E\"/\$1\"$next\"/" Cargo.toml
    cargo update --workspace --quiet
fi

cp "$changelog" CHANGELOG.md
finished=true

echo
echo "written. Read the diff, commit it, then tag:"
echo "    git tag -a -m \"release $next\" v$next && git push origin v$next"
