#!/usr/bin/env bash
# Memory-only Subspace API fixture, foreground, numeric loopback only.
# scripts/subspace-api-test-server.sh --access-jwt fixture-access
# Debug app: SUBSPACE_API_URL=http://127.0.0.1:18161/api
#            SUBSPACE_API_ACCESS_JWT_FILE=<private file containing fixture-access>
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$root/scripts/subspace_api_test_server.py" "$@"
