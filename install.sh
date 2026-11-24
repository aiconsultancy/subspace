#!/usr/bin/env bash
# Subspace installer for macOS and Linux. On Windows, use install.ps1.
#
# A fleet is one private Linux VM: Stargate, the rear admiral and the captains
# (one Linux user and tmux session each), CLIProxyAPI, Ergo IRC and the
# Subspace API. This script creates the VM (or connects to yours), copies the
# installer onto it, runs `subspace-setup` there, locks the VM down, and
# connects the desktop app to it through an SSH tunnel.
#
# Read INSTALL.md first. Coding agents: INSTALL.md tells you what to ask the
# user and which flags to pass.
#
# Usage: ./install.sh [COMMAND] [OPTIONS] [FLEET OPTIONS]
#
# Commands:
#   install       create or connect a VM and set up a fleet on it (default)
#   irc-server    create or connect a host and run a shared IRC server on it
#   tunnel        forward the fleet's ports to 127.0.0.1 here (--background to detach)
#   providers     show the model provider accounts; `providers login P` signs
#                 one in (codex-device, codex, claude, ...). Agents need one.
#   skills        install Subspace skills and the API's MCP server into your local
#                 agents' user profiles (--agents claude,codex,opencode --name YOU)
#   desktop       write the desktop app's IRC connection for this fleet
#   ssh           open a shell on the VM, or run a command: ./install.sh ssh -- uptime
#   status        show the VM, the fleet and its services
#   audit         run the privacy audit on the VM
#   stop | start  stop or start a local VM
#   destroy       delete a local VM and its saved credentials (asks first)
#
# Options:
#   --local              create a local VM: Lima with Apple Virtualization on
#                        macOS, Lima with QEMU/KVM on Linux
#   --remote USER@HOST   use an existing Ubuntu 24.04 or Debian 12 VM over SSH
#   --target NAME        this installer's name for the VM (default subspace-fleet,
#                        or subspace-irc for irc-server)
#   --identity FILE      SSH private key for --remote
#   --ssh-port N         SSH port for --remote (default 22)
#   --host-key SHA256:…  expected SSH host key fingerprint for --remote
#   --ssh-proxy CMD      SSH ProxyCommand for --remote, for example
#                        "cloudflared access ssh --hostname %h"
#   --cpus N --memory GIB --disk GIB   local VM size (default 4, 8, 60)
#   --install-deps       allow installing Lima (and QEMU on Linux) without asking
#   --irc-server NAME    use the shared IRC server set up with
#                        `./install.sh irc-server --target NAME`
#   --api-target NAME    for a further fleet: skip this VM's Subspace API and
#                        register the fleet with the API on fleet NAME instead
#   --database-url-file FILE  run this VM's Subspace API against an existing
#                        PostgreSQL: a private (chmod 600) file on this machine
#                        holding its postgresql://...?sslmode=verify-full URL,
#                        copied to the VM as root's; add --migrate to migrate it
#   --database-ca-file FILE   that server's CA certificate (PEM), when the
#                        VM does not already trust it
#   --irc-operator-password-file FILE  with --irc-operator NAME: a private
#                        (chmod 600) file holding a remote IRC server's operator
#                        password, so setup registers the officers' certificate
#                        accounts itself; removed from the VM after the run
#   --irc-operator-account-password-file FILE  with --irc-operator-account
#                        NAME: the same for the IRC account the operator's
#                        connection signs in to first (servers with require-sasl)
#   --api-enrolment-token-file FILE  with --api-url URL (a Subspace API on
#                        another host): a private (chmod 600) file holding the
#                        fleet's enrolment token; removed from the VM after the run
#   --fleet-console ID   (desktop) the Board ID of the Fleet Console web Space
#                        (default fleet-console, the one built into Subspace)
#   --yes                never prompt; fail when an answer is missing
#   --background         (tunnel) run the tunnel in the background
#   -h, --help           show this help and the fleet options
#
# Every other option is passed to subspace-setup on the VM, for example
#   --name "Example Fleet" --rear-admiral Atlas --captains Orion,Vega
#   --admin admiral --demo | --no-demo --crew-claude 2 --crew-codex 2
#   --repositories https://github.com/example/project.git
#   --agent-dispatch | --no-agent-dispatch (default off)
#   --access ssh|tailscale|cloudflare --ssh-from 203.0.113.7/32
# Run `./install.sh --help` to see them all.

# on_target changes $state only inside its own subshell.
# shellcheck disable=SC2031
set -euo pipefail
umask 077

root=$(cd "$(dirname "$0")" && pwd)
home=${SUBSPACE_HOME:-$HOME/.subspace}
payload_dir=/opt/subspace/bootstrap
setup_cmd=/opt/subspace/bootstrap/subspace-installer/vm/setup.sh

command=install
mode=
target=
remote=
identity=
ssh_port=22
host_key=
ssh_proxy=
cpus=4
memory=8
disk=60
install_deps=false
irc_target=
api_target=
database_url_file=
database_ca_file=
irc_operator_password_file=
irc_operator_account_password_file=
api_enrolment_token_file=
fleet_console=fleet-console
assume_yes=false
background=false
passthrough=()
transient=()

die() { printf 'install: %s\n' "$*" >&2; exit 1; }
say() { printf '==> %s\n' "$*"; }
note() { printf '    %s\n' "$*"; }
interactive() { [ "$assume_yes" = false ] && [ -t 0 ] && [ -t 1 ]; }

confirm() {
    # confirm "question" -> 0 for yes. Non-interactive runs answer no.
    local answer
    interactive || return 1
    printf '%s [y/N] ' "$1"
    read -r answer || return 1
    case $answer in [yY]|[yY][eE][sS]) return 0 ;; *) return 1 ;; esac
}

usage() {
    awk 'NR == 1 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "$0"
}

# Arguments ------------------------------------------------------------------

case ${1:-} in
    install|irc-server|tunnel|providers|skills|desktop|ssh|status|audit|stop|start|destroy) command=$1; shift ;;
