#!/usr/bin/env bash
# Allow signing only for the matching Release Please PR's merge push to main.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
	echo 'usage: release-signing-allowed.sh RELEASE_PLEASE_CONFIG' >&2
	exit 2
fi

if [[ "${GITHUB_EVENT_NAME:-}" != push || "${GITHUB_REF:-}" != refs/heads/main ]]; then
	echo false
	exit 0
fi

component="$(jq -er '.packages["subspace-desktop"].component' "$1")"
pending_label="$(jq -er '.label' "$1")"
tagged_label="$(jq -er '."release-label"' "$1")"

gh api "repos/${GITHUB_REPOSITORY:?}/commits/${GITHUB_SHA:?}/pulls" |
	jq -r --arg repository "$GITHUB_REPOSITORY" --arg sha "$GITHUB_SHA" \
		--arg branch "release-please--branches--main--components--$component" \
		--arg pending "$pending_label" --arg tagged "$tagged_label" '
		any(.[];
			.merged_at != null and
			.merge_commit_sha == $sha and
			.base.ref == "main" and
			.base.repo.full_name == $repository and
			.head.repo.full_name == $repository and
			.head.ref == $branch and
			.user.type == "Bot" and
			any(.labels[]; .name == $pending or .name == $tagged)
		)'
