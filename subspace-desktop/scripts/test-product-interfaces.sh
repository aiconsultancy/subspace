#!/usr/bin/env bash
# Exercise the real product binary without a TTY, GUI, credentials or live services.
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
# The default build, as released. Space Lab is not in it, so its optional
# checks (SUBSPACE_FIXTURE_SPACE_LAB=1, macOS ARM64) build with its feature.
build_args=(build --locked -p subspace)
if [[ "${SUBSPACE_FIXTURE_SPACE_LAB:-0}" == 1 ]]; then
    build_args+=(--features space-lab)
fi
if [[ "${SUBSPACE_FIXTURE_SKIP_BUILD:-0}" != 1 ]]; then
    cargo "${build_args[@]}"
fi
metadata="$(cargo metadata --no-deps --format-version 1 --locked)"
target="$(resolve_target_dir "${CARGO_TARGET_DIR:-$(jq -er .target_directory <<<"$metadata")}")"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-interfaces.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
trap 'printf "interface fixture failed at line %s: %s\n" "$LINENO" "$BASH_COMMAND" >&2; cat "$fixture/output" "$fixture/error" >&2' ERR
mkdir -p "$fixture/home" "$fixture/work" "$fixture/bin"
for command in az gh kubectl sudo copilot; do
    cat >"$fixture/bin/$command" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$0 $*" >>"$SUBSPACE_FIXTURE_FORBIDDEN"
exit 97
EOF
    chmod 755 "$fixture/bin/$command"
done
export HOME="$fixture/home" XDG_CONFIG_HOME="$fixture/home/.config" XDG_STATE_HOME="$fixture/home/.local/state"
export APPDATA="$fixture/home/AppData/Roaming" LOCALAPPDATA="$fixture/home/AppData/Local"
export SUBSPACE_FIXTURE_FORBIDDEN="$fixture/forbidden.log"
export PATH="$fixture/bin:$PATH"
unset GH_TOKEN GITHUB_TOKEN SUBSPACE_GITHUB_TOKEN
cd "$fixture/work"

invoke() {
    "$binary" "$@" </dev/null >"$fixture/output" 2>"$fixture/error"
}

reject() {
    if invoke "$@"; then
        echo "expected rejection: $binary $*" >&2
        exit 1
    fi
}

reject_code() {
    local expected="$1" actual=0
    shift
    invoke "$@" || actual=$?
    [[ "$actual" == "$expected" ]]
}

# Set only for an explicit evidence refresh. Ordinary gates leave the tree clean.
transcript() {
    if [[ -n "${SUBSPACE_CLI_EVIDENCE_DIR:-}" ]]; then
        mkdir -p "$SUBSPACE_CLI_EVIDENCE_DIR"
        { printf '$ %s\n' "$2"; cat "$fixture/output" "$fixture/error"; printf 'exit: %s\n' "$3"; } >"$SUBSPACE_CLI_EVIDENCE_DIR/$1.txt"
    fi
}

complete_bash() {
    PATH="$fixture/bin" /bin/bash -c '
        source "$1"
        shift
        COMP_WORDS=("$@")
        COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
        _subspace_complete
        printf "%s\n" "${COMPREPLY[@]}"
    ' -- "$fixture/completion.bash" subspace "$@"
}

mkdir -p "$XDG_CONFIG_HOME/subspace"
printf '%s\n' '{"repos":["fixture/example"],"boards":{"irc":true,"github-copilot-agents":true}}' >"$XDG_CONFIG_HOME/subspace/config.json"

binary="$target/debug/subspace"
test -x "$binary"
ln -s "$binary" "$fixture/bin/subspace"
invoke __product_metadata
version="$(jq -er .version "$fixture/output")"
framework_version="$(jq -er .subspace_version "$fixture/output")"
test "$(jq -r .binary_name "$fixture/output")" = subspace
# Neither unreleased Space (Space Lab, the Component gallery) is in the
# default build.
expected_boards='["github","github-copilot-agents","irc","issues","decisions","actions"]'
if [[ "${SUBSPACE_FIXTURE_SPACE_LAB:-0}" == 1 ]]; then
    # Space Lab is built only for macOS ARM64. Name a wrong target here, not
    # as a missing board in the list below.
    invoke --about
    if ! grep -Fxq 'Rust target: aarch64-apple-darwin' "$fixture/output"; then
        built_for="$(sed -n 's/^Rust target: //p' "$fixture/output")"
        echo "SUBSPACE_FIXTURE_SPACE_LAB=1 needs a macOS ARM64 build (aarch64-apple-darwin), not ${built_for:-an unknown target}" >&2
        exit 2
    fi
    expected_boards='["github","github-copilot-agents","irc","issues","decisions","space-lab","actions"]'
fi
test "$(jq -c .boards "$fixture/output")" = "$expected_boards"
invoke --version
# A local debug build marks its version: Subspace v<version>-dev-<host>-<unix time>.
shown="$(cat "$fixture/output")"
[[ "$shown" == "Subspace v$version-dev-"* && "${shown#"Subspace v$version-dev-"}" =~ ^[0-9A-Za-z-]+-[0-9]+$ ]]
invoke --about
# The app's version line, then the Framework's.
[[ "$(sed -n 2p "$fixture/output")" == "Subspace Framework v$framework_version" ]]
grep -Fq "Repository: https://github.com/aiconsultancy/subspace" "$fixture/output"
if grep -Fq "Contact:" "$fixture/output"; then
    echo 'About must not name a contact' >&2
    exit 1