esac
while [ $# -gt 0 ]; do
    if [ "$command" = ssh ] && [ "$1" = -- ]; then
        # The remote command is not the installer's: everything from -- goes to ssh unchanged.
        passthrough+=("$@"); break
    fi
    case $1 in
        --local) mode=local; shift ;;
        --remote) mode=remote; remote=${2:?--remote needs USER@HOST}; shift 2 ;;
        --target) target=${2:?}; shift 2 ;;
        --identity) identity=${2:?}; shift 2 ;;
        --ssh-port) ssh_port=${2:?}; shift 2 ;;
        --host-key) host_key=${2:?}; shift 2 ;;
        --ssh-proxy) ssh_proxy=${2:?}; shift 2 ;;
        --cpus) cpus=${2:?}; shift 2 ;;
        --memory) memory=${2:?}; shift 2 ;;
        --disk) disk=${2:?}; shift 2 ;;
        --install-deps) install_deps=true; shift ;;
        --irc-server) irc_target=${2:?}; shift 2 ;;
        --api-target) api_target=${2:?}; shift 2 ;;
        --database-url-file) database_url_file=${2:?}; shift 2 ;;
        --database-ca-file) database_ca_file=${2:?}; shift 2 ;;
        --irc-operator-password-file) irc_operator_password_file=${2:?}; shift 2 ;;
        --irc-operator-account-password-file) irc_operator_account_password_file=${2:?}; shift 2 ;;
        --api-enrolment-token-file) api_enrolment_token_file=${2:?}; shift 2 ;;
        --fleet-console) fleet_console=${2:?}; shift 2 ;;
        --yes|-y) assume_yes=true; shift ;;
        --background) background=true; shift ;;
        -h|--help)
            usage
            if [ -f "$root/subspace-installer/vm/fleet_setup.py" ] && command -v python3 >/dev/null 2>&1; then
                printf '\nFleet options (passed to subspace-setup on the VM):\n'
                python3 -B "$root/subspace-installer/vm/fleet_setup.py" --help 2>/dev/null | sed -n '/^opt/,$p' || true
            fi
            exit 0 ;;
        *) passthrough+=("$1"); shift ;;
    esac
done
if [ -z "$target" ]; then
    if [ "$command" = irc-server ]; then target=subspace-irc; else target=subspace-fleet; fi
fi
for value in "$target" "$irc_target" "$api_target"; do
    case $value in
        *[!a-z0-9-]*|-*) die "Target names must be lower-case letters, digits and hyphens." ;;
    esac
done
[ -n "$target" ] || die "--target must be lower-case letters, digits and hyphens."
for value in "$cpus" "$memory" "$disk" "$ssh_port"; do
    case $value in *[!0-9]*|'') die "VM size and SSH port must be whole numbers." ;; esac
done
state=$home/targets/$target

# Platform -------------------------------------------------------------------

os=$(uname -s)
arch=$(uname -m)
case $os in
    Darwin) vm_type=vz ;;
    Linux)
        vm_type=qemu
        if grep -qi microsoft /proc/version 2>/dev/null; then
            die "This is WSL. Run install.ps1 from PowerShell on Windows instead, or use --remote with a VM elsewhere."
        fi ;;
    MINGW*|MSYS*|CYGWIN*) die "On Windows, run install.ps1 from PowerShell." ;;
    *) die "Unsupported system $os." ;;
esac

# SSH to the target ----------------------------------------------------------

load_target() {
    [ -f "$state/target.env" ] || die "No target '$target'. Run ./install.sh --local or --remote USER@HOST first."
    # shellcheck disable=SC1091
    . "$state/target.env"
}

ssh_opts() {
    # Prints nothing; sets the global ssh_args array for the saved target.
    if [ "$KIND" = lima ]; then
        ssh_args=(-F "$HOME/.lima/$VM/ssh.config" "lima-$VM")
    else
        ssh_args=(-F "$state/ssh_config" subspace-target)
    fi
}

run_only_file() {
    # The local file for one of the run-only options, by the option's name without --...-file.
    case $1 in
        irc-operator-password) printf '%s' "$irc_operator_password_file" ;;
        irc-operator-account-password) printf '%s' "$irc_operator_account_password_file" ;;
        api-enrolment-token) printf '%s' "$api_enrolment_token_file" ;;
    esac
}

remove_transient() {
    # The EXIT trap's cleanup: remove this run's own copies from the VM, once.
    (( ${#transient[@]} )) || return 0
    local -a dirs=("${transient[@]}")
    transient=()
    vm sudo -n rm -rf "${dirs[@]}" 2>/dev/null ||
        note "Remove ${dirs[*]} on the VM by hand: they hold this run's copies, such as the IRC operator's password."
}

do_ssh() {
    # ./install.sh ssh [ssh options] [-- command...]: ssh options go before the host, the command after it.
    local -a options=() remote_command=()
    while [ $# -gt 0 ]; do
        if [ "$1" = -- ]; then shift; remote_command=("$@"); break; fi
        options+=("$1"); shift
    done
    exec ssh ${options[@]+"${options[@]}"} "${ssh_args[@]}" ${remote_command[@]+"${remote_command[@]}"}
}

vm() {
    # vm [ssh options --] command...: run a command on the target.
    ssh_opts
    ssh -o BatchMode=yes "${ssh_args[@]}" "$@"
}

vm_tty() {
    ssh_opts
    if [ -t 0 ] && [ -t 1 ]; then
        ssh -t "${ssh_args[@]}" "$@"
    else
        ssh -o BatchMode=yes "${ssh_args[@]}" "$@"
    fi
}

vm_fresh() {
    # A new SSH connection, never a shared control master: proves new logins still work.
    ssh_opts
    ssh -o BatchMode=yes -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=20 "${ssh_args[@]}" "$@"
}

quote_args() {
    local out='' arg
    for arg in "$@"; do out="$out $(printf '%q' "$arg")"; done
    printf '%s' "$out"
}

# Local VM (Lima) ------------------------------------------------------------

lima_install() {
    command -v limactl >/dev/null 2>&1 && return 0
    say "Lima is not installed. It runs the fleet VM with Apple's Virtualization framework (macOS) or QEMU/KVM (Linux)."
    note "Project: https://lima-vm.io (Apache-2.0, a CNCF project)."
    if command -v brew >/dev/null 2>&1; then
        if [ "$install_deps" = true ] || confirm "Install Lima with Homebrew (brew install lima)?"; then
            brew install lima
            return 0
        fi
        die "Lima is required for a local VM. Install it (brew install lima) or rerun with --install-deps."
    fi
    if [ "$os" = Linux ]; then
        if [ "$install_deps" = true ] || confirm "Download the latest Lima release from GitHub into ~/.local (checksum verified)?"; then
            lima_from_github
            return 0
        fi
    fi
    die "Install Lima (https://lima-vm.io/docs/installation/), or rerun with --install-deps."
}

lima_from_github() {
    local tag version file work machine
    command -v curl >/dev/null 2>&1 || die "curl is required to download Lima."
    tag=$(curl -fsSL https://api.github.com/repos/lima-vm/lima/releases/latest | sed -n 's/.*"tag_name": *"\(v[0-9.]*\)".*/\1/p' | head -n1)
    [ -n "$tag" ] || die "Could not find the latest Lima release."
    version=${tag#v}
    case $arch in x86_64|amd64) machine=x86_64 ;; aarch64|arm64) machine=aarch64 ;; *) die "Unsupported CPU $arch." ;; esac
    file=lima-$version-Linux-$machine.tar.gz
    work=$(mktemp -d)
    curl -fsSL -o "$work/$file" "https://github.com/lima-vm/lima/releases/download/$tag/$file"
    curl -fsSL -o "$work/SHA256SUMS" "https://github.com/lima-vm/lima/releases/download/$tag/SHA256SUMS"
    (cd "$work" && grep " $file\$" SHA256SUMS | sha256sum --check --status) || die "Lima download failed its checksum."
    mkdir -p "$HOME/.local"
    tar -C "$HOME/.local" -xzf "$work/$file"
    rm -rf "$work"
    export PATH="$HOME/.local/bin:$PATH"
    note "Installed Lima $version in ~/.local. Add ~/.local/bin to your PATH."
}

qemu_check() {
    [ "$os" = Linux ] || return 0
    local qemu
    case $arch in x86_64|amd64) qemu='qemu-system-x86_64' ;; *) qemu='qemu-system-aarch64' ;; esac
    if ! command -v "$qemu" >/dev/null 2>&1; then
        say "QEMU is not installed. Lima uses it with KVM to run the VM."
        local pkgs=
        if command -v apt-get >/dev/null 2>&1; then
            case $arch in x86_64|amd64) pkgs="qemu-system-x86 qemu-utils ovmf" ;; *) pkgs="qemu-system-arm qemu-utils qemu-efi-aarch64" ;; esac
            if [ "$install_deps" = true ] || confirm "Install $pkgs with apt (needs sudo)?"; then
                # shellcheck disable=SC2086
                sudo apt-get update -qq && sudo apt-get install -y $pkgs
            fi
        elif command -v dnf >/dev/null 2>&1; then
            if [ "$install_deps" = true ] || confirm "Install qemu-kvm, qemu-img and edk2-ovmf with dnf (needs sudo)?"; then
                sudo dnf install -y qemu-kvm qemu-img edk2-ovmf
            fi
        elif command -v pacman >/dev/null 2>&1; then
            if [ "$install_deps" = true ] || confirm "Install qemu-base and edk2-ovmf with pacman (needs sudo)?"; then
                sudo pacman -S --needed --noconfirm qemu-base edk2-ovmf
            fi
        fi
        command -v "$qemu" >/dev/null 2>&1 || die "Install QEMU ($qemu) and run this again."
    fi
    if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
        die "You cannot use /dev/kvm. Enable virtualisation in firmware, then: sudo usermod -aG kvm \"\$USER\" and log in again."
    fi
}

