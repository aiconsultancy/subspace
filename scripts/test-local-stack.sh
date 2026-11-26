#!/usr/bin/env bash
# Exercise the root stack commands in a fixture checkout with a fake Docker.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
temp_root="${TMPDIR:-/tmp}"
fixture="$(mktemp -d "${temp_root%/}/subspace-local-stack.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT

# Each demo fleet has the standard channels (subspace-installer/README.md,
# "Channels"): Starbase, fleet command and one ship channel per captain, with
# the installer's topics, and an IRC account <fleet>-<captain-id> per agent.
python3 - "$root" <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
fleets = {"aurora": ("Aurora fleet", ["lyra", "orion", "vega", "castor"]),
          "borealis": ("Borealis fleet", ["rigel"])}
channels = json.loads((root / "scripts/local-stack/irc-channels.json").read_text())
expected = [{"name": "#subspace", "topic": "Local workspace"}]
accounts = ["developer"]
for slug, (name, captains) in fleets.items():
    expected += [
        {"name": f"#{slug}-starbase", "topic": f"{name} Starbase: admirals and the rear admiral"},
        {"name": f"#{slug}-command",
         "topic": f"{name} command: the rear admiral and the captains, and agent dispatch"},
        *({"name": f"#{slug}-{captain}", "topic": f"{name}: {captain.capitalize()}'s ship"}
          for captain in captains),
    ]
    accounts += [f"{slug}-admiral", *(f"{slug}-{captain}" for captain in captains)]
assert channels == expected, channels
script = (root / "scripts/local-stack.sh").read_text()
assert re.search(r"^accounts=\((.*)\)$", script, re.M)[1].split() == accounts
PY

checkout="$fixture/checkout with spaces"
mkdir -p "$checkout/scripts/local-stack" "$fixture/bin" "$fixture/home"
cp "$root/Makefile" "$checkout/"
cp "$root/scripts/local-stack.sh" "$checkout/scripts/"
cp "$root/scripts/local-stack/desktop-irc-config.py" "$checkout/scripts/local-stack/"
cd "$checkout"

export PATH="$fixture/bin:$PATH" HOME="$fixture/home" DOCKER_LOG="$fixture/docker.log"
export SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/settings/irc/config.json"
unset SUBSPACE_STACK_DESKTOP_CONFIG SUBSPACE_STACK_DEMO_DATA XDG_CONFIG_HOME
export IRC_PLAIN_PORT=16667 IRC_TLS_PORT=16697
config="$SUBSPACE_STACK_DESKTOP_CONFIG_FILE"
cat >"$fixture/bin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [ "${EXPECT_MISSING_CONFIG:-0}" = 1 ]; then
    test ! -e "$SUBSPACE_STACK_DESKTOP_CONFIG_FILE"
fi
printf '%s\n' "$*" >>"$DOCKER_LOG"
SH
chmod 755 "$fixture/bin/docker"

cat >"$fixture/bin/python3" <<'SH'
#!/usr/bin/env bash
exit 1
SH
chmod 755 "$fixture/bin/python3"
if make -s stack-up >"$fixture/old-python" 2>&1; then
    echo "expected unsupported Python to fail before stack-up" >&2
    exit 1
fi
grep -Fq 'Python 3.12 or newer is required to configure Channels' "$fixture/old-python"
test ! -e "$DOCKER_LOG"
test ! -e .stack
rm "$fixture/bin/python3"