fi
grep -Fq "Git commit:" "$fixture/output"
grep -Fq "Rust target:" "$fixture/output"
invoke about
grep -Fq "Built:" "$fixture/output"
invoke board_help list --json
jq -e 'type == "object"' "$fixture/output" >/dev/null
invoke web_read --help
invoke --help
grep -Fq subspace "$fixture/output"
# `upgrade` is a Subspace Framework root command for every app with an
# update source, never a board command. `--help` must work without a
# token or a network request.
grep -wq upgrade "$fixture/output"
invoke upgrade --help
grep -Fq -- "--check" "$fixture/output"
grep -Fq -- "--version" "$fixture/output"
reject upgrade --not-a-valid-flag
reject_code 2 github upgrade
reject_code 2 unknown-root
reject_code 2 unknown-root '--literal=two words'
reject __serve

for board in github github-copilot-agents; do
    invoke "$board" --help
done
for command in scope tasks task refresh github-agent-task; do
    grep -wq -- "$command" "$fixture/output"
done
invoke github --help
cp "$fixture/output" "$fixture/command-help"
for command in assignees comment diff item refresh review scope search update-pr; do
    grep -wq "$command" "$fixture/command-help"
    invoke github "$command" --help
done
if grep -Fq github_item "$fixture/command-help"; then
    echo 'legacy action alias leaked into help' >&2
    exit 1
fi
transcript cli-help 'subspace github --help' 0
invoke github
grep -wq item "$fixture/output"
invoke github github_item --help
cp "$fixture/output" "$fixture/alias-help"
invoke github item --help
cmp "$fixture/alias-help" "$fixture/output"
invoke github scope --json
jq -e '.repos == ["fixture/example"]' "$fixture/output" >/dev/null
transcript cli-read 'subspace github scope --json (synthetic configuration fixture)' 0
reject_code 1 github comment --key 'fixture/example#42' --body fixture --json
grep -Fq 'TTY or explicit --yes' "$fixture/error"
transcript cli-denied "subspace github comment --key 'fixture/example#42' --body fixture --json </dev/null" 1
reject_code 2 github scope --invalid-flag
grep -Fq -- --invalid-flag "$fixture/error"
transcript cli-invalid 'subspace github scope --invalid-flag' 2
reject_code 2 github unknown-command
grep -Fq unknown-command "$fixture/error"
# The IRC tools act on the running app's live connections; headless commands
# have none, and the board's guide stays available.
reject_code 2 irc networks
grep -Fq 'unexpected word: irc' "$fixture/error"
invoke board_help get --board_id irc --json
jq -e '.board_id == "irc" and (.markdown | contains("irc_write"))' "$fixture/output" >/dev/null

invoke __complete-word -- github sc
grep -qx scope "$fixture/output"
invoke __complete-word -- github item --
grep -qx -- --key "$fixture/output"
invoke __complete-word -- github github_
[[ ! -s "$fixture/output" ]]
invoke completion bash
cp "$fixture/output" "$fixture/completion.bash"
complete_bash github sc >"$fixture/completed"
grep -qx scope "$fixture/completed"
complete_bash github item -- >"$fixture/completed"
grep -qx -- --key "$fixture/completed"
complete_bash github github_item -- >"$fixture/completed"
grep -qx -- --key "$fixture/completed"
complete_bash board_help get --board >"$fixture/completed"
grep -qx -- --board_id "$fixture/completed"
invoke completion zsh
/bin/zsh -n "$fixture/output"
invoke completion fish
grep -Fq "subspace __complete-word" "$fixture/output"

if [[ "${SUBSPACE_FIXTURE_SPACE_LAB:-0}" == 1 ]]; then
    jq '.boards["space-lab"] = true' "$XDG_CONFIG_HOME/subspace/config.json" >"$fixture/lab-config.json"
    cp "$fixture/lab-config.json" "$XDG_CONFIG_HOME/subspace/config.json"
    invoke board_help get --board_id space-lab --json
    jq -e '.board_id == "space-lab" and ([.images[].image_id] | sort) == ["apply", "dashboard", "settings"] and all(.images[]; .media_type == "image/webp" and .byte_length > 0 and (.caption | length) > 0)' "$fixture/output" >/dev/null
    invoke board_help image --board_id space-lab --image_id dashboard --json
    jq -e '.board_id == "space-lab" and .image_id == "dashboard" and .media_type == "image/webp" and (.data | length) > 0' "$fixture/output" >/dev/null
    reject board_help image --board_id space-lab --image_id nonexistent --json
    grep -Fq 'No bundled help image' "$fixture/error"
    invoke space-lab-inspect --help
    grep -Fq -- '--action' "$fixture/output"
    invoke space-lab-inspect --action environments --json
    jq -e '.environments | type == "array"' "$fixture/output" >/dev/null
    invoke space-lab list --help
fi

test ! -s "$SUBSPACE_FIXTURE_FORBIDDEN"
echo 'real product headless interface checks passed'
