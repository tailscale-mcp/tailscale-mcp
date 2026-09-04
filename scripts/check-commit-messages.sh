#!/usr/bin/env bash
# Every commit message added since the convention was adopted is in the
# conventional form.
#
# The next version number and the changelog are both read out of these subjects
# (`cliff.toml`), so one that does not say what kind of change it is becomes a
# patch release with an entry nobody can act on — quietly, because the catch-all
# parser that keeps the pre-1.0.0 history in the changelog will happily absorb
# it. This is what stops that (DECISIONS Q110).
#
# The history before the convention is not checked, because it cannot pass:
# BASELINE is the last commit written the old way, and everything after it is
# held to the new one.
set -euo pipefail

# `Continuous integration, and the Linux bug it found (ticket 27)`.
BASELINE=993c15a74280e8cfa0e6b50378239c41cc09c2ae

# `type(scope)!: subject`, with the types `cliff.toml` gives a group to. A
# merge commit's subject is git's, not an author's, so merges are left out.
PATTERN='(feat|fix|perf|refactor|docs|test|ci|build|chore|revert)(\([a-z0-9._/-]+\))?!?: .+'

cd "$(git rev-parse --show-toplevel)"

if ! git cat-file -e "$BASELINE^{commit}" 2> /dev/null; then
    echo "the baseline commit $BASELINE is not in this clone; fetch the full history" >&2
    exit 1
fi

offences=$(git log --no-merges --format='%h %s' "$BASELINE..HEAD" |
    grep -vE "^[0-9a-f]+ $PATTERN" || true)

if [ -n "$offences" ]; then
    echo "these commit subjects are not in the conventional form:" >&2
    echo "$offences" >&2
    echo >&2
    echo "expected <type>[(scope)][!]: <subject>, where <type> is one of" >&2
    echo "feat fix perf refactor docs test ci build chore revert" >&2
    exit 1
fi

echo "every commit since $BASELINE is in the conventional form"