EXPECT_MISSING_CONFIG=1 make -s stack-up >"$fixture/up"
grep -Fxq "Created IRC config at $config" "$fixture/up"
grep -Eq '^compose up .*--wait$' "$DOCKER_LOG"
grep -Eq '^compose exec .* seed:demo --with-irc$' "$DOCKER_LOG"
grep -Fxq 'compose exec -T irc python3 /run/subspace-stack/seed-irc.py' "$DOCKER_LOG"
test "$(wc -l <"$DOCKER_LOG" | tr -d ' ')" = 3
bash scripts/local-stack.sh desktop-irc-config >"$fixture/printed.json"
cmp "$fixture/printed.json" "$config"
python3 - "$config" <<'PY'
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
assert path.stat().st_mode & 0o777 == 0o600
assert path.parent.stat().st_mode & 0o777 == 0o700
assert not list(path.parent.glob("*.bak"))
config = json.loads(path.read_text())
for network, network_id, port, tls in zip(
    config["networks"], ("aurora", "borealis"), (16667, 16697), (False, True), strict=True
):
    assert network["id"] == network_id
    assert network["host"] == "127.0.0.1"
    assert network["port"] == port
    assert network["tls"] is network["acceptInvalidCerts"] is tls
    assert network["nickname"] == network["sasl"]["username"] == "developer"
    password = Path(network["sasl"]["passwordFile"])
    assert password.resolve() == Path.cwd() / ".stack/irc/developer.password"
    assert password.read_text().strip() not in path.read_text()
    for generated in password.parent.glob("*.password"):
        assert generated.stat().st_mode & 0o777 == 0o600
PY

cat >"$config" <<'JSON'
{
  "networks": [
    {"id": "remote", "name": "Remote", "host": "irc.example.com", "port": 16697, "tls": true},
    {"id": "aurora", "host": "localhost", "port": 6667, "tls": true, "acceptInvalidCerts": true},
    {"id": "handmade", "name": "My local TLS", "host": "IRC.LOCALHOST", "port": 16697,
     "tls": false, "channels": ["#example"], "autoConnect": false,
     "sasl": {"username": "developer", "password": "fixture", "custom": true}}
  ],
  "highlights": ["fixture"],
  "captainNotifications": true,
  "custom": {"preserve": true}
}
JSON
cp "$config" "$fixture/previous.json"
chmod 644 "$config"
make -s stack-desktop-config >"$fixture/merge"
grep -Fxq "Updated IRC config at $config" "$fixture/merge"
test "$(wc -l <"$DOCKER_LOG" | tr -d ' ')" = 3
python3 - "$config" "$fixture/previous.json" <<'PY'
import json
from pathlib import Path
import sys

path, previous = map(Path, sys.argv[1:])
assert path.stat().st_mode & 0o777 == 0o600
backup, = path.parent.glob("config.json.*.bak")
assert backup.stat().st_mode & 0o777 == 0o600
assert backup.read_bytes() == previous.read_bytes()
before = json.loads(previous.read_text())
after = json.loads(path.read_text())
remote, plain, tls = after.pop("networks")
original_remote, _, handmade = before.pop("networks")
assert remote == original_remote and after == before
assert plain["id"] == "aurora" and tls["id"] == handmade["id"]
assert plain["port"] == 16667 and not plain["tls"] and not plain["acceptInvalidCerts"]
assert tls["port"] == 16697 and tls["tls"] and tls["acceptInvalidCerts"]
assert tls["host"] == plain["host"] == "127.0.0.1"
assert plain["nickname"] == tls["nickname"] == "developer"
assert {key: tls[key] for key in ("name", "channels", "autoConnect")} == {
    key: handmade[key] for key in ("name", "channels", "autoConnect")
}
assert plain["sasl"]["username"] == "developer"
assert Path(plain["sasl"]["passwordFile"]).resolve() == Path.cwd() / ".stack/irc/developer.password"
assert tls["sasl"] == dict(plain["sasl"], custom=True)
PY

cp "$config" "$fixture/first-merge.json"
IRC_PLAIN_PORT=16668 make -s stack-desktop-config >/dev/null
backups=("$(dirname "$config")"/config.json.*.bak)
test "${#backups[@]}" = 1
cmp "${backups[0]}" "$fixture/first-merge.json"
make -s stack-desktop-config >/dev/null
cmp "$config" "$fixture/first-merge.json"

backup_dir=$(dirname "$config")
stale_backup="$backup_dir/config.json.20000101T000000000000Z.bak"
cp "$fixture/first-merge.json" "$stale_backup"
ln "$config" "$fixture/current-inode"
make -s stack-desktop-config >"$fixture/unchanged"
grep -Fxq "IRC config unchanged at $config" "$fixture/unchanged"
test "$config" -ef "$fixture/current-inode"
test ! -e "$stale_backup"