lima_create() {
    local vm=$target status
    lima_install
    qemu_check
    if [ "$os" = Darwin ]; then
        major=$(sw_vers -productVersion | cut -d. -f1)
        [ "$major" -ge 13 ] || die "Apple's Virtualization framework needs macOS 13 or later."
    fi
    status=$(limactl list --format '{{.Status}}' "$vm" 2>/dev/null || true)
    if [ -z "$status" ]; then
        say "Creating local VM '$vm' (Ubuntu 24.04, $cpus CPUs, ${memory} GiB memory, ${disk} GiB disk, $vm_type)."
        note "Plain mode: no host folders are shared and no ports are forwarded automatically."
        limactl create --tty=false --name="$vm" --vm-type="$vm_type" --plain --mount-none \
            --cpus="$cpus" --memory="$memory" --disk="$disk" \
            --set='.ssh.loadDotSSHPubKeys = false | .ssh.forwardAgent = false | .ssh.forwardX11 = false' \
            template:ubuntu-24.04
        status=Stopped
    fi
    if [ "$status" != Running ]; then
        say "Starting '$vm'."
        limactl start --tty=false "$vm"
    fi
    mkdir -p "$state"
    printf 'KIND=lima\nVM=%s\n' "$vm" > "$state/target.env"
}

# Remote VM (SSH) ------------------------------------------------------------

pin_keys() {
    # pin_keys IN OUT: keep only IN's known_hosts lines whose key has the fingerprint $host_key.
    local line fingerprint one
    one=$(mktemp)
    : > "$2"
    while IFS= read -r line; do
        case $line in ''|'#'*) continue ;; esac
        printf '%s\n' "$line" > "$one"
        fingerprint=$(ssh-keygen -lf "$one" 2>/dev/null | awk '{ print $2 }')
        if [ "$fingerprint" = "$host_key" ]; then printf '%s\n' "$line" >> "$2"; fi
    done < "$1"
    rm -f "$one"
    [ -s "$2" ]
}

scan_keys() {
    # scan_keys HOST OUT: record HOST's SSH host key(s) without trusting them yet.
    if [ -n "$ssh_proxy" ]; then
        # ssh-keyscan cannot use a ProxyCommand: record the one key a first connection offers.
        : > "$2"
        ssh -F "$state/ssh_config" -o BatchMode=yes -o StrictHostKeyChecking=no -o ControlMaster=no \
            -o ControlPath=none -o UserKnownHostsFile="$2" subspace-target true >/dev/null 2>&1 || true
    elif [ -n "$host_key" ]; then
        ssh-keyscan -p "$ssh_port" -t ed25519,ecdsa,rsa "$1" 2>/dev/null > "$2" || true
    else
        # Just one key, so the fingerprint the user compares is the only key trusted.
        local type
        for type in ed25519 ecdsa rsa; do
            ssh-keyscan -p "$ssh_port" -t "$type" "$1" 2>/dev/null > "$2" || true
            [ -s "$2" ] && break
        done
    fi
    [ -s "$2" ] || die "Could not reach SSH on $1:$ssh_port."
}

remote_connect() {
    local user host
    case $remote in *@*) user=${remote%@*}; host=${remote#*@} ;; *) die "--remote needs USER@HOST." ;; esac
    case "$user$host" in *[!A-Za-z0-9._@:%-]*) die "Unexpected characters in $remote." ;; esac
    [ -z "$identity" ] || [ -f "$identity" ] || die "No SSH key at $identity."
    mkdir -p "$state" "$home/cm"
    chmod 700 "$home" "$home/targets" "$state" "$home/cm"
    {
        printf 'Host subspace-target\n'
        printf '  HostName %s\n  User %s\n  Port %s\n' "$host" "$user" "$ssh_port"
        if [ -n "$identity" ]; then printf '  IdentityFile "%s"\n  IdentitiesOnly yes\n' "$identity"; fi
        if [ -n "$ssh_proxy" ]; then printf '  ProxyCommand %s\n' "$ssh_proxy"; fi
        printf '  UserKnownHostsFile "%s"\n  GlobalKnownHostsFile none\n' "$state/known_hosts"
        printf '  StrictHostKeyChecking yes\n  ForwardAgent no\n  ForwardX11 no\n'
        printf '  ServerAliveInterval 30\n  ServerAliveCountMax 3\n'
        printf '  ControlMaster auto\n  ControlPath "%s/cm/%%C"\n  ControlPersist 60\n' "$home"
    } > "$state/ssh_config"
    if [ -s "$state/known_hosts" ] && [ -n "$host_key" ]; then
        # An explicit fingerprint always wins: keep only that key, or check the host again
        # (for example after the VM was rebuilt).
        if pin_keys "$state/known_hosts" "$state/known_hosts.new"; then
            mv -f "$state/known_hosts.new" "$state/known_hosts"
        else
            note "The saved host key for $host is not $host_key, so checking the host again."
            rm -f "$state/known_hosts" "$state/known_hosts.new"
        fi
    fi
    if [ ! -s "$state/known_hosts" ] && [ "$ssh_port" = 22 ] &&
        ssh-keygen -F "$host" 2>/dev/null | grep -v '^#' > "$state/known_hosts.new" && [ -s "$state/known_hosts.new" ]; then
        if [ -z "$host_key" ]; then
            mv -f "$state/known_hosts.new" "$state/known_hosts"
            note "Using the host key already in ~/.ssh/known_hosts for $host."
        elif pin_keys "$state/known_hosts.new" "$state/known_hosts"; then
            note "Using the host key for $host from ~/.ssh/known_hosts, which matches $host_key."
        fi
    fi
    rm -f "$state/known_hosts.new"
    if [ ! -s "$state/known_hosts" ]; then
        say "Checking $host's SSH host key."
        scan_keys "$host" "$state/known_hosts.new"
        if [ -n "$host_key" ]; then
            if ! pin_keys "$state/known_hosts.new" "$state/known_hosts"; then
                rm -f "$state/known_hosts" "$state/known_hosts.new"
                die "No host key on $host matches $host_key. Do not continue until you know why."
            fi
        else
            ssh-keygen -lf "$state/known_hosts.new" | sed 's/^/    /'
            note "Compare this with the VM's own fingerprint (cloud console boot log, or"
            note "'ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub' run through your cloud's run-command)."
            if ! confirm "Does it match?"; then
                rm -f "$state/known_hosts.new"
                die "Rerun with --host-key SHA256:... once you have the VM's real fingerprint."
            fi
            mv -f "$state/known_hosts.new" "$state/known_hosts"
        fi
        rm -f "$state/known_hosts.new"
    fi
    # Never reuse a connection opened before the key was (re)pinned.
    ssh -F "$state/ssh_config" -O exit subspace-target >/dev/null 2>&1 || true
    printf 'KIND=ssh\nHOST=%s\n' "$remote" > "$state/target.env"
    load_target
    vm true || die "SSH to $remote failed. Check the user, key and that the VM allows your address."
    vm sudo -n true || die "$user needs passwordless sudo on the VM for setup."
}

