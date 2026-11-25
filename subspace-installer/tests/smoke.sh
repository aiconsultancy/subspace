#!/usr/bin/env bash
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
docker run --rm --mount "type=bind,source=$here,target=/installer,readonly" \
    --mount "type=bind,source=${here%/*}/agent-skills,target=/agent-skills,readonly" \
    --tmpfs /opt:exec,size=6g --tmpfs /tmp:exec,size=3g --tmpfs /var/cache/apt:size=256m \
    ubuntu:24.04 \
    bash /installer/tests/smoke-container.sh
