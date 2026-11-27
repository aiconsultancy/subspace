#!/usr/bin/env bash
# Screenshot evidence from the debug capture build, isolated from the real
# user. Usage: scripts/capture.sh <out-dir> [steps]
#
# steps is SUBSPACE_CAPTURE_STEPS (see AGENTS.md), default
# "wait:5000;shot:main". The app starts in the background from an empty
# environment and sees only fixtures: a new HOME and XDG folders, a gh
# config with the fictional octo-dev accounts and a gh that hands out
# fixture tokens, the fictional GitHub and Subspace API servers on loopback (so no
# real API or credentials; the GitHub one also answers Sign in with GitHub,
# whose debug-build tokens stay in a file in the fixture home), no token
# variables and no login-shell PATH. Never launch a capture any other way: `open` passes the caller's
# environment to the app, and XDG_CONFIG_HOME alone exposes the real gh
# accounts.
#
# Optional:
#   CAPTURE_THEME=<name>       saved appearance.theme (default: none saved, which shows Cupertino Dark)
#   CAPTURE_FIRST_RUN=1        no state.json or config.json, as at a new install's first start;
#                              it refuses the options below that write them
#   CAPTURE_STATE='{...}'      keys merged into state.json
#   CAPTURE_CONFIG='{...}'     keys merged into config.json, such as boards
#   CAPTURE_RUNS_DELAY=<secs>  hold each Actions run list
#   CAPTURE_ITEM_DELAY=<secs>  hold each GitHub issue or pull request detail read
#   CAPTURE_IRC_CONFIG=<file>  IRC board config; use scripts/irc-test-server.sh
#   CAPTURE_NO_GH=1            no gh accounts, as for a user without the GitHub CLI
#   CAPTURE_NO_API=1           no Subspace API variables for the app, for a build with no API
#                              backend (SUBSPACE_BUILD_CONFIG naming {"api_backend": "none"}),
#                              which refuses to start with them
#   CAPTURE_NO_UPDATES_STREAM=1   the Subspace API fixture has no live updates stream
#   CAPTURE_DECISION_SCENARIO=<normal|missing|forbidden|unavailable|conflict>  Decisions fixture
#   CAPTURE_IRC_CREDENTIAL=1   the Subspace API fixture issues fictional IRC sign-ins, so Channels
#                              keeps a network for each server the fixture's fleets use
#   CAPTURE_NO_MEMBERSHIPS=1   the Subspace API fixture's person belongs to no initiative
#   CAPTURE_API_UNREACHABLE=1  the app's Subspace API address is a loopback port nothing listens on,
#                              so every read finds the API unreachable
#   CAPTURE_INITIATIVES_DELAY=<secs>  the Subspace API fixture holds each memberships read
#   CAPTURE_CHANGE_AFTER=<s>   the fixture retitles SUB-1 after <s> seconds, as a change elsewhere
#   CAPTURE_FLEET=1            the fixture adds the neutral FLT project and the demo world's NAV, MAP,
#                              WARP and RELAY issues for web Space evidence
#   CAPTURE_FLEET_API=1        a fictional Stargate Fleet API (the aurora fleet) on loopback, with the
#                              built-in Fleet Console's address and token set to it and the fleet's
#                              Settings > Fleets token saved for that address; with CAPTURE_FLEET=1 the
#                              API's aurora fleet records it as its fleetApiUrl, so the app uses it
#   CAPTURE_FLEET_TOKEN=<saved|none|moved>  with CAPTURE_FLEET_API=1, the aurora fleet's Settings > Fleets
#                              token: saved for the fixture's address (the default), none saved, or
#                              fixture-fleet-moved saved for https://aurora-old.example.test, so the
#                              fleet shows Address changed; the fixture's log (CAPTURE_KEEP_LOG=1)
#                              fingerprints each request's token
#   CAPTURE_MOVE_UNCONFIRMED=1  a committed move lists its sub-issues as not confirmed, as a NETT move can
#   CAPTURE_UPLOAD_SCENARIO=<unknown|absent>  the first issue opened shows an upload NETT left unresolved
#   CAPTURE_AGENTSVIEW_CONFIG=path  copy synthetic AgentsView settings into the capture home
#   CAPTURE_KEEP_LOG=1         copy the app's logs into <out-dir>
#   CAPTURE_FEATURES=<list>    comma-separated Cargo features added to the build, such as
#                              space-lab,component-gallery; built in, they start off until
#                              CAPTURE_CONFIG turns them on
# Linux uses the native executable, a private Xvfb display and a private
# D-Bus session, including on Wayland desktops. A Vulkan driver is
# required; Mesa lavapipe supports headless machines without a GPU.
# A web Space's page shows as a snapshot drawn over its native view, and
# click: and move: steps on it reach the page (spaces/web/src/mirror.rs).
set -euo pipefail

