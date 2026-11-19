#!/usr/bin/env bash
# Exercise the root stack commands in a fixture checkout with a fake Docker.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
temp_root="${TMPDIR:-/tmp}"
fixture="$(mktemp -d "${temp_root%/}/subspace-local-stack.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT

checkout="$fixture/checkout with spaces"
mkdir -p "$checkout/scripts/local-stack" "$fixture/bin" "$fixture/home"
cp "$root/Makefile" "$checkout/"
cp "$root/scripts/local-stack.sh" "$checkout/scripts/"
cp "$root/scripts/local-stack/desktop-irc-config.py" "$checkout/scripts/local-stack/"
cd "$checkout"

export PATH="$fixture/bin:$PATH" HOME="$fixture/home" DOCKER_LOG="$fixture/docker.log"
export SUBSPACE_STACK_DESKTOP_CONFIG_FILE="$fixture/settings/irc/config.json"
unset SUBSPACE_STACK_DESKTOP_CONFIG XDG_CONFIG_HOME
export IRC_PLAIN_PORT=16667 IRC_TLS_PORT=16697
config="$SUBSPACE_STACK_DESKTOP_CONFIG_FILE"
cat >"$fixture/bin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [ "${EXPECT_MISSING_CONFIG:-0}" = 1 ]; then
    test ! -e "$SUBSPACE_STACK_DESKTOP_CONFIG_FILE"
fi
printf '%s %s\n' "$1" "$2" >>"$DOCKER_LOG"
SH
chmod 755 "$fixture/bin/docker"

EXPECT_MISSING_CONFIG=1 make -s stack-up >"$fixture/up"
grep -Fxq "Created IRC config at $config" "$fixture/up"
diff -u <(printf 'compose up\ncompose exec\n') "$DOCKER_LOG"
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
     "sasl": {"username": "old", "password": "fixture", "external": true, "custom": true}}
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
diff -u <(printf 'compose up\ncompose exec\n') "$DOCKER_LOG"
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
make -s stack-up >"$fixture/restart"
grep -Fxq "Updated IRC config at $config" "$fixture/restart"
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

echo "local-stack fixtures passed"
