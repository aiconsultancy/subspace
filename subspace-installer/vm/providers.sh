#!/usr/bin/env bash
# Sign the fleet's model proxy (CLIProxyAPI) in to your agentic provider
# accounts. The agents stay offline until it has at least one account.
#
# Usage:
#   providers.sh [status]          list the signed-in accounts (no secrets shown)
#   providers.sh login PROVIDER    sign in an account, then restart the proxy
#
# Providers: codex-device (Codex, device code: no callback needed), codex,
# claude, antigravity, kimi, kimi-ai, xai, devin, meta.
#
# claude and codex finish in your browser and call back to port 54545 (Claude)
# or 1455 (Codex) on the machine running the browser. From your computer, use
# `./install.sh providers login claude`, which forwards that port for you.
set -euo pipefail
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

proxy=/opt/subspace-fleet/bin/cli-proxy-api
config=/etc/subspace-fleet/proxy.json
auth=/var/lib/subspace-fleet/proxy/auth
user=ss-fleet-proxy

die() { printf 'providers: %s\n' "$*" >&2; exit 1; }
(( EUID == 0 )) || die 'Run as root (sudo).'
[[ -x $proxy && -f $config ]] || die 'The fleet is not installed yet. Run subspace-setup first.'

status() {
    local -a files=()
    local file name
    shopt -s nullglob
    files=("$auth"/*.json)
    if (( ${#files[@]} == 0 )); then
        printf 'No provider accounts yet. The agents stay offline until you sign one in:\n'
        printf '  ./install.sh providers login codex-device   (from your computer, .\\install.ps1 on Windows; or claude, codex)\n'
        printf '  sudo subspace-setup providers login codex-device   (on the VM)\n'
        return 1
    fi
    printf 'Signed-in provider accounts:\n'
    for file in "${files[@]}"; do
        name=${file##*/}
        printf '  %s\n' "${name%.json}"
    done
}

login() {
    local provider=${1:-} flag
    case $provider in
        codex-device) flag=--codex-device-login ;;
        codex|claude|antigravity|kimi|kimi-ai|xai|devin|meta) flag=--$provider-login ;;
        *) die "Choose a provider: codex-device, codex, claude, antigravity, kimi, kimi-ai, xai, devin or meta." ;;
    esac
    printf 'Open the link below in your browser and sign in with the account the agents should use.\n'
    sudo -H -u "$user" "$proxy" --config "$config" "$flag" --no-browser
    systemctl restart subspace-fleet-proxy.service
    printf 'Restarted the model proxy.\n'
    status || true
}

case ${1:-status} in
    status) status ;;
    login) shift; login "$@" ;;
    *) sed -n '2,15p' "${BASH_SOURCE[0]}" >&2; exit 2 ;;
esac