root="${SUBSPACE_RUN_APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
out="${1:?usage: scripts/capture.sh <out-dir> [steps]}"
steps="${2:-wait:5000;shot:main}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
cd "$root"
platform="$(uname -s)"
case "$platform" in
    Darwin | Linux) ;;
    *) echo "capture supports macOS and Linux, not $platform" >&2; exit 2 ;;
esac
case "${CAPTURE_UPLOAD_SCENARIO:-}" in
    '' | unknown | absent) ;;
    *) echo "CAPTURE_UPLOAD_SCENARIO must be unknown or absent" >&2; exit 2 ;;
esac
if [ -n "${CAPTURE_FIRST_RUN:-}" ]; then
    for option in CAPTURE_THEME CAPTURE_STATE CAPTURE_CONFIG CAPTURE_IRC_CONFIG CAPTURE_FLEET_API; do
        if [ -n "${!option:-}" ]; then
            echo "CAPTURE_FIRST_RUN starts with no state or config file, so it cannot take $option" >&2
            exit 2
        fi
    done
fi
case "${CAPTURE_FLEET_TOKEN:-saved}" in
    saved | none | moved) ;;
    *) echo "CAPTURE_FLEET_TOKEN must be saved, none or moved" >&2; exit 2 ;;
esac
if [[ "$platform" = Linux ]]; then
    for helper in dbus-run-session dbus-daemon xvfb-run Xvfb xauth; do
        type -P "$helper" >/dev/null || {
            echo "Linux capture needs $helper (install dbus-daemon, xvfb and xauth on Ubuntu)." >&2
            exit 2
        }
    done
fi
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
load_product
features=(--features capture)
# The debug-only terminal needs Zig, which the terminal mise environment
# (`make capture` sets MISE_ENV=terminal) provides.
if command -v zig >/dev/null 2>&1; then
    features+=(--features terminal)
fi
if [[ -n "${CAPTURE_FEATURES:-}" ]]; then
    features+=(--features "$CAPTURE_FEATURES")
fi
cargo build --locked -p "$binary_name" "${features[@]}"

fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-capture.XXXXXX")"
servers=()
cleanup() {
    if [ -n "${servers[0]:-}" ]; then kill "${servers[@]}" 2>/dev/null || true; fi
    rm -rf "$fixture"
}
trap cleanup EXIT
(umask 077; printf '%s\n' fixture-access >"$fixture/access-jwt")

home="$fixture/home"
if [[ "$platform" = Darwin ]]; then
    state_dir="$home/Library/Application Support/subspace"
    config_dir="$state_dir"
else
    state_dir="$home/.local/state/subspace"
    config_dir="$home/.config/subspace"
fi
mkdir -p "$state_dir" "$config_dir" "$fixture/gh" "$fixture/bin" "$home/.config" "$home/.local/share" "$home/.local/state" "$home/.cache"
# A terminal session's default prompt names the real user and host; the
# fixture shells say octo-dev@fixture instead.
printf '%s\n' "PROMPT='octo-dev@fixture %1~ %# '" >"$home/.zshrc"
printf '%s\n' "PS1='octo-dev@fixture \\W \\$ '" >"$home/.bashrc"

# The loopback address a fixture server reports in its log.
server_address() {
    local address=""
    for _ in $(seq 100); do
        address="$(grep -o -m1 'http://127\.0\.0\.1:[0-9]*' "$1" || true)"
        if [ -n "$address" ]; then break; fi
        sleep 0.1
    done
    test -n "$address" || { echo "fixture server did not start:" >&2; cat "$1" >&2; return 1; }
    printf '%s\n' "$address"
}
python3 -u "$root/scripts/github_test_server.py" --port 0 --runs-delay "${CAPTURE_RUNS_DELAY:-0}" \
    --item-delay "${CAPTURE_ITEM_DELAY:-0}" \
    >"$fixture/github.log" 2>&1 &
