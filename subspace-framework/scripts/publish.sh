#!/usr/bin/env bash
# Publish every Framework crate to crates.io in dependency order. See
# scripts/publish.py and docs/publishing.md. `--dry-run` uploads nothing.
set -euo pipefail
exec python3 "$(dirname "${BASH_SOURCE[0]}")/publish.py" "$@"
