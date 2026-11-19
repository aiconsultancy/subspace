#!/usr/bin/env bash
# Usage: prerelease-retention.sh CURRENT_TAG
# Only exact Subspace pre-release names of this type are eligible, including drafts.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
validate_prerelease_run
current="${1:?}"
releases="$(release_list | same_type_releases)"
tags="$(jq -r --arg current "$current" '
  ([.[] | select(.draft == false)] | sort_by(.parsed.id|tonumber) | reverse | .[3:]) +
  [.[] | select(.draft == true and .tag_name != $current)] |
  .[] | select(.tag_name != $current) | [.tag_name, .draft] | @tsv' <<<"$releases")"
while IFS=$'\t' read -r tag draft; do
    [[ -n "$tag" ]] || continue
    args=("$tag" --repo "$GITHUB_REPOSITORY" --yes)
    # Draft creation does not create a tag. gh unconditionally deletes the ref
    # with --cleanup-tag, which fails for a draft's absent ref.
    if [[ "$draft" = false ]]; then args+=(--cleanup-tag); fi
    gh release delete "${args[@]}"
done <<<"$tags"
