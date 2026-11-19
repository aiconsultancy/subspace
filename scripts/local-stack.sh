#!/usr/bin/env bash
# The whole system on this machine: PostgreSQL, the API and Ergo IRC from the
# root compose.yaml, with optional demo data. Run through the root Makefile.
#
# Usage: scripts/local-stack.sh <command>
#   up                    generate credentials, start, seed, configure Channels
#   demo                  add or top up demo data and IRC history
#   token                 print a development bearer token for the API
#   desktop-irc-config    print a desktop irc/config.json for the local IRC
#   stack-desktop-config  install the local IRC networks in the desktop config
#   reset                 remove containers, volumes and generated credentials
#
# Generated IRC passwords live in .stack/irc/ (owner-only folder, ignored by
# Git). The desktop reads its SASL password from there through passwordFile.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
stack="$root/.stack"
irc="$stack/irc"
email=developer@example.com
organisation=local
# The desktop user and the fleets' rear admirals and captains.
accounts=(developer aurora-admiral aurora-lyra aurora-orion borealis-admiral borealis-rigel)

die() { echo "local-stack: $*" >&2; exit 1; }

require_python() {
    python3 -c 'import sys; sys.exit(sys.version_info < (3, 12))' 2>/dev/null ||
        die "Python 3.12 or newer is required to configure Channels. Put it first on PATH."
}

secrets() {
    command -v openssl >/dev/null || die "openssl is required to generate local passwords"
    (umask 077; mkdir -p "$irc")
    chmod 700 "$stack" "$irc"
    local name file
    for name in local-operator "${accounts[@]}"; do
        file="$irc/$name.password"
        [ -s "$file" ] || (umask 077; openssl rand -hex 24 >"$file")
    done
    # Ergo's UID 10001 needs to read these two bind mounts. Their host
    # directory stays owner-only, as do the individual password files.
    cp "$irc/local-operator.password" "$irc/operator.seed"
    {
        printf '['
        local separator=""
        for name in "${accounts[@]}"; do
            printf '%s{"name":"%s","password":"%s"}' "$separator" "$name" "$(cat "$irc/$name.password")"
            separator=","
        done
        printf ']\n'
    } >"$irc/accounts.json"
    chmod 644 "$irc/accounts.json" "$irc/operator.seed"
}

seed_api() {
    docker compose exec -T \
        -e SEED_ROOT_STACK=1 \
        -e SEED_IRC_PLAIN_PORT="${IRC_PLAIN_PORT:-6667}" \
        -e SEED_IRC_TLS_PORT="${IRC_TLS_PORT:-6697}" \
        -e SEED_EMAIL="$email" -e SEED_ORGANISATION="$organisation" \
        api bun run --silent seed:demo "$@"
}

seed_demo() {
    local transcript
    transcript="$(seed_api --with-irc)"
    docker compose exec -T irc python3 /run/subspace-stack/seed-irc.py <<<"$transcript"
}

desktop_irc_config() {
    [ -s "$irc/developer.password" ] || die "run 'make stack-up' first"
    python3 "$root/scripts/local-stack/desktop-irc-config.py" "$irc/developer.password" "$@"
}

install_desktop_irc_config() {
    [ "${SUBSPACE_STACK_DESKTOP_CONFIG:-1}" != 0 ] || return 0
    desktop_irc_config --install
}

case "${1:-}" in
    up)
        demo="${SUBSPACE_STACK_DEMO_DATA:-1}"
        [[ "$demo" == 0 || "$demo" == 1 ]] || die "SUBSPACE_STACK_DEMO_DATA must be 0 or 1"
        if [ "${SUBSPACE_STACK_DESKTOP_CONFIG:-1}" != 0 ]; then require_python; fi
        secrets
        docker compose up --build --detach --wait
        # Older stacks mounted operator.password directly. Tighten old files
        # once Compose has switched the container to the new seed mount.
        chmod 600 "$irc/"*.password
        if [ "$demo" = 0 ]; then seed_api --workspace-only; else seed_demo; fi
        install_desktop_irc_config
        echo "API: http://127.0.0.1:${API_PORT:-3000}  IRC: 127.0.0.1:${IRC_PLAIN_PORT:-6667} (TLS ${IRC_TLS_PORT:-6697})"
        ;;
    demo|seed) seed_demo ;;
    token) docker compose exec -T api bun run --silent auth:dev "$email" "$organisation" ;;
    desktop-irc-config) require_python; desktop_irc_config ;;
    stack-desktop-config)
        if [ "${SUBSPACE_STACK_DESKTOP_CONFIG:-1}" != 0 ]; then require_python; fi
        install_desktop_irc_config
        ;;
    reset)
        docker compose down --volumes --remove-orphans
        rm -rf "$stack"
        ;;
    *) sed -n '5,11p' "$0" >&2; exit 2 ;;
esac
