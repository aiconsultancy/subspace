#!/usr/bin/env bash
# Verify the complete three-target Subspace release asset set.
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
	echo "usage: verify-release-assets.sh VERSION DIST" >&2
	exit 2
fi

version="$1"
dist="$2"
prefix=subspace
app_name=Subspace
binary=subspace

bundle_name="$app_name"
if [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-(nightly|alpha|beta)\.[1-9][0-9]{11}\.g[0-9a-f]{7}$ ]]; then
    channel="${BASH_REMATCH[4]}"
    case "$channel" in nightly) title=Nightly ;; alpha) title=Alpha ;; beta) title=Beta ;; esac
    bundle_name="$app_name ($title)"
    app_name="$app_name-$title"
elif [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	echo "version must be stable or a supported pre-release semver: $version" >&2
	exit 2
fi
if [[ ! -d "$dist" ]]; then
	echo "release asset directory does not exist: $dist" >&2
	exit 1
fi

assets=(
	"$prefix-$version-x86_64-unknown-linux-gnu.tar.gz"
	"$prefix-$version-aarch64-apple-darwin.tar.gz"
	"$prefix-$version-x86_64-pc-windows-msvc.zip"
	"${app_name}_${version}_darwin_arm64.zip"
)

is_expected_asset() {
	local candidate="$1" expected
	for expected in "${assets[@]}"; do
		if [[ "$candidate" = "$expected" ]]; then
			return 0
		fi
	done
	return 1
}

for path in "$dist"/*; do
	[[ -e "$path" ]] || continue
	[[ -f "$path" ]] || continue
	name="$(basename "$path")"
	case "$name" in
	*.sha256) continue ;;
	esac
	if ! is_expected_asset "$name"; then
		echo "unexpected release archive for $version: $name" >&2
		exit 1
	fi
done

if ! command -v unzip >/dev/null 2>&1; then
	echo "unzip is required to inspect the macOS app archive" >&2
	exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
	hash_file() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
	hash_file() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
	echo "neither sha256sum nor shasum is installed" >&2
	exit 1
fi

for asset in "${assets[@]}"; do
	archive="$dist/$asset"
	checksum="$archive.sha256"
	if [[ ! -f "$archive" ]]; then
		echo "missing release archive: $asset" >&2
		exit 1
	fi
	if [[ ! -f "$checksum" ]]; then
		echo "missing release checksum: $asset.sha256" >&2
		exit 1
	fi
	expected="$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1; exit }' "$checksum")"
	if [[ -z "$expected" ]]; then
		echo "$asset.sha256 has no entry for $asset" >&2
		exit 1
	fi
	actual="$(hash_file "$archive")"
	if [[ "$actual" != "$expected" ]]; then
		echo "checksum mismatch for $asset" >&2
		exit 1
	fi
	echo "verified $asset"
done

app_archive="$dist/${app_name}_${version}_darwin_arm64.zip"
app_executable="${bundle_name}.app/Contents/MacOS/${binary}"
if ! unzip -Z1 "$app_archive" | grep -Fx "$app_executable" >/dev/null; then
	echo "macOS app archive does not contain $app_executable" >&2
	exit 1
fi
echo "verified $app_executable"
