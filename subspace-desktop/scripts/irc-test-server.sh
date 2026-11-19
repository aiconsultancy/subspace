#!/usr/bin/env bash
# Runs a local Ergo IRC server for the IRC client's integration test, in the
# foreground: plaintext on 127.0.0.1:16667 (loopback only), in-memory
# history, and a fresh account database under .cache/ergo/run.
#
#   scripts/irc-test-server.sh &
#   IRC_TEST_SERVER=127.0.0.1:16667 cargo test -p subspace-client-irc --test ergo
#
# Uses $ERGO_BIN (an `ergo` binary inside an unpacked Ergo release) when set,
# otherwise downloads and verifies Ergo $VERSION into .cache/ergo.
#
# IRC_TEST_CHANNELS="#design #random" keeps those fictional channels open,
# each with a topic, through a client named fixture-bot, so the Channels
# Space's channel list has something to show:
#
#   IRC_TEST_CHANNELS="#design #random #releases" scripts/irc-test-server.sh &
set -euo pipefail

VERSION=2.19.1
# IRC_TEST_LISTEN picks another loopback port, such as when another checkout
# on the same computer already runs this server. Only 127.0.0.1 is accepted.
LISTEN="${IRC_TEST_LISTEN:-127.0.0.1:16667}"
case "$LISTEN" in
  127.0.0.1:[0-9]*) ;;
  *) echo "irc-test-server: IRC_TEST_LISTEN must be 127.0.0.1:<port>, not $LISTEN" >&2; exit 2 ;;
esac
root="$(cd "$(dirname "$0")/.." && pwd)"
cache="$root/.cache/ergo"

platform() {
  local os arch
  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    *) echo "irc-test-server: unsupported OS $(uname -s); set ERGO_BIN" >&2; exit 1 ;;
  esac
  case "$(uname -m)" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64 | amd64) arch=x86_64 ;;
    *) echo "irc-test-server: unsupported CPU $(uname -m); set ERGO_BIN" >&2; exit 1 ;;
  esac
  echo "$os-$arch"
}

download() {
  local name="ergo-$VERSION-$(platform)"
  local dist="$cache/$name"
  if [[ ! -x "$dist/ergo" ]]; then
    local base="https://github.com/ergochat/ergo/releases/download/v$VERSION"
    local archive="$cache/$name.tar.gz"
    mkdir -p "$cache"
    curl -fsSL "$base/$name.tar.gz" -o "$archive"
    curl -fsSL "$base/ergo-$VERSION-checksums.txt" -o "$cache/checksums.txt"
    (cd "$cache" && grep " $name.tar.gz\$" checksums.txt | shasum -a 256 -c - >&2)
    tar -xzf "$archive" -C "$cache"
    rm -f "$archive"
  fi
  echo "$dist/ergo"
}

bin="${ERGO_BIN:-$(download)}"
dist="$(cd "$(dirname "$bin")" && pwd)"
if [[ ! -f "$dist/default.yaml" ]]; then
  echo "irc-test-server: $dist has no default.yaml; ERGO_BIN must be inside an Ergo release" >&2
  exit 1
fi

run="$cache/run"
rm -rf "$run"
mkdir -p "$run"
# The release's default config with only the loopback plaintext listener
# (no TLS certificate needed) and the database under .cache.
sed \
  -e "s|^        \"127.0.0.1:6667\":.*\$|        \"$LISTEN\": {}|" \
  -e '/^        "\[::1\]:6667":/d' \
  -e '/^        ":6697":/,/min-tls-version: 1.2/d' \
  -e "s|^    path: ircd.db\$|    path: $run/ircd.db|" \
  "$dist/default.yaml" > "$run/ergo.yaml"

echo "irc-test-server: Ergo $("$bin" version 2>/dev/null || echo "$VERSION") on $LISTEN; set IRC_TEST_SERVER=$LISTEN" >&2
# Keeps the IRC_TEST_CHANNELS open: registers fixture-bot, joins each with
# a topic, then answers PING until the server goes.
seed_channels() {
  local host="${LISTEN%:*}" port="${LISTEN#*:}" line channel
  for _ in $(seq 100); do
    if exec 3<>"/dev/tcp/$host/$port"; then break; fi 2>/dev/null
    sleep 0.1
  done
  printf 'NICK fixture-bot\r\nUSER fixture 0 * :Fixture bot\r\n' >&3
  while IFS= read -r line <&3; do
    line="${line%$'\r'}"
    case "$line" in
      PING*) printf 'PONG %s\r\n' "${line#PING }" >&3 ;;
      *" 001 "*)
        for channel in $IRC_TEST_CHANNELS; do
          printf 'JOIN %s\r\nTOPIC %s :Fictional %s channel for screenshots\r\n' \
            "$channel" "$channel" "$channel" >&3
        done
        ;;
    esac
  done
}

cd "$dist"
if [[ -z "${IRC_TEST_CHANNELS:-}" ]]; then
  exec "$bin" run --conf "$run/ergo.yaml"
fi
"$bin" run --conf "$run/ergo.yaml" &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT
seed_channels
wait "$server"
