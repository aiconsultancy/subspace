#!/usr/bin/env bash
# Lock a fleet VM down: drop every inbound connection except SSH (and anything
# allowed explicitly), harden sshd, and keep the agents' Linux users away from
# operator-only services such as the development-sign-in API and from the
# cloud metadata service.
#
# Usage:
#   lockdown.sh isolate --operator USER [--operator USER2] [--operator-only-port PORT]...
#                       [--protect-metadata] [--metadata-user USER]...
#   lockdown.sh apply   --operator USER [--operator USER2] [--ssh-from CIDR]...
#                       [--tailscale | --cloudflare] [--ssh-port PORT]... [--allow-port PORT]...
#                       [isolate options] [--timeout SECONDS] [--confirm-now]
#   lockdown.sh confirm [ID]   keep the pending change (only if its ID is ID) and load it at boot
#   lockdown.sh rollback [--from-timer]
#   lockdown.sh status
#   lockdown.sh boot           load agent isolation, undo any pending change, load the confirmed rules
#
# Agent isolation (`isolate`, table inet subspace_isolation) only limits what
# users other than root and the operators can connect to, so it cannot lock
# you out: it applies at once, loads at boot and is never rolled back.
# --operator-only-port keeps a loopback or container port to root and the
# operators; --protect-metadata does the same for the cloud metadata service,
# plus any --metadata-user accounts.
#
# The inbound firewall and sshd hardening (`apply`, table inet subspace_lockdown)
# can lock you out, so a change stays pending: `apply` schedules a rollback
# (default 180 s) and prints the change's ID. From a NEW SSH session, run
# `lockdown.sh confirm ID` to keep it. A rollback or a restart while it is
# pending returns to the last confirmed rules and sshd settings. With nothing
# pending, a manual rollback removes the inbound firewall and sshd hardening
# entirely (agent isolation stays). A re-apply that changes nothing leaves the
# confirmed rules alone. `--confirm-now` skips the timer and is meant for a
# local console.
#
# --tailscale accepts SSH on the tailnet interface; --cloudflare accepts it only
# through cloudflared on loopback. Without either (or --ssh-from), SSH is open
# on every interface and the cloud firewall must restrict it. SSH ports default
# to those sshd and ssh.socket listen on; --ssh-port replaces them. --allow-port
# accepts TCP from private source addresses only, also for ports Docker publishes.
set -euo pipefail
umask 077
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

state=/etc/subspace-lockdown
rules=$state/rules.nft
pending=$state/pending.nft
isolation=$state/isolation.nft
sshd_conf=/etc/ssh/sshd_config.d/00-subspace-lockdown.conf
backup=$state/sshd-previous.conf
rollback_unit=subspace-lockdown-rollback
boot_unit=/etc/systemd/system/subspace-lockdown.service
stable=/usr/local/sbin/subspace-lockdown
self=$(realpath -- "${BASH_SOURCE[0]}")
locked=false

die() { printf 'lockdown: %s\n' "$*" >&2; exit 1; }
say() { printf 'lockdown: %s\n' "$*"; }
(( EUID == 0 )) || die 'Run as root.'

private_v4='{ 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 100.64.0.0/10 }'
private_v6='{ fc00::/7, fe80::/10 }'

unique_list() { printf '%s\n' "$@" | awk 'NF && !seen[$0]++' | paste -sd, -; }

ssh_listen_ports() {
    # Every port sshd is configured for, plus ssh.socket's when SSH is socket-activated
    # (an ssh.socket override can listen on a port sshd -T does not report).
    local config
    config=$(sshd -T 2>/dev/null) || config=
    awk '$1 == "port" { print $2 } $1 == "listenaddress" { n = split($2, part, ":"); print part[n] }' <<<"$config"
    if systemctl is-active --quiet ssh.socket 2>/dev/null; then
        systemctl show -p Listen --value ssh.socket 2>/dev/null | grep -oE '[0-9]+ \(Stream\)' | cut -d' ' -f1
    fi
}

