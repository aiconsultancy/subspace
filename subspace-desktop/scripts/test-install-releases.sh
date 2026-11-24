#!/usr/bin/env bash
# Install only fixture archives through a fake GitHub CLI. No network or user PATH edits.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-install.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT
# Name the failing check: most checks are silent cmp, grep or test commands.
trap 'echo "test-install-releases.sh: check failed at line $LINENO" >&2; if [[ -f "$fixture/output" ]]; then cat "$fixture/output" >&2; fi' ERR
mkdir -p "$fixture/bin" "$fixture/releases" "$fixture/home"

checksum() {
	local archive="$1"
	(
		cd "$fixture/releases"
		if command -v sha256sum >/dev/null 2>&1; then
			sha256sum "$archive" >"$archive.sha256"
		else
			shasum -a 256 "$archive" >"$archive.sha256"
		fi
	)
}

make_release_archives() {
	local version="$1" variant="${2:-}" product=subspace app_name=Subspace asset_stem source="$fixture/source-subspace"
	local prefix="$product"
	asset_stem="$app_name"
	if [[ "$version" = *-nightly.* ]]; then
		app_name="$app_name (Nightly)"
		asset_stem="$asset_stem-Nightly"
	fi
	# The build with no API backend: its own archive names and binary.
	if [[ "$variant" = no-api ]]; then
		asset_stem="$asset_stem-NoAPI"
		prefix="$product-no-api"
		source="$fixture/source-subspace-no-api"
	fi
	mkdir -p "$source"
	printf '%s %s fixture binary\n' "$product" "$variant" >"$source/$product"
	chmod 755 "$source/$product"
	cp "$source/$product" "$source/$product.exe"
	for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
		aarch64-apple-darwin x86_64-pc-windows-msvc; do
		archive="$prefix-$version-$target"
		if [[ "$target" == *windows* ]]; then
			archive="$archive.zip"
			if command -v 7z >/dev/null 2>&1; then
				(
					cd "$source"
					7z a -tzip -bso0 "$fixture/releases/$archive" "$product.exe"
				)
			else
				(
					cd "$source"
					zip -q "$fixture/releases/$archive" "$product.exe"
				)
			fi
		else
			archive="$archive.tar.gz"
			tar -czf "$fixture/releases/$archive" -C "$source" "$product"
		fi
		checksum "$archive"
	done
	app="$source/$app_name.app"
	mkdir -p "$app/Contents/MacOS"
	cp "$source/$product" "$app/Contents/MacOS/$product"
	(
		cd "$source"
		# Git Bash on Windows runners has 7z but no zip.
		if command -v 7z >/dev/null 2>&1; then
			7z a -tzip -bso0 "$fixture/releases/${asset_stem}_${version}_darwin_arm64.zip" "$app_name.app"
		else
			zip -qr "$fixture/releases/${asset_stem}_${version}_darwin_arm64.zip" "$app_name.app"
		fi
	)
	checksum "${asset_stem}_${version}_darwin_arm64.zip"
}

make_release_archives 0.1.0
make_release_archives 0.1.0 no-api
make_release_archives 0.1.1-nightly.202611010200.g0123456
make_release_archives 0.2.2-feat-subspace-desktop.202611151345.gabc1234
make_release_archives 0.3.0-nightly-fix.202611151345.gabc1234
make_release_archives 0.3.0-beta-2.202611151345.gabc1234

