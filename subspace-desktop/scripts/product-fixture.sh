#!/usr/bin/env bash
# Isolated command fixtures shared by run and package tests. No host app or
# signing/cloud command is delivered to the real executable.

setup_product_fixture() {
    local root="${root:?}"
    fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-products.XXXXXX")"
    trap 'rm -rf "${fixture:?}"' EXIT
    mkdir -p "$fixture/scripts" "$fixture/bin" "$fixture/metadata"
    cp "$root/scripts/"{product.sh,prerelease-common.sh,run-app.sh,package-release.sh,package-macos.sh,package-macos-ci.sh,build-macos-app.sh,install-macos.sh,github-oauth-secret.sh} "$fixture/scripts/"
    cp "$root/README.md" "$fixture/README.md"
    cp -R "$root/licenses" "$fixture/licenses"
    if [[ -n "${1:-}" ]]; then
        workspace_metadata="$(cat "$1/workspace.json")"
    else
        workspace_metadata="$(cargo metadata --format-version=1 --no-deps --locked)"
    fi
    printf '%s\n' "$workspace_metadata" >"$fixture/metadata/workspace.json"
    version="$(jq -er '.packages[] | select(.name == "subspace") | .version' <<<"$workspace_metadata")"
    mkdir -p "$fixture/products/subspace/assets"
    cp "$root/products/subspace/assets/"*.{icns,svg} "$fixture/products/subspace/assets/"
    if [[ -n "${1:-}" ]]; then
        cp "$1/subspace.json" "$fixture/metadata/subspace.json"
    else
        cargo run --quiet --locked -p subspace-descriptor --bin subspace-product-metadata -- subspace "$version" >"$fixture/metadata/subspace.json"
    fi
    cat >"$fixture/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/cargo.log"
case "$1" in
    run) cat "$FIXTURE/metadata/subspace.json" ;;
    metadata)
        target_directory="${FAKE_TARGET_DIRECTORY:-$FIXTURE/target}"
        jq -c --arg target "$target_directory" '.target_directory = $target' "$FIXTURE/metadata/workspace.json"
        ;;
    build)
        if [ "${FAKE_BUILD_FAIL:-0}" = 1 ]; then exit 23; fi
        # Record whether the build saw the OAuth secret, never its value.
        if [ -n "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}" ]; then echo 'oauth secret' >>"$FIXTURE/cargo.log"; fi
        if [ "${ASSERT_SIGNING_SECRETS_UNSET:-0}" = 1 ]; then
            for name in MACOS_CERTIFICATE_BASE64 MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY \
                APPLE_API_KEY_BASE64 APPLE_API_KEY_ID APPLE_API_ISSUER_ID; do
                test -z "${!name:-}"
            done
            test -f "$SUBSPACE_MACOS_KEYCHAIN"
            test ! -e "$(dirname "$SUBSPACE_MACOS_KEYCHAIN")/AuthKey.p8"
            test ! -e "$(dirname "$SUBSPACE_MACOS_KEYCHAIN")/certificate.p12"
        fi
        target=""; package=""; release=debug
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --target) target="$2"; shift ;;
                -p) package="$2"; shift ;;
                --release) release=release ;;
            esac
            shift
        done
        directory="$FIXTURE/target${target:+/$target}/$release"
        mkdir -p "$directory"
        exe=""; case "$target" in *-windows-*) exe=.exe ;; esac
        cat >"$directory/$package$exe" <<'BINARY'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/binary.log"
if [ "${1:-}" = __product_metadata ]; then
    version="$(jq -r .version "$FIXTURE/metadata/subspace.json")"
    jq --arg version "${FAKE_BINARY_VERSION:-$version}" '.version = $version' "$FIXTURE/metadata/subspace.json"
fi
BINARY
        chmod 755 "$directory/$package$exe"
        # A real build compiles the OAuth secret in; FAKE_DROP_OAUTH_SECRET
        # stands for a stale build cache that did not.
        if [ -n "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}" ] && [ "${FAKE_DROP_OAUTH_SECRET:-0}" != 1 ]; then
            printf '# %s\n' "$SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET" >>"$directory/$package$exe"
        fi
        ;;
    *) echo "unexpected cargo operation: $*" >&2; exit 24 ;;
