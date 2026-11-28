#!/usr/bin/env bash
# Standalone so a provider's management channel can send this file without a checkout.
set -euo pipefail
umask 077

usage() {
    cat <<'USAGE'
Usage: ./install.sh headscale [--dry-run] HEADSCALE_FQDN PUBLIC_IPV4 API_IPV4 IRC_IPV4

Set up a dedicated Ubuntu 24.04 Headscale VM via the provider's management
channel. The host firewall allows public TCP 443 and closes SSH. API_IPV4
and IRC_IPV4 are the private IPv4 addresses that the fleet router can reach.
USAGE
}

die() { printf 'headscale setup: %s\n' "$*" >&2; exit 1; }

dry_run=false
case ${1:-} in
    --help|-h) usage; exit 0 ;;
    --dry-run) dry_run=true; shift ;;
esac
[[ $# == 4 ]] || { usage >&2; exit 2; }
hostname=$1
public_ip=$2
api_ip=$3
irc_ip=$4

validate_ipv4() {
    local address=$1 octet
    local -a octets
    [[ $address =~ ^(0|[1-9][0-9]{0,2})(\.(0|[1-9][0-9]{0,2})){3}$ ]] || die "invalid IPv4 address: $address"
    IFS=. read -r -a octets <<<"$address"
    for octet in "${octets[@]}"; do
        (( octet <= 255 )) || die "invalid IPv4 address: $address"
    done
}

[[ ${#hostname} -le 253 && $hostname =~ ^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]([a-z0-9-]{0,61}[a-z0-9])?$ ]] ||
    die 'use a fully qualified lowercase DNS name'
for address in "$public_ip" "$api_ip" "$irc_ip"; do
    validate_ipv4 "$address"
done

private_ipv4() {
    [[ $1 =~ ^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.) ]]
}
if ! private_ipv4 "$api_ip" || ! private_ipv4 "$irc_ip"; then
    die 'API and IRC must use private network addresses'
fi
if private_ipv4 "$public_ip" || [[ $public_ip =~ ^(0\.|127\.|169\.254\.) ]]; then
    die 'the coordinator requires a public IPv4 address'
fi

version=0.29.8
if $dry_run; then
    printf '%s\n' \
        "Coordinator: https://$hostname at $public_ip, TCP 443 only" \
        "Headscale $version with verified-member DERP" \
        "Workstation access: TCP $api_ip:443 and $irc_ip:6697 only" \
        'Ubuntu package upgrades, unattended security updates, default-drop host firewall and no SSH ingress'
    exit 0
fi

[[ $EUID == 0 ]] || die 'run as root on the dedicated VM through its provider management channel'
# shellcheck disable=SC1091
. /etc/os-release
[[ $ID == ubuntu && $VERSION_ID == 24.04 ]] || die 'Ubuntu 24.04 LTS is required'
case $(dpkg --print-architecture) in
    amd64) checksum=4f55dc066924bdc234a4b300c7330df4fbd89d42ba180176b6335d4a3c19759b ;;
    arm64) checksum=0b02e6f23e0fc3cbe679f9fd3abf9933f6c579e1cb7fca94e2d921d15948b755 ;;
    *) die 'only amd64 and arm64 are supported' ;;
esac
arch=$(dpkg --print-architecture)

exec 9>/run/subspace-headscale-setup.lock
flock -n 9 || die 'another Headscale setup is running'
staging=$(mktemp -d)
trap 'rm -r -- "$staging"' EXIT

state=/var/lib/subspace-headscale
config=/etc/subspace-headscale
settings=$config/install-settings
printf '%s\n' "$hostname" "$public_ip" "$api_ip" "$irc_ip" >"$staging/install-settings"
if [[ -f $settings ]]; then
    cmp -s "$staging/install-settings" "$settings" ||
        die 'this coordinator is already configured with a different endpoint or routes'
elif [[ -e $state || -e $config/config.yaml || -e /etc/systemd/system/subspace-headscale.service ]]; then
    die 'an unmanaged Headscale installation already uses these paths'
fi

export DEBIAN_FRONTEND=noninteractive
apt_get() { apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Options::=--force-confold "$@"; }
apt_get update
apt_get install -y ca-certificates curl jq nftables unattended-upgrades
apt_get upgrade --with-new-pkgs -y
install -m 0644 /dev/stdin /etc/apt/apt.conf.d/51subspace-headscale-upgrades <<'APT'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT
systemctl enable --now apt-daily.timer apt-daily-upgrade.timer

if [[ ! -f $settings ]]; then
    install -d -m 0750 "$config"
    install -m 0600 "$staging/install-settings" "$settings"
fi

# Own one table. Loading the rules atomically preserves any provider-managed rules.
install -m 0600 /dev/stdin "$config/firewall.nft" <<'NFT'
add table inet subspace_headscale
flush table inet subspace_headscale
table inet subspace_headscale {
    chain input {
        type filter hook input priority 10; policy drop;
        ct state invalid drop
        ct state established,related accept
        iifname "lo" accept
        ip protocol icmp accept
        ip6 nexthdr ipv6-icmp accept
        udp sport 67 udp dport 68 accept
        meta nfproto ipv4 tcp dport 443 accept
    }
    chain forward {
        type filter hook forward priority 10; policy drop;
    }
}
NFT
nft --check --file "$config/firewall.nft"
install -m 0644 /dev/stdin /etc/systemd/system/subspace-headscale-firewall.service <<'UNIT'
[Unit]
Description=Headscale coordinator host firewall
After=nftables.service ufw.service firewalld.service

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/sbin/nft --file /etc/subspace-headscale/firewall.nft

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable subspace-headscale-firewall.service
systemctl restart subspace-headscale-firewall.service
for unit in ssh.socket ssh.service; do
    if [[ $(systemctl show "$unit" --property=LoadState --value) == loaded ]]; then
        systemctl disable --now "$unit"
    fi
done

if ! getent passwd subspace-headscale >/dev/null; then
    useradd --system --user-group --home-dir "$state" --shell /usr/sbin/nologin subspace-headscale
fi
install -d -m 0700 -o subspace-headscale -g subspace-headscale "$state"
install -d -m 0750 -o root -g subspace-headscale "$config"

asset=headscale_${version}_linux_$arch
curl --fail --silent --show-error --location --retry 3 --max-time 180 \
    "https://github.com/juanfont/headscale/releases/download/v$version/$asset" -o "$staging/headscale"
printf '%s  %s\n' "$checksum" "$staging/headscale" | sha256sum --check --status ||
    die 'Headscale release checksum mismatch'

route_approvers() {
    printf '    "%s/32": ["tag:router"]' "$api_ip"
    if [[ $api_ip != "$irc_ip" ]]; then
        printf ',\n    "%s/32": ["tag:router"]' "$irc_ip"
    fi
}
# Account sign-in remains on the API and IRC. Give enrolled workstations only their service ports.
install -m 0640 -g subspace-headscale /dev/stdin "$config/policy.hujson" <<POLICY
{
  "tagOwners": {"tag:router": []},
  "autoApprovers": {"routes": {
$(route_approvers)
  }},
  "grants": [
    {"src": ["workstation@"], "dst": ["$api_ip/32"], "ip": ["tcp:443"]},
    {"src": ["workstation@"], "dst": ["$irc_ip/32"], "ip": ["tcp:6697"]}
  ],
  "nodeAttrs": [{"target": ["*"], "attr": ["disable-captive-portal-detection", "debug-disable-upnp"]}]
}
POLICY

install -m 0640 -g subspace-headscale /dev/stdin "$config/config.yaml" <<CONFIG
server_url: https://$hostname
listen_addr: 0.0.0.0:443
metrics_listen_addr: ""
grpc_listen_addr: 127.0.0.1:50443
grpc_allow_insecure: false
tls_letsencrypt_hostname: $hostname
tls_letsencrypt_challenge_type: TLS-ALPN-01
tls_letsencrypt_cache_dir: $state/acme
noise:
  private_key_path: $state/noise.key
prefixes:
  v4: 100.64.0.0/10
  v6: fd7a:115c:a1e0::/48
  allocation: sequential
derp:
  server:
    enabled: true
    region_id: 999
    region_code: subspace
    region_name: Subspace Headscale Relay
    verify_clients: true
    stun_listen_addr: "127.0.0.1:13478"
    private_key_path: $state/derp.key
    automatically_add_embedded_derp_region: true
    ipv4: $public_ip
    ipv6: none
  urls: []
  paths: []
  auto_update_enabled: false
disable_check_updates: true
database:
  type: sqlite
  sqlite:
    path: $state/state.sqlite
    write_ahead_log: true
policy:
  mode: file
  path: $config/policy.hujson
dns:
  magic_dns: false
  base_domain: ""
  override_local_dns: false
unix_socket: $state/headscale.sock
unix_socket_permission: "0600"
logtail:
  enabled: false
taildrop:
  enabled: false
CONFIG

# configtest opens a SQLite database. Keep its writes away from live state.
install -d -m 0700 -o subspace-headscale -g subspace-headscale "$staging/state"
sed "s|$state/|$staging/state/|g" "$config/config.yaml" >"$staging/test-config.yaml"
chgrp subspace-headscale "$staging" "$staging/test-config.yaml"
chmod 0750 "$staging"
chmod 0640 "$staging/test-config.yaml"
chmod 0755 "$staging/headscale"
runuser -u subspace-headscale -- "$staging/headscale" --config "$staging/test-config.yaml" configtest
install -m 0755 "$staging/headscale" /usr/local/bin/.subspace-headscale.new
mv -f /usr/local/bin/.subspace-headscale.new /usr/local/bin/subspace-headscale

install -m 0644 /dev/stdin /etc/systemd/system/subspace-headscale.service <<'UNIT'
[Unit]
Description=Headscale coordination and relay
Requires=subspace-headscale-firewall.service
Wants=network-online.target
After=network-online.target subspace-headscale-firewall.service

[Service]
User=subspace-headscale
Group=subspace-headscale
ExecStart=/usr/local/bin/subspace-headscale --config /etc/subspace-headscale/config.yaml serve
Restart=on-failure
RestartSec=5
UMask=0077
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectHome=true
ProtectSystem=strict
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictNamespaces=true
LockPersonality=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK
IPAddressDeny=169.254.169.254/32 fd00:ec2::254/128
ReadWritePaths=/var/lib/subspace-headscale

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable subspace-headscale.service
systemctl restart subspace-headscale.service
systemctl is-active subspace-headscale.service
printf 'Verify trusted TLS from another machine with: curl --fail https://%s/health\n' "$hostname"
if [[ -f /run/reboot-required ]]; then
    printf '%s\n' 'Ubuntu requires a reboot to finish applying updates.'
fi