cat >"$fixture/protected.json" <<'JSON'
{"networks": [
  {"id":"aurora","host":"irc.example.com","port":6667,"sasl":{"username":"developer","password":"fixture"}},
  {"id":"aurora-local","host":"localhost","port":16667,"password":"fixture"},
  {"id":"borealis","host":"localhost","port":16697,"tls":true,"acceptInvalidCerts":false,
   "sasl":{"username":"tunnel-user","password":"fixture","external":true}},
  {"id":"certificate-tunnel","host":"127.0.0.1","port":16697,"clientCert":"/fixture/client.pem"}
]}
JSON
cp "$fixture/protected.json" "$fixture/protected-before.json"
SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/protected.json" make -s stack-desktop-config >/dev/null 2>"$fixture/protected-warning"
grep -Fq 'kept existing IRC connection and added aurora-local-2' "$fixture/protected-warning"
grep -Fq 'kept existing IRC connection and added borealis-local' "$fixture/protected-warning"
cp "$fixture/protected.json" "$fixture/protected-after.json"
SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/protected.json" make -s stack-desktop-config >/dev/null
cmp "$fixture/protected-after.json" "$fixture/protected.json"
SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/protected.json" IRC_PLAIN_PORT=16668 make -s stack-desktop-config >/dev/null
python3 - "$fixture/protected-before.json" "$fixture/protected.json" <<'PY'
import json
from pathlib import Path
import sys

before, after = [json.loads(Path(file).read_text())["networks"] for file in sys.argv[1:]]
assert after[:len(before)] == before
plain, tls = after[len(before):]
assert (plain["id"], plain["port"]) == ("aurora-local-2", 16668)
assert (tls["id"], tls["port"]) == ("borealis-local", 16697)
assert all(network["sasl"]["username"] == "developer" for network in (plain, tls))
PY

printf '{"highlights":["fixture"]}\n' >"$fixture/empty-networks.json"
SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/empty-networks.json" make -s stack-desktop-config >/dev/null
python3 - "$fixture/empty-networks.json" <<'PY'
import json
from pathlib import Path
import sys

config = json.loads(Path(sys.argv[1]).read_text())
assert config["highlights"] == ["fixture"]
assert [network["id"] for network in config["networks"]] == ["aurora", "borealis"]
PY

cat >"$fixture/renumbered.json" <<'JSON'
{"networks": [
  {"id": "aurora", "host": "127.0.0.1", "port": 6667, "nickname": "old"},
  {"id": "handmade", "host": "localhost", "port": 16667, "nickname": "manual"}
]}
JSON
SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/renumbered.json" make -s stack-desktop-config >/dev/null
python3 - "$fixture/renumbered.json" <<'PY'
import json
from pathlib import Path
import sys

config = json.loads(Path(sys.argv[1]).read_text())
old, handmade, tls = config["networks"]
assert old == {"id": "aurora", "host": "127.0.0.1", "port": 6667, "nickname": "old"}
assert handmade["id"] == "handmade" and handmade["port"] == 16667
assert handmade["sasl"]["username"] == "developer"
assert tls["id"] == "borealis"
PY

cp "$config" "$fixture/merged.json"
cp .stack/irc/developer.password "$fixture/old-password"
make -s stack-reset
cmp "$fixture/merged.json" "$config"
test ! -e .stack/irc/developer.password
SUBSPACE_STACK_DESKTOP_CONFIG=0 make -s stack-desktop-config >"$fixture/opt-out"
test ! -s "$fixture/opt-out"
SUBSPACE_STACK_DESKTOP_CONFIG=0 make -s stack-up >>"$fixture/opt-out"
if grep -Fq 'IRC config at' "$fixture/opt-out"; then
    echo "opt-out attempted a config write" >&2
    exit 1
