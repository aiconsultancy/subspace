#!/usr/bin/env bash
# Only web DOM tests use a private session; the guardian stays on the host bus.
set -euo pipefail

if [[ "$(uname -s)" = Linux ]]; then
    exec dbus-run-session -- "$@"
fi
exec "$@"
