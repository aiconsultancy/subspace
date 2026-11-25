#!/usr/bin/env bash
# subspace-setup: set up and look after a Subspace fleet VM. Run as root.
#
#   subspace-setup [setup] [options]   set up or update this fleet (see --help)
#   subspace-setup irc-server ...      run a shared IRC server on this host instead
#   subspace-setup providers [login P] model provider accounts (agents need one)
#   subspace-setup status              summary, service state and memory
#   subspace-setup audit               check that the VM is private
#   subspace-setup lockdown apply|confirm|rollback|status
#   subspace-setup secret irc-admin    print your IRC password (local IRC)
#   subspace-setup token               print a development API token for the admin
#   subspace-setup stargate-token      print the Stargate Fleet API token
#   subspace-setup irc-bundle          print this fleet's accounts for a shared IRC server
#   subspace-setup fleet-registration  print this fleet's details for another fleet's API
#   subspace-setup register-fleet -    register a further fleet (from stdin) with this VM's API
#   subspace-setup rename-officers     give an installed fleet's officers plain names (--help)
#   subspace-setup toolchains [detect] the recorded toolchains, or a proposal from the repositories
#   subspace-setup demo                seed the fictional demo data again
#
# The host installer (install.sh or install.ps1 at the repository root) copies
# this payload to /opt/subspace/bootstrap and links this script as
# /usr/local/sbin/subspace-setup.
set -euo pipefail
umask 077
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

here=$(cd -- "$(dirname -- "$(realpath -- "${BASH_SOURCE[0]}")")" && pwd)
die() { printf 'subspace-setup: %s\n' "$*" >&2; exit 1; }

(( EUID == 0 )) || die 'Run with sudo.'
# shellcheck disable=SC1091
. /etc/os-release
case "${ID:-}:${VERSION_ID:-}" in
    ubuntu:24.04|debian:12) ;;
    *) die "Use Ubuntu 24.04 or Debian 12 (this is ${PRETTY_NAME:-unknown})." ;;
esac

need=()
command -v python3 >/dev/null || need+=(python3)
command -v git >/dev/null || need+=(git)
command -v curl >/dev/null || need+=(curl ca-certificates)
command -v nft >/dev/null || need+=(nftables)
dry_run=false
for arg in "$@"; do [[ $arg == --dry-run ]] && dry_run=true; done
if (( ${#need[@]} )); then
    # Setup cannot run, not even a dry run, without these; the one thing a dry run may change on the VM.
    "$dry_run" && printf 'subspace-setup: the dry run first installs %s, which setup needs to run.\n' "${need[*]}"
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${need[@]}"
fi
# A dry run may run from a copy that is removed afterwards: the command keeps naming the installed one.
if ! "$dry_run" && [[ $(readlink /usr/local/sbin/subspace-setup 2>/dev/null) != "$here/setup.sh" ]]; then
    ln -sfnT "$here/setup.sh" /usr/local/sbin/subspace-setup
fi

command=${1:-setup}
case $command in
    irc-server) shift; exec python3 -B "$here/irc_server.py" "$@" ;;
    lockdown) shift; exec "$here/lockdown.sh" "$@" ;;
    providers) shift; exec "$here/providers.sh" "$@" ;;
    audit) exec "$here/audit.sh" ;;
    stargate-token) exec /usr/local/bin/subspace-fleetctl api-token ;;
    status)
        if [[ -f /root/fleet/summary.env ]]; then cat /root/fleet/summary.env; fi
        if [[ -f /root/irc/summary.env ]]; then cat /root/irc/summary.env; fi
        systemctl --no-pager --plain list-units 'subspace-*' || true
        # Each engine's live memory limits and use, swap, and fleet processes killed for memory since boot.
        if [[ -x /usr/local/bin/subspace-fleetctl ]]; then /usr/local/bin/subspace-fleetctl memory || true; fi
        "$here/lockdown.sh" status || true
        ;;
    -h|--help|help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exec python3 -B "$here/fleet_setup.py" --help ;;
    *) exec python3 -B "$here/fleet_setup.py" "$@" ;;
esac