# Payload --------------------------------------------------------------------

send_payload() {
    # send_payload [DIR]: the installer goes to /opt/subspace/bootstrap, or DIR for a dry run.
    local payload_dir=${1:-$payload_dir}
    say "Copying the installer to $payload_dir on the VM."
    local -a tar_opts=()
    if tar --help 2>&1 | grep -q -- '--no-xattrs'; then tar_opts+=(--no-xattrs); fi
    if tar --help 2>&1 | grep -q -- '--no-mac-metadata'; then tar_opts+=(--no-mac-metadata); fi
    (
        cd "$root"
        if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
            git ls-files -z --cached --others --exclude-standard -- \
                subspace-installer subspace-api subspace-irc scripts/local-stack agent-skills |
                COPYFILE_DISABLE=1 tar ${tar_opts[@]+"${tar_opts[@]}"} --null -T - -czf -
        else
            COPYFILE_DISABLE=1 tar ${tar_opts[@]+"${tar_opts[@]}"} --exclude node_modules --exclude .data --exclude .env \
                -czf - subspace-installer subspace-api subspace-irc scripts/local-stack agent-skills
        fi
    ) | vm sudo -n sh -c "'rm -rf $payload_dir.new && mkdir -p $payload_dir.new && \
tar --warning=no-unknown-keyword --no-same-owner -xzf - -C $payload_dir.new && \
chown -R root:root $payload_dir.new && chmod -R go-w $payload_dir.new && \
rm -rf $payload_dir && mv $payload_dir.new $payload_dir'"
}

on_target() {
    # on_target NAME command...: run one of this script's functions against another saved target.
    # shellcheck disable=SC2030
    ( state=$home/targets/$1; shift; load_target; "$@" )
}

fetch_private() {
    # fetch_private FILE command...: save a remote command's output as an owner-only file.
    local file=$1
    shift
    ( umask 077; vm "$@" > "$file.new" ) && mv -f "$file.new" "$file" && chmod 600 "$file"
}

confirm_lockdown() {
    say "Confirming the lockdown over a new SSH connection."
    if vm_fresh sudo -n subspace-setup lockdown confirm "${SUBSPACE_LOCKDOWN_ID:-}"; then
        return 0
    fi
    note "A new SSH connection failed, so the change rolls back automatically within 3 minutes."
    note "Check the VM's firewall and SSH settings (INSTALL.md, 'If you get locked out'), then rerun."
    return 1
}

require_private() {
    # Keep the lockdown only once a fresh SSH connection works, then require a passing audit.
    local status report
    if [ "${SUBSPACE_LOCKDOWN:-1}" = 0 ]; then
        say "WARNING: setup ran with --no-lockdown, so this VM has no lockdown firewall or SSH hardening."
        report=$(vm sudo -n subspace-setup audit 2>&1) || printf '%s\n' "$report" | grep '^FAIL' >&2 || true
        return 0
    fi
    status=$(vm sudo -n subspace-setup lockdown status 2>&1) || die "Could not read the lockdown status on the VM."
    case $status in
        *PENDING*) confirm_lockdown || die "The lockdown was not confirmed, so setup is not finished." ;;
    esac
    status=$(vm sudo -n subspace-setup lockdown status 2>&1) || die "Could not read the lockdown status on the VM."
    case $status in
        *PENDING*|*'firewall: NOT loaded'*) die "The lockdown is not in place: $status" ;;
    esac
    case $status in
        *'firewall: loaded'*) ;;
        *) die "The lockdown firewall is not loaded on the VM." ;;
    esac
    case $status in
        *'boot: confirmed'*) ;;
        *) die "The lockdown is not confirmed, so the VM would boot without it." ;;
    esac
    if [ -n "${SUBSPACE_LOCKDOWN_ID:-}" ]; then
        case $status in
            *"(ID $SUBSPACE_LOCKDOWN_ID)"*) ;;
            *) die "The lockdown change $SUBSPACE_LOCKDOWN_ID rolled back before it was confirmed. Run the installer again." ;;
        esac
    fi
    if ! report=$(vm sudo -n subspace-setup audit 2>&1); then
        printf '%s\n' "$report" | grep '^FAIL' >&2 || true
        die "Setup finished, but the privacy audit failed. Fix the FAIL lines, then run the installer again."
    fi
    say "The lockdown is confirmed and the privacy audit passed."
}

read_summary() {
    [ -f "$state/summary.env" ] || die "No fleet summary yet; run the install first."
    SUBSPACE_PORTS='' SUBSPACE_IRC_MODE='' SUBSPACE_STARGATE_API='' SUBSPACE_FLEET_NAME='' SUBSPACE_FLEET_SLUG=''
    SUBSPACE_ADMIN_ACCOUNT='' SUBSPACE_CHANNELS='' SUBSPACE_IRC_HOST='' SUBSPACE_IRC_PORT=''
    SUBSPACE_LOCKDOWN='' SUBSPACE_LOCKDOWN_ID=''
    local key value
    while IFS='=' read -r key value; do
        case $key in
            *[!A-Z_]*|'') continue ;;
            SUBSPACE_*) eval "$key=\$value" ;;
        esac
    done < "$state/summary.env"
}

