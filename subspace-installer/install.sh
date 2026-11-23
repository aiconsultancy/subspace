#!/usr/bin/env bash
set -euo pipefail
umask 077
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
app=/opt/subspace-fleet
etc=/etc/subspace-fleet
state=/var/lib/subspace-fleet
docker_gpg_sha256=1500c1f56fa9e26b9b8f42452a553675796ade0807cdce11975eb98170b3a570
action=install
config=
secrets=
app_secrets=
prune_secrets=false
dry_run=false
no_systemd=false

usage() {
    cat <<'USAGE'
Usage: install.sh [install|upgrade|uninstall] [--config FILE] [--secrets FILE]
                  [--secrets-dir DIR] [--prune-secrets] [--dry-run] [--no-systemd]
Config is required for the first install. Upgrade reuses the saved inputs by
default. Uninstall stops the fleet and retains its accounts and persistent data.
USAGE
}

die() { printf 'subspace-fleet: %s\n' "$*" >&2; exit 1; }

while (( $# )); do
    case $1 in
        install|upgrade|uninstall) action=$1; shift ;;
        --config) config=${2:?missing config file}; shift 2 ;;
        --secrets) secrets=${2:?missing secrets file}; shift 2 ;;
        --secrets-dir) app_secrets=${2:?missing secrets directory}; shift 2 ;;
        --prune-secrets) prune_secrets=true; shift ;;
        --dry-run) dry_run=true; shift ;;
        --no-systemd) no_systemd=true; shift ;;
        --help|-h) usage; exit ;;
        *) die "Unknown argument $1" ;;
    esac
done

if [[ $action == uninstall ]]; then
    [[ -z $config && -z $secrets && -z $app_secrets && $prune_secrets == false ]] || die 'Uninstall uses the installed fleet.'
else
    config=${config:-$etc/fleet.json}
    [[ -f $config ]] || die 'Supply a fleet config with --config.'
    if [[ $(realpath -- "$config") == "$etc/fleet.json" ]]; then
        secrets=${secrets:-$etc/secrets.json}
    else
        if [[ -z $secrets ]]; then
            secrets=${config%.json}.secrets.json
            if [[ $action == upgrade && ! -e $secrets && -f $etc/secrets.json ]]; then
                secrets=$etc/secrets.json
            fi
        fi
        if [[ -z $app_secrets && -e ${config%.json}.secrets ]]; then
            app_secrets=${config%.json}.secrets
        fi
    fi
    [[ -z $app_secrets || -d $app_secrets ]] || die 'The secrets directory does not exist.'
    [[ $prune_secrets == false || -n $app_secrets ]] || die '--prune-secrets requires a secrets directory.'
fi
if ! "$dry_run"; then
    (( EUID == 0 )) || die 'Run the installer as root.'
fi

# shellcheck disable=SC1091
. /etc/os-release
case "${ID:-}:${VERSION_ID:-}" in
    ubuntu:24.04|debian:12) ;;
    *) die 'Use Ubuntu 24.04 or Debian 12.' ;;
esac
arch=$(dpkg --print-architecture)
[[ $arch == amd64 || $arch == arm64 ]] || die "Unsupported architecture $arch."
runtime=$here/lib/runtime.py

if "$dry_run"; then
    command -v python3 >/dev/null || die 'Install python3 to validate a dry run.'
    if [[ $action == uninstall ]]; then
        printf 'Would disable fleet services and remove installed programs and units. Fleet state and system packages are retained.\n'
    else
        python3 "$runtime" plan "$config" "$secrets" "$app_secrets" "$prune_secrets"
        printf 'Would install packages and pinned %s binaries, converge service files, and apply optional fleet registration.\n' "$arch"
        if "$no_systemd"; then
            printf 'Would skip systemd and container startup.\n'
        fi
    fi
    exit
fi

exec 9>/run/lock/subspace-fleet.lock
flock 9
if ! "$no_systemd"; then
    [[ $(cat /proc/1/comm) == systemd ]] || die 'Use --no-systemd when PID 1 is not systemd.'
fi

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

