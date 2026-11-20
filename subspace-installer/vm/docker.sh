#!/usr/bin/env bash
# Install Docker Engine and the Compose plugin from Docker's apt repository,
# the same way the fleet installer does for local IRC. Idempotent.
set -euo pipefail
umask 022
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Docker's release key file (fingerprint 9DC8 5822 9FC7 DD38 854A E2D8 8D81 803C 0EBF CD88).
docker_gpg_sha256=1500c1f56fa9e26b9b8f42452a553675796ade0807cdce11975eb98170b3a570

die() { printf 'subspace-setup: %s\n' "$*" >&2; exit 1; }
(( EUID == 0 )) || die 'Run as root.'

if docker compose version >/dev/null 2>&1; then
    exit 0
fi

# shellcheck disable=SC1091
. /etc/os-release
case "${ID:-}" in
    ubuntu|debian) ;;
    *) die 'Docker setup supports Ubuntu and Debian.' ;;
esac
arch=$(dpkg --print-architecture)
base=https://download.docker.com/linux/$ID
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates curl
install -d -m 0755 /etc/apt/keyrings
if [[ ! -s /etc/apt/keyrings/subspace-docker.asc ]]; then
    curl -fsSL --retry 3 "$base/gpg" -o "$scratch/docker.asc"
    printf '%s  %s\n' "$docker_gpg_sha256" "$scratch/docker.asc" | sha256sum --check --status ||
        die 'Docker apt signing key checksum mismatch.'
    install -m 0644 "$scratch/docker.asc" /etc/apt/keyrings/subspace-docker.asc
fi
cat > /etc/apt/sources.list.d/subspace-docker.sources <<SOURCE
Types: deb
URIs: $base
Suites: $VERSION_CODENAME
Components: stable
Architectures: $arch
Signed-By: /etc/apt/keyrings/subspace-docker.asc
SOURCE
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
systemctl enable --now docker.service
