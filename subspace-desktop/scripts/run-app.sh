#!/usr/bin/env bash
# CLI arguments run headless. No arguments open the desktop app.
set -euo pipefail

root="${SUBSPACE_RUN_APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
if [[ "$(uname -s)" = Darwin ]]; then
    load_product aarch64-apple-darwin
else
    load_product
fi
build_args=(build --locked -p "$binary_name")
if [ -n "${SUBSPACE_BUILD_FEATURES:-}" ]; then
    build_args+=(--features "$SUBSPACE_BUILD_FEATURES")
fi
cargo "${build_args[@]}"
# The secret is a build input, never a runtime environment variable for the
# desktop, its terminals or agents, or a headless command.
unset SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET
binary="$target_dir/debug/$binary_name"
jwt_dir=""
stage=""
cleanup() {
    if [ -n "$stage" ]; then rm -rf "$stage"; fi
    if [ -n "$jwt_dir" ]; then rm -rf "$jwt_dir"; fi
}
trap cleanup EXIT
# LaunchServices receives only a private temporary file path, never the assertion.
if [ -n "${SUBSPACE_API_URL:-}" ] && [ -n "${SUBSPACE_API_ACCESS_JWT_FILE:-}" ]; then
    jwt_dir="$(umask 077; mktemp -d "${TMPDIR:-/tmp}/subspace-jwt.XXXXXX")"
    (umask 077; cat "$SUBSPACE_API_ACCESS_JWT_FILE" >"$jwt_dir/assertion")
    export SUBSPACE_API_ACCESS_JWT_FILE="$jwt_dir/assertion"
fi
if [ "$#" -gt 0 ] || [ "$(uname -s)" != Darwin ]; then
    "$binary" "$@"
    exit "$?"
fi

# A running debug app keeps its old code while the bundle on disk changes,
# and macOS then refuses its keychain access without asking, so quit it
# before the bundle is replaced. `open` would only bring it to the front.
quit_running() {
    local executable="$1/Contents/MacOS/$binary_name" identifier="$2"
    pgrep -f -- "$executable" >/dev/null || return 0
    echo "quitting the running $display_name (Debug)"
    osascript -e "tell application id \"$identifier\" to quit" >/dev/null 2>&1 || true
    for _ in $(seq 50); do
        pgrep -f -- "$executable" >/dev/null || return 0
        sleep 0.2
    done
    pkill -f -- "$executable" || true
}

dev_dir="$root/dist/dev"
mkdir -p "$dev_dir"
stage="$(mktemp -d "$dev_dir/.$product.app.XXXXXX")"
app="$stage/$display_name.app"
create_bundle "$binary" "$app" "$bundle_id.dev" "$display_name (Debug)"
final="$dev_dir/$display_name.app"
if [ -L "$final" ]; then echo "refusing to replace symbolic link: $final" >&2; exit 1; fi
quit_running "$final" "$bundle_id.dev"
rm -rf "$final"
mv "$app" "$final"
rmdir "$stage"
stage=""
# LaunchServices needs explicit debug overrides from this terminal session.
open_args=(-a "$final")
for name in SUBSPACE_API_URL SUBSPACE_API_ACCESS_JWT_FILE; do
    if [ -n "${!name:-}" ]; then open_args+=(--env "$name=${!name}"); fi
done
if [ -n "$jwt_dir" ]; then
    # Keep the file alive until the app exits, including a delayed client start.
    open -W "${open_args[@]}"
else
    open "${open_args[@]}"
fi
