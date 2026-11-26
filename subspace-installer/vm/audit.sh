#!/usr/bin/env bash
# Check that this fleet VM is private. Prints PASS, WARN or FAIL lines and
# exits 1 when anything FAILs. Read-only; safe to run at any time.
#
# Usage: audit.sh
set -uo pipefail
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

failures=0
pass() { printf 'PASS  %s\n' "$*"; }
warn() { printf 'WARN  %s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; failures=$((failures + 1)); }

(( EUID == 0 )) || { echo 'audit: run as root (sudo).' >&2; exit 2; }

private_address() {
    local address=${1#[}
    address=${address%]}
    case $address in
        127.*|::1|localhost) return 0 ;;
        10.*|192.168.*|169.254.*|fe80:*|fc*|fd*) return 0 ;;
        172.1[6-9].*|172.2[0-9].*|172.3[01].*) return 0 ;;
        100.6[4-9].*|100.[7-9][0-9].*|100.1[01][0-9].*|100.12[0-7].*) return 0 ;;
    esac
    return 1
}

loopback_address() {
    case ${1#[} in
        127.*|::1*|'::ffff:127.'*) return 0 ;;
    esac
    return 1
}

# 1. Listening sockets. Only SSH, DHCP and VPN agents may listen beyond loopback.
sshd_config=$(sshd -T 2>/dev/null) || sshd_config=
# SSH may listen where sshd is configured to, and where ssh.socket listens when socket-activated.
ssh_ports=$(awk '$1 == "port" { print $2 } $1 == "listenaddress" { n = split($2, part, ":"); print part[n] }' <<<"$sshd_config")
if systemctl is-active --quiet ssh.socket 2>/dev/null; then
    ssh_ports+=$'\n'$(systemctl show -p Listen --value ssh.socket 2>/dev/null | grep -oE '[0-9]+ \(Stream\)' | cut -d' ' -f1)
fi
listeners_ok=true
while read -r proto _state _recv _send local _peer process; do
    address=${local%:*}
    port=${local##*:}
    address=${address%\%*}
    loopback_address "$address" && continue
    name=$(sed -n 's/.*(("\([^"]*\)".*/\1/p' <<<"$process")
    case "$proto:$port:$name" in
        tcp:*:sshd|tcp:*:systemd)
            grep -Fxq "$port" <<<"${ssh_ports:-22}" && continue ;;  # Includes socket-activated SSH.
        udp:68:*|udp:546:*) continue ;;                  # DHCP clients
        *:*:tailscaled|*:*:cloudflared) continue ;;      # ZTNA agents
    esac
    if [[ $address == '*' || $address == 0.0.0.0 || $address == '[::]' || $address == '::' ]]; then
        fail "$name listens on every interface ($proto $local). Bind it to 127.0.0.1."
        listeners_ok=false
    elif private_address "$address"; then
        warn "$name listens on private address $proto $local. Make sure only your private network reaches it."
    else
        fail "$name listens on public address $proto $local."
        listeners_ok=false
    fi
done < <(ss -H -tulpn 2>/dev/null)
"$listeners_ok" && pass 'Services listen on loopback only (apart from SSH and private-network agents).'

# 2. Lockdown firewall.
if ruleset=$(nft list table inet subspace_lockdown 2>/dev/null); then
    if grep -q 'policy drop' <<<"$ruleset"; then
        pass 'Lockdown firewall is loaded and drops unsolicited inbound traffic.'
    else
        fail 'Lockdown firewall table exists but does not drop inbound traffic.'
    fi
else
    ruleset=
    fail 'Lockdown firewall is not loaded. Run: sudo subspace-setup lockdown apply --operator <your-login>'
fi
if [[ -f /etc/subspace-lockdown/pending.nft ]]; then
    warn "Lockdown change $(cat /etc/subspace-lockdown/pending.id 2>/dev/null) is pending. Confirm it from a new SSH session: sudo subspace-setup lockdown confirm"
fi
if [[ ! -f /etc/subspace-lockdown/rules.nft ]] || ! systemctl is-enabled --quiet subspace-lockdown.service 2>/dev/null; then
    warn 'Lockdown is not confirmed, so the firewall is not loaded at boot.'
