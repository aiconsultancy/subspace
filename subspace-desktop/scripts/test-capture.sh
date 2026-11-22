#!/usr/bin/env bash
# Test platform launch and isolation with stub builds and launchers; both
# fixture APIs are real loopback servers.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product-fixture.sh
source "$root/scripts/product-fixture.sh"
setup_product_fixture ""
host_platform="$(/usr/bin/uname -s)"
cp "$root/scripts/"{capture.sh,github_test_server.py,subspace_api_test_server.py,fleet_api_test_server.py} "$fixture/scripts/"

# The capture launcher is macOS-only, but its shell contract is portable:
# validate the generated plist without depending on macOS's plutil binary.
cat >"$fixture/bin/plutil" <<'EOF'
#!/usr/bin/env python3
import plistlib
import sys
assert sys.argv[1] == "-lint", sys.argv
with open(sys.argv[2], "rb") as source:
    plistlib.load(source)
EOF
chmod 755 "$fixture/bin/plutil"

# The stub runs under `env -i`, so it finds its log folder from its own path.
cat >"$fixture/bin/open" <<'EOF'
#!/bin/sh
log="$(cd "$(dirname "$0")/.." && pwd)/open"
mkdir -p "$log"
env >"$log/env.txt"
printf '%s\n' "$@" >"$log/args.txt"
while [ $# -gt 0 ]; do
    case "$1" in
    --stderr)
        if [ -f "$log/../fail-capture" ]; then
            echo 'capture failed: action fixture::Missing: no handler' >"$2"
        else
            echo "captured main" >"$2"
        fi
        shift ;;
    --env)
        case "$2" in
        HOME=*)
            cp "${2#HOME=}/Library/Application Support/subspace/"*.json "$log/"
            fleet="$(jq -r '.webBoards["fleet-console"].settings.endpoint // empty' "$log/config.json")"
            if [ -n "$fleet" ]; then
                printf '%s\n' "$fleet" >>"$log/urls.txt"
                fleet_token="$(jq -r '.webBoards["fleet-console"].secrets.token // empty' "$log/config.json")"
                curl -s -H "Authorization: Bearer $fleet_token" -o /dev/null -w 'fleet-api %{http_code}\n' "$fleet/api/v1/fleet" >>"$log/servers.txt"
            fi
            ;;
        GH_CONFIG_DIR=*) ls "${2#*=}" >"$log/gh.txt" ;;
        SUBSPACE_GITHUB_API_URL=*) printf '%s\n' "${2#*=}" >>"$log/urls.txt"; curl -s -o /dev/null -w 'github %{http_code}\n' "${2#*=}/user" >>"$log/servers.txt" ;;
        SUBSPACE_GITHUB_OAUTH_URL=*) curl -s -o /dev/null -w 'oauth %{http_code}\n' -d client_id=fixture "${2#*=}/login/device/code" >>"$log/servers.txt" ;;
        SUBSPACE_API_URL=*)
            printf '%s\n' "${2#*=}" >>"$log/urls.txt"
            token=fixture-access
            curl -s -H "Authorization: Bearer $token" -o /dev/null -w 'subspace-api %{http_code}\n' "${2#*=}/v1/me" >>"$log/servers.txt"
            curl -s -H "Authorization: Bearer $token" -o /dev/null -w 'decisions %{http_code}\n' "${2#*=}/v1/decisions" >>"$log/servers.txt"
            ;;
        esac
        shift
        ;;
    esac
    shift
done
if [ -f "$log/../open-status" ]; then exit "$(cat "$log/../open-status")"; fi
EOF
chmod 755 "$fixture/bin/open"