install_file() {
    local source=$1 destination=$2 mode=$3
    if ! cmp -s -- "$source" "$destination" || [[ $(stat -c %a -- "$destination") != "${mode#0}" ]]; then
        install -m "$mode" -- "$source" "$destination.tmp"
        mv -Tf -- "$destination.tmp" "$destination"
        touch "$state/pending"
    fi
}

uninstall() {
    local unit
    local -a units=() enabled=()
    if [[ -f $etc/units || -f $etc/retired-units ]]; then
        mapfile -t units < <(sort -u -- "$etc/"*units)
        for unit in "${units[@]}"; do
            [[ $unit =~ ^subspace-fleet-[a-z0-9@-]+\.(service|timer|target)$ ]] || die 'Invalid installed unit name.'
            if [[ -f /etc/systemd/system/$unit ]]; then
                enabled+=("$unit")
            fi
        done
        if ! "$no_systemd" && (( ${#enabled[@]} )); then
            systemctl disable --now "${enabled[@]}"
        fi
        for unit in "${enabled[@]}"; do
            rm -f -- "/etc/systemd/system/$unit"
        done
        if ! "$no_systemd"; then
            systemctl daemon-reload
            if /usr/sbin/nft list table inet subspace_fleet >/dev/null 2>&1; then
                /usr/sbin/nft delete table inet subspace_fleet
            fi
        fi
    fi
    rm -rf -- "$app"
    rm -f -- /usr/local/bin/subspace-fleetctl "$etc/units" "$etc/retired-units" "$state/pending"
    printf 'Fleet programs removed. Configuration and data remain under %s and %s.\n' "$etc" "$state"
}

if [[ $action == uninstall ]]; then
    uninstall
    exit
fi

ensure_packages() {
    local package
    local -a missing=()
    for package in "$@"; do
        if [[ $(dpkg-query -W -f '${db:Status-Status}' "$package" 2>/dev/null) != installed ]]; then
            missing+=("$package")
        fi
    done
    if (( ${#missing[@]} )); then
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${missing[@]}"
    fi
}

install_docker() {
    local base=https://download.docker.com/linux/$ID
    install -d -m 0755 /etc/apt/keyrings
    if [[ ! -s /etc/apt/keyrings/subspace-docker.asc ]]; then
        curl -fsSL --retry 3 "$base/gpg" -o "$scratch/docker.asc"
        # Docker's release key file (fingerprint 9DC8 5822 9FC7 DD38 854A E2D8 8D81 803C 0EBF CD88).
        printf '%s  %s\n' "$docker_gpg_sha256" "$scratch/docker.asc" | sha256sum --check --status ||
            die 'Docker apt signing key checksum mismatch.'
        install_file "$scratch/docker.asc" /etc/apt/keyrings/subspace-docker.asc 0644
    fi
    cat > "$scratch/docker.sources" <<SOURCE
Types: deb
URIs: $base
Suites: $VERSION_CODENAME
Components: stable
Architectures: $arch
Signed-By: /etc/apt/keyrings/subspace-docker.asc
SOURCE
    install_file "$scratch/docker.sources" /etc/apt/sources.list.d/subspace-docker.sources 0644
    ensure_packages docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
}

install_releases() {
    local tool target_arch version repo asset checksum member tag record previous
    while read -r tool target_arch version repo asset checksum member; do
        [[ $target_arch == "$arch" ]] || continue
        record="$app/bin/.$tool"
        if [[ -x $app/bin/$tool && -f $record ]] &&
            read -r previous < "$record" && [[ $previous == "$checksum" ]]; then
            continue
        fi
        tag=v$version
        [[ $tool != bun ]] || tag=bun-$tag
        curl -fsSL --retry 3 "https://github.com/$repo/releases/download/$tag/$asset" -o "$scratch/$asset"
        (cd -- "$scratch" && printf '%s  %s\n' "$checksum" "$asset" | sha256sum --check --status)
        case $asset in
            *.zip) unzip -p "$scratch/$asset" "$member" > "$scratch/$tool" ;;
            *.tar.gz) tar -xOzf "$scratch/$asset" "$member" > "$scratch/$tool" ;;
            *)
                [[ $member == - ]] || die 'Unknown release asset format.'
                cp -- "$scratch/$asset" "$scratch/$tool"
                ;;
        esac
        install_file "$scratch/$tool" "$app/bin/$tool" 0755
        printf '%s\n' "$checksum" > "$record"
        printf 'Installed %s %s\n' "$tool" "$version"
    done < "$here/lib/releases.tsv"
}

checkout_source() {
    local name=$1 repo=$2 revision=$3 destination=$app/src/$1-$3 staging=$app/src/.$1-$3
    if [[ -d $destination ]]; then
        [[ $(git -C "$destination" rev-parse HEAD) == "$revision" ]] || die "Unexpected revision in $destination."
        # Restore the immutable installer-owned source on upgrades from patched releases.
        if [[ $name == stargate ]] && ! git -C "$destination" diff --quiet HEAD --; then
            (umask 022; git -C "$destination" restore --source=HEAD --staged --worktree -- .)
            touch "$state/pending"
        fi
        return
    fi
    rm -rf -- "$staging"
    git init -q "$staging"
    git -C "$staging" -c advice.detachedHead=false fetch -q --depth=1 "$repo" "$revision"
    git -C "$staging" checkout -q --detach FETCH_HEAD
    [[ $(git -C "$staging" rev-parse HEAD) == "$revision" ]] || die "Unexpected $name revision."
    # Fetch the release tag that points at this commit, if any, so the checkout knows its
    # version (Stargate reports `git tag --points-at HEAD` through its Fleet API).
    local tag
    tag=$(git ls-remote --tags "$repo" 2>/dev/null | awk -v sha="$revision" \
        '$1 == sha { ref = $2; sub("^refs/tags/", "", ref); sub("[\\^][{][}]$", "", ref); print ref }' |
        grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n 1) || true
    if [[ -n $tag ]]; then
        git -C "$staging" fetch -q --depth=1 --no-tags "$repo" "refs/tags/$tag:refs/tags/$tag" || true
    fi
    chmod -R a+rX "$staging"
    mv -T -- "$staging" "$destination"
    touch "$state/pending"
}

start_services() {
    local unit
    local -a units=() retired=()
    if "$no_systemd"; then
        printf 'Service definitions installed. Activation skipped (--no-systemd).\n'
        return
    fi
    if [[ -f $etc/retired-units ]]; then
        mapfile -t retired < "$etc/retired-units"
    fi
    while IFS= read -r unit; do
        if grep -qxF '[Install]' "/etc/systemd/system/$unit"; then
            units+=("$unit")
        fi
    done < "$etc/units"
    for unit in "${retired[@]}"; do
        if [[ -f /etc/systemd/system/$unit ]]; then
            systemctl disable --now "$unit"
            rm -f -- "/etc/systemd/system/$unit"
        fi
    done
    if [[ -e $state/pending ]]; then
        systemctl daemon-reload
        systemctl enable "${units[@]}"
        systemctl restart "${units[@]}"
    else
        for unit in "${units[@]}"; do
            if ! systemctl is-enabled --quiet "$unit"; then
                systemctl enable "$unit"
            fi
        done
        systemctl start "${units[@]}"
    fi
    rm -f -- "$etc/retired-units"
}

if ! command -v python3 >/dev/null; then
    ensure_packages python3
fi
python3 "$runtime" prepare "$config" "$secrets" "$scratch" "$app_secrets" "$prune_secrets"
install -d -m 0755 "$app" "$app/bin" "$app/src" "$app/lib" "$state"
install -d -m 0711 "$etc"
# Copy and use the validated snapshots so startup never reads a changed input file.
install_file "$scratch/fleet.json" "$etc/fleet.json" 0600
install_file "$scratch/secrets.json" "$etc/secrets.json" 0600
install_file "$scratch/extra-secrets.json" "$etc/extra-secrets.json" 0600
mapfile -t source < "$scratch/sources"
ensure_packages ca-certificates curl git openssh-client tmux unzip xz-utils build-essential jq ripgrep rsync nftables python3-venv
if [[ ${source[2]} == local ]]; then
    install_docker
fi
if [[ -d $scratch/github-apps ]]; then
    ensure_packages gh openssl
    install -d -m 0700 "$etc/github-apps"
    for key in "$scratch/github-apps/"*/github-app.pem; do
        member=${key%/github-app.pem}
        member=${member##*/}
        openssl rsa -in "$key" -passin pass: -noout -check >/dev/null 2>&1 ||
            die "Invalid RSA private key for $member."
        install -d -m 0700 "$etc/github-apps/$member"
        install_file "$key" "$etc/github-apps/$member/github-app.pem" 0600
    done
fi
install_releases
checkout_source stargate "${source[0]}" "${source[1]}"
stargate=$app/src/stargate-${source[1]}
if [[ ! -f $stargate/.subspace-deps ]]; then
    install -d -m 0700 "$scratch/bun-cache"
    BUN_INSTALL_CACHE_DIR="$scratch/bun-cache" "$app/bin/bun" install --cwd "$stargate" --frozen-lockfile --production --ignore-scripts
    chmod -R a+rX "$stargate/node_modules"
    touch "$stargate/.subspace-deps" "$state/pending"
fi
if [[ ${source[2]} == local ]]; then
    checkout_source irc "${source[3]}" "${source[4]}"
    [[ -f $app/src/irc-${source[4]}/subspace-irc/compose.yaml ]] || die 'Pinned Subspace revision has no IRC Compose file.'
fi
for program in runtime.py dashboard.py fleetctl.py github_app.py github_policy.py agent_files.py workspaces.py network.py roles.py medical.py sessions.py standard_channels.py fleet_records.py; do
    install_file "$here/lib/$program" "$app/lib/$program" 0755
done
install -d -m 0755 "$app/lib/git-hooks"
install_file "$here/lib/git-hooks/pre-push" "$app/lib/git-hooks/pre-push" 0755
# The generic Subspace API skill that officers with the API's agent listener receive (lib/roles.py).
if [[ -f $here/../agent-skills/opencode/subspace-tracker/SKILL.md ]]; then
    install -d -m 0755 "$app/lib/agent-skills/subspace-tracker"
    install_file "$here/../agent-skills/opencode/subspace-tracker/SKILL.md" "$app/lib/agent-skills/subspace-tracker/SKILL.md" 0644
fi
for role in rear-admiral captain crew; do
    install -d -m 0755 "$app/lib/roles/$role"
    for template in "$here/lib/roles/$role/"*.md; do
        install_file "$template" "$app/lib/roles/$role/${template##*/}" 0644
    done
done
if [[ $(readlink /usr/local/bin/subspace-fleetctl) != "$app/lib/fleetctl.py" ]]; then
    ln -sfnT "$app/lib/fleetctl.py" /usr/local/bin/subspace-fleetctl
fi
if [[ $(readlink "$app/bin/subspace-github") != "$app/lib/github_app.py" ]]; then
    ln -sfnT "$app/lib/github_app.py" "$app/bin/subspace-github"
fi
python3 "$app/lib/runtime.py" render "$etc/fleet.json" "$etc/secrets.json" "$prune_secrets"
python3 "$app/lib/runtime.py" check
if [[ ${source[2]} == local ]]; then
    "$app/lib/runtime.py" irc config --quiet
fi
if ! "$no_systemd"; then
    # Update pinned captain sessions' permissions on their running engines before the gateway
    # restarts with the new policy. Best effort: the full commission below reports any failure.
    python3 "$app/lib/runtime.py" commission "$prune_secrets" existing
fi
start_services
if "$no_systemd"; then
    printf 'Captains with crew pin their root session once their OpenCode engine runs (runtime.py commission).\n'
else
    rm -f -- "$state/pending"
    # Crew need their captain's root session, created on the running engine. The gateway serves a
    # captain with crew only once its pin is rendered, so it keeps serving the rest meanwhile.
    commissioned=true
    python3 "$app/lib/runtime.py" commission "$prune_secrets" || commissioned=false
    if [[ -e $state/pending ]]; then
        python3 "$app/lib/runtime.py" check
        start_services
        rm -f -- "$state/pending"
    fi
    "$commissioned" || die 'Commissioning did not complete (see above). Running services keep running. Fix the cause and run the installer again.'
fi
if "$prune_secrets"; then
    python3 "$app/lib/runtime.py" prune-app-keys
fi
if ! "$no_systemd"; then
    "$app/lib/fleetctl.py" register
fi
printf 'Fleet installation complete.\n'