# Desktop ----------------------------------------------------------------------

desktop_dir() {
    if [ -n "${SUBSPACE_DESKTOP_DIR:-}" ]; then
        printf '%s' "$SUBSPACE_DESKTOP_DIR"
    elif [ "$os" = Darwin ] && [ ! -f "$HOME/.config/subspace/config.json" ]; then
        printf '%s' "$HOME/Library/Application Support/subspace"
    else
        printf '%s' "${XDG_CONFIG_HOME:-$HOME/.config}/subspace"
    fi
}

json_string() {
    printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"
}

network_entry() {
    local id name host port tls password ca channels='' channel
    for channel in $SUBSPACE_CHANNELS; do channels="$channels${channels:+, }$(json_string "$channel")"; done
    if [ "$SUBSPACE_IRC_MODE" = remote ]; then
        id=$IRC_TARGET name="Shared IRC ($SUBSPACE_IRC_HOST)" host=$SUBSPACE_IRC_HOST port=$SUBSPACE_IRC_PORT tls=true
        password=$home/targets/$IRC_TARGET/$SUBSPACE_ADMIN_ACCOUNT.password
        ca=$home/targets/$IRC_TARGET/ca.pem
    else
        id=$SUBSPACE_FLEET_SLUG name="$SUBSPACE_FLEET_NAME (fleet VM)" host=127.0.0.1 port=6667 tls=false
        password=$state/irc.password
        ca=
    fi
    printf '{\n'
    printf '  "id": %s,\n  "name": %s,\n' "$(json_string "$id")" "$(json_string "$name")"
    printf '  "host": %s,\n  "port": %s,\n  "tls": %s,\n' "$(json_string "$host")" "$port" "$tls"
    if [ -n "$ca" ] && [ -f "$ca" ]; then printf '  "caCert": %s,\n' "$(json_string "$ca")"; fi
    printf '  "nickname": %s,\n' "$(json_string "$SUBSPACE_ADMIN_ACCOUNT")"
    printf '  "sasl": { "username": %s, "passwordFile": %s },\n' \
        "$(json_string "$SUBSPACE_ADMIN_ACCOUNT")" "$(json_string "$password")"
    printf '  "channels": [%s],\n  "autoConnect": true\n}\n' "$channels"
}

write_desktop_config() {
    read_summary
    IRC_TARGET=${IRC_TARGET:-}
    if [ "$SUBSPACE_IRC_MODE" = remote ] && [ -z "$IRC_TARGET" ]; then
        [ -f "$state/irc-target" ] && IRC_TARGET=$(cat "$state/irc-target")
    fi
    if [ -n "$IRC_TARGET" ] && ! grep -qsFx "SUBSPACE_IRC_HOST=$SUBSPACE_IRC_HOST" "$home/targets/$IRC_TARGET/summary.env"; then
        IRC_TARGET=  # The fleet moved to another IRC server: its saved password and CA no longer apply.
        rm -f "$state/irc-target"
    fi
    if [ "$SUBSPACE_IRC_MODE" = remote ] && [ -z "$IRC_TARGET" ]; then
        say "This fleet uses an IRC server this installer does not manage ($SUBSPACE_IRC_HOST:$SUBSPACE_IRC_PORT)."
        note "In the desktop, add it under Settings > Channels > Add network: that host and port with TLS,"
        note "your IRC account '$SUBSPACE_ADMIN_ACCOUNT' and the password that server's operator gave you."
        write_fleet_console
        return
    fi
    local dir file entry
    dir=$(desktop_dir)/irc
    file=$dir/config.json
    entry=$(network_entry)
    mkdir -p "$dir"
    chmod 700 "$dir"
    if [ ! -s "$file" ]; then
        printf '{\n  "networks": [\n%s\n  ]\n}\n' "$(printf '%s\n' "$entry" | sed 's/^/    /')" > "$file"
        chmod 600 "$file"
        say "Wrote the desktop's IRC connection to $file."
        write_fleet_console
        return
    fi
    if have_python; then
        python3 -B - "$file" "$entry" <<'PY'
import json, os, sys
path, entry = sys.argv[1], json.loads(sys.argv[2])
config = json.load(open(path))
networks = [n for n in config.get("networks", []) if n.get("id") != entry["id"]]
config["networks"] = networks + [entry]
tmp = path + ".new"
with open(tmp, "w") as handle:
    os.fchmod(handle.fileno(), 0o600)
    json.dump(config, handle, indent=2)
    handle.write("\n")
os.replace(tmp, path)
PY
        say "Added the fleet's IRC connection to $file."
    else
        say "Add this network to the \"networks\" array in $file (or use Settings > Channels > Add network):"
        printf '%s\n' "$entry"
    fi
    write_fleet_console
}

have_python() {
    command -v python3 >/dev/null 2>&1 && python3 -c 'import json' >/dev/null 2>&1
}

write_fleet_console() {
    # Point the Fleet Console web Space at this fleet's Stargate API through the tunnel.
    local token=$state/stargate-api.token file endpoint built_in=0
    [ "$SUBSPACE_STARGATE_API" = 1 ] && [ -s "$token" ] || return 0
    endpoint=http://127.0.0.1:${SUBSPACE_STARGATE_API_PORT:-8484}
    file=$(desktop_dir)/config.json
    # Subspace on macOS builds Fleet Console in as fleet-console and adopts an
    # entry that has only its settings, so it can be written before the app
    # first starts. Linux shows no web Spaces; another ID is an installed copy.
    if [ "$fleet_console" = fleet-console ] && [ "$os" = Darwin ]; then built_in=1; fi
    if have_python && python3 -B - "$file" "$fleet_console" "$endpoint" "$token" "$built_in" <<'PY'
import json, os, sys
path, board, endpoint, token_file, built_in = sys.argv[1:6]
config = json.load(open(path)) if os.path.exists(path) else {}
boards = config.get("webBoards")
entry = boards.get(board) if isinstance(boards, dict) else None
if entry is None and built_in == "1" and (boards is None or isinstance(boards, dict)):
    entry = config.setdefault("webBoards", {}).setdefault(board, {})
if not isinstance(entry, dict):
    sys.exit(3)
os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
entry.setdefault("settings", {})["endpoint"] = endpoint
entry.setdefault("secrets", {})["token"] = open(token_file).read().strip()
tmp = path + ".new"
with open(tmp, "w") as handle:
    os.fchmod(handle.fileno(), 0o600)
    json.dump(config, handle, indent=2)
    handle.write("\n")
os.replace(tmp, path)
PY
    then
        say "Set the Fleet Console's Stargate API address ($endpoint) and token in $file."
    else
        say "Fleet Console: enter the address $endpoint and the token from $token"
        note "in Settings > Spaces > Fleet Console; or, once Subspace has a web Space with Board ID '$fleet_console',"
        note "quit the app and run './install.sh desktop' again."
    fi
}

# Commands -------------------------------------------------------------------