mkdir -p "$fixture/tmp" "$fixture/shots"
shots="$(cd "$fixture/shots" && pwd)"
fail() { echo "capture: $*" >&2; exit 1; }
if [[ "$host_platform" = Darwin ]]; then
TMPDIR="$fixture/tmp" XDG_CONFIG_HOME=/real-user/config GH_CONFIG_DIR=/real-user/gh GH_TOKEN=real-user-token \
    CAPTURE_FEATURES='' CAPTURE_LEAK_MARK=real-user CAPTURE_THEME="One Dark" \
    CAPTURE_CONFIG='{"boards": {"actions": true}, "webBoards": {"notes": {"path": "/apps/notes"}}}' \
    CAPTURE_DECISION_SCENARIO=forbidden CAPTURE_MOVE_UNCONFIRMED=1 CAPTURE_UPLOAD_SCENARIO=unknown \
    CAPTURE_FLEET_API=1 bash "$fixture/scripts/capture.sh" "$fixture/shots" 'wait:10;shot:main' >"$fixture/capture.out"

args="$fixture/open/args.txt"
grep -Fq "build --locked -p subspace --features capture" "$fixture/cargo.log" || fail "no capture build"
grep -Eq -e '--features capture( --features terminal)?$' "$fixture/cargo.log" || fail "the capture build added features by default"
for flag in -g -n -W; do grep -Fxq -- "$flag" "$args" || fail "open without $flag"; done
if grep -F real-user "$fixture/open/env.txt"; then fail "the caller's environment reached open"; fi
if grep -F real-user "$args"; then fail "a real value reached the app"; fi
home="$(sed -n 's/^HOME=//p' "$args")"
case "$home" in "$fixture/tmp/"*) ;; *) fail "HOME is not the fixture: $home" ;; esac
for expected in "XDG_CONFIG_HOME=$home/.config" "XDG_DATA_HOME=$home/.local/share" \
    "XDG_STATE_HOME=$home/.local/state" "XDG_CACHE_HOME=$home/.cache" \
    GH_TOKEN= GITHUB_TOKEN= SUBSPACE_GUI_LOGIN_PATH=0 \
    "SUBSPACE_CAPTURE_DIR=$shots" 'SUBSPACE_CAPTURE_STEPS=wait:10;shot:main'; do
    grep -Fxq -- "$expected" "$args" || fail "missing --env $expected"
done
grep -Eq "^GH_CONFIG_DIR=$fixture/tmp/" "$args" || fail "gh config is not the fixture"
grep -Fxq SUBSPACE_CAPTURE_MOVE_UNCONFIRMED=1 "$args" || fail "CAPTURE_MOVE_UNCONFIRMED did not reach the app"
grep -Fxq SUBSPACE_CAPTURE_UPLOAD_SCENARIO=unknown "$args" || fail "CAPTURE_UPLOAD_SCENARIO did not reach the app"
grep -Eq "^SUBSPACE_API_ACCESS_JWT_FILE=$fixture/tmp/[^/]+/access-jwt$" "$args" || fail "the access assertion is not a private fixture file"
if grep -Fq fixture-access "$args"; then fail "the access assertion itself reached open"; fi
grep -Eq "^PATH=$fixture/tmp/[^:]+/bin:/usr/bin:/bin:/usr/sbin:/sbin$" "$args" || fail "PATH is not the fixture"
for name in SUBSPACE_GITHUB_API_URL SUBSPACE_GITHUB_OAUTH_URL SUBSPACE_API_URL; do
    grep -Eq "^$name=http://127\.0\.0\.1:[0-9]+" "$args" || fail "$name is not a loopback fixture"
done
grep -Eq '^github [1-5][0-9][0-9]$' "$fixture/open/servers.txt" || fail "the GitHub fixture did not answer"
grep -Eq '^subspace-api 200$' "$fixture/open/servers.txt" || fail "the Subspace API fixture did not answer"
grep -Eq '^decisions 403$' "$fixture/open/servers.txt" || fail "the Decisions fixture scenario was not applied"
grep -Eq '^oauth 200$' "$fixture/open/servers.txt" || fail "the sign-in fixture did not answer"
grep -Eq '^fleet-api 200$' "$fixture/open/servers.txt" || fail "the Fleet API fixture did not answer"
jq -e '.webBoards["fleet-console"] | (.settings.endpoint | test("^http://127\\.0\\.0\\.1:[0-9]+$")) and .secrets.token == "fixture-fleet"' \
    "$fixture/open/config.json" >/dev/null || fail "Fleet Console is not pointed at the Fleet API fixture"