parse() {
    # parse OPTIONS...: set the option globals shared by isolate and apply.
    operators=() v4=() v6=() allow=() protected=() ssh_ports=() metadata_users=()
    uids=(0) metadata_uids=()
    tailscale=false cloudflare=false protect_metadata=false
    local user source uid
    while (( $# )); do
        case $1 in
            --operator) operators+=("${2:?}"); shift 2 ;;
            --ssh-from)
                source=${2:?}
                [[ $source =~ ^[0-9a-fA-F:./]+$ ]] || die "Invalid --ssh-from $source."
                if [[ $source == *:* ]]; then v6+=("$source"); else v4+=("$source"); fi
                shift 2 ;;
            --tailscale) tailscale=true; shift ;;
            --cloudflare) cloudflare=true; shift ;;
            --allow-port) [[ ${2:?} =~ ^[0-9]{1,5}$ ]] || die 'Invalid --allow-port.'; allow+=("$2"); shift 2 ;;
            --operator-only-port) [[ ${2:?} =~ ^[0-9]{1,5}$ ]] || die 'Invalid --operator-only-port.'; protected+=("$2"); shift 2 ;;
            --ssh-port) [[ ${2:?} =~ ^[0-9]{1,5}$ ]] || die 'Invalid --ssh-port.'; ssh_ports+=("$2"); shift 2 ;;
            --protect-metadata) protect_metadata=true; shift ;;
            --metadata-user) metadata_users+=("${2:?}"); shift 2 ;;
            *) die "Unknown option $1." ;;
        esac
    done
    (( ${#operators[@]} )) || die 'Name at least one --operator (the Linux login you SSH in with).'
    for user in "${operators[@]}"; do
        [[ $user =~ ^[a-z_][a-z0-9_.-]{0,31}$ && $user != root ]] || die "Invalid operator $user."
        uid=$(id -u -- "$user" 2>/dev/null) || die "No Linux user $user."
        uids+=("$uid")
    done
    for user in ${metadata_users[@]+"${metadata_users[@]}"}; do
        [[ $user =~ ^[a-z_][a-z0-9_.-]{0,31}$ && $user != root ]] || die "Invalid metadata user $user."
        uid=$(id -u -- "$user" 2>/dev/null) || die "No Linux user $user."
        metadata_uids+=("$uid")
    done
    if grep -Eqs '"keyVault": *"[^"]+"' /root/fleet/answers.json; then
        protect_metadata=true  # A Key Vault identity on this VM must never reach the agents.
    fi
}

isolation_wanted() { "$protect_metadata" || (( ${#protected[@]} )); }

render_isolation() {
    {
        printf 'add table inet subspace_isolation\ndelete table inet subspace_isolation\n'
        isolation_wanted || return 0
        printf 'table inet subspace_isolation {\n'
        printf '    chain output {\n'
        # Before Docker's DNAT (priority -100), so published loopback ports are matched whether
        # or not Docker's userland proxy is in use. Container traffic is forwarded, so it does
        # not pass through this chain.
        printf '        type filter hook output priority -110; policy accept;\n'
        if (( ${#protected[@]} )); then
            printf '        oifname "lo" tcp dport { %s } meta skuid != { %s } reject with tcp reset\n' \
                "$(unique_list "${protected[@]}")" "$(unique_list "${uids[@]}")"
            printf '        oifname { "docker0", "br-*" } tcp dport { %s } meta skuid != { %s } reject with tcp reset\n' \
                "$(unique_list "${protected[@]}")" "$(unique_list "${uids[@]}")"
        fi
        if "$protect_metadata"; then
            printf '        ip daddr 169.254.169.254 meta skuid != { %s } reject\n' \
                "$(unique_list "${uids[@]}" ${metadata_uids[@]+"${metadata_uids[@]}"})"
            printf '        ip6 daddr fd00:ec2::254 meta skuid != { %s } reject\n' \
                "$(unique_list "${uids[@]}" ${metadata_uids[@]+"${metadata_uids[@]}"})"
        fi
        printf '    }\n'
        printf '}\n'
    } > "$1"
}

render_lockdown() {
    local rules_new=$1 sshd_new=$2 ssh_port_set allow_set
    if (( ${#ssh_ports[@]} == 0 )); then
        mapfile -t ssh_ports < <(ssh_listen_ports)
        (( ${#ssh_ports[@]} )) || die 'Could not find the SSH port. Check sshd or pass --ssh-port.'
    fi
    ssh_port_set="{ $(unique_list "${ssh_ports[@]}") }"
    allow_set="{ $(unique_list ${allow[@]+"${allow[@]}"}) }"
    {
        printf 'add table inet subspace_lockdown\ndelete table inet subspace_lockdown\n'
        printf 'table inet subspace_lockdown {\n'
        if (( ${#allow[@]} )); then
            # Filter allowed ports before Docker's DNAT sends published ones through FORWARD.
            printf '    chain prerouting {\n'
            printf '        type filter hook prerouting priority -110; policy accept;\n'
            printf '        iif "lo" accept\n'
            printf '        ct state established,related accept\n'
            printf '        tcp dport %s ip saddr != %s drop\n' "$allow_set" "$private_v4"
            printf '        tcp dport %s ip6 saddr != %s drop\n' "$allow_set" "$private_v6"
            printf '    }\n'
        fi
        printf '    chain input {\n'
        printf '        type filter hook input priority filter - 10; policy drop;\n'
        printf '        iif "lo" accept\n'
        printf '        ct state established,related accept\n'
        printf '        ct state invalid drop\n'
        printf '        meta l4proto icmp icmp type { echo-request, destination-unreachable, time-exceeded, parameter-problem } limit rate 20/second accept\n'
        printf '        meta l4proto ipv6-icmp accept\n'
        printf '        udp sport 67 udp dport 68 accept\n'
        printf '        udp sport 547 udp dport 546 accept\n'
        if "$tailscale"; then
            printf '        iifname "tailscale0" tcp dport %s accept\n' "$ssh_port_set"
            printf '        udp dport 41641 accept\n'
        fi
        if (( ${#v4[@]} )); then
            printf '        ip saddr { %s } tcp dport %s accept\n' "$(unique_list "${v4[@]}")" "$ssh_port_set"
        fi
        if (( ${#v6[@]} )); then
            printf '        ip6 saddr { %s } tcp dport %s accept\n' "$(unique_list "${v6[@]}")" "$ssh_port_set"
        fi
        if ! "$tailscale" && ! "$cloudflare" && (( ${#v4[@]} + ${#v6[@]} == 0 )); then
            printf '        tcp dport %s accept\n' "$ssh_port_set"
        fi
        if (( ${#allow[@]} )); then
            printf '        ip saddr %s tcp dport %s accept\n' "$private_v4" "$allow_set"
            printf '        ip6 saddr %s tcp dport %s accept\n' "$private_v6" "$allow_set"
        fi
        printf '    }\n'
        printf '}\n'
    } > "$rules_new"

    {
        printf '# Managed by subspace-installer/vm/lockdown.sh. Run "lockdown.sh rollback" to remove.\n'
        printf 'PasswordAuthentication no\n'
        printf 'KbdInteractiveAuthentication no\n'
        printf 'PermitEmptyPasswords no\n'
        printf 'PermitRootLogin no\n'
        printf 'PubkeyAuthentication yes\n'
        printf 'AuthenticationMethods publickey\n'
        printf 'AllowUsers %s\n' "${operators[*]}"
        printf 'AllowAgentForwarding no\n'
        printf 'AllowTcpForwarding local\n'
        printf 'AllowStreamLocalForwarding no\n'
        printf 'GatewayPorts no\n'
        printf 'PermitTunnel no\n'
        printf 'X11Forwarding no\n'
        printf 'PermitUserEnvironment no\n'
        printf 'MaxAuthTries 6\n'
        printf 'LoginGraceTime 30\n'
        printf 'ClientAliveInterval 60\n'
        printf 'ClientAliveCountMax 3\n'
    } > "$sshd_new"
}

change_id() {
    # change_id RULES SSHD: a short ID for this exact inbound firewall and sshd change.
    cat -- "$1" "$2" | sha256sum | cut -c1-16
}

reload_sshd() {
    if systemctl is-active --quiet ssh.service; then
        systemctl reload ssh.service
    elif systemctl is-active --quiet sshd.service; then
        systemctl reload sshd.service
    fi
}

lock_state() {
    if "$locked"; then return; fi
    install -d -m 0700 "$state"
    exec 9>"$state/lock"
    flock -x 9
    locked=true
}

cancel_rollback() {
    systemctl stop "$rollback_unit.timer" "$rollback_unit.service" >/dev/null 2>&1 || true
    systemctl reset-failed "$rollback_unit.timer" "$rollback_unit.service" >/dev/null 2>&1 || true
}

stop_rollback_timer() {
    # From inside the rollback service itself: stopping the service would kill this run.
    systemctl stop "$rollback_unit.timer" >/dev/null 2>&1 || true
}

install_boot_unit() {
    install -o root -g root -m 0755 -- "$self" "$stable.new" &&
        mv -f -- "$stable.new" "$stable" || return 1
    cat > "$boot_unit.new" <<UNIT || return 1
[Unit]
Description=Subspace fleet VM lockdown firewall
DefaultDependencies=no
Before=network-pre.target
Wants=network-pre.target
After=local-fs.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=$stable boot

[Install]
WantedBy=sysinit.target
UNIT
    mv -f -- "$boot_unit.new" "$boot_unit" &&
        systemctl daemon-reload && systemctl enable --quiet subspace-lockdown.service
}

load_isolation() {
    # load_isolation STAGED: check, keep and load agent isolation. It never blocks root or the operators.
    nft -c -f "$1" || die 'The generated agent isolation rules are invalid; nothing changed.'
    lock_state
    mv -f -- "$1" "$isolation"
    nft -f "$isolation"
    install_boot_unit || die 'Could not install the boot unit.'
    sync
}

isolate() (
    local staging
    command -v nft >/dev/null || { apt-get update -qq; DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends nftables; }
    parse "$@"
    install -d -m 0700 "$state"
    staging=$(mktemp -d "$state/isolate.XXXXXX")
    trap 'rm -rf -- "$staging"' EXIT
    render_isolation "$staging/isolation.nft"
    load_isolation "$staging/isolation.nft"
    if isolation_wanted; then
        say 'agent isolation loaded: only root and the operators reach operator-only ports and cloud metadata.'
    else
        say 'agent isolation is off.'
    fi
)

apply() (
    local timeout=180 confirm_now=false staging id
    local -a args=()
    while (( $# )); do
        case $1 in
            --timeout) [[ ${2:?} =~ ^[0-9]+$ ]] || die 'Invalid --timeout.'; timeout=$2; shift 2 ;;
            --confirm-now) confirm_now=true; shift ;;
            --no-wait) shift ;;  # Accepted for compatibility; apply never waits.
            *) args+=("$1"); shift ;;
        esac
    done
    command -v nft >/dev/null || { apt-get update -qq; DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends nftables; }
    parse "${args[@]}"
    install -d -m 0700 "$state"
    staging=$(mktemp -d "$state/apply.XXXXXX")
    trap 'rm -rf -- "$staging"' EXIT
    render_lockdown "$staging/rules.nft.new" "$staging/sshd.conf.new"
    nft -c -f "$staging/rules.nft.new" || die 'The generated firewall rules are invalid; nothing changed.'
    id=$(change_id "$staging/rules.nft.new" "$staging/sshd.conf.new")

    lock_state
    if isolation_wanted || [[ -f $isolation ]]; then
        render_isolation "$staging/isolation.nft"
        load_isolation "$staging/isolation.nft"
    fi
    cancel_rollback
    if [[ ! -f $pending ]]; then
        if cmp -s -- "$staging/rules.nft.new" "$rules" && cmp -s -- "$staging/sshd.conf.new" "$sshd_conf"; then
            nft -f "$rules"
            printf '%s\n' "$id" > "$state/rules.id"
            printf '%s\n' "$id" > "$state/applied.id"
            install_boot_unit
            sync
            say "unchanged; the confirmed lockdown stays in place (ID $id)."
            return
        fi
        if [[ -f $sshd_conf ]]; then cp -p -- "$sshd_conf" "$backup"; else : > "$backup"; fi
    fi

    mv -f -- "$staging/rules.nft.new" "$pending"
    printf '%s\n' "$id" > "$state/pending.id"
    printf '%s\n' "$id" > "$state/applied.id"
    if ! install_boot_unit || ! sync; then
        rollback
        die 'Could not prepare lockdown recovery.'
    fi
    if ! "$confirm_now" && ! systemd-run --quiet --unit "$rollback_unit" --on-active="${timeout}s" \
        --timer-property=AccuracySec=1s "$stable" rollback --from-timer; then
        rollback
        die 'Could not arm the lockdown rollback timer.'
    fi
    if ! install -m 0644 "$staging/sshd.conf.new" "$sshd_conf.new" || ! mv -f -- "$sshd_conf.new" "$sshd_conf"; then
        rollback
        die 'Could not install the hardened sshd settings.'
    fi
    if ! sshd -t; then
        rollback
        die 'sshd rejected the hardened settings; restored the previous ones.'
    fi
    if ! nft -f "$pending"; then
        rollback
        die 'Could not load the pending firewall rules.'
    fi
    if ! reload_sshd || ! printf '%q ' "${args[@]}" > "$state/arguments" || ! sync; then
        rollback
        die 'Could not finish applying the lockdown.'
    fi
    if "$confirm_now"; then
        confirm "$id"
    else
        say "applied change $id. Rolls back automatically in ${timeout}s unless you run 'sudo $stable confirm $id' from a NEW SSH session."
    fi
)

restore_pending() {
    [[ -f $backup ]] || die 'Could not find the previous sshd settings for this pending change.'
    if [[ -s $backup ]]; then
        cp -p -- "$backup" "$sshd_conf.new"
        mv -f -- "$sshd_conf.new" "$sshd_conf"
    else
        rm -f -- "$sshd_conf" "$sshd_conf.new"
    fi
    rm -f -- "$pending" "$state/pending.id" "$backup"
}

confirm() {
    local expected=${1:-} actual
    lock_state
    [[ -f $pending ]] || die 'Nothing is pending. The change may already have rolled back. Run setup again.'
    actual=$(cat "$state/pending.id" 2>/dev/null || true)
    if [[ -n $expected && $expected != "$actual" ]]; then
        die "The pending change is ${actual:-unknown}, not $expected, so another run replaced it. Run setup again."
    fi
    cancel_rollback
    mv -f -- "$pending" "$rules"
    mv -f -- "$state/pending.id" "$state/rules.id" 2>/dev/null || rm -f -- "$state/rules.id"
    rm -f -- "$backup"
    systemctl enable --quiet subspace-lockdown.service
    sync  # Make the confirmation durable before reporting it.
    say "confirmed change ${actual:-unknown}. The firewall now loads at boot."
}

boot() {
    lock_state
    if [[ -f $isolation ]]; then nft -f "$isolation" || true; fi
    if [[ -f $pending ]]; then
        # Best effort: the confirmed rules must load even if the old sshd settings are missing.
        if [[ -f $backup ]]; then restore_pending; else rm -f -- "$pending" "$state/pending.id"; fi
    fi
    if [[ -f $rules ]]; then nft -f "$rules"; fi
}

delete_table() {
    if nft list table inet subspace_lockdown >/dev/null 2>&1; then
        nft delete table inet subspace_lockdown
    fi
}

rollback() {
    local from_timer=${1:-}
    lock_state
    if [[ $from_timer == --from-timer ]]; then stop_rollback_timer; else cancel_rollback; fi
    if [[ -f $pending ]]; then
        restore_pending
        if [[ -f $rules ]]; then
            nft -f "$rules"
        else
            delete_table
        fi
        if sshd -t; then reload_sshd; fi
        sync
        if [[ -f $rules ]]; then
            say 'rolled back to the last confirmed lockdown.'
        else
            say 'rolled back; no lockdown had been confirmed, so the inbound firewall is off. Agent isolation stays.'
        fi
    elif [[ $from_timer == --from-timer ]]; then
        return
    else
        delete_table
        rm -f -- "$rules" "$state/rules.id" "$sshd_conf" "$backup"
        if [[ ! -f $isolation ]]; then
            systemctl disable --quiet subspace-lockdown.service || true
            rm -f -- "$boot_unit"
            systemctl daemon-reload
        fi
        if sshd -t; then reload_sshd; fi
        sync
        say 'rolled back: inbound firewall and sshd hardening removed. Agent isolation stays; it never blocks your login.'
    fi
}

status() {
    if nft list table inet subspace_lockdown >/dev/null 2>&1; then
        say 'firewall: loaded'
    else
        say 'firewall: NOT loaded'
    fi
    if nft list table inet subspace_isolation >/dev/null 2>&1; then
        say 'isolation: loaded'
    else
        say 'isolation: NOT loaded'
    fi
    if [[ -f $rules ]] && systemctl is-enabled --quiet subspace-lockdown.service 2>/dev/null; then
        say "boot: confirmed rules load at boot (ID $(cat "$state/rules.id" 2>/dev/null || echo unknown))"
    else
        say 'boot: no confirmed rules'
    fi
    if [[ -f $pending ]]; then
        say "rollback: PENDING change $(cat "$state/pending.id" 2>/dev/null || echo unknown); run confirm from a new SSH session to keep it"
    fi
    if [[ -f $sshd_conf ]]; then say "sshd: hardened ($sshd_conf)"; else say 'sshd: not hardened by lockdown'; fi
}

case ${1:-} in
    isolate) shift; isolate "$@" ;;
    apply) shift; apply "$@" ;;
    confirm) confirm "${2:-}" ;;
    rollback) rollback "${2:-}" ;;
    status) status ;;
    boot) boot ;;
    applied-id) cat "$state/applied.id" 2>/dev/null ;;
    *) sed -n '2,/^set -euo pipefail/{ /^#/p; }' "$self" >&2; exit 2 ;;
esac
