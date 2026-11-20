#!/usr/bin/env bash
# Memory-only NETT API and device sign-in fixture, foreground, numeric loopback only.
# scripts/nett-api-test-server.sh [--port 0] [--single-org] [--auto-approve]
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$root/scripts/nett_api_test_server.py" "$@"