cat >"$fixture/releases.json" <<'JSON'
[
  {"tagName":"other-component-v0.5.0","prerelease":false},
  {"tagName":"subspace-desktop-v0.2.2-feat-subspace-desktop.202611151345.gabc1234","prerelease":true},
  {"tagName":"subspace-desktop-v0.1.0","prerelease":false}
]
JSON
cat >"$fixture/mislabeled-releases.json" <<'JSON'
[
  {"tagName":"subspace-desktop-v0.3.0-nightly-fix.202611151345.gabc1234","prerelease":false},
  {"tagName":"subspace-desktop-v0.1.0","prerelease":false}
]
JSON
cat >"$fixture/prereleases.json" <<'JSON'
[[
  {"tag_name":"subspace-desktop-v0.1.1-nightly.202611020200.g0123456","draft":true,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.1.1-beta.202611030200.g0123456","draft":false,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.2.2-feat-subspace-desktop.202611151345.gabc1234","draft":false,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.3.0-nightly-fix.202611151345.gabc1234","draft":false,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.3.0-beta-2.202611151345.gabc1234","draft":false,"prerelease":true},
  {"tag_name":"other-component-v1.1.1-nightly.202611040200.g0123456","draft":false,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.1.1-nightly.202611010200.g0123456","draft":false,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.1.1-nightly.202610310200.g0123456","draft":false,"prerelease":true}
],[
  {"tag_name":"subspace-desktop-v0.1.0-nightly.202610300200.g0123456","draft":false,"prerelease":true},
  {"tag_name":"subspace-desktop-v0.1.0","draft":false,"prerelease":false}
]]
JSON
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$INSTALL_FIXTURE/gh.log"
if [[ "${1:-}" = api ]]; then
    # Like gh: --slurp cannot be combined with --jq, and --jq runs per page.
    query=""; slurp=false
    while [[ "$#" -gt 0 ]]; do
        case "$1" in
            --jq) query="$2"; shift ;;
            --slurp) slurp=true ;;
        esac
        shift
    done
    if [[ "$slurp" = true && -n "$query" ]]; then
        echo 'the `--slurp` option is not supported with `--jq` or `--template`' >&2
        exit 1
    fi
    [[ -n "$query" ]] || exit 25
    jq -c '.[]' "$INSTALL_FIXTURE/prereleases.json" | while IFS= read -r page; do
        jq -r "$query" <<<"$page"
    done
    exit
fi
test "${1:-}" = release
operation="${2:-}"
shift 2
case "$operation" in
    list)
        query="."; exclude_prereleases=false; exclude_drafts=false
        releases="$INSTALL_FIXTURE/releases.json"
        if [[ "${INSTALL_FIXTURE_MISLABELED:-0}" = 1 ]]; then
            releases="$INSTALL_FIXTURE/mislabeled-releases.json"
        fi
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --jq) query="$2"; shift ;;
                --exclude-pre-releases) exclude_prereleases=true ;;
                --exclude-drafts) exclude_drafts=true ;;
            esac
            shift
        done
        jq --argjson pre "$exclude_prereleases" --argjson draft "$exclude_drafts" \
            --argjson empty "${INSTALL_FIXTURE_EMPTY:-0}" '
            map(select(($pre == false or .prerelease != true) and ($draft == false or .draft != true))) |
            if $empty == 1 then map(select(.tagName | startswith("subspace-desktop-v") | not)) else . end' \
            "$releases" | jq -r "$query"
        ;;
    download)
        case "$1" in
            subspace-desktop-v0.1.0|subspace-desktop-v0.1.1-nightly.202611010200.g0123456|subspace-desktop-v0.2.2-feat-subspace-desktop.202611151345.gabc1234|subspace-desktop-v0.3.0-nightly-fix.202611151345.gabc1234|subspace-desktop-v0.3.0-beta-2.202611151345.gabc1234) ;;
            *) echo "wrong release tag: $1" >&2; exit 23 ;;
        esac
        shift
        directory=""
        patterns=()
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --dir) directory="$2"; shift ;;
                --pattern) patterns+=("$2"); shift ;;
                --repo) test "$2" = aiconsultancy/subspace; shift ;;
            esac
            shift
        done
        test -n "$directory"
        for file in "${patterns[@]}"; do
            cp "$INSTALL_FIXTURE/releases/$file" "$directory/"
            if [[ "$file" == *.sha256 ]] && [ "${INSTALL_FIXTURE_BAD_CHECKSUM:-0}" = 1 ]; then
                printf '%064d  %s\n' 0 "${file%.sha256}" >"$directory/$file"
            fi
        done
        ;;
    *) echo "unexpected GitHub operation: $operation" >&2; exit 24 ;;
esac
SH
chmod 755 "$fixture/bin/gh"
cat >"$fixture/bin/gh.ps1" <<'PS'
& bash (Join-Path $PSScriptRoot 'gh') @args
exit $LASTEXITCODE
PS
export INSTALL_FIXTURE="$fixture" PATH="$fixture/bin:$PATH"
export HOME="$fixture/home" USERPROFILE="$fixture/home" SUBSPACE_VERSION=latest
export LOCALAPPDATA="$fixture/local-app-data"
export SUBSPACE_RELEASE_REPO=aiconsultancy/subspace
unset GH_TOKEN GITHUB_TOKEN

