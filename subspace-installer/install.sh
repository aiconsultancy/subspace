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
verify_toolchains=false

usage() {
    cat <<'USAGE'
Usage: install.sh [install|upgrade|uninstall] [--config FILE] [--secrets FILE]
                  [--secrets-dir DIR] [--prune-secrets] [--dry-run] [--no-systemd]
                  [--verify-toolchains]
Config is required for the first install. Upgrade reuses the saved inputs by
default. Uninstall stops the fleet and retains its accounts and persistent data.
--verify-toolchains runs each repository's basic check as a crew member would
after installing, and reports pass or fail per repository without failing.
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
        --verify-toolchains) verify_toolchains=true; shift ;;
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
# Officers with the API's agent listener get the generic Subspace API skill (lib/roles.py), which the
# payload carries beside the installer. Say which file is missing rather than failing the render later.
api_skill=$here/../agent-skills/opencode/subspace-tracker/SKILL.md
if [[ $action != uninstall && ! -f $api_skill ]] &&
    python3 -c 'import json, sys; sys.exit(0 if (json.load(open(sys.argv[1])).get("api") or {}).get("agentUrl") else 1)' "$config" 2>/dev/null; then
    die 'The Subspace API skill agent-skills/opencode/subspace-tracker/SKILL.md is missing, and api.agentUrl needs it. Copy agent-skills/ beside subspace-installer/, or remove api.agentUrl.'
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
        plan=(plan "$config" "$secrets" "$app_secrets" "$prune_secrets")
        # The plan says what this run would do: with --no-systemd, leave swap as it is.
        if "$no_systemd"; then plan+=(no-systemd); fi
        python3 "$runtime" "${plan[@]}"
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
impeccable_stage=
trap 'rm -rf -- "$scratch"; if [[ -n $impeccable_stage ]]; then rm -rf -- "$impeccable_stage"; fi' EXIT

install_file() {
    local source=$1 destination=$2 mode=$3
    if ! cmp -s -- "$source" "$destination" || [[ $(stat -c %a -- "$destination") != "${mode#0}" ]]; then
        install -m "$mode" -- "$source" "$destination.tmp"
        mv -Tf -- "$destination.tmp" "$destination"
        touch "$state/pending"
    fi
}