do_install() {
    if [ -z "$mode" ]; then
        if [ -f "$state/target.env" ]; then
            load_target
            [ "$KIND" = lima ] && mode=local || mode=reuse
        elif interactive; then
            printf 'Where should the fleet run?\n  1) a new local VM on this machine\n  2) an existing VM over SSH (for example in the cloud)\n'
            printf 'Choose 1 or 2: '
            read -r choice
            case $choice in
                1) mode=local ;;
                2) printf 'SSH login (USER@HOST): '; read -r remote; mode=remote ;;
                *) die "Choose 1 or 2." ;;
            esac
        else
            die "Choose --local or --remote USER@HOST."
        fi
    fi
    case $mode in
        local) lima_create; load_target ;;
        remote) remote_connect ;;
        reuse) ;;
    esac
    vm true || die "Cannot reach the VM over SSH."
    # The fleet's record in the Subspace API names the VM by the host this computer reaches it at (target.env's HOST).
    # shellcheck disable=SC2153
    if [ "$KIND" = ssh ]; then passthrough+=(--vm-host "${HOST#*@}"); fi

    # A Subspace API on another host (--api-url, passed to setup) replaces another fleet's API, so a saved
    # --api-target from an earlier run is set aside, and forgotten once a real run succeeds.
    local hosted=false arg
    for arg in ${passthrough[@]+"${passthrough[@]}"}; do case $arg in --api-url|--api-url=*) hosted=true ;; esac; done
    if [ -z "$api_target" ] && [ -f "$state/api-target" ]; then
        if "$hosted"; then
            note "Leaving out the saved --api-target $(cat "$state/api-target"): --api-url names this fleet's API now."
        else
            api_target=$(cat "$state/api-target")
        fi
    fi
    if [ -n "$api_target" ]; then
        [ "$api_target" != "$target" ] || die "--api-target must name another fleet."
        grep -qs '^SUBSPACE_API=1$' "$home/targets/$api_target/summary.env" ||
            die "No fleet '$api_target' with a Subspace API. Install that fleet first, without --no-api."
        if [ -z "$irc_target" ] && [ ! -f "$state/irc-target" ]; then
            die "--api-target needs a shared IRC server (--irc-server NAME): IRC on this fleet's own VM is unreachable from the first fleet's API."
        fi
        on_target "$api_target" vm sudo -n subspace-setup register-fleet --check ||
            die "The Subspace API on '$api_target' cannot register this fleet; nothing was changed."
        passthrough+=(--no-api)
    fi
    # subspace-setup --dry-run changes nothing on the VM: the files it is given go to a scratch directory that
    # is removed afterwards, never over the inputs (such as the database URL) that the next real run uses.
    local dry_run=false inputs=/root/fleet
    for arg in ${passthrough[@]+"${passthrough[@]}"}; do
        if [ "$arg" = --dry-run ]; then dry_run=true; inputs=/root/subspace-setup-dry-run; fi
    done
    # Directories on the VM that hold this run's own copies (a dry run's files, the IRC operator's passwords),
    # removed when the run ends, however it ends.
    transient=()
    trap remove_transient EXIT
    "$dry_run" && transient+=("$inputs")
    # A Subspace API on another host excludes another fleet's API and a database here.
    if "$hosted"; then
        [ -z "$api_target" ] || die "--api-url names the fleet's API, so --api-target does not apply."
        [ -z "$database_url_file" ] || die "--api-url uses an API on another host, which keeps its own database: leave out --database-url-file."
    else
        [ -z "$api_enrolment_token_file" ] || die "--api-enrolment-token-file goes with --api-url."
    fi
    if [ -n "$irc_target" ]; then
        [ -f "$home/targets/$irc_target/summary.env" ] || die "No shared IRC server '$irc_target'. Run ./install.sh irc-server first."
        local irc_host irc_port self_signed
        irc_host=$(sed -n 's/^SUBSPACE_IRC_HOST=//p' "$home/targets/$irc_target/summary.env")
        irc_port=$(sed -n 's/^SUBSPACE_IRC_PORT=//p' "$home/targets/$irc_target/summary.env")
        self_signed=$(sed -n 's/^SUBSPACE_IRC_SELF_SIGNED=//p' "$home/targets/$irc_target/summary.env")
        # The shared server this installer runs seeds each officer's account with a password.
        passthrough+=(--irc remote --irc-host "$irc_host" --irc-port "$irc_port" --irc-auth password)
        if [ "$self_signed" = 1 ]; then
            vm sudo -n sh -c "'umask 077; mkdir -p $inputs; cat > $inputs/irc-ca.pem'" < "$home/targets/$irc_target/ca.pem"
            passthrough+=(--irc-ca "$inputs/irc-ca.pem")
        fi
        "$dry_run" || printf '%s\n' "$irc_target" > "$state/irc-target"
    fi

    if [ -n "$database_url_file" ]; then
        # The URL holds the database's password: it travels over SSH on stdin, never in a command line.
        [ -f "$database_url_file" ] && [ -r "$database_url_file" ] || die "Cannot read --database-url-file $database_url_file."
        local perms
        if [ "$os" = Darwin ]; then perms=$(stat -f %Lp "$database_url_file"); else perms=$(stat -c %a "$database_url_file"); fi
        case $perms in [0-7]00) ;; *) die "--database-url-file $database_url_file must be private to you: chmod 600 it." ;; esac
        [ -z "$api_target" ] || die "--database-url-file runs this VM's own Subspace API, which --api-target leaves out."
        vm sudo -n sh -c "'umask 077; mkdir -p $inputs; cat > $inputs/database-url.new && mv -f $inputs/database-url.new $inputs/database-url'" < "$database_url_file" ||
            die "Could not copy the database URL to the VM."
        passthrough+=(--database-url-file "$inputs/database-url")
    fi
    if [ -n "$database_ca_file" ]; then
        [ -f "$database_ca_file" ] || die "Cannot read --database-ca-file $database_ca_file."
        vm sudo -n sh -c "'umask 077; mkdir -p $inputs; cat > $inputs/database-ca.pem.new && mv -f $inputs/database-ca.pem.new $inputs/database-ca.pem'" < "$database_ca_file" ||
            die "Could not copy the database CA certificate to the VM."
        passthrough+=(--database-ca-file "$inputs/database-ca.pem")
    fi
    # The IRC operator's passwords and the hosted API's enrolment token are for this run only: checked here,
    # copied to the VM only after the installer, never into the fleet's inputs (setup keeps what it needs), and
    # removed when the run ends.
    local operator_dir=/root/subspace-setup-operator option file name operator_perms
    "$dry_run" && operator_dir=$inputs
    for option in irc-operator-password irc-operator-account-password api-enrolment-token; do
        file=$(run_only_file "$option")
        [ -n "$file" ] || continue
        [ -f "$file" ] && [ -r "$file" ] || die "Cannot read --$option-file $file."
        if [ "$os" = Darwin ]; then operator_perms=$(stat -f %Lp "$file"); else operator_perms=$(stat -c %a "$file"); fi
        case $operator_perms in [0-7]00) ;; *) die "--$option-file $file must be private to you: chmod 600 it." ;; esac
    done

    # The dry run's installer copy goes beside its files and is removed with them, so the installed
    # /opt/subspace/bootstrap and the subspace-setup command stay as the last real run left them.
    if "$dry_run"; then send_payload "$inputs/bootstrap"; else send_payload; fi
    for option in irc-operator-password irc-operator-account-password api-enrolment-token; do
        file=$(run_only_file "$option")
        [ -n "$file" ] || continue
        name=$operator_dir/$option
        transient+=("$operator_dir")
        vm sudo -n sh -c "'umask 077; mkdir -p $operator_dir; cat > $name'" < "$file" ||
            die "Could not copy the --$option-file to the VM."
        passthrough+=("--$option-file" "$name")
    done

    local -a setup_args=(setup)
    interactive || setup_args+=(--non-interactive)
    setup_args+=(${passthrough[@]+"${passthrough[@]}"})
    local setup_status=0
    if "$dry_run"; then
        say "Running subspace-setup --dry-run on the VM from $inputs, which is removed afterwards."
        vm_tty sudo "$inputs/bootstrap/subspace-installer/vm/setup.sh" "$(quote_args "${setup_args[@]}")" || setup_status=$?
        remove_transient
        [ "$setup_status" = 0 ] || die "The dry run failed; read subspace-setup's output above."
        say "Dry run complete. Run the same command without --dry-run to install."
        return
    fi
    say "Running subspace-setup on the VM. This takes a while the first time."
    vm_tty sudo "$setup_cmd" "$(quote_args "${setup_args[@]}")" || setup_status=$?
    remove_transient
    [ "$setup_status" = 0 ] || die "subspace-setup failed on the VM; read its output above."

    fetch_private "$state/summary.env" sudo -n cat /root/fleet/summary.env
    read_summary
    require_private
    if [ "$SUBSPACE_IRC_MODE" = local ]; then
        fetch_private "$state/irc.password" sudo -n subspace-setup secret irc-admin
    elif [ -n "$irc_target" ]; then
        local bundle
        bundle=$(mktemp)
        fetch_private "$bundle" sudo -n subspace-setup irc-bundle
        say "Adding the fleet's accounts and channels to the shared IRC server '$irc_target'."
        on_target "$irc_target" vm sudo -n subspace-setup irc-server add - < "$bundle"
        rm -f "$bundle"
        on_target "$irc_target" fetch_private "$home/targets/$irc_target/$SUBSPACE_ADMIN_ACCOUNT.password" \
            sudo -n subspace-setup irc-server secret "$SUBSPACE_ADMIN_ACCOUNT"
    fi
    if [ "$SUBSPACE_STARGATE_API" = 1 ]; then
        fetch_private "$state/stargate-api.token" sudo -n subspace-setup stargate-token
    fi
    if [ -n "$api_target" ]; then
        local registration
        registration=$(mktemp)
        vm sudo -n subspace-setup fleet-registration > "$registration"
        say "Registering fleet '$SUBSPACE_FLEET_NAME' and its initiative with the Subspace API on '$api_target'."
        on_target "$api_target" vm sudo -n subspace-setup register-fleet - < "$registration"
        rm -f "$registration"
        printf '%s\n' "$api_target" > "$state/api-target"
    fi
    if "$hosted"; then rm -f "$state/api-target"; fi
    IRC_TARGET=$irc_target write_desktop_config
    say "Done. Fleet '$SUBSPACE_FLEET_NAME' is running on a private VM."
    note "1. Sign in to the fleet's shared CLIProxyAPI for the models you chose:"
    note "   Claude: ./install.sh providers login claude --target $target"
    note "   Codex:  ./install.sh providers login codex-device --target $target"
    note "   Check existing accounts with ./install.sh providers --target $target."
    note "2. Start the tunnel:   ./install.sh tunnel --target $target --background"
    note "3. Run the desktop:    make run    (it signs in to 127.0.0.1:3000 as the admin)"
    note "4. Check privacy:      ./install.sh audit --target $target"
}

