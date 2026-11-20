#!/usr/bin/env bash
# Build, sign and notarize the macOS app and publish its GitHub release with
# the app archive and its .sha256 file, which install-release.sh reads.
# A stable release is the draft Release Please created when its release pull
# request merged: run this on that release's tag, and it attaches the files
# to the draft and publishes it. It never changes a published release.
# Usage: mise run release-macos -- SIGNING_IDENTITY KEYCHAIN_PROFILE
# Environment: RELEASE_REPO (default aiconsultancy/subspace), RELEASE_DRAFT=1
# to leave the release a draft, RELEASE_NOTES_FILE for notes instead of the
# changelog's (or, for a pre-release, generated ones).
# Pass --prerelease before the signing arguments to cut a branch integration
# build, which this script creates as a new pre-release.
set -euo pipefail

# Signing needs the desktop login session; from any other shell, run there.
if [[ "$(uname -s)" = Darwin && "$(launchctl managername 2>/dev/null || true)" != Aqua ]]; then
	scripts="$(cd "$(dirname "$0")" && pwd)"
	exec bash "$scripts/desktop-session.sh" bash "$scripts/$(basename "$0")" "$@"
fi

integration=false
if [[ "${1:-}" = --prerelease ]]; then integration=true; shift; fi
if [[ "$#" -ne 2 ]]; then
	echo "usage: mise run release-macos -- [--prerelease] SIGNING_IDENTITY KEYCHAIN_PROFILE" >&2
	exit 2
fi
identity="$1"
profile="$2"
repo="${RELEASE_REPO:-aiconsultancy/subspace}"
root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
package="${SUBSPACE_RELEASE_PACKAGE:-$root/scripts/package-macos.sh}"
cd "$root"
# The GitHub OAuth secret leaves the environment at once; only the packaging
# gets it back.
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/github-oauth-secret.sh
source "$root/scripts/github-oauth-secret.sh"
oauth_secret="$(github_oauth_secret)"
unset SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET

if [[ "$(uname -s)" != Darwin ]]; then
	echo "the macOS release must be built on macOS" >&2
	exit 2
fi
command -v gh >/dev/null || { echo "the GitHub CLI (gh) is required" >&2; exit 2; }
gh auth status --hostname github.com >/dev/null 2>&1 || {
	echo "gh is not signed in to github.com; run: gh auth login" >&2
	exit 2
}
if [[ -n "$(git status --porcelain -- .)" ]]; then
	echo "commit or stash local changes first; a release must match a pushed commit" >&2
	exit 1
fi
if [[ "$integration" = true ]]; then
    # The planner rejects dirty files anywhere in the repository and detached
    # HEADs, and reserves channel names before any stamping or publication.
    integration_version="$(RELEASE_REPO="$repo" bash "$root/scripts/integration-prerelease-plan.sh")"
    export SUBSPACE_RELEASE_CHANNEL=stable
fi
sha="$(git rev-parse HEAD)"
gh api "repos/$repo/commits/$sha" --silent 2>/dev/null || {
	echo "commit $sha is not on $repo; push it first" >&2
	exit 1
}

if [[ "$integration" = true ]]; then
    tag="subspace-desktop-v$integration_version"
else
    tag=''
fi
# Check collisions before temporarily changing the workspace version.
if [[ -n "$tag" ]] && gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
    echo "release $tag already exists on $repo" >&2; exit 1
fi
if [[ "$integration" = true ]]; then
    saved_versions="$(mktemp -d "${TMPDIR:-/tmp}/subspace-release-versions.XXXXXX")"
    cp Cargo.toml Cargo.lock "$saved_versions/"
    restore_versions() {
        cp "$saved_versions/Cargo.toml" Cargo.toml
        cp "$saved_versions/Cargo.lock" Cargo.lock
        rm -rf "${saved_versions:?}"
    }
    trap restore_versions EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    bash "$root/scripts/stamp-prerelease-version.sh" "$integration_version"
fi
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
load_release_toolchain
load_product
if [[ "$integration" = true && "$version" != "$integration_version" ]]; then
    echo 'integration version does not match the planned version; refusing to package or publish' >&2
    exit 2
fi
if [[ -z "$oauth_secret" && "$channel" = stable ]]; then
	github_oauth_secret_missing
	exit 1
fi
tag="subspace-desktop-v$version"
# Pre-releases are created here; stable releases come from Release Please.
create=false
if [[ "$integration" = true || "$channel" != stable ]] || validate_integration_version "$version"; then
	create=true
fi
if [[ "$create" = true ]]; then
	if gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
		echo "release $tag already exists on $repo; bump the version first" >&2
		exit 1
	fi
else
	draft="$(gh release view "$tag" --repo "$repo" --json isDraft --jq .isDraft 2>/dev/null)" || {
		echo "no release $tag on $repo: merge the app's release pull request first; Release Please creates the draft" >&2
		exit 1
	}
	if [[ "$draft" != true ]]; then
		echo "release $tag on $repo is already published; a published release is never changed" >&2
		exit 1
	fi
	tagged="$(gh api "repos/$repo/commits/$tag" --jq .sha 2>/dev/null)" || {
		echo "the draft $tag has no tag on $repo yet" >&2
		exit 1
	}
	if [[ "$tagged" != "$sha" ]]; then
		echo "HEAD is $sha but $tag is $tagged; check out $tag first" >&2
		exit 1
	fi
fi

# Only the packaging sees the secret.
SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET="$oauth_secret" bash "$package" "$identity" "$profile" aarch64-apple-darwin
unset oauth_secret
archive="${asset_stem}_${version}_darwin_arm64.zip"
test -f "dist/$archive" || { echo "missing archive: dist/$archive" >&2; exit 1; }
(cd dist && shasum -a 256 "$archive" >"$archive.sha256")

if [[ "$create" = true ]]; then
	release_args=(release create "$tag" "dist/$archive" "dist/$archive.sha256"
		--repo "$repo" --target "$sha" --title "$display_name $version")
	if [[ -n "${RELEASE_NOTES_FILE:-}" ]]; then
		release_args+=(--notes-file "$RELEASE_NOTES_FILE")
	else
		release_args+=(--generate-notes)
	fi
	release_args+=(--prerelease --latest=false)
	if [[ "${RELEASE_DRAFT:-}" = 1 ]]; then release_args+=(--draft); fi
	gh "${release_args[@]}"
	exit 0
fi

# Still a draft, so replacing files from an earlier DRAFT=1 run is safe. The
# files go up before publishing: publishing starts the packslip workflow,
# which signs what the release holds.
gh release upload "$tag" "dist/$archive" "dist/$archive.sha256" --repo "$repo" --clobber
edit_args=(release edit "$tag" --repo "$repo" --title "$display_name $version")
if [[ -n "${RELEASE_NOTES_FILE:-}" ]]; then edit_args+=(--notes-file "$RELEASE_NOTES_FILE"); fi
if [[ "${RELEASE_DRAFT:-}" != 1 ]]; then edit_args+=(--draft=false --latest); fi
gh "${edit_args[@]}"