servers+=("$!")
api_args=(--port 0 --access-jwt fixture-access)
if [ -n "${CAPTURE_NO_UPDATES_STREAM:-}" ]; then api_args+=(--no-updates-stream); fi
if [ -n "${CAPTURE_CHANGE_AFTER:-}" ]; then api_args+=(--change-after "$CAPTURE_CHANGE_AFTER"); fi
if [ -n "${CAPTURE_DECISION_SCENARIO:-}" ]; then api_args+=(--decision-scenario "$CAPTURE_DECISION_SCENARIO"); fi
if [ -n "${CAPTURE_IRC_CREDENTIAL:-}" ]; then api_args+=(--irc-credential); fi
if [ -n "${CAPTURE_NO_MEMBERSHIPS:-}" ]; then api_args+=(--no-memberships); fi
if [ -n "${CAPTURE_INITIATIVES_DELAY:-}" ]; then
    api_args+=(--initiatives-delay "$CAPTURE_INITIATIVES_DELAY")
fi
if [ -n "${CAPTURE_FLEET:-}" ]; then api_args+=(--fleet); fi
fleet_api=""
if [ -n "${CAPTURE_FLEET_API:-}" ]; then
    python3 -u "$root/scripts/fleet_api_test_server.py" --port 0 --token fixture-fleet --log-requests \
        >"$fixture/fleet-api.log" 2>&1 &
    servers+=("$!")
    fleet_api="$(server_address "$fixture/fleet-api.log")"
    # The Subspace API fixture's aurora fleet records this address.
    api_args+=(--fleet-api-url "$fleet_api")
fi
python3 -u "$root/scripts/subspace_api_test_server.py" "${api_args[@]}" >"$fixture/subspace-api.log" 2>&1 &
servers+=("$!")
github="$(server_address "$fixture/github.log")"
subspace_api="$(server_address "$fixture/subspace-api.log")"

if [ -z "${CAPTURE_NO_GH:-}" ]; then
    cat >"$fixture/gh/hosts.yml" <<'EOF'
github.com:
    users:
        octo-dev:
        octo-dev_acme:
    git_protocol: https
    user: octo-dev
EOF
fi
cat >"$fixture/bin/gh" <<'EOF'
#!/bin/sh
# Fixture gh: `gh auth token ... --user <login>` prints fixture-<login>.
[ "$1 $2" = "auth token" ] || exit 1
user=octo-dev
while [ $# -gt 0 ]; do
    [ "$1" = "--user" ] && user=$2
    shift
done
case "$user" in
    octo-dev|octo-dev_acme) echo "fixture-$user" ;;
    *) exit 1 ;;
esac
EOF
chmod 755 "$fixture/bin/gh"

if [ -n "${CAPTURE_AGENTSVIEW_CONFIG:-}" ]; then
    mkdir -m 700 "$config_dir/agentsview"
    install -m 600 "$CAPTURE_AGENTSVIEW_CONFIG" "$config_dir/agentsview/config.json"
fi

# A first run has neither file: the app records a new install's theme.
if [ -z "${CAPTURE_FIRST_RUN:-}" ]; then
python3 - "$state_dir" "$config_dir" "${CAPTURE_THEME:-}" "${CAPTURE_STATE:-}" "${CAPTURE_CONFIG:-}" "$fleet_api" \
    "${CAPTURE_FLEET_TOKEN:-saved}" <<'EOF'
import json, os, sys
state_dir, config_dir, theme, state_extra, config_extra, fleet_api, fleet_token = sys.argv[1:]
state = {"appearance.theme": theme} if theme else {}
state.update(json.loads(state_extra or "{}"))
config = {"repos": ["acme/web", "acme/api", "acme/design-system"]}
boards = {}
if fleet_api:
    # The app adds its built-in Fleet Console to this entry at start.
    boards["fleet-console"] = {"settings": {"endpoint": fleet_api}, "secrets": {"token": "fixture-fleet"}}
extra = json.loads(config_extra or "{}")
# CAPTURE_CONFIG's web Spaces join these by ID; its own fleet-console entry replaces the one above.
boards.update(extra.pop("webBoards", None) or {})
config.update(extra)
if boards:
    config["webBoards"] = boards
files = [(state_dir, "state.json", state), (config_dir, "config.json", config)]
# The aurora fleet's token in Settings > Fleets, as a debug build keeps it
# beside state.json: saved for the Fleet API fixture's address, or with
# CAPTURE_FLEET_TOKEN=moved another token saved for an address the fleet
# had before, which must never reach the fixture.
saved = {"saved": (fleet_api, "fixture-fleet"),
         "moved": ("https://aurora-old.example.test", "fixture-fleet-moved")}.get(fleet_token)
if fleet_api and saved:
    token = json.dumps({"v": 1, "endpoint": saved[0], "token": saved[1]})
    files.append((state_dir, "fleet-api-tokens.json", {"fleet-api:aurora": token}))
