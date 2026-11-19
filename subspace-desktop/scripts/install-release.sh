#!/bin/sh
# Download a Subspace release with the GitHub CLI and install it for the
# current user (Linux, macOS, and Windows under Git Bash).
#
# Releases use the subspace-desktop-v tag prefix. The GitHub CLI must be
# installed and signed in. Override the repository with --repository
# OWNER/REPO or SUBSPACE_RELEASE_REPO.
#
# The script picks the newest release, or --version, downloads the archive for
# this machine and its .sha256 file, verifies the checksum, and installs.

set -eu

repository="${SUBSPACE_RELEASE_REPO:-aiconsultancy/subspace}"
version="${SUBSPACE_VERSION:-latest}"
bin_dir="${SUBSPACE_BIN_DIR:-}"
channel=stable
link=false

usage() {
	cat <<'USAGE'
Usage: install-release.sh [--channel CHANNEL] [--link] [--repository OWNER/REPO] [--version VERSION] [--bin-dir PATH]

Download a Subspace release. On macOS, install the signed app into
~/Applications and link its executable into a per-user bin directory. On Linux
and Windows, install the executable.

Options:
  --channel CHANNEL        stable (default), nightly, alpha or beta.
  --link                   Make a pre-release the PATH CLI. Otherwise print its full path.
  --repository OWNER/REPO  Override aiconsultancy/subspace.
  --version VERSION        Release to install, such as 0.1.0. The default is the newest.
  --bin-dir PATH           Install or link the CLI here instead of the default.
  -h, --help               Show this text.

Environment:
  SUBSPACE_RELEASE_REPO     Same as --repository. Only subspace-desktop-v tags are used.
  SUBSPACE_VERSION          Same as --version.
  SUBSPACE_BIN_DIR          Same as --bin-dir.
USAGE
}

while [ $# -gt 0 ]; do
	case "$1" in
	--channel)
		channel="${2:?--channel requires a value}"
		shift 2
		;;
	--link)
		link=true
		shift
		;;
	--repository)
		repository="${2:?--repository requires a value}"
		shift 2
		;;
	--version)
		version="${2:?--version requires a value}"
		shift 2
		;;
	--bin-dir)
		bin_dir="${2:?--bin-dir requires a value}"
		shift 2
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		echo "unknown argument: $1" >&2
		usage >&2
		exit 2
		;;
	esac
done

tag_prefix="subspace-desktop-v"
asset_prefix="subspace"
executable="subspace"
windows_install_directory="subspace"
product_name="Subspace"
app_name="Subspace"
asset_stem="$app_name"
case "$channel" in
stable) link=true ;;
nightly | alpha | beta)
	case "$channel" in nightly) title=Nightly ;; alpha) title=Alpha ;; beta) title=Beta ;; esac
	app_name="$app_name ($title)"
	asset_stem="$asset_stem-$title"
	windows_install_directory="$windows_install_directory-$channel"
	;;
*) echo "unsupported channel: $channel" >&2; exit 2 ;;
esac

validate_version() {
	if [ "$channel" = stable ]; then
		pattern='(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
		# Explicit branch versions use stable's identity. Latest selection below
		# still accepts only stable releases; integration builds are download-only.
		if printf '%s\n' "$version" | grep -Eq "^$pattern-[0-9a-z-]+\.[1-9][0-9]{11}\.g[0-9a-f]{7}$"; then
			prerelease="${version#*-}"
			slug="${prerelease%%.*}"
			case "$slug" in
				nightly | alpha | beta | stable) ;;
				*) if ! printf '%s\n' "$slug" | grep -Eq '^0[0-9]+$'; then return 0; fi ;;
			esac
		fi
	else
		pattern="(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)-$channel\\.[1-9][0-9]{11}\\.g[0-9a-f]{7}"
	fi
	if ! printf '%s\n' "$version" | grep -Eq "^$pattern$"; then
		echo "version $version is not a $channel version" >&2
		exit 2
	fi
}
if [ "$version" != latest ]; then
	version="${version#v}"
	validate_version
fi

if [ -z "$repository" ]; then
	echo "a release repository is required" >&2
	exit 1
fi

require_tool() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "required tool not found: $1" >&2
		exit 1
	fi
}

