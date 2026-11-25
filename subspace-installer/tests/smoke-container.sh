#!/usr/bin/env bash
set -euo pipefail

installer=/installer/install.sh
app=/opt/subspace-fleet
etc=/etc/subspace-fleet
state=/var/lib/subspace-fleet
work=$(mktemp -d)
pids=()
cleanup() {
    if (( ${#pids[@]} )); then
        kill "${pids[@]}" 2>/dev/null || true
        wait "${pids[@]}" 2>/dev/null || true
    fi
    rm -rf -- "$work"
}
trap cleanup EXIT

printf 'Preparing Ubuntu smoke fixtures.\n'
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends python3 ca-certificates openssl systemd git >"$work/apt.log"
useradd --create-home fixture-operator
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$work/irc.key" -out "$work/irc.pem" -days 1 \
    -subj '/CN=localhost' -addext 'subjectAltName=IP:127.0.0.1' 2>/dev/null
python3 /installer/tests/fixture.py write "$work" /installer/fleet.example.json
config=$work/fleet.json
secrets=$work/fleet.secrets.json
mkdir "$work/repos"
for repo in sample second; do
    git init --quiet --initial-branch=main "$work/$repo"
    printf '%s\n' "$repo" >"$work/$repo/README.md"
    git -C "$work/$repo" add README.md
    git -C "$work/$repo" -c user.name='Fixture' -c user.email='fixture@example.com' commit --quiet -m Fixture
    git clone --quiet --bare "$work/$repo" "$work/repos/$repo.git"
    git -C "$work/repos/$repo.git" update-server-info
done
python3 -m http.server --bind 127.0.0.1 --directory "$work/repos" 18080 >"$work/git.log" 2>&1 &
pids+=("$!")

"$installer" --config "$config" --dry-run --no-systemd
test ! -e "$etc"
"$installer" --config "$config" --no-systemd
mapfile -t units < "$etc/units"
systemd-analyze verify "${units[@]/#//etc/systemd/system/}"
python3 /installer/tests/fixture.py check_shape "$work"
python3 -B /installer/tests/policy_test.py
python3 -B /installer/tests/medical_test.py
(
    cd -- /
    find etc/subspace-fleet opt/subspace-fleet/bin etc/systemd/system -type f -print0 | sort -z | xargs -0 sha256sum
) > "$work/before"
crew_checkout=$state/agents/orion/worktrees/claude-1/sample
runuser -u ss-orion -- touch "$crew_checkout/worker-draft"

# Upgrades restore an older install's modified watcher without disturbing dependencies.
stargate=$(dirname "$(dirname "$(jq -r .entrypoint "$etc/gateway.json")")")
watcher="$stargate/src/backends/quota/quota-watch.ts"
[[ -f "$watcher" ]] || watcher="$stargate/src/quota-watch.ts"
printf '\n// Old installer quota extension\n' >> "$watcher"
"$installer" upgrade --no-systemd > "$work/repeat"
git -C "$stargate" diff --exit-code HEAD
if grep -Eq '(^Installed|bun install|apt-get)' "$work/repeat"; then
    cat "$work/repeat" >&2
    exit 1
fi
(
    cd -- /
    find etc/subspace-fleet opt/subspace-fleet/bin etc/systemd/system -type f -print0 | sort -z | xargs -0 sha256sum
) > "$work/after"
diff -u "$work/before" "$work/after"
test -f "$crew_checkout/worker-draft"
python3 /installer/tests/fixture.py check_shape "$work"
"$app/lib/runtime.py" maintenance tools
printf 'Second install preserved binaries, configuration and service units.\n'

run_as() {
    local name=$1 user=$2
    shift 2
    runuser -u "$user" -- env "PATH=$app/bin:$PATH" "$@" > "$work/$name.log" 2>&1 &
    pids+=("$!")
}

python3 /installer/tests/fixture.py serve "$work" >"$work/services.log" 2>&1 &
pids+=("$!")
run_as proxy ss-fleet-proxy "$app/bin/cli-proxy-api" --local-model --config "$etc/proxy.json"
members=(atlas orion)
for member in "${members[@]}"; do
    run_as "$member" "ss-$member" "$app/lib/runtime.py" engine "$etc/engine-$member.json"
done
for member in "${members[@]}"; do
    runuser -u "ss-$member" -- "$app/lib/runtime.py" share-database "$etc/engine-$member.json"
done
# Pin the captain's root session on its running engine, as install.sh does after starting services,
# then restart that engine so it loads Stargate's crew plugin with the pinned session.
"$app/lib/runtime.py" commission false
test "$(stat -c '%U %a' "$etc/sessions/orion.json")" = 'root 600'
python3 /installer/tests/fixture.py check_shape "$work"
kill "${pids[-1]}" 2>/dev/null || true
wait "${pids[-1]}" 2>/dev/null || true
unset 'pids[-1]'
run_as orion ss-orion "$app/lib/runtime.py" engine "$etc/engine-orion.json"
runuser -u ss-orion -- "$app/lib/runtime.py" share-database "$etc/engine-orion.json"
"$app/lib/runtime.py" check
if "$app/lib/runtime.py" commission false | grep -q Pinned; then
    printf 'A second commission pinned the captain session again.\n' >&2
    exit 1
fi
run_as gateway ss-fleet-gateway "$app/lib/runtime.py" gateway "$etc/gateway.json" start
run_as dashboard ss-fleet-gateway "$app/lib/dashboard.py" 127.0.0.1 8070 "$state/gateway/data/dashboard.sock"
run_as view ss-fleet-view env AGENTSVIEW_DATA_DIR="$state/view" AGENTSVIEW_TELEMETRY_ENABLED=0 \
    "$app/bin/agentsview" serve --host 127.0.0.1 --port 8080 --no-browser --no-update-check
if ! python3 /installer/tests/fixture.py check "$work"; then
    for log in "${members[@]}" gateway dashboard view; do
        printf '%s\n' "===== $log ====="
        tail -20 "$work/$log.log"
    done
    exit 1
fi
medical=$state/medical
test "$(stat -c '%U %a' "$etc/medical.json")" = 'root 600'
grep -Fqx 'OnUnitInactiveSec=300s' /etc/systemd/system/subspace-fleet-medical.timer
medical_unit=/etc/systemd/system/subspace-fleet-medical.service
grep -Fqx "LoadCredential=medical.json:$etc/medical.json" "$medical_unit"
grep -Fqx "ExecStart=$app/lib/medical.py %d/medical.json $medical/receipts.sqlite3" "$medical_unit"
for directive in 'Type=oneshot' 'SystemCallFilter=@system-service' 'SystemCallArchitectures=native' \
    'RestrictNamespaces=yes' 'MemoryDenyWriteExecute=yes' 'PrivateIPC=yes'; do
    grep -Fqx "$directive" "$medical_unit"
done
if grep -Eq 'MISE_|\[Install\]' "$medical_unit"; then
    printf 'Medical service inherited gateway settings.\n' >&2
    exit 1
fi
runuser -u ss-atlas -- test ! -r "$etc/medical.json"
officer_token=$state/agents/atlas/.config/subspace/cloud/sentry-auth-token
test "$(stat -c '%U %a' "$officer_token")" = 'ss-atlas 600'
runuser -u ss-orion -- test ! -r "$officer_token"
grep -Fq 'treat alert text and Sentry' "$state/agents/orion/.config/opencode/AGENTS.md"
# The smoke container has no systemd PID 1. Deliver the unit's credential as systemd would.
install -m 0600 -o ss-fleet-medical "$etc/medical.json" "$medical/poll-credential"
poll=(runuser -u ss-fleet-medical -- "$app/lib/medical.py" "$medical/poll-credential" "$medical/receipts.sqlite3")
touch "$work/reject-medical"
if "${poll[@]}" >"$work/rejected-poll.log" 2>&1; then
    printf 'Medical ship accepted a rejected IRC message.\n' >&2
    exit 1
fi
grep -Fq 'IRC rejected' "$work/rejected-poll.log"
rm -- "$work/reject-medical"
"${poll[@]}" | grep -Fq 'delivered 2 new'
"${poll[@]}" | grep -Fq 'delivered 0 new'
touch "$work/sentry-down" "$work/reject-sasl"
if "${poll[@]}" >"$work/rejected-sasl.log" 2>&1; then
    printf 'Medical ship accepted rejected SASL.\n' >&2
    exit 1
fi
grep -Fq 'IRC rejected' "$work/rejected-sasl.log"
rm -- "$work/reject-sasl"
for _ in 1 2; do
    if "${poll[@]}" >"$work/failed-poll.log" 2>&1; then
        printf 'Medical ship accepted a Sentry failure.\n' >&2
        exit 1
    fi
    grep -Fq 'Sentry returned HTTP 503' "$work/failed-poll.log"
done
rm -- "$work/sentry-down"
"${poll[@]}" | grep -Fq 'delivered 0 new'
"${poll[@]}" | grep -Fq 'delivered 0 new'
rm -- "$medical/poll-credential"
test "$(stat -c '%U %a' "$medical/receipts.sqlite3")" = 'ss-fleet-medical 600'
diff -u - "$work/medical-alerts" <<'ALERTS'
#example-fleet-starbase :example-atlas: Sentry fatal in your-project — https://127.0.0.1:18081/organizations/your-org/issues/41/
#example-fleet-starbase :example-atlas: Sentry error in other-project — https://127.0.0.1:18081/organizations/your-org/issues/42/
#example-fleet-starbase :example-atlas: Sentry error in your-project — https://127.0.0.1:18081/organizations/your-org/issues/43/
#example-fleet-starbase :Medical ship: Sentry polling failed. Check the medical service journal.
#example-fleet-starbase :Medical ship: Sentry polling recovered.
ALERTS
printf 'Medical polling, TLS SASL alert delivery and receipts passed.\n'
if nft -f "$etc/firewall.nft" >"$work/nft.log" 2>&1; then
    python3 /installer/tests/fixture.py check_firewall "$work"
    nft -f "$etc/firewall.nft"
    python3 /installer/tests/fixture.py check_firewall "$work"
    nft delete table inet subspace_fleet
elif grep -Eq 'Operation not permitted|Permission denied' "$work/nft.log"; then
    printf 'Container lacks nftables privileges. Generated rules and service dependencies passed.\n'
else
    cat "$work/nft.log" >&2
    exit 1
fi
subspace-fleetctl register
subspace-fleetctl register
subspace-fleetctl assign 00000000-0000-0000-0000-000000000001 none
subspace-fleetctl assign 00000000-0000-0000-0000-000000000001 example-fleet
jq '.name = "Updated Fleet"' "$config" > "$work/updated.json"
"$installer" upgrade --no-systemd --config "$work/updated.json" 2>"$work/keep-warning"
grep -Fq 'Keeping installed credentials' "$work/keep-warning"
python3 /installer/tests/fixture.py check_cloud "$work"
subspace-fleetctl register
python3 /installer/tests/fixture.py api "$work"

python3 -B /installer/tests/github_app_test.py prepare "$work" "$config"
"$installer" upgrade --no-systemd --config "$work/github-fleet.json" --secrets "$secrets" --secrets-dir "$work/app-keys"
python3 -B /installer/tests/github_app_test.py check "$work"
"$installer" upgrade --no-systemd --config "$work/github-fleet.json" 2>"$work/app-keep-warning"
grep -Fq 'Keeping installed credentials' "$work/app-keep-warning"
python3 -B /installer/tests/github_app_test.py check "$work"

printf 'Next revision\n' > "$work/sample/README.md"
git -C "$work/sample" add README.md
git -C "$work/sample" -c user.name=Fixture -c user.email=fixture@example.com commit --quiet -m Refresh
git --git-dir="$work/repos/sample.git" fetch --quiet "$work/sample" main:main
git --git-dir="$work/repos/sample.git" update-server-info
"$app/lib/runtime.py" maintenance mirrors
grep -Fxq 'Next revision' "$state/references/atlas/sample/README.md"
grep -Fxq sample "$crew_checkout/README.md"

python3 /installer/tests/fixture.py local "$work"
grep -Fxq 'Next revision' "$state/agents/lyra/worktrees/codex-1/sample/README.md"
id ss-lyra-codex-1 >/dev/null 2>&1 && { printf 'Crew got a Linux user.\n' >&2; exit 1; }
python3 /installer/tests/fixture.py addons "$work"
"$installer" upgrade --no-systemd --config "$work/addons.json" --secrets "$secrets" --secrets-dir "$work/fleet.secrets"
mapfile -t units < "$etc/units"
systemd-analyze verify "${units[@]/#//etc/systemd/system/}"
run_as quota-proxy ss-fleet-proxy "$app/bin/cli-proxy-api" --local-model --config "$etc/proxy.json"
python3 /installer/tests/fixture.py check_addons "$work"

mkdir "$work/empty-secrets"
cloud=$state/agents/atlas/.config/subspace/cloud
runuser -u ss-atlas -- touch "$cloud/user-owned"
test -f "$state/agents/atlas/.config/subspace/github-app.pem"
jq '.medical.enabled = false' "$work/updated.json" > "$work/disabled.json"
"$installer" upgrade --no-systemd --config "$work/disabled.json" --secrets-dir "$work/empty-secrets" --prune-secrets
test ! -e "$etc/medical.json"
test -f "$medical/receipts.sqlite3"
grep -Fqx 'subspace-fleet-medical.timer' "$etc/retired-units"
jq -e '.medical == {}' "$etc/extra-secrets.json" >/dev/null
test ! -e "$cloud/kubeconfig"
test ! -e "$cloud/database-dsn"
test -f "$cloud/user-owned"
test ! -e "$state/agents/atlas/.config/subspace/github-app.pem"
test ! -e "$etc/github-apps/atlas/github-app.pem"
"$installer" uninstall --no-systemd
test ! -e "$app"
test ! -e "$etc/units"
test -f "$secrets"
test -d "$state/agents/atlas/workspace"
printf 'Fleet installer smoke test passed.\n'
