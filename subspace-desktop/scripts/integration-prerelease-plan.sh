#!/usr/bin/env bash
# Print a deterministic branch pre-release version without changing the checkout.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
branch="$(git symbolic-ref --quiet --short HEAD)" || {
    echo 'integration pre-releases require an attached branch' >&2; exit 2;
}
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
    echo 'integration pre-releases require a clean repository' >&2; exit 2
fi
slug="$(LC_ALL=C printf '%s' "$branch" | LC_ALL=C tr '[:upper:]' '[:lower:]' | LC_ALL=C sed 's/[^0-9a-z-]/-/g')"
validate_integration_slug "$slug" || {
    echo "branch $branch produces a reserved or invalid integration slug: $slug" >&2; exit 2;
}
sha="$(git rev-parse HEAD)"
[[ "$sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'invalid source SHA' >&2; exit 2; }
id="$(prerelease_id "$(git -c log.showSignature=false show -s --format=%cI "$sha")")"
export GITHUB_REPOSITORY="${RELEASE_REPO:-${GITHUB_REPOSITORY:-aiconsultancy/subspace}}"
releases="$(release_list)"
stable="$(published_stable <<<"$releases")" || {
    echo 'no published stable Subspace release found; cannot compute the integration base' >&2; exit 2;
}
base="$(integration_base "$stable" "$sha")"
version="$(prerelease_version "$base" "$slug" "$id" "$sha")"
validate_prerelease_version "$version"
printf '%s\n' "$version"
