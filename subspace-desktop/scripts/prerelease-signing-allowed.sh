#!/usr/bin/env bash
# Independent from the stable Release Please signing gate.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
validate_prerelease_run
echo true