do_irc_server() {
    case $mode in
        local) lima_create; load_target ;;
        remote) remote_connect ;;
        '') load_target ;;
    esac
    send_payload
    local -a args=(irc-server setup)
    interactive || args+=(--non-interactive)
    args+=(${passthrough[@]+"${passthrough[@]}"})
    vm_tty sudo "$setup_cmd" "$(quote_args "${args[@]}")"
    fetch_private "$state/summary.env" sudo -n cat /root/irc/summary.env
    read_summary
    require_private
    vm sudo -n subspace-setup irc-server ca > "$state/ca.pem"
    chmod 644 "$state/ca.pem"
    say "Shared IRC server '$target' is ready. Add fleets with: ./install.sh --irc-server $target ..."
}

do_tunnel() {
    load_target
    read_summary
    local -a forwards=()
    local port busy=
    for port in $SUBSPACE_PORTS; do
        if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then busy="$busy $port"; fi
        forwards+=(-L "127.0.0.1:$port:127.0.0.1:$port")
    done
    [ -z "$busy" ] || die "Ports already in use here:$busy. Stop whatever uses them (for example 'make stack-down') first."
    ssh_opts
    say "Forwarding 127.0.0.1 ports$SUBSPACE_PORTS to the fleet VM."
    note "API http://127.0.0.1:3000  IRC 127.0.0.1:6667  dashboard http://127.0.0.1:8070  AgentsView http://127.0.0.1:8080"
    [ "$SUBSPACE_STARGATE_API" = 1 ] && note "Stargate Fleet API http://127.0.0.1:8484 (token in $state/stargate-api.token)"
    local -a extra=()
    [ "$background" = true ] && extra+=(-f)
    exec ssh -N ${extra[@]+"${extra[@]}"} -o ExitOnForwardFailure=yes -o ControlMaster=no -o ControlPath=none \
        -o ServerAliveInterval=30 "${forwards[@]}" "${ssh_args[@]}"
}

do_providers() {
    load_target
    local provider port=
    if [ "${passthrough[0]:-}" != login ]; then
        vm sudo -n subspace-setup providers status || true
        return
    fi
    provider=${passthrough[1]:-}
    case $provider in claude) port=54545 ;; codex) port=1455 ;; esac
    ssh_opts
    if [ -n "$port" ]; then
        say "Forwarding port $port so the $provider login can finish in your browser."
        ssh -t -o ExitOnForwardFailure=yes -o ControlMaster=no -o ControlPath=none \
            -L "$port:127.0.0.1:$port" "${ssh_args[@]}" sudo subspace-setup providers login "$provider"
    else
        vm_tty sudo subspace-setup providers login "$(quote_args "$provider")"
    fi
}