disable_units() {
    local unit
    local -a regular=() instances=()
    for unit; do
        if [[ $unit == *@.service ]]; then
            instances+=("${unit/@./@*.}")
        else
            regular+=("$unit")
        fi
    done
    # Stop sockets before stopping any connections they accepted.
    if (( ${#regular[@]} )); then
        systemctl disable --now "${regular[@]}"
    fi
    if (( ${#instances[@]} )); then
        systemctl stop "${instances[@]}"
    fi
}

uninstall() {
    local unit
    local -a units=() enabled=()
    local -a ingress_cleanup=(remove)
    if "$no_systemd"; then ingress_cleanup+=(--no-systemd); fi
    # Keep the account until guided setup removes its UID's firewall exception.
    python3 -B "$here/vm/private_network.py" "${ingress_cleanup[@]}"
    if [[ -f $etc/units || -f $etc/retired-units ]]; then
        mapfile -t units < <(sort -u -- "$etc/"*units)
        for unit in "${units[@]}"; do
            [[ $unit =~ ^subspace-fleet-[a-z0-9@-]+\.(service|socket|timer|target)$ ]] || die 'Invalid installed unit name.'
            if [[ -f /etc/systemd/system/$unit ]]; then
                enabled+=("$unit")
            fi
        done
        if ! "$no_systemd" && (( ${#enabled[@]} )); then
            disable_units "${enabled[@]}"
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

# apt and dpkg run from /, never from the caller's working directory.
apt_get() { (cd / && DEBIAN_FRONTEND=noninteractive apt-get "$@"); }
package_installed() { [[ $(cd / && dpkg-query -W -f '${db:Status-Status}' "$1" 2>/dev/null) == installed ]]; }

ensure_packages() {
    local package
    local -a missing=()
    for package in "$@"; do
        package_installed "$package" || missing+=("$package")
    done
    if (( ${#missing[@]} )); then
        apt_get update -qq
        apt_get install -y --no-install-recommends "${missing[@]}"
    fi
}

# Packages named in fleet.json (systemPackages) or by the installer's screenshot list, installed and never
# removed. apt-get install reads a name ending in "-" as "remove this package" and a name it does not know as a
# pattern, so each name must be a plain package name that apt knows exactly, and --no-remove stops apt rather
# than remove anything to make room. With "optional", a package no apt source offers, or that apt cannot install,
# is reported and left out instead of stopping the install.
ensure_listed_packages() {
    local mode=$1 package names
    local -a missing=() offered=()
    shift
    for package in "$@"; do
        [[ $package =~ ^[a-z0-9][a-z0-9+.-]*[a-z0-9+.]$ ]] || die "Invalid system package name $package."
        package_installed "$package" || missing+=("$package")
    done
    (( ${#missing[@]} )) || return 0
    apt_get update -qq
    for package in "${missing[@]}"; do
        names=$(cd / && apt-cache pkgnames "$package") || names=
        if grep -qxF -- "$package" <<< "$names"; then
            offered+=("$package")
        elif [[ $mode == optional ]]; then
            printf 'subspace-fleet: no apt source on this VM offers %s; left out.\n' "$package" >&2
        else
            die "No apt source on this VM offers the package $package."
        fi
    done
    (( ${#offered[@]} )) || return 0
    if [[ $mode == optional ]]; then
        apt_get install -y --no-install-recommends --no-remove "${offered[@]}" ||
            printf 'subspace-fleet: apt could not install %s; left out.\n' "${offered[*]}" >&2
    else
        apt_get install -y --no-install-recommends --no-remove "${offered[@]}"
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
        [[ $tool != agentsview || ! -e $scratch/skip-agentsview ]] || continue
        [[ $tool != caddy || ! -e $scratch/skip-caddy ]] || continue
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

install_impeccable() {
    [[ ! -e $scratch/skip-impeccable ]] || return 0
    local skill=4.11.1 engine=0.1.31 architecture checksum release destination
    case $arch in
        amd64) architecture=x64; checksum=01c642862b0e505277bb8810b1a74aacc42b2b38eb24bc69bc0bcf324089be77 ;;
        arm64) architecture=arm64; checksum=fd488471ae6df728e260bb54c26d0f7fa9235765777f8ee2ff28e9d3dba0dc6b ;;
    esac
    release=https://github.com/pbakaus/impeccable/releases/download
    destination="$app/share/impeccable-$skill-$engine"
    if [[ ! -x $destination/scripts/bin/linux-$architecture/impeccable ]]; then
        install -d -m 0755 "$app/share"
        impeccable_stage=$(mktemp -d "$app/share/.impeccable.XXXXXXXX")
        curl -fsSL --retry 3 "$release/skill-v$skill/universal.zip" -o "$scratch/impeccable.zip"
        printf '%s  %s\n' c0907957a5892b7a0eae6f8d6fb12420ec370a16dfad27115e09e946285e9531 "$scratch/impeccable.zip" |
            sha256sum --check --status
        unzip -q "$scratch/impeccable.zip" '.opencode/skills/impeccable/*' -d "$impeccable_stage"
        local package="$impeccable_stage/.opencode/skills/impeccable"
        curl -fsSL --retry 3 "$release/engine-v$engine/impeccable-linux-$architecture" -o "$scratch/impeccable-engine"
        printf '%s  %s\n' "$checksum" "$scratch/impeccable-engine" | sha256sum --check --status
        install -D -m 0755 "$scratch/impeccable-engine" "$package/scripts/bin/linux-$architecture/impeccable"
        chmod 0755 "$package/scripts/impeccable"
        chmod -R a+rX "$package"
        mv -T "$package" "$destination"
    fi
    if [[ $(readlink "$app/share/impeccable") != "$destination" ]]; then
        ln -sfnT "$destination" "$app/share/impeccable"
        touch "$state/pending"
    fi
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
    local -a units=() retired=() removing=()
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
            removing+=("$unit")
        fi
    done
    disable_units "${removing[@]}"
    for unit in "${removing[@]}"; do
        rm -f -- "/etc/systemd/system/$unit"
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
if ! command -v git >/dev/null; then
    ensure_packages git
fi
python3 "$runtime" prepare "$config" "$secrets" "$scratch" "$app_secrets" "$prune_secrets"
install -d -m 0755 "$app" "$app/bin" "$app/src" "$app/lib" "$state"
install -d -m 0711 "$etc"
# Copy and use the validated snapshots so startup never reads a changed input file.
install_file "$scratch/fleet.json" "$etc/fleet.json" 0600
install_file "$scratch/secrets.json" "$etc/secrets.json" 0600
install_file "$scratch/extra-secrets.json" "$etc/extra-secrets.json" 0600
mapfile -t source < "$scratch/sources"
ensure_packages ca-certificates curl git openssh-client tmux unzip zip xz-utils build-essential jq ripgrep rsync nftables python3-venv
# fleet.json's systemPackages, which runtime.py prepare checked are Debian package names. Never removed.
mapfile -t system_packages < "$scratch/system-packages"
if (( ${#system_packages[@]} )); then
    ensure_listed_packages required "${system_packages[@]}"
fi
# The base screenshot tools' packages (lib/screenshots/package.json): a virtual display with keyboard maps, fonts
# and the browser's libraries. Unless fleet.json turns screenshots off; one apt cannot install is reported.
mapfile -t screenshot_packages < "$scratch/screenshot-packages"
if (( ${#screenshot_packages[@]} )); then
    ensure_listed_packages optional "${screenshot_packages[@]}"
fi
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
# Officers' IRC certificate bundles supplied in the secrets directory. The render gives each to the gateway
# user that serves its officer, and generates any that are missing with openssl (runtime.py irc_certificates).
ensure_packages openssl
install -d -m 0711 "$etc/irc-certs"
if [[ -d $scratch/irc-certs ]]; then
    for bundle in "$scratch/irc-certs/"*.pem; do
        install_file "$bundle" "$etc/irc-certs/${bundle##*/}" 0600
    done
fi
install_releases
install_impeccable
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
for program in runtime.py dashboard.py fleetctl.py github_app.py github_policy.py agent_files.py irc_accounts.py workspaces.py network.py roles.py medical.py sessions.py standard_channels.py fleet_records.py toolchains.py screenshots.py memory.py mockups.py; do
    install_file "$here/lib/$program" "$app/lib/$program" 0755
done
# The screenshot tools' pinned versions and renderer, which runtime.py render installs from.
install -d -m 0755 "$app/lib/screenshots"
for asset in package.json package-lock.json mermaid-render.cjs flint-render.mjs; do
    install_file "$here/lib/screenshots/$asset" "$app/lib/screenshots/$asset" 0644
done
install -d -m 0755 "$app/lib/git-hooks"
install_file "$here/lib/git-hooks/pre-push" "$app/lib/git-hooks/pre-push" 0755
# The generic Subspace API skill that officers with the API's agent listener receive (lib/roles.py).
if [[ -f $api_skill ]]; then
    install -d -m 0755 "$app/lib/agent-skills/subspace-tracker"
    install_file "$api_skill" "$app/lib/agent-skills/subspace-tracker/SKILL.md" 0644
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
if ! "$no_systemd"; then
    # Swap for the engines' swap allowances, before they start (lib/memory.py). A problem is reported, never fatal.
    python3 "$app/lib/runtime.py" swap || printf 'subspace-fleet: swap could not be checked; the install carries on.\n' >&2
fi
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
# With certificates, the fingerprints an IRC operator binds to the officers' accounts. No key material.
python3 "$app/lib/runtime.py" irc-fingerprints
# A crew member takes one screenshot of a local page; a failure is reported, never fatal.
python3 "$app/lib/runtime.py" check-screenshots || printf 'subspace-fleet: the screenshot check could not run.\n' >&2
if "$verify_toolchains"; then
    # Reports each repository's result; a failed check never fails the install.
    python3 "$app/lib/runtime.py" verify-toolchains || printf 'subspace-fleet: the toolchain check could not run.\n' >&2
fi
python3 "$app/lib/runtime.py" usage-guard-summary || printf 'subspace-fleet: the usage guard summary could not run.\n' >&2
# The VM's memory, each engine's limits and why, limits set by hand, and swap: for the operator and their agent.
summary=(memory-summary)
if "$no_systemd"; then summary+=(no-systemd); fi
python3 "$app/lib/runtime.py" "${summary[@]}" || printf 'subspace-fleet: the memory summary could not run.\n' >&2
printf 'Fleet installation complete.\n'