for directory, name, value in files:
    path = os.path.join(directory, name)
    with open(path, "w") as file:
        json.dump(value, file)
    os.chmod(path, 0o600)
EOF
fi
if [ -n "${CAPTURE_IRC_CONFIG:-}" ]; then
    mkdir -m 700 "$config_dir/irc"
    install -m 600 "$CAPTURE_IRC_CONFIG" "$config_dir/irc/config.json"
fi

app_env=(
    "HOME=$home" "XDG_CONFIG_HOME=$home/.config" "XDG_DATA_HOME=$home/.local/share"
    "XDG_STATE_HOME=$home/.local/state" "XDG_CACHE_HOME=$home/.cache"
    "GH_CONFIG_DIR=$fixture/gh" GH_TOKEN= GITHUB_TOKEN=
    "PATH=$fixture/bin:/usr/bin:/bin:/usr/sbin:/sbin" SUBSPACE_GUI_LOGIN_PATH=0
    "SUBSPACE_GITHUB_API_URL=$github" "SUBSPACE_GITHUB_OAUTH_URL=$github" SUBSPACE_IDENTITY_PROVIDER=
    "SUBSPACE_CAPTURE_DIR=$out" "SUBSPACE_CAPTURE_STEPS=$steps"
)
if [ -n "${CAPTURE_API_UNREACHABLE:-}" ]; then
    # Port 9 (discard) on loopback, where no capture server listens.
    subspace_api="http://127.0.0.1:9"
fi
if [ -z "${CAPTURE_NO_API:-}" ]; then
    app_env+=("SUBSPACE_API_URL=$subspace_api/api" "SUBSPACE_API_ACCESS_JWT_FILE=$fixture/access-jwt")
fi
if [ -n "${CAPTURE_MOVE_UNCONFIRMED:-}" ]; then app_env+=(SUBSPACE_CAPTURE_MOVE_UNCONFIRMED=1); fi
if [ -n "${CAPTURE_UPLOAD_SCENARIO:-}" ]; then
    app_env+=("SUBSPACE_CAPTURE_UPLOAD_SCENARIO=$CAPTURE_UPLOAD_SCENARIO")
fi
status=0
if [[ "$platform" = Darwin ]]; then
    app="$fixture/$display_name Capture.app"
    create_bundle "$target_dir/debug/$binary_name" "$app" "$bundle_id.capture" "$display_name Capture"
    open_args=(-g -n -W)
    for value in "${app_env[@]}"; do open_args+=(--env "$value"); done
    env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin "$(command -v open)" "${open_args[@]}" \
        --stderr "$fixture/stderr.log" "$app" || status=$?
else
    # Resolve only the required helpers into the isolated PATH, including
    # installations outside the system directories. Do not inherit caller PATH.
    mkdir -m 700 "$fixture/runtime"
    for helper in Xvfb xauth dbus-daemon; do
        helper_path="$(type -P "$helper")"
        helper_path="$(cd "$(dirname "$helper_path")" && pwd)/$(basename "$helper_path")"
        ln -s "$helper_path" "$fixture/bin/$helper"
    done
    # A private display keeps window pixel readback isolated from the desktop,
    # including when the caller uses Wayland. Never inherit session sockets.
    launch=("$(type -P xvfb-run)" --auto-servernum --auth-file "$fixture/Xauthority"
        --server-args='-screen 0 1600x1000x24 -nolisten tcp'
        "$(type -P dbus-run-session)" -- "$target_dir/debug/$binary_name")
    app_env+=(LC_ALL=C.UTF-8 "XDG_RUNTIME_DIR=$fixture/runtime")
    env -i "${app_env[@]}" "${launch[@]}" >"$fixture/stderr.log" 2>&1 || status=$?
fi
grep -E 'captured|capture failed' "$fixture/stderr.log" || tail -5 "$fixture/stderr.log"
if [ -n "${CAPTURE_KEEP_LOG:-}" ]; then
    cp "$fixture/stderr.log" "$fixture/github.log" "$fixture/subspace-api.log" "$out/"
    if [ -n "$fleet_api" ]; then cp "$fixture/fleet-api.log" "$out/"; fi
    find "$home" -name '*.log' -exec cp {} "$out/" \;
fi
if [[ "$status" -ne 0 ]] || grep -q 'capture failed:' "$fixture/stderr.log" || ! grep -q '^captured ' "$fixture/stderr.log"; then
    echo 'capture failed; use CAPTURE_KEEP_LOG=1 to retain diagnostic logs' >&2
    exit 1
fi