fi
# Agent isolation: operator-only ports and the cloud metadata service (never rolled back).
isolation=$(nft list table inet subspace_isolation 2>/dev/null) || isolation=
metadata_rule=$(grep -E 'ip daddr 169\.254\.169\.254 .*skuid != .* reject' <<<"$isolation$ruleset" | head -n 1)
if [[ -n $metadata_rule ]]; then
    allowed='' agents=''
    for uid in $(sed -E 's/.*skuid != \{ ?([^}]*)\}.*/\1/' <<<"$metadata_rule" | tr -d ' ' | tr ',' ' '); do
        if [[ $uid =~ ^[0-9]+$ ]]; then uid=$(getent passwd "$uid" | cut -d: -f1 || echo "$uid"); fi
        allowed+="${allowed:+, }$uid"
        # Agents run as ss-<member>; only Stargate's shared gateway user may be exempt.
        if [[ $uid == ss-* && $uid != ss-fleet-gateway ]]; then agents+=" $uid"; fi
    done
    if [[ -n $agents ]] && grep -Eqs '"keyVault": *"[^"]+"' /root/fleet/answers.json; then
        fail "Agent user(s)$agents can reach the cloud metadata service and so the VM's Key Vault identity."
    elif [[ -n $agents ]]; then
        warn "Agent user(s)$agents can reach the cloud metadata service (allowed: $allowed)."
    else
        pass "Only these users can reach the cloud metadata service: $allowed."
    fi
elif grep -Eqs '"keyVault": *"[^"]+"' /root/fleet/answers.json; then
    fail "Agents can request the VM's managed-identity token and read the Key Vault secrets."
else
    warn 'Agents can reach the cloud metadata service, so a cloud identity on this VM is available to them.'
