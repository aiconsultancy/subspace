#!/usr/bin/env bash
# Usage: prerelease-notes.sh PREVIOUS_TAG SOURCE_SHA
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
validate_prerelease_run
previous="${1:?}"; sha="${2:?}"
echo "This is a $RELEASE_TYPE pre-release of $release_name."
echo
echo "Commits touching the project since $previous:"
echo
git -C "$project_directory" log --format='- %h %s' "$(release_commit "$previous")..$sha" -- "${release_paths[@]}"
echo
echo "<!-- subspace-prerelease-run:${GITHUB_RUN_ID:?} -->"
