#!/usr/bin/env bash
# Shared pre-release boundary checks. Source from the workflow entry scripts.

# Public outputs used by the entry scripts that source this library.
# shellcheck disable=SC2034
tag_prefix=subspace-desktop-v
# shellcheck disable=SC2034
release_name=Subspace
# Releases watch this project directory, wherever it sits in its repository.
project_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# The history a release covers: this project, and the Framework while the app
# builds it by path from a folder in the same repository, because a Framework
# change then changes the app. With the published crate, only this project.
release_paths=(.)
framework_path="$(sed -nE 's/^subspace-framework = \{ *path = "([^"]+)".*/\1/p' "$project_directory/Cargo.toml" 2>/dev/null | head -n 1 || true)"
if [[ -n "$framework_path" && -d "$project_directory/$framework_path" ]] &&
    [[ "$(git -C "$project_directory/$framework_path" rev-parse --show-toplevel 2>/dev/null)" = "$(git -C "$project_directory" rev-parse --show-toplevel 2>/dev/null)" ]]; then
    release_paths+=("$framework_path")
fi

validate_prerelease_run() {
    [[ "${GITHUB_REF:-}" = refs/heads/main ]] || { echo 'pre-releases require refs/heads/main' >&2; return 2; }
    case "${GITHUB_EVENT_NAME:-}:${RELEASE_TYPE:-}:${RELEASE_FORCE-false}" in
        schedule:nightly:false | workflow_dispatch:alpha:true | workflow_dispatch:alpha:false | workflow_dispatch:beta:true | workflow_dispatch:beta:false) ;;
        *) echo 'invalid pre-release event, type or force value' >&2; return 2 ;;
    esac
}

release_list() {
    gh api --paginate --slurp "repos/${GITHUB_REPOSITORY:?}/releases?per_page=100" | jq 'add'
}

same_type_releases() {
    jq --arg prefix "$tag_prefix" --arg type "${RELEASE_TYPE:?}" '
      [.[] | select(.prerelease == true) |
        . + {parsed: (try (.tag_name | ltrimstr($prefix) |
          capture("^(?<major>0|[1-9][0-9]*)\\.(?<minor>0|[1-9][0-9]*)\\.(?<patch>0|[1-9][0-9]*)-" + $type + "\\.(?<id>[1-9][0-9]{11})\\.g[0-9a-f]{7}$")) catch null)} |
        select((.tag_name | startswith($prefix)) and .parsed != null)]'
}

release_commit() {
    git rev-parse --verify "${1:?tag required}^{commit}"
}

prerelease_id() {
    # GNU and BSD date disagree; Python is already used by release contracts.
    python3 -c 'from datetime import datetime, timezone; import sys; print(datetime.fromisoformat(sys.argv[1].replace("Z", "+00:00")).astimezone(timezone.utc).strftime("%Y%m%d%H%M"))' "${1:?timestamp required}"
}

published_stable() {
    jq -er --arg prefix "$tag_prefix" '
      [.[] | select(.draft == false and .prerelease == false) |
        select(.tag_name | startswith($prefix)) |
        .tag_name | ltrimstr($prefix) |
        select(test("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$"))] |
      sort_by(split(".") | map(tonumber)) | last // empty'
}

next_patch() {
    [[ "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || return 2
    printf '%s.%s.%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "$((BASH_REMATCH[3] + 1))"
}

integration_base() {
    local stable="$1" sha="$2" previous
    previous="$(release_commit "$tag_prefix$stable")" || {
        echo "published stable tag $tag_prefix$stable is missing; fetch tags first" >&2; return 2;
    }
    # Restrict the history to the app and its path-built Framework. Bodies
    # accompany subjects so BREAKING CHANGE footers also determine the bump.
    git -C "$project_directory" -c log.showSignature=false log "$previous..$sha" --format='%B%x00' -- "${release_paths[@]}" |
        python3 -c '
import re
import sys

rank = 0
for record in sys.stdin.read().split("\0"):
    message = record.lstrip("\n")
    subject = message.partition("\n")[0]
    conventional = re.fullmatch(r"([a-z]+)(?:\([^()\n]+\))?(!)?: .+", subject)
    footer = re.split(r"\n[ \t]*\n", message.partition("\n")[2].rstrip())[-1]
    if (conventional and conventional[2]) or re.search(r"(?m)^BREAKING(?: CHANGE|-CHANGE):[ \t]*\S", footer):
        rank = 3
        break
    if conventional:
        rank = max(rank, {"feat": 2, "fix": 1, "perf": 1}.get(conventional[1], 0))
if not rank:
    sys.exit("no app-changing commits since the last stable release; refusing an integration build")
major, minor, patch = map(int, sys.argv[1].split("."))
if rank == 3 and major > 0:
    major, minor, patch = major + 1, 0, 0
elif rank >= 2:
    minor, patch = minor + 1, 0
else:
    patch += 1
print(f"{major}.{minor}.{patch}")
' "$stable"
}

prerelease_version() {
    local base="$1" kind="$2" id="$3" sha="$4"
    printf '%s-%s.%s.g%s\n' "$base" "$kind" "$id" "${sha:0:7}"
}

validate_integration_slug() {
    [[ "$1" =~ ^[0-9a-z-]+$ ]] || return 2
    case "$1" in nightly | alpha | beta | stable) return 2 ;; esac
    # Pure numeric SemVer identifiers cannot have a leading zero.
    [[ ! "$1" =~ ^0[0-9]+$ ]]
}

validate_prerelease_version() {
    [[ "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-([0-9a-z-]+)\.[1-9][0-9]{11}\.g[0-9a-f]{7}$ ]] || return 2
    case "${BASH_REMATCH[4]}" in
        nightly | alpha | beta) return 0 ;;
        *) validate_integration_slug "${BASH_REMATCH[4]}" ;;
    esac
}

validate_integration_version() {
    validate_prerelease_version "$1" || return 2
    local prerelease="${1#*-}"
    validate_integration_slug "${prerelease%%.*}"
}
