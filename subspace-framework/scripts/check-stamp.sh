#!/usr/bin/env bash
# Records that a check passed for HEAD, for scripts/publish.sh. Run by the
# make targets after the check passes; a dirty tree gets no stamp, because
# the check did not run on a commit.
set -euo pipefail
name="${1:?usage: check-stamp.sh <check-name>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
if [[ -n "$(git status --porcelain -- .)" ]]; then
    echo "$name passed on an uncommitted tree; no stamp for publishing"
    exit 0
fi
sha="$(git rev-parse HEAD)"
mkdir -p .local/checks
touch ".local/checks/$name-$sha"
echo "$name passed for $sha"