test "$(jq -r '.webBoards.notes.path' "$fixture/open/config.json")" = /apps/notes ||
    fail "CAPTURE_CONFIG's web Spaces were not merged with Fleet Console"
test "$(jq -r '."appearance.theme"' "$fixture/open/state.json")" = "One Dark" || fail "theme not saved"
test "$(jq -c '[.repos[0], .boards.actions]' "$fixture/open/config.json")" = '["acme/web",true]' || fail "config not merged"
grep -Fq 'captured main' "$fixture/capture.out" || fail "capture result not reported"
test -z "$(ls -A "$fixture/tmp")" || fail "the fixture home was left behind"
# Check the actual listening endpoints; process listing is unavailable in some sandboxes.
while IFS= read -r address; do
    for _ in $(seq 20); do
        curl -s --max-time 1 -o /dev/null "$address" || break
        sleep 0.1
    done
    if curl -s --max-time 1 -o /dev/null "$address"; then fail "a fixture server is still listening"; fi
done <"$fixture/open/urls.txt"
grep -Fxq hosts.yml "$fixture/open/gh.txt" || fail "the fixture gh accounts are missing"
rm -rf "$fixture/open"
TMPDIR="$fixture/tmp" CAPTURE_NO_GH=1 CAPTURE_FEATURES=space-lab,component-gallery \
    bash "$fixture/scripts/capture.sh" "$fixture/shots" 'wait:10;shot:main' >/dev/null
grep -Eq -e '--features capture( --features terminal)? --features space-lab,component-gallery$' "$fixture/cargo.log" ||
    fail "CAPTURE_FEATURES did not reach the build"
if grep -Fxq hosts.yml "$fixture/open/gh.txt"; then fail "CAPTURE_NO_GH left gh accounts"; fi
if grep -Fq SUBSPACE_CAPTURE_MOVE_UNCONFIRMED "$fixture/open/args.txt"; then fail "the move fixture is on by default"; fi
if grep -Fq SUBSPACE_CAPTURE_UPLOAD_SCENARIO "$fixture/open/args.txt"; then fail "the upload fixture is on by default"; fi
if jq -e '.webBoards' "$fixture/open/config.json" >/dev/null; then fail "the Fleet API fixture is on by default"; fi
if TMPDIR="$fixture/tmp" CAPTURE_UPLOAD_SCENARIO=other bash "$fixture/scripts/capture.sh" "$fixture/shots" 'wait:10;shot:main' >/dev/null 2>&1; then
    fail "an unknown upload scenario was accepted"
fi
fi

# The Linux native executable records the environment it actually receives.
cat >"$fixture/native-capture" <<'EOF'
#!/bin/bash
set -euo pipefail
fixture="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$fixture/native"
env >"$fixture/native/env.txt"
ls "$GH_CONFIG_DIR" >"$fixture/native/gh.txt"
for file in "$XDG_STATE_HOME/subspace/state.json" "$XDG_CONFIG_HOME/subspace/config.json"; do
    if [ -e "$file" ]; then cp "$file" "$fixture/native/"; fi
