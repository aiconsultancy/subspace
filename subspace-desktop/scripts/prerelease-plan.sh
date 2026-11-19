#!/usr/bin/env bash
# Plan a pre-release without mutating releases, tags or source files.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
validate_prerelease_run
sha="${GITHUB_SHA:?}"
[[ "$sha" =~ ^[0-9a-f]{40}$ ]] || { echo 'invalid source SHA' >&2; exit 2; }
[[ "$(git rev-parse HEAD)" = "$sha" ]] || { echo 'checkout does not match source SHA' >&2; exit 2; }
created_at="$(gh api "repos/${GITHUB_REPOSITORY:?}/actions/runs/${GITHUB_RUN_ID:?}" --jq .created_at)"
id="$(prerelease_id "$created_at")"
releases="$(release_list)"
printf '| Type | Previous tag | Previous commit | Decision |\n| --- | --- | --- | --- |\n' >>"${GITHUB_STEP_SUMMARY:?}"
# A stable release PR can be merged while its draft is still building.
# Use the last published stable, not that in-flight manifest bump.
stable="$(published_stable <<<"$releases")"
base="$(next_patch "$stable")"
version="$(prerelease_version "$base" "$RELEASE_TYPE" "$id" "$sha")"
channel_releases="$(same_type_releases <<<"$releases" | jq '[.[] | select(.draft == false)]')"
previous="$(jq -r 'sort_by([(.parsed.major|tonumber),(.parsed.minor|tonumber),(.parsed.patch|tonumber),(.parsed.id|tonumber)]) | last | .tag_name // empty' <<<"$channel_releases")"
previous_commit=''; build=true; reason='no previous release'
if [[ -n "$previous" ]]; then
    previous_commit="$(release_commit "$previous")"
    reason='relevant changes'
    if [[ "${RELEASE_FORCE:-false}" = true ]]; then
        reason='forced manual build'
    elif ! git merge-base --is-ancestor "$previous_commit" "$sha"; then
        reason='previous release is not an ancestor'
    elif git -C "$project_directory" diff --quiet "$previous_commit" "$sha" -- "${release_paths[@]}"; then
        build=false; reason='no changes to the project'
    fi
fi
newest_id="$(jq -r '[.[].parsed.id | tonumber] | max // 0' <<<"$channel_releases")"
if [[ "$build" = true && "$id" -le "$newest_id" ]]; then
    echo "run timestamp $id is not newer than $RELEASE_TYPE $newest_id; refusing an old or colliding run" >&2
    exit 1
fi
{
    echo "build=$build"
    echo "version=$version"
    echo "previous=${previous:-$tag_prefix$stable}"
} >>"${GITHUB_OUTPUT:?}"
printf '| %s | %s | %s | %s |\n' "$RELEASE_TYPE" "${previous:-none}" "${previous_commit:-none}" "$reason (build=$build)" >>"${GITHUB_STEP_SUMMARY:?}"