for installer in shell powershell; do
	if [[ "$installer" = powershell ]] && ! command -v pwsh >/dev/null 2>&1; then
		echo "PowerShell installer fixtures skipped: pwsh is not installed"
		continue
	fi
	if [[ "$installer" = shell ]]; then
		base_command=(sh "$root/scripts/install-release.sh")
		version_flag=--version
		bin_flag=--bin-dir
		channel_flag=--channel
		link_flag=--link
		no_api_flag=--no-api
	else
		export INSTALL_FIXTURE_BIN="$fixture/bin"
		case "$(uname -s)" in
		MINGW* | MSYS* | CYGWIN* | Windows_NT)
			INSTALL_FIXTURE_BIN="$(cygpath --windows "$INSTALL_FIXTURE_BIN")"
			;;
		esac
		# Refuse to run if PowerShell would use the real GitHub CLI.
		# shellcheck disable=SC2016 # PowerShell, not Bash, expands this source.
		pwsh -NoProfile -Command '
            $command = (Get-Command gh -ErrorAction Stop).Source
            if ([IO.Path]::GetDirectoryName($command) -ne [IO.Path]::GetFullPath($env:INSTALL_FIXTURE_BIN)) {
                throw "PowerShell did not select the fixture GitHub CLI"
            }
        '
		base_command=(pwsh -NoProfile -File "$root/scripts/install-release.ps1" -NoPath)
		version_flag=-Version
		bin_flag=-BinDir
		channel_flag=-Channel
		link_flag=-Link
		no_api_flag=-NoApi
	fi

	executable=subspace
	app_name=Subspace
	version=0.1.0
	directory="$fixture/installed-$installer"
	mkdir -p "$directory"
	case "$(uname -s)" in
	MINGW* | MSYS* | CYGWIN* | Windows_NT) installed="$directory/$executable.exe" ;;
	*) installed="$directory/$executable" ;;
	esac
	command=("${base_command[@]}" "$bin_flag" "$directory")

	: >"$fixture/gh.log"
	"${command[@]}" >"$fixture/output" 2>&1 || {
		cat "$fixture/output" >&2
		echo "$installer latest installation failed" >&2
		exit 1
	}
	cmp "$fixture/source-$executable/$executable" "$installed"
	grep -Fq 'release download subspace-desktop-v0.1.0 ' "$fixture/gh.log"
	if grep -Fq 'release download subspace-desktop-v0.2.2-feat-subspace-desktop' "$fixture/gh.log"; then
		echo 'stable latest must not install an integration pre-release' >&2; exit 1
	fi
	if [[ "$(uname -s)" = Darwin ]]; then
		test -x "$fixture/home/Applications/$app_name.app/Contents/MacOS/$executable"
	fi
	grep -F 'startswith("subspace-desktop-v")' "$fixture/gh.log" >/dev/null

	"${command[@]}" "$version_flag" "v$version" >"$fixture/output" 2>&1
	cmp "$fixture/source-$executable/$executable" "$installed"

	# The build with no API backend, from the same release.
	: >"$fixture/gh.log"
	"${command[@]}" "$no_api_flag" >"$fixture/output" 2>&1 || { cat "$fixture/output" >&2; exit 1; }
	cmp "$fixture/source-$executable-no-api/$executable" "$installed"
	grep -Eq -- '--pattern (Subspace-NoAPI_0\.1\.0_darwin_arm64\.zip|subspace-no-api-0\.1\.0-)' "$fixture/gh.log"
	SUBSPACE_NO_API=1 "${command[@]}" >"$fixture/output" 2>&1
	cmp "$fixture/source-$executable-no-api/$executable" "$installed"
	"${command[@]}" >"$fixture/output" 2>&1
	cmp "$fixture/source-$executable/$executable" "$installed"

	if [[ "$installer" = shell ]]; then
		: >"$fixture/gh.log"
		INSTALL_FIXTURE_MISLABELED=1 "${command[@]}" >"$fixture/output" 2>&1
		grep -Fq 'release download subspace-desktop-v0.1.0 ' "$fixture/gh.log"
		for integration in 0.2.2-feat-subspace-desktop.202611151345.gabc1234 \
			0.3.0-nightly-fix.202611151345.gabc1234 0.3.0-beta-2.202611151345.gabc1234; do
			"${command[@]}" --version "$integration" >"$fixture/output" 2>&1
			cmp "$fixture/source-$executable/$executable" "$installed"
			grep -Fq "release download subspace-desktop-v$integration" "$fixture/gh.log"
		done
		for slug in stable nightly alpha beta 01; do
			: >"$fixture/gh.log"
			if "${command[@]}" --version "0.2.2-$slug.202611151345.gabc1234" >"$fixture/output" 2>&1; then
				echo "stable installer accepted reserved or invalid slug: $slug" >&2; exit 1
			fi
			test ! -s "$fixture/gh.log"
		done
	fi

	printf '%s\n' 'previous installation' >"$installed"
	cp "$installed" "$fixture/previous"
	if INSTALL_FIXTURE_BAD_CHECKSUM=1 "${command[@]}" >"$fixture/output" 2>&1; then
		echo "$installer accepted an invalid checksum" >&2
		exit 1
	fi
	grep -qi 'checksum mismatch' "$fixture/output"
	cmp "$fixture/previous" "$installed"

	if INSTALL_FIXTURE_EMPTY=1 "${command[@]}" >"$fixture/output" 2>&1; then
		echo "$installer selected a release without the Subspace tag prefix" >&2
		exit 1
	fi
	grep -qi 'no published release found' "$fixture/output"
	cmp "$fixture/previous" "$installed"

	if [[ "$(uname -s)" = Darwin ]]; then
		app_installed="$fixture/home/Applications/$app_name.app"
		touch "$app_installed/previous-marker"
		blocked="$fixture/blocked-$installer"
		mkdir -p "$blocked/$executable"
		if "${base_command[@]}" "$bin_flag" "$blocked" >"$fixture/output" 2>&1; then
			echo "$installer linked over a directory" >&2
			exit 1
		fi
		grep -qi 'is a directory' "$fixture/output"
		test -d "$blocked/$executable"
		# The marker proves the previous app was not replaced.
		test -f "$app_installed/previous-marker"
		rm "$app_installed/previous-marker"
	fi

	app_name='Subspace (Nightly)'
	version=0.1.1-nightly.202611010200.g0123456
	windows_directory=subspace-nightly
	directory="$fixture/channel-bin-$installer"
	mkdir -p "$directory"
	printf 'stable CLI\n' >"$directory/$executable"
	command=("${base_command[@]}" "$channel_flag" nightly "$bin_flag" "$directory")
	"${command[@]}" >"$fixture/output" 2>&1 || { cat "$fixture/output" >&2; exit 1; }
	grep -qx 'stable CLI' "$directory/$executable"
	case "$(uname -s)" in
		Darwin) installed="$fixture/home/Applications/$app_name.app/Contents/MacOS/$executable" ;;
		MINGW* | MSYS* | CYGWIN* | Windows_NT) installed="$LOCALAPPDATA/Programs/$windows_directory/$executable.exe" ;;
		*) installed="$HOME/.local/share/$executable-nightly/bin/$executable" ;;
	esac
	cmp "$fixture/source-$executable/$executable" "$installed"
	case "$(uname -s)" in
		# PowerShell prints native Windows paths, such as C:\Users\RUNNER~1\...,
		# so match the per-channel directory and executable instead.
		MINGW* | MSYS* | CYGWIN* | Windows_NT)
			grep -E "CLI: .*Programs[\\/]${windows_directory}[\\/]${executable}\.exe" "$fixture/output" >/dev/null ;;
		*) grep -F "$installed" "$fixture/output" >/dev/null ;;
	esac
	if grep -q 'not on PATH' "$fixture/output"; then echo 'pre-release suggested changing PATH without --link' >&2; exit 1; fi
	"${command[@]}" "$version_flag" "$version" "$link_flag" >"$fixture/output" 2>&1
	case "$(uname -s)" in
		MINGW* | MSYS* | CYGWIN* | Windows_NT) ;; # -NoPath protects the real Windows user PATH.
		*) test -L "$directory/$executable"; cmp "$installed" "$directory/$executable" ;;
	esac
	for invalid_version in 1.10.1 1.10.2-beta.202611010200.g0123456; do
		: >"$fixture/gh.log"
		if "${command[@]}" "$version_flag" "$invalid_version" >"$fixture/output" 2>&1; then
			echo 'installer accepted a version outside its channel' >&2; exit 1
		fi
		grep -qi 'not a nightly version' "$fixture/output"
		test ! -s "$fixture/gh.log"
	done
done

echo 'release tag selection and checksum-before-install fixtures passed'