done
curl -s -o /dev/null -w 'github %{http_code}\n' "$SUBSPACE_GITHUB_API_URL/user" >"$fixture/native/servers.txt"
curl -s -o /dev/null -w 'oauth %{http_code}\n' -d client_id=fixture "$SUBSPACE_GITHUB_OAUTH_URL/login/device/code" >>"$fixture/native/servers.txt"
token=fixture-access
curl -s -H "Authorization: Bearer $token" -o /dev/null -w 'subspace-api %{http_code}\n' "$SUBSPACE_API_URL/v1/me" >>"$fixture/native/servers.txt"
if [ -f "$fixture/fail-native" ]; then echo 'capture failed: fixture render error' >&2; exit 0; fi
if [ -f "$fixture/no-shot-native" ]; then echo 'bad capture step: fixture-invalid' >&2; exit 0; fi
echo "captured $SUBSPACE_CAPTURE_DIR/main.png" >&2
EOF
chmod 755 "$fixture/native-capture"
mv "$fixture/bin/cargo" "$fixture/bin/cargo-stub"
cat >"$fixture/bin/cargo" <<'EOF'
#!/bin/bash
set -euo pipefail
"$FIXTURE/bin/cargo-stub" "$@"
if [ "$1" = build ]; then cp "$FIXTURE/native-capture" "$FIXTURE/target/debug/subspace"; fi
EOF
cat >"$fixture/bin/dbus-run-session" <<'EOF'
#!/bin/bash
set -euo pipefail
command -v dbus-daemon >/dev/null
test "$1" = --
shift
exec "$@"
EOF
cat >"$fixture/bin/xvfb-run" <<'EOF'
#!/bin/bash
set -euo pipefail
log="$(cd "$(dirname "$0")/.." && pwd)/xvfb-args.txt"
printf '%s\n' "$@" >"$log"
command -v Xvfb >/dev/null
command -v xauth >/dev/null
test "$1" = --auto-servernum
test "$2" = --auth-file
export XAUTHORITY="$3"
test "$4" = '--server-args=-screen 0 1600x1000x24 -nolisten tcp'
shift 4
export DISPLAY=:99
exec "$@"
EOF
for helper in Xvfb xauth dbus-daemon; do
    printf '#!/bin/sh\nexit 0\n' >"$fixture/bin/$helper"
done
chmod 755 "$fixture/bin/"{cargo,dbus-run-session,dbus-daemon,xvfb-run,Xvfb,xauth}

run_linux() {
    FAKE_UNAME=Linux TMPDIR="$fixture/tmp" XDG_CONFIG_HOME=/real-user/config \
        GH_CONFIG_DIR=/real-user/gh GH_TOKEN=real-user-token CAPTURE_LEAK_MARK=real-user \
        DBUS_SESSION_BUS_ADDRESS=real-user-bus WAYLAND_DISPLAY='' DISPLAY='' \
        CAPTURE_THEME='One Dark' CAPTURE_CONFIG='{"boards":{"actions":true}}' \
        bash "$fixture/scripts/capture.sh" "$shots" 'wait:10;shot:main'
}
run_linux >"$fixture/capture-linux.out"
native_env="$fixture/native/env.txt"
if grep -F real-user "$native_env"; then fail "caller credentials/configuration reached the Linux executable"; fi
home="$(sed -n 's/^HOME=//p' "$native_env")"
case "$home" in "$fixture/tmp/"*) ;; *) fail "Linux HOME is not the fixture: $home" ;; esac
for expected in "XDG_CONFIG_HOME=$home/.config" "XDG_DATA_HOME=$home/.local/share" \
    "XDG_STATE_HOME=$home/.local/state" "XDG_CACHE_HOME=$home/.cache" \
    GH_TOKEN= GITHUB_TOKEN= SUBSPACE_GUI_LOGIN_PATH=0 DISPLAY=:99 \
    "SUBSPACE_CAPTURE_DIR=$shots" 'SUBSPACE_CAPTURE_STEPS=wait:10;shot:main'; do
    grep -Fxq "$expected" "$native_env" || fail "missing Linux $expected"
