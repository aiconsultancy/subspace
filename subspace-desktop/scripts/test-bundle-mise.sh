#!/usr/bin/env bash
# Verify the mise bundling helper with a stubbed download, a real archive and
# checksum, and no network access.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
helper="$root/scripts/bundle-mise.sh"

if [[ "$(uname -s)" != Darwin ]]; then
	echo "bundle-mise fixtures are macOS-only, like the helper itself"
	exit 0
fi

fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-bundle-mise-fixture.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/root/.cache" "$fixture/app/Contents/Resources" "$fixture/runner temp"

version="2026.10.17"
asset="mise-v${version}-macos-arm64.tar.gz"

build_fixture_archive() {
	local out="$1" report_version="$2"
	local stage
	stage="$(mktemp -d "$fixture/stage.XXXXXX")"
	mkdir -p "$stage/mise/bin"
	printf 'fixture mise license notice\n' >"$stage/mise/LICENSE"
	cat >"$stage/mise/bin/mise" <<SH
#!/usr/bin/env bash
set -euo pipefail
if [[ "\$1" == --version ]]; then
	echo "mise ${report_version} macos-arm64 (fixture)"
	exit 0
fi
exit 1
SH
	chmod 755 "$stage/mise/bin/mise"
	(cd "$stage" && tar -czf "$out" "mise")
	rm -rf "$stage"
}

# A stub `curl` records the exact URL it was asked to fetch and serves a
# locally built fixture archive instead of reaching the real network.
cat >"$fixture/bin/curl" <<SH
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "\$*" >>"$fixture/curl.log"
url="\${@: -1}"
out=""
args=("\$@")
for ((i = 0; i < \${#args[@]}; i++)); do
	if [[ "\${args[i]}" == -o ]]; then
		out="\${args[i + 1]}"
	fi
done
case "\$url" in
*"/$asset")
	cp "$fixture/served-archive" "\$out"
	;;
*)
	echo "fixture curl: unexpected URL \$url" >&2
	exit 1
	;;
esac
SH
chmod 755 "$fixture/bin/curl"

export PATH="$fixture/bin:$PATH"
export SUBSPACE_PACKAGE_ROOT="$fixture/root"
export SUBSPACE_MISE_CACHE_DIR="$fixture/root/.cache"
export RUNNER_TEMP="$fixture/runner temp"

build_fixture_archive "$fixture/served-archive" "$version"
archive_sha256="$(shasum -a 256 "$fixture/served-archive" | cut -d' ' -f1)"
export SUBSPACE_MISE_SHA256="$archive_sha256"

: >"$fixture/curl.log"
output="$("$helper" "$fixture/app")"
[[ "$output" == "$fixture/app/Contents/Resources/bin/mise" ]]
[[ -x "$fixture/app/Contents/Resources/bin/mise" ]]
grep -Fxq 'fixture mise license notice' "$fixture/app/Contents/Resources/licenses/mise/LICENSE"
[[ "$("$fixture/app/Contents/Resources/bin/mise" --version)" == *"$version"* ]]
[[ "$(wc -l <"$fixture/curl.log" | tr -d ' ')" == 1 ]]
grep -Fq "/$asset" "$fixture/curl.log"

cached_sha256="$(shasum -a 256 "$SUBSPACE_MISE_CACHE_DIR/$asset" | cut -d' ' -f1)"
[[ "$cached_sha256" == "$SUBSPACE_MISE_SHA256" ]]

# A second run must reuse the cached archive and never call curl again.
rm -f "$fixture/bin/curl"
cat >"$fixture/bin/curl" <<'SH'
#!/usr/bin/env bash
echo "fixture curl: must not be called when the archive is cached" >&2
exit 1
SH
chmod 755 "$fixture/bin/curl"
rm -rf "$fixture/app"
mkdir -p "$fixture/app/Contents/Resources"
"$helper" "$fixture/app" >/dev/null
[[ -x "$fixture/app/Contents/Resources/bin/mise" ]]

# A corrupted cached archive must fail the bundle instead of silently
# accepting a tampered binary, and must not leave the bad archive behind.
printf 'not a real archive\n' >"$SUBSPACE_MISE_CACHE_DIR/$asset"
rm -rf "$fixture/app"
mkdir -p "$fixture/app/Contents/Resources"
if "$helper" "$fixture/app" >"$fixture/mismatch.output" 2>&1; then
	echo "bundle-mise accepted a checksum mismatch" >&2
	exit 1
fi
grep -Fq 'checksum mismatch' "$fixture/mismatch.output"
[[ ! -f "$SUBSPACE_MISE_CACHE_DIR/$asset" ]]
[[ ! -e "$fixture/app/Contents/Resources/bin/mise" ]]

# An archive that extracts cleanly but reports the wrong version must fail,
# not silently bundle an unexpected mise build.
rm -f "$fixture/bin/curl"
cat >"$fixture/bin/curl" <<SH
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "\$*" >>"$fixture/curl.log"
url="\${@: -1}"
out=""
args=("\$@")
for ((i = 0; i < \${#args[@]}; i++)); do
	if [[ "\${args[i]}" == -o ]]; then
		out="\${args[i + 1]}"
	fi
done
cp "$fixture/bad-version-archive" "\$out"
SH
chmod 755 "$fixture/bin/curl"
rm -f "$SUBSPACE_MISE_CACHE_DIR/$asset"
build_fixture_archive "$fixture/bad-version-archive" "9999.1.1"
bad_version_sha256="$(shasum -a 256 "$fixture/bad-version-archive" | cut -d' ' -f1)"
export SUBSPACE_MISE_SHA256="$bad_version_sha256"
rm -rf "$fixture/app"
mkdir -p "$fixture/app/Contents/Resources"
if "$helper" "$fixture/app" >"$fixture/bad-version.output" 2>&1; then
	echo "bundle-mise accepted an unexpected mise version" >&2
	exit 1
fi
grep -Fq 'unexpected version' "$fixture/bad-version.output"
[[ ! -e "$fixture/app/Contents/Resources/bin/mise" ]]
rm -f "$SUBSPACE_MISE_CACHE_DIR/$asset"
export SUBSPACE_MISE_SHA256="$archive_sha256"

# Missing Resources directory (not an app bundle) must fail before download.
rm -f "$fixture/bin/curl"
cat >"$fixture/bin/curl" <<'SH'
#!/usr/bin/env bash
echo "fixture curl: must not be called for an invalid app bundle" >&2
exit 1
SH
chmod 755 "$fixture/bin/curl"
if "$helper" "$fixture/not-an-app" >"$fixture/missing-app.output" 2>&1; then
	echo "bundle-mise accepted a missing app bundle" >&2
	exit 1
fi
grep -Fq 'Resources' "$fixture/missing-app.output"

echo 'bundle-mise fixtures passed: checksum verification, cache reuse, corrupt-archive rejection, bad-version rejection, invalid-bundle rejection'