esac
EOF
    # The login keychain: FAKE_OAUTH_SECRET is the GitHub OAuth client secret
    # item, else there is none. Never the real keychain.
    cat >"$fixture/bin/security" <<'EOF'
#!/usr/bin/env bash
printf 'security %s\n' "$1" >>"$FIXTURE/security.log"
[ "$*" = "find-generic-password -s subspace-github-oauth -a client-secret -w" ] || exit 90
[ -n "${FAKE_OAUTH_SECRET:-}" ] || { echo 'security: The specified item could not be found in the keychain.' >&2; exit 44; }
printf '%s\n' "$FAKE_OAUTH_SECRET"
EOF
    cat >"$fixture/bin/uname" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${FAKE_UNAME:-Darwin}"
EOF
    # The desktop login session, unless a test says otherwise; the signing
    # scripts move to it through desktop-session.sh from anywhere else.
    cat >"$fixture/bin/launchctl" <<'EOF'
#!/usr/bin/env bash
[ "$1" = managername ] || { echo "unexpected launchctl call: $*" >&2; exit 26; }
printf '%s\n' "${FAKE_LAUNCHD_MANAGER:-Aqua}"
EOF
    cat >"$fixture/bin/rustc" <<'EOF'
#!/usr/bin/env bash
printf 'host: %s\n' "${FAKE_RUST_HOST:-aarch64-apple-darwin}"
EOF
    cat >"$fixture/bin/rustup" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FIXTURE/rustup.log"
if [ "$1" = target ] && [ "$2" = list ]; then
    printf '%s\n' aarch64-apple-darwin x86_64-unknown-linux-gnu x86_64-pc-windows-msvc
fi
EOF
    for name in open codesign xcrun spctl; do
        cat >"$fixture/bin/$name" <<'EOF'
#!/usr/bin/env bash
printf '%s %s\n' "$(basename "$0")" "$*" >>"$FIXTURE/actions.log"
EOF
    done
    # Isolated stub for the mise bundling helper: no network, no real
    # extraction. It only proves that build-macos-app.sh bundles mise and
    # package-macos.sh's nested-before-outer signing order; the helper's own
    # download/checksum/version logic is covered directly by
    # test-bundle-mise.sh.
    cat >"$fixture/scripts/bundle-mise.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
app="$1"
printf 'bundle-mise %s\n' "$app" >>"$FIXTURE/actions.log"
mkdir -p "$app/Contents/Resources/bin"
cat >"$app/Contents/Resources/bin/mise" <<'INNER'
#!/usr/bin/env bash
echo "mise 2026.10.17 macos-arm64 (fixture)"
INNER
chmod 755 "$app/Contents/Resources/bin/mise"
mkdir -p "$app/Contents/Resources/licenses/mise"
printf 'fixture mise license notice\n' >"$app/Contents/Resources/licenses/mise/LICENSE"
EOF
    chmod 755 "$fixture/scripts/bundle-mise.sh"
    cat >"$fixture/bin/pgrep" <<'EOF'
#!/usr/bin/env bash
case "${2:-}" in
    *"/${FAKE_RUNNING_BINARY:-}") exit 0 ;;
    *) exit 1 ;;
esac
EOF
    cat >"$fixture/bin/cygpath" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = -u
printf '%s\n' "$2" >>"$FIXTURE/cygpath.log"
case "$2" in
    'Z:\fixture\target' | 'Z:/fixture/target' | '\\fixture\share\target') printf '%s\n' "$FIXTURE/target" ;;
    *) echo "unexpected Windows path: $2" >&2; exit 25 ;;
esac
EOF
    chmod 755 "$fixture/bin/"*
    export FIXTURE="$fixture"
    export SUBSPACE_RUN_APP_ROOT="$fixture" SUBSPACE_PACKAGE_ROOT="$fixture"
    export PATH="$fixture/bin:$PATH"
    unset CARGO_TARGET_DIR
}
