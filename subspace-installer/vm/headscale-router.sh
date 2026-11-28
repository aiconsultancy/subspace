#!/usr/bin/env bash
# Run on the private fleet host or on a private VM that can reach its services.
set -euo pipefail
umask 077

usage() {
    cat <<'USAGE'
Usage: ./install.sh headscale-router [--dry-run] HEADSCALE_FQDN API_IPV4 IRC_IPV4 [AUTH_KEY_FILE]

Join the private VM to Headscale and advertise the API and IRC host routes.
On first install, supply a root-owned mode-0600 file with a one-use router
key. Reuse the same endpoint and routes when updating an enrolled router.
USAGE
}

die() { printf 'headscale router setup: %s\n' "$*" >&2; exit 1; }

dry_run=false
case ${1:-} in
    --help|-h) usage; exit 0 ;;
    --dry-run) dry_run=true; shift ;;
esac
[[ $# == 3 || $# == 4 ]] || { usage >&2; exit 2; }
hostname=$1
api_ip=$2
irc_ip=$3
key_file=${4:-}

[[ ${#hostname} -le 253 && $hostname =~ ^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]([a-z0-9-]{0,61}[a-z0-9])?$ ]] ||
    die 'use a fully qualified lowercase DNS name'
for address in "$api_ip" "$irc_ip"; do
    [[ $address =~ ^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.)(0|[1-9][0-9]{0,2})(\.(0|[1-9][0-9]{0,2})){1,2}$ ]] ||
        die 'use private IPv4 addresses for the API and IRC'
    IFS=. read -r -a octets <<<"$address"
    [[ ${#octets[@]} == 4 ]] || die "invalid IPv4 address: $address"
    for octet in "${octets[@]}"; do
        (( octet <= 255 )) || die "invalid IPv4 address: $address"
    done
done

routes=$api_ip/32
if [[ $api_ip != "$irc_ip" ]]; then routes+=,$irc_ip/32; fi
if $dry_run; then
    printf '%s\n' "Router for https://$hostname via userspace networking" "Advertised routes: $routes"
    exit 0
fi

[[ $EUID == 0 ]] || die 'run as root on the private router'
for dependency in curl tar sha256sum systemctl; do
    command -v "$dependency" >/dev/null || die "required command is missing: $dependency"
done
arch=$(uname -m)
case $arch in
    x86_64) arch=amd64; checksum=e56693ff74a64f2591b7e819c280635a100aae1ddfb0ef058fcd052b7cd8d16c ;;
    aarch64) arch=arm64; checksum=d6bed2720310b328b861f943d7b4016c5358a9bad60575eb51e8143b79068484 ;;
    *) die 'only amd64 and arm64 are supported' ;;
esac
if [[ -n $key_file ]]; then
    [[ -f $key_file && ! -L $key_file && $(stat -c '%u:%a' "$key_file") == 0:600 ]] ||
        die 'the router key must be a root-owned mode-0600 regular file'
fi

exec 9>/run/subspace-headscale-router-setup.lock
flock -n 9 || die 'another router setup is running'
staging=$(mktemp -d)
trap 'rm -r -- "$staging"' EXIT
dir=/opt/subspace-headscale-router
state=/var/lib/subspace-headscale-router
socket=/run/subspace-headscale-router/tailscaled.sock
service=subspace-headscale-router.service
settings=$dir/install-settings
printf '%s\n' "$hostname" "$routes" >"$staging/install-settings"
if [[ -f $settings ]]; then
    cmp -s "$staging/install-settings" "$settings" ||
        die 'this router is already configured with a different endpoint or routes'
elif [[ -z $key_file ]]; then
    die 'a one-use auth key file is required for the first install'
fi
curl --fail --silent --show-error --max-time 20 "https://$hostname/health" >/dev/null

version=1.104.7
asset=tailscale_${version}_$arch
curl --fail --silent --show-error --location --retry 3 --max-time 180 \
    "https://pkgs.tailscale.com/stable/$asset.tgz" -o "$staging/tailscale.tgz"
printf '%s  %s\n' "$checksum" "$staging/tailscale.tgz" | sha256sum --check --status ||
    die 'Tailscale release checksum mismatch'
tar -xzf "$staging/tailscale.tgz" -C "$staging" "$asset/tailscale" "$asset/tailscaled"

if ! getent passwd subspace-headscale-router >/dev/null; then
    useradd --system --user-group --home-dir "$state" --shell /usr/sbin/nologin subspace-headscale-router
fi
install -d -m 0755 "$dir"
install -m 0600 "$staging/install-settings" "$settings"
for binary in tailscale tailscaled; do
    install -m 0755 "$staging/$asset/$binary" "$dir/.$binary.new"
    mv -f "$dir/.$binary.new" "$dir/$binary"
done
install -m 0644 /dev/stdin "/etc/systemd/system/$service" <<'UNIT'
[Unit]
Description=Private Headscale router for Subspace API and IRC
After=network-online.target
Wants=network-online.target

[Service]
User=subspace-headscale-router
Group=subspace-headscale-router
StateDirectory=subspace-headscale-router
StateDirectoryMode=0700
RuntimeDirectory=subspace-headscale-router
RuntimeDirectoryMode=0700
Environment=TS_NO_LOGS_NO_SUPPORT=true TS_FORCE_NOISE_443=true TS_DISABLE_PORTMAPPER=1 TS_DEBUG_ALWAYS_USE_DERP=true
ExecStart=/opt/subspace-headscale-router/tailscaled --tun=userspace-networking --statedir=/var/lib/subspace-headscale-router --socket=/run/subspace-headscale-router/tailscaled.sock --port=0 --no-logs-no-support
Restart=on-failure
RestartSec=5
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectHome=true
ProtectSystem=strict
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
CapabilityBoundingSet=

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable "$service"
systemctl restart "$service"
for _ in {1..50}; do
    [[ ! -S $socket ]] || break
    sleep 0.2
done
[[ -S $socket ]] || die 'the router did not open its local control socket'
join_args=()
if [[ -n $key_file ]]; then join_args=(--auth-key="file:$key_file"); fi
"$dir/tailscale" --socket="$socket" up --login-server="https://$hostname" \
    --hostname="$(hostname -s)" --advertise-routes="$routes" --accept-dns=false --timeout=30s "${join_args[@]}"
