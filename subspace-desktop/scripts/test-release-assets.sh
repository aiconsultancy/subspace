#!/usr/bin/env bash
# Validate release asset sets using arbitrary fixture bytes and real checksums.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-assets.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

checksum() {
	local file="$1"
	if command -v sha256sum >/dev/null 2>&1; then
		(
			cd "$(dirname "$file")"
			sha256sum "$(basename "$file")" >"$(basename "$file").sha256"
		)
	else
		(
			cd "$(dirname "$file")"
			shasum -a 256 "$(basename "$file")" >"$(basename "$file").sha256"
		)
	fi
}

make_assets() {
	local version="$1" directory="$2" prefix=subspace app_name=Subspace binary=subspace app bundle_name
	bundle_name="$app_name"
	if [[ "$version" = *-nightly.* ]]; then
		bundle_name="$app_name (Nightly)"
		app_name="$app_name-Nightly"
	fi
	mkdir -p "$directory"
	for suffix in \
		x86_64-unknown-linux-gnu.tar.gz \
		aarch64-apple-darwin.tar.gz \
		x86_64-pc-windows-msvc.zip; do
		printf 'fixture %s\n' "$suffix" >"$directory/$prefix-$version-$suffix"
		checksum "$directory/$prefix-$version-$suffix"
	done
	app="$directory/app/$bundle_name.app/Contents/MacOS"
	mkdir -p "$app"
	printf 'app fixture\n' >"$app/$binary"
	(
		cd "$directory/app"
		zip -qr "$directory/${app_name}_${version}_darwin_arm64.zip" "$bundle_name.app"
	)
	checksum "$directory/${app_name}_${version}_darwin_arm64.zip"
	rm -rf "$directory/app"
}

expect_failure() {
	local message="$1"
	shift
	if "$@" >"$fixture/output" 2>&1; then
		echo "expected failure: $message" >&2
		exit 1
	fi
}

make_assets 0.1.0 "$fixture/subspace"
bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/subspace"

version=1.10.2-nightly.202611010200.g0123456
make_assets "$version" "$fixture/nightly"
bash "$root/scripts/verify-release-assets.sh" "$version" "$fixture/nightly"

cp -R "$fixture/subspace" "$fixture/missing-target"
rm "$fixture/missing-target/subspace-0.1.0-aarch64-apple-darwin.tar.gz"
expect_failure 'missing target archive' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/missing-target"

cp -R "$fixture/subspace" "$fixture/missing-app"
rm "$fixture/missing-app/Subspace_0.1.0_darwin_arm64.zip"*
expect_failure 'missing macOS app archive' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/missing-app"

cp -R "$fixture/subspace" "$fixture/invalid-app"
rm "$fixture/invalid-app/Subspace_0.1.0_darwin_arm64.zip"*
printf 'not an app\n' >"$fixture/not-an-app"
(cd "$fixture" && zip -q "$fixture/invalid-app/Subspace_0.1.0_darwin_arm64.zip" not-an-app)
checksum "$fixture/invalid-app/Subspace_0.1.0_darwin_arm64.zip"
expect_failure 'macOS app archive missing executable' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/invalid-app"

cp -R "$fixture/subspace" "$fixture/missing-checksum"
rm "$fixture/missing-checksum/subspace-0.1.0-x86_64-pc-windows-msvc.zip.sha256"
expect_failure 'missing checksum' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/missing-checksum"

cp -R "$fixture/subspace" "$fixture/corrupt-checksum"
printf 'changed\n' >>"$fixture/corrupt-checksum/subspace-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
expect_failure 'corrupt checksum' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/corrupt-checksum"

cp -R "$fixture/subspace" "$fixture/wrong-version"
printf 'old release\n' >"$fixture/wrong-version/subspace-0.0.9-x86_64-unknown-linux-gnu.tar.gz"
expect_failure 'wrong version' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/wrong-version"

cp -R "$fixture/subspace" "$fixture/foreign-archive"
printf 'foreign release\n' >"$fixture/foreign-archive/other-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
expect_failure 'foreign archive' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/foreign-archive"

cp -R "$fixture/subspace" "$fixture/unsupported-target"
printf 'unsupported release\n' >"$fixture/unsupported-target/subspace-0.1.0-x86_64-apple-darwin.tar.gz"
expect_failure 'unsupported target archive' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/unsupported-target"

cp -R "$fixture/subspace" "$fixture/unsupported-format"
printf 'unsupported release\n' >"$fixture/unsupported-format/subspace-0.1.0-x86_64-unknown-linux-gnu.tgz"
expect_failure 'unsupported archive format' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0 "$fixture/unsupported-format"

expect_failure 'missing arguments' \
	bash "$root/scripts/verify-release-assets.sh" 0.1.0
expect_failure 'invalid version' \
	bash "$root/scripts/verify-release-assets.sh" v0.1.0 "$fixture/subspace"

echo 'release asset completeness and checksum fixtures passed'