fi
if [[ -f /etc/subspace-fleet/fleet.json ]]; then
    if nft list table inet subspace_fleet >/dev/null 2>&1; then
        pass "Fleet service firewall for the configured loopback services is loaded."
    else
        fail 'Fleet service firewall (inet subspace_fleet) is not loaded. Re-run the installer.'
    fi
    hook=$(nft -n list chain inet subspace_fleet private_egress 2>/dev/null) || hook=
    egress=$(nft -n list chain inet subspace_fleet officer_destinations 2>/dev/null) || egress=
    egress_users=$(sed -nE 's/.*skuid *(\{ *)?([0-9, ]+)( *\})? jump officer_destinations.*/\2/p' <<<"$hook" | tr ',' ' ')
    if ! grep -q 'type filter hook output' <<<"$hook" ||
          ! grep -Eq 'ip daddr \{ 10\.0\.0\.0/8, 100\.64\.0\.0/10, 169\.254\.0\.0/16, 172\.16\.0\.0/12, 192\.168\.0\.0/16 \} reject' <<<"$egress" ||
          ! grep -Eq 'ip6 daddr \{ fc00::/7, fe80::/10 \} reject' <<<"$egress"; then
        fail 'Officer private network egress rules are missing. Re-run the installer.'
    elif officers=$(jq -er '(.rearAdmiral, .captains[]) | .id' /etc/subspace-fleet/fleet.json); then
        egress_missing=()
        while read -r person; do
            if ! uid=$(id -u -- "ss-$person" 2>/dev/null) || ! grep -qw "$uid" <<<"$egress_users"; then
                egress_missing+=("$person")
            fi
        done <<<"$officers"
        if (( ${#egress_missing[@]} )); then
            fail "Private egress restrictions are missing for: ${egress_missing[*]}."
        else
            pass 'Private network egress is restricted for each officer.'
        fi
    else
        fail 'Could not read the officer roster for the private egress audit.'
    fi
fi

# 3. SSH daemon.
if [[ -n $sshd_config ]]; then
    check_sshd() {
        local key=$1 want=$2 label=$3 value
        value=$(awk -v key="$key" '$1 == key { print $2; exit }' <<<"$sshd_config")
        if [[ $value == "$want" ]]; then pass "$label"; else fail "$label (sshd has $key ${value:-unset})"; fi
    }
    check_sshd passwordauthentication no 'SSH refuses passwords.'
    check_sshd kbdinteractiveauthentication no 'SSH refuses keyboard-interactive logins.'
    check_sshd permitrootlogin no 'SSH refuses root logins.'
    if grep -q '^allowusers ' <<<"$sshd_config"; then
        pass "SSH admits only: $(awk '$1 == "allowusers" { $1 = ""; print }' <<<"$sshd_config" | xargs)."
    else
        warn 'SSH has no AllowUsers list. Lockdown sets one to your operator login.'
    fi
    if grep -q '^allowtcpforwarding \(yes\|all\|remote\)' <<<"$sshd_config"; then
        warn 'SSH allows remote port forwarding. Lockdown limits it to local forwarding.'
    fi
else
    warn 'Could not read the effective sshd configuration.'
fi

# 4. Docker must publish on loopback only.
if command -v docker >/dev/null && docker info >/dev/null 2>&1; then
    exposed=$(docker ps --format '{{.Names}} {{.Ports}}' | grep -E '(0\.0\.0\.0|\[::\]|:::)[0-9]*:' || true)
    if [[ -n $exposed ]]; then
        while read -r line; do fail "Container publishes beyond loopback: $line"; done <<<"$exposed"
    else
        pass 'Containers publish ports on loopback (or a private address) only.'
    fi
fi

# 5. Secret files.
secrets_ok=true
check_private() {
    local path=$1 mode owner
    [[ -e $path ]] || return 0
    mode=$(stat -c %a -- "$path")
    owner=$(stat -c %U -- "$path")
    if [[ $owner != root ]] || (( 8#$mode & 8#077 )); then
        fail "$path is $owner mode $mode; it must be root-owned and private."
        secrets_ok=false
    fi
}
check_private /root/fleet
for path in /root/fleet/*.json /root/fleet/*.env /root/fleet/*.yaml /root/irc /root/irc/*.json \
    /etc/subspace-fleet/secrets.json /etc/subspace-fleet/extra-secrets.json; do
    check_private "$path"
done
"$secrets_ok" && pass 'Fleet secrets are root-only.'

# 6. Development sign-in must stay away from the agents.
if grep -qs '^ENABLE_DEV_AUTH=true' /root/fleet/api.env; then
    # nft prints one port as "dport 3000" and several as "dport { 3000, 5432 }".
    if grep -Eq 'dport (\{ )?([0-9]+, )*3000[ ,}].*skuid' <<<"$isolation$ruleset"; then
        pass 'The development-sign-in API on port 3000 is limited to root and your operator login.'
    else
        fail 'Development sign-in is on but agents can reach the API. Re-run the installer, which isolates port 3000.'
    fi
    warn 'Development sign-in is on: anyone who reaches the API can act as its administrator. Keep it single-operator or switch to WorkOS or Entra.'
fi

# 6b. The agent listener is the agents' only way into the API: its own port, without sign-in or admin routes.
agent_port=$(sed -n 's/^AGENT_PORT=\([0-9][0-9]*\)$/\1/p' /root/fleet/api.env 2>/dev/null | tail -n 1)
if [[ -n $agent_port ]]; then
    if [[ $agent_port == 3000 || $agent_port == 5432 ]]; then
        fail "The API's agent listener shares port $agent_port with an operator-only service. Set AGENT_PORT in /root/fleet/api.env to its own port."
    else
        # An invalid body: a served sign-in route answers 400 without signing anyone in; the agent listener has none.
        status=$(curl -s -o /dev/null -m 5 -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
            --data '{"email":1}' "http://127.0.0.1:$agent_port/api/v1/auth/dev/token" 2>/dev/null)
        # Like port 3000: only root, the operator and the officers' users (allowed before this rule) may reach it.
        if grep -Eq "dport (\{ )?([0-9]+, )*${agent_port}[ ,}].*skuid" <<<"$isolation$ruleset"; then
            pass "The agent listener on port $agent_port is limited to root, your operator login and the officers."
        else
            fail "Every local user can reach the agent listener on port $agent_port and act as any agent. Re-run the installer, which limits it to the officers."
        fi
        case $status in
            404) pass "Agents reach the API only on its agent listener (port $agent_port), which serves no sign-in route." ;;
            000) warn "The API's agent listener on port $agent_port did not answer. Is the API running?" ;;
            *) fail "Port $agent_port answers the development sign-in route (HTTP $status), so agents could sign in as anyone. Check AGENT_PORT and PORT in /root/fleet/api.env." ;;
        esac
    fi
fi

# 7. Public addresses.
public=
while read -r _ _ family address _; do
    address=${address%/*}
    if [[ $family == inet ]] && ! private_address "$address"; then public+=" $address"; fi
    if [[ $family == inet6 && $address == [23]* ]]; then public+=" $address"; fi
done < <(ip -o addr show scope global 2>/dev/null)
azure_ip=$(curl -fsS -m 2 -H Metadata:true \
    'http://169.254.169.254/metadata/instance/network/interface/0/ipv4/ipAddress/0/publicIpAddress?api-version=2021-02-01&format=text' 2>/dev/null || true)
[[ -n $azure_ip ]] && public+=" $azure_ip (Azure public IP)"
if [[ -n $public ]]; then
    warn "This VM has a public address:$public. Its cloud firewall must block all inbound traffic you have not chosen; prefer Tailscale or Cloudflare Tunnel and no public SSH."
else
    pass 'No public address found on this VM.'
fi

# 8. Security updates.
if systemctl is-enabled --quiet unattended-upgrades.service 2>/dev/null; then
    pass 'Unattended security upgrades are enabled.'
else
    warn 'Unattended security upgrades are off. Enable them: sudo apt-get install unattended-upgrades'
fi

if (( failures )); then
    printf '\n%d check(s) FAILED. Fix them before using this VM.\n' "$failures"
    exit 1
fi
printf '\nNo failures.\n'