require_tool gh

case "$(uname -m)" in
x86_64 | amd64) arch=x86_64 ;;
arm64 | aarch64) arch=aarch64 ;;
*)
	echo "unsupported architecture: $(uname -m)" >&2
	exit 1
	;;
esac

archive_extension="tar.gz"
exe=""
install_app=false
case "$(uname -s)" in
Linux) target="${arch}-unknown-linux-gnu" ;;
Darwin)
	if [ "$arch" = x86_64 ]; then
		echo "no ${product_name} release is published for Intel Macs (x86_64-apple-darwin)" >&2
		exit 1
	fi
	target="${arch}-apple-darwin"
	archive_extension="zip"
	install_app=true
	require_tool unzip
	;;
MINGW* | MSYS* | CYGWIN* | Windows_NT)
	if [ "$arch" = aarch64 ]; then
		echo "no ${product_name} release is published for Windows arm64 (aarch64-pc-windows-msvc)" >&2
		exit 1
	fi
	target="${arch}-pc-windows-msvc"
	archive_extension="zip"
	exe=".exe"
	require_tool unzip
	;;
*)
	echo "unsupported operating system: $(uname -s)" >&2
	exit 1
	;;
esac
[ "$archive_extension" = "tar.gz" ] && require_tool tar

if [ "$version" = "latest" ]; then
	if [ "$channel" = stable ]; then
	tag="$(gh release list --repo "$repository" --exclude-drafts --exclude-pre-releases \
		--limit 100 --json tagName \
		--jq "[.[] | select(.tagName | startswith(\"${tag_prefix}\")) | select(.tagName | test(\"^${tag_prefix}(0|[1-9][0-9]*)[.](0|[1-9][0-9]*)[.](0|[1-9][0-9]*)$\"))][0].tagName // empty")"
	else
		# Retention keeps a bounded number, but paginate so other releases in
		# the repository cannot push this channel outside an arbitrary release-list limit.
		# gh applies --jq to each page and rejects --slurp with --jq, so the
		# filter prints matching tags and the newest fixed-width ID wins here.
		# [.] instead of \. keeps the regex free of shell escapes.
		tag="$(gh api --paginate "repos/$repository/releases?per_page=100" --jq \
			".[] | select(.draft == false and .prerelease == true) | .tag_name | select(test(\"^${tag_prefix}[0-9]+[.][0-9]+[.][0-9]+-${channel}[.][1-9][0-9]{11}[.]g[0-9a-f]{7}$\"))" |
			awk -v marker="-${channel}." '{ print substr($0, index($0, marker) + length(marker), 12), $0 }' |
			sort | tail -n 1 | cut -d ' ' -f 2)"
	fi
	if [ -z "$tag" ]; then
		echo "no published release found with a ${tag_prefix} tag in ${repository}; use --version to select an older release" >&2
		exit 1
	fi
	version="${tag#"$tag_prefix"}"
else
	version="${version#v}"
	tag="${tag_prefix}${version}"
fi
validate_version

if [ "$install_app" = true ]; then
	asset="${asset_stem}_${version}_darwin_arm64.zip"
else
	asset="${asset_prefix}-${version}-${target}.${archive_extension}"
fi

temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT HUP INT TERM

echo "downloading ${asset} from ${tag}"
gh release download "$tag" --repo "$repository" --dir "$temporary_directory" \
	--pattern "$asset" --pattern "${asset}.sha256" || {
	echo "could not download ${asset} and ${asset}.sha256 from release ${tag} in ${repository}" >&2
	exit 1
}
for file in "$asset" "${asset}.sha256"; do
	if [ ! -f "${temporary_directory}/${file}" ]; then
		echo "release ${tag} has no asset named ${file}" >&2
		exit 1
	fi
done

expected="$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1 }' \
	"${temporary_directory}/${asset}.sha256")"
if [ -z "$expected" ]; then
	echo "${asset}.sha256 has no entry for ${asset}" >&2
	exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
	actual="$(sha256sum "${temporary_directory}/${asset}" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
	actual="$(shasum -a 256 "${temporary_directory}/${asset}" | cut -d' ' -f1)"
else
	echo "neither sha256sum nor shasum is installed; cannot verify the download" >&2
	exit 1
fi
if [ "$expected" != "$actual" ]; then
	echo "checksum mismatch for ${asset}" >&2
	echo "  expected ${expected}" >&2
	echo "  actual   ${actual}" >&2
	exit 1
fi
echo "checksum verified"

extract_directory="${temporary_directory}/extract"
mkdir -p "$extract_directory"
if [ "$archive_extension" = "zip" ]; then
	unzip -q "${temporary_directory}/${asset}" -d "$extract_directory"
else
	tar -xzf "${temporary_directory}/${asset}" -C "$extract_directory"
fi

home="${HOME:-${USERPROFILE:-}}"
if [ -n "$bin_dir" ]; then
	target_directory="$bin_dir"
elif [ -n "${exe}" ] && [ -n "${LOCALAPPDATA:-}" ]; then
	target_directory="${LOCALAPPDATA}/Programs/${windows_install_directory}"
elif [ -z "$home" ]; then
	echo "HOME and USERPROFILE are both empty; set SUBSPACE_BIN_DIR instead" >&2
	exit 1
else
	target_directory="${home}/.local/bin"
fi
link_directory="$target_directory"
if [ "$channel" != stable ] && [ "$install_app" != true ]; then
	if [ -n "$exe" ]; then
		target_directory="${LOCALAPPDATA:?LOCALAPPDATA is required}/Programs/${windows_install_directory}"
	else
		target_directory="${home:?HOME is required}/.local/share/${executable}-${channel}/bin"
	fi
fi
if [ "$install_app" != true ] || [ "$link" = true ]; then mkdir -p "$target_directory"; fi

if [ "$install_app" = true ]; then
	app_source="${extract_directory}/${app_name}.app"
	source_file="${app_source}/Contents/MacOS/${executable}"
else
	source_file="${extract_directory}/${executable}${exe}"
fi
if [ ! -f "$source_file" ]; then
	echo "the archive did not contain the expected ${executable}${exe}" >&2
	exit 1
fi
installed="${target_directory}/${executable}${exe}"
if [ "$install_app" = true ]; then
	# Check the CLI link path before the app is replaced, so a failure cannot
	# leave a new app with a missing command.
	if [ "$link" = true ] && [ -d "$installed" ] && [ ! -L "$installed" ]; then
		echo "${installed} is a directory; remove it or choose another --bin-dir" >&2
		exit 1
	fi
	if [ -z "$home" ]; then
		echo "HOME and USERPROFILE are both empty; cannot select ~/Applications" >&2
		exit 1
	fi
	applications="${home}/Applications"
	app_installed="${applications}/${app_name}.app"
	mkdir -p "$applications"
	app_staged="${app_installed}.new$$"
	rm -rf "$app_staged"
	mv "$app_source" "$app_staged"
	rm -rf "$app_installed"
	mv "$app_staged" "$app_installed"
	echo "installed ${app_name}.app ${version} to ${app_installed}"
	if [ "$link" = true ]; then
		ln -sfn "$app_installed/Contents/MacOS/$executable" "$installed"
		echo "linked ${executable} to ${installed}"
	fi
	installed="$app_installed/Contents/MacOS/$executable"
else
	# The binary is renamed into place. A write over a running binary fails with
	# "text file busy", and a rename does not.
	staged="${installed}.new$$"
	cp "$source_file" "$staged"
	chmod 755 "$staged"
	mv -f "$staged" "$installed"
	echo "installed ${executable} ${version} to ${installed}"
	if [ "$channel" != stable ] && [ "$link" = true ] && [ -z "$exe" ]; then
		mkdir -p "$link_directory"
		ln -sfn "$installed" "$link_directory/$executable"
		target_directory="$link_directory"
	fi
fi

if [ "$channel" != stable ] && [ "$link" != true ]; then
	echo "CLI: $installed"
	exit 0
fi

case ":${PATH}:" in
*":${target_directory}:"*) ;;
*)
	echo
	echo "${target_directory} is not on PATH. Add it with one of these:"
	echo "  bash, zsh:   export PATH=\"${target_directory}:\$PATH\""
	echo "  fish:        fish_add_path ${target_directory}"
	;;
esac