fi
cmp "$fixture/merged.json" "$config"
test "$(find "$(dirname "$config")" -name '*.bak' | wc -l | tr -d ' ')" = 1
grep -Fxq api .stack/irc/connection-mode
for secret in credential.secret auth-hook.token; do
    grep -Eq '^[0-9a-f]{64}$' ".stack/irc/$secret"
    cp ".stack/irc/$secret" "$fixture/$secret"
done
if cmp -s .stack/irc/credential.secret .stack/irc/auth-hook.token; then
    echo "the signing secret and the hook token must differ" >&2
    exit 1
fi
make -s stack-up >"$fixture/restart"
grep -Fxq "IRC config unchanged at $config" "$fixture/restart"
grep -Fxq manual .stack/irc/connection-mode
for secret in credential.secret auth-hook.token; do
    cmp ".stack/irc/$secret" "$fixture/$secret"
done
cmp "$fixture/merged.json" "$config"
if cmp -s "$fixture/old-password" .stack/irc/developer.password; then
    echo "expected stack-up to generate a new password after reset" >&2
    exit 1
fi

echo '{ invalid' >"$fixture/invalid.json"
if SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/invalid.json" make -s stack-desktop-config 2>"$fixture/error"; then
    echo "expected invalid JSON to fail" >&2
    exit 1
fi
grep -Fq 'could not read existing settings as JSON' "$fixture/error"
grep -Fxq '{ invalid' "$fixture/invalid.json"

unset SUBSPACE_STACK_DESKTOP_CONFIG_FILE
if [[ "$(uname -s)" == Darwin ]]; then
    make -s stack-desktop-config >"$fixture/native"
    grep -Fxq "Created IRC config at $HOME/Library/Application Support/subspace/irc/config.json" "$fixture/native"
    mkdir -p "$HOME/.config/subspace"
    echo '{}' >"$HOME/.config/subspace/config.json"
    make -s stack-desktop-config >"$fixture/cross-platform"
    grep -Fxq "Created IRC config at $HOME/.config/subspace/irc/config.json" "$fixture/cross-platform"
else
    make -s stack-desktop-config >"$fixture/default"
    grep -Fxq "Created IRC config at $HOME/.config/subspace/irc/config.json" "$fixture/default"
    XDG_CONFIG_HOME="$fixture/xdg" make -s stack-desktop-config >"$fixture/xdg-output"
    grep -Fxq "Created IRC config at $fixture/xdg/subspace/irc/config.json" "$fixture/xdg-output"
fi

for setting in environment make-variable; do
    : >"$DOCKER_LOG"
    if [ "$setting" = environment ]; then
        SUBSPACE_STACK_DESKTOP_CONFIG=0 SUBSPACE_STACK_DEMO_DATA=0 make -s stack-up >/dev/null
    else
        SUBSPACE_STACK_DESKTOP_CONFIG=0 SUBSPACE_STACK_DEMO_DATA=1 make -s stack-up DEMO=0 >/dev/null
    fi
    grep -Eq '^compose exec .* seed:demo --workspace-only$' "$DOCKER_LOG"
    test "$(wc -l <"$DOCKER_LOG" | tr -d ' ')" = 2
done

: >"$DOCKER_LOG"
SUBSPACE_STACK_DEMO_DATA=0 make -s stack-demo >/dev/null
grep -Eq '^compose exec .* seed:demo --with-irc$' "$DOCKER_LOG"
grep -Fxq 'compose exec -T irc python3 /run/subspace-stack/seed-irc.py' "$DOCKER_LOG"
test "$(wc -l <"$DOCKER_LOG" | tr -d ' ')" = 2

: >"$DOCKER_LOG"
if make -s stack-up DEMO=invalid >"$fixture/invalid-demo" 2>&1; then
    echo "expected an invalid demo switch to fail" >&2
    exit 1
fi
grep -Fq 'SUBSPACE_STACK_DEMO_DATA must be 0 or 1' "$fixture/invalid-demo"
test ! -s "$DOCKER_LOG"

echo "local-stack fixtures passed"