do_skills() {
    # Local agents (Claude Code, Codex, OpenCode) on this computer: skills, helpers and MCP.
    local agents='' person='' agent bin=$home/bin mcp hosted i=0
    command -v python3 >/dev/null 2>&1 || die "The skills' helpers need Python 3."
    hosted=$(python3 "$root/agent-skills/bin/subspace_token.py" --workos-mcp-url) || return 1
    mcp=${hosted:-http://127.0.0.1:3000/mcp}
    if [ -z "$hosted" ]; then
        load_target
        [ -f "$state/summary.env" ] || die "Install the fleet first."
    fi
    while [ $i -lt ${#passthrough[@]} ]; do
        case ${passthrough[$i]} in
            --agents) i=$((i + 1)); agents=${passthrough[$i]:-} ;;
            --name) i=$((i + 1)); person=${passthrough[$i]:-} ;;
            *) die "Unknown skills option ${passthrough[$i]}." ;;
        esac
        i=$((i + 1))
    done
    if [ -z "$agents" ]; then
        for agent in claude codex opencode; do
            command -v "$agent" >/dev/null 2>&1 && agents="$agents${agents:+,}$agent"
        done
        [ -n "$agents" ] || die "No Claude Code, Codex or OpenCode found. Pass --agents to choose."
    fi
    if [ -z "$person" ] && [ -f "$home/agents.env" ]; then
        person=$(sed -n 's/^SUBSPACE_HUMAN_NAME=//p' "$home/agents.env")
    fi
    if [ -z "$person" ] && [ -f "$state/summary.env" ]; then
        person=$(sed -n 's/^SUBSPACE_ADMIN_ACCOUNT=//p' "$state/summary.env")
    fi
    [ -z "$hosted" ] || [ -n "$person" ] || die "Pass --name for the person these agents act for."
    mkdir -p "$bin"
    install -m 0755 "$root/agent-skills/bin/subspace_token.py" "$root/agent-skills/bin/subspace_irc.py" "$bin/"
    printf 'SUBSPACE_TARGET=%s\nSUBSPACE_HUMAN_NAME=%s\n' "$target" "$person" > "$home/agents.env"
    say "Local agents will sign in as you and mark their messages 'on behalf of $person (<model>)'."
    local IFS_save=$IFS dest claude_config
    # A hosted registration pins its URL, so the helper refuses to send a token after agent.json moves to
    # another API. The development registration stays as it was.
    claude_config=$(python3 -B - "$bin/subspace_token.py" "$mcp" "$hosted" <<'PY'
import json, shlex, sys
script, url, hosted = sys.argv[1:]
helper = ["python3", script, "--header"] + (["--mcp-endpoint", url] if hosted else [])
print(json.dumps({"type": "http", "url": url, "headersHelper": shlex.join(helper)}))
PY
    )
    IFS=,
    for agent in $agents; do
        IFS=$IFS_save
        case $agent in
            claude) dest=$HOME/.claude/skills ;;
            codex) dest=${CODEX_HOME:-$HOME/.codex}/skills ;;
            opencode) dest=${XDG_CONFIG_HOME:-$HOME/.config}/opencode/skills ;;
            *) die "Unknown agent $agent (use claude, codex or opencode)." ;;
        esac
        mkdir -p "$dest"
        rm -rf "$dest/subspace-irc" "$dest/subspace-tracker"
        cp -R "$root/agent-skills/subspace-irc" "$root/agent-skills/$agent/subspace-tracker" "$dest/"
        case $agent in
            claude)
                if command -v claude >/dev/null 2>&1; then
                    claude mcp remove --scope user subspace >/dev/null 2>&1 || true
                    claude mcp add-json --scope user subspace "$claude_config" >/dev/null
                fi ;;
            codex)
                if command -v codex >/dev/null 2>&1; then
                    codex mcp remove subspace >/dev/null 2>&1 || true
                    if [ -n "$hosted" ]; then
                        codex mcp add subspace -- python3 "$bin/subspace_token.py" --mcp-stdio --mcp-endpoint "$mcp" >/dev/null
                    else
                        codex mcp add subspace --url "$mcp" --bearer-token-env-var SUBSPACE_API_TOKEN >/dev/null
                    fi
                fi ;;
            opencode)
                python3 -B - "${XDG_CONFIG_HOME:-$HOME/.config}/opencode/opencode.json" "$mcp" "$hosted" "$bin/subspace_token.py" <<'PY' ||
import json, os, sys
path, url, hosted, script = sys.argv[1:]
config = {"$schema": "https://opencode.ai/config.json"}
if os.path.exists(path):
    config = json.load(open(path))
config.setdefault("mcp", {})["subspace"] = (
    {"type": "local", "command": ["python3", script, "--mcp-stdio", "--mcp-endpoint", url], "enabled": True}
    if hosted else
    {"type": "remote", "url": url, "enabled": True,
     "headers": {"Authorization": "Bearer {env:SUBSPACE_API_TOKEN}"}}
)
os.makedirs(os.path.dirname(path), exist_ok=True)
tmp = path + ".new"
with open(tmp, "w") as handle:
    json.dump(config, handle, indent=2)
    handle.write("\n")
os.replace(tmp, path)
PY
                    note "Could not edit opencode.json (comments?). Add mcp.subspace by hand; see agent-skills/README.md." ;;
        esac
        say "Installed the Subspace skills for $agent in $dest."
        IFS=,
    done
    IFS=$IFS_save
    if [ -n "$hosted" ]; then
        note "Sign in once: python3 \"$bin/subspace_token.py\" --login"
        return
    fi
    note "Claude Code refreshes its API token by itself. For Codex and OpenCode, start them with:"
    note "  export SUBSPACE_API_TOKEN=\"\$(python3 $bin/subspace_token.py)\""
    note "Keep the tunnel running: ./install.sh tunnel --target $target --background"
}

case $command in
    install) do_install ;;
    skills) do_skills ;;
    providers) do_providers ;;
    irc-server) do_irc_server ;;
    tunnel) do_tunnel ;;
    desktop)
        load_target
        if grep -q '^SUBSPACE_STARGATE_API=1' "$state/summary.env" 2>/dev/null && vm true 2>/dev/null; then
            fetch_private "$state/stargate-api.token" sudo -n subspace-setup stargate-token || true
        fi
        IRC_TARGET=$irc_target write_desktop_config ;;
    ssh) load_target; ssh_opts; do_ssh ${passthrough[@]+"${passthrough[@]}"} ;;
    audit) load_target; vm sudo -n subspace-setup audit ;;
    status)
        load_target
        cat "$state/target.env"
        [ "$KIND" = lima ] && limactl list "$VM"
        vm sudo -n subspace-setup status ;;
    stop|start)
        load_target
        [ "$KIND" = lima ] || die "Only local VMs can be stopped or started here."
        limactl "$command" "$VM" ;;
    destroy)
        load_target
        [ "$KIND" = lima ] || die "Delete remote VMs with your cloud provider. This only removes local VMs."
        interactive || die "destroy asks for confirmation; run it in a terminal."
        printf 'Type the VM name (%s) to delete it and its saved credentials: ' "$VM"
        read -r answer
        [ "$answer" = "$VM" ] || die "Not deleted."
        limactl delete --force "$VM"
        rm -rf "$state"
        say "Deleted $VM." ;;
esac
