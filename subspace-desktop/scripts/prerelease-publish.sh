#!/usr/bin/env bash
# Usage: prerelease-publish.sh VERSION SHA NOTES DIST
# All builds and local asset verification must have succeeded before this step.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
validate_prerelease_run
version="${1:?}"; sha="${2:?}"; notes="${3:?}"; dist="${4:?}"
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-${RELEASE_TYPE}\.[1-9][0-9]{11}\.g[0-9a-f]{7}$ ]] || exit 2
[[ "$sha" =~ ^[0-9a-f]{40}$ && "$version" = *".g${sha:0:7}" ]] || exit 2
tag="$tag_prefix$version"
existing="$(release_list | jq -c --arg tag "$tag" '[.[] | select(.tag_name == $tag)] | first // null')"
if [[ "$existing" != null ]]; then
    jq -e --arg marker "<!-- subspace-prerelease-run:${GITHUB_RUN_ID:?} -->" --arg sha "$sha" \
        '.draft == true and .prerelease == true and .target_commitish == $sha and (.body | contains($marker))' <<<"$existing" >/dev/null || {
        echo "refusing to change an existing release: $tag" >&2; exit 1;
    }
else
    # A bare tag is also a collision, even when it has no GitHub release.
    refs="$(gh api --paginate "repos/$GITHUB_REPOSITORY/git/matching-refs/tags/$tag" --jq '.[].ref')"
    if grep -Fxq "refs/tags/$tag" <<<"$refs"; then
        echo "tag already exists: $tag" >&2; exit 1
    fi
    case "$RELEASE_TYPE" in nightly) title=Nightly ;; alpha) title=Alpha ;; beta) title=Beta ;; esac
    gh release create "$tag" --repo "$GITHUB_REPOSITORY" --target "$sha" \
        --draft --prerelease --latest=false --title "$release_name ($title) $version" --notes-file "$notes"
fi
gh release upload "$tag" --repo "$GITHUB_REPOSITORY" --clobber \
    "$dist/"*.tar.gz "$dist/"*.tar.gz.sha256 "$dist/"*.zip "$dist/"*.zip.sha256 \
    "$(dirname "$0")/install-release.sh" "$(dirname "$0")/install-release.ps1"
gh release edit "$tag" --repo "$GITHUB_REPOSITORY" --draft=false --prerelease --latest=false