done
grep -Fq -- '--server-args=-screen 0 1600x1000x24 -nolisten tcp' "$fixture/xvfb-args.txt" || fail 'Xvfb has no private screen'
test "$(jq -r '."appearance.theme"' "$fixture/native/state.json")" = 'One Dark' || fail 'Linux theme not saved in XDG state'
test "$(jq -c '[.repos[0], .boards.actions]' "$fixture/native/config.json")" = '["acme/web",true]' || fail 'Linux config not saved in XDG config'
grep -Eq '^subspace-api 200$' "$fixture/native/servers.txt" || fail 'Linux API fixture did not answer'
grep -Eq '^oauth 200$' "$fixture/native/servers.txt" || fail 'Linux OAuth fixture did not answer'
grep -Fxq hosts.yml "$fixture/native/gh.txt" || fail 'Linux gh fixture accounts are missing'
CAPTURE_NO_GH=1 run_linux >"$fixture/capture-linux-no-gh.out"
if grep -Fxq hosts.yml "$fixture/native/gh.txt"; then fail 'Linux CAPTURE_NO_GH left accounts'; fi
grep -Fq captured "$fixture/capture-linux.out" || fail 'Linux capture result not reported'
test -z "$(ls -A "$fixture/tmp")" || fail 'Linux fixture home was left behind'

# A first run starts with neither file, and refuses options that write one.
rm "$fixture/native/state.json" "$fixture/native/config.json"
FAKE_UNAME=Linux TMPDIR="$fixture/tmp" CAPTURE_FIRST_RUN=1 \
    bash "$fixture/scripts/capture.sh" "$shots" 'wait:10;shot:main' >"$fixture/capture-first-run.out"
grep -Fq captured "$fixture/capture-first-run.out" || fail 'first-run capture result not reported'
test ! -e "$fixture/native/state.json" || fail 'CAPTURE_FIRST_RUN wrote a state file'
test ! -e "$fixture/native/config.json" || fail 'CAPTURE_FIRST_RUN wrote a config file'
if FAKE_UNAME=Linux TMPDIR="$fixture/tmp" CAPTURE_FIRST_RUN=1 CAPTURE_THEME='One Dark' \
    bash "$fixture/scripts/capture.sh" "$shots" 'wait:10;shot:main' >/dev/null 2>&1; then
    fail 'CAPTURE_FIRST_RUN took a theme'
fi

# A caller's desktop connection must never reach the private capture display.
rm "$fixture/xvfb-args.txt"
FAKE_UNAME=Linux DISPLAY=:7 XAUTHORITY=/display-cookie WAYLAND_DISPLAY='' \
    DBUS_SESSION_BUS_ADDRESS=real-user-bus CAPTURE_LEAK_MARK=real-user \
    bash "$fixture/scripts/capture.sh" "$shots" 'shot:main' >"$fixture/capture-x11.out"
test -f "$fixture/xvfb-args.txt" || fail 'X11 desktop capture did not use Xvfb'
grep -Fxq DISPLAY=:99 "$native_env" || fail 'private X11 display not passed'
if grep -Fq /display-cookie "$native_env"; then fail 'desktop X11 cookie path leaked'; fi
if grep -F real-user "$native_env"; then fail 'session environment leaked to X11 capture'; fi

rm "$fixture/xvfb-args.txt" "$native_env"
FAKE_UNAME=Linux DISPLAY='' WAYLAND_DISPLAY=wayland-fixture XDG_RUNTIME_DIR=/display-runtime \
    bash "$fixture/scripts/capture.sh" "$shots" 'shot:main' >"$fixture/capture-wayland.out"
test -f "$fixture/xvfb-args.txt" || fail 'Wayland desktop capture did not use Xvfb'
if grep -q '^WAYLAND_DISPLAY=' "$native_env"; then fail 'Wayland leaked into headless capture'; fi
if grep -Fq /display-runtime "$native_env"; then fail 'Wayland session socket directory leaked'; fi

# A render failure must fail the command even when the app exits successfully.
touch "$fixture/fail-native"
if run_linux >"$fixture/capture-failed.out" 2>&1; then fail 'render failure was reported as success'; fi
test -z "$(ls -A "$fixture/tmp")" || fail 'failed Linux capture left its fixture home behind'
mv "$fixture/fail-native" "$fixture/no-shot-native"
if run_linux >"$fixture/capture-no-shot.out" 2>&1; then fail 'a run with no captured image was reported as success'; fi
echo 'capture checks passed'
