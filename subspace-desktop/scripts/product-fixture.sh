#!/usr/bin/env bash
# Isolated command fixtures shared by run and package tests. No host app or
# signing/cloud command is delivered to the real executable.

setup_product_fixture() {
    local root="${root:?}"
    local checksum
    fixture_repo="$(mktemp -d "${TMPDIR:-/tmp}/subspace-products.XXXXXX")"
    trap 'rm -rf "${fixture_repo:?}"' EXIT
    fixture="$fixture_repo/subspace-desktop"
    mkdir -p "$fixture/scripts" "$fixture/bin" "$fixture/metadata" "$fixture/spaces/actions/assets"
    cp "$root/scripts/"{product.sh,prerelease-common.sh,run-app.sh,package-release.sh,package-macos.sh,package-macos-ci.sh,build-macos-app.sh,install-macos.sh,github-oauth-secret.sh,resolve-licence-notices.py} "$fixture/scripts/"
    cp "$root/"{README.md,NOTICE,about.jq,about.toml} "$fixture/"
    cp "$root/../LICENSE" "$fixture_repo/"
    cp -R "$root/licenses" "$fixture/licenses"
    cp "$root/spaces/actions/assets/"{README.md,OCTICONS-LICENSE.txt} "$fixture/spaces/actions/assets/"
    printf '[tools]\nrust = { version = "1.77.0", mr_boxington = true }\n' >"$fixture/mise.toml"
    local rust_docs="$fixture/fake sysroot/share/doc/rust"
    mkdir -p "$rust_docs/licenses"
    printf '<p>Fixture standard library copyright</p>\n' >"$rust_docs/COPYRIGHT-library.html"
    printf 'Fixture Apache licence\n' >"$rust_docs/licenses/Apache-2.0.txt"
    printf 'Fixture MIT licence\n' >"$rust_docs/licenses/MIT.txt"
    echo 'Fixture linked licence' >"$fixture/linked-licence.txt"
    checksum="$(python3 -c 'import hashlib, pathlib, sys; print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())' "$fixture/linked-licence.txt")"
    jq -n --arg checksum "$checksum" '
      [{crates: ["fixture-linked v1.0.0"], id: "MIT",
        path: "linked-licence.txt", sha256: $checksum}]
    ' >"$fixture/about-notices.json"
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
    tree)
        test -z "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}"
        printf 'subspace v%s\n' "$(jq -r .version "$FIXTURE/metadata/subspace.json")"
        printf '%s\n' 'fixture-linked v1.0.0' 'fixture-generic v1.0.0' 'fixture-mpl v3.1.4'
        if [[ "${FAKE_MISSING_LICENCE:-0}" = 1 ]]; then echo 'fixture-unreported v1.0.0'; fi
        ;;
    about)
        test -z "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}"
        if [[ "${FAKE_LICENCE_FAIL:-0}" = 1 ]]; then exit 29; fi
        if [[ "${FAKE_CLARIFICATION_FAIL:-0}" = 1 ]]; then
            echo '[WARN] failed to validate all files specified in clarification for crate fixture-linked 1.0.0: checksum mismatch' >&2
        fi
        if [[ "${FAKE_FALLBACK_LOG_CHANGED:-0}" = 1 ]]; then
            echo '[DEBUG] using canonical MIT text for fixture-linked and fixture-generic' >&2
        else
            for name in fixture-linked fixture-generic; do
                echo "[DEBUG] unable to find text for license 'MIT' for crate '$name 1.0.0', falling back to canonical text" >&2
            done
        fi
        while [[ "$1" != --output-file ]]; do shift; done
        version="$(jq -r .version "$FIXTURE/metadata/subspace.json")"
        jq -n --arg version "$version" '
          {name: "subspace", version: $version} as $product |
          "registry+https://github.com/rust-lang/crates.io-index" as $registry |
          {name: "fixture-linked", version: "1.0.0", source: $registry} as $linked |
          {name: "fixture-generic", version: "1.0.0", source: $registry} as $generic |
          {name: "fixture-mpl", version: "3.1.4", source: $registry} as $mpl |
          {name: "fixture-macro", version: "2.0.0"} as $macro |
          {
            crates: ([$product, $linked, $generic, $mpl, $macro] | map({package: .})),
            licenses: [
              {id: "Apache-2.0", text: "Fixture project licence", source_path: "LICENSE",
               used_by: [{crate: $product}]},
              {id: "MIT", text: "Fixture generic licence", source_path: null,
               used_by: [{crate: $linked}, {crate: $generic}]},
              {id: "MPL-2.0", text: "Fixture MPL licence", source_path: "LICENSE",
               used_by: [{crate: $mpl}]},
              {id: "ISC", text: "Fixture build-only licence", source_path: null,
               used_by: [{crate: $macro}]}
            ]
          }' >"$2"
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
case "$*" in
    -vV) printf 'host: %s\n' "${FAKE_RUST_HOST:-aarch64-apple-darwin}" ;;
    --version) printf 'rustc %s (fixture)\n' "${FAKE_RUST_VERSION:-1.77.0}" ;;
    '--print sysroot') printf '%s\n' "$FIXTURE/fake sysroot" ;;
    *) echo "unexpected rustc operation: $*" >&2; exit 25 ;;
esac
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
    unset CARGO_TARGET_DIR RUSTC CARGO_BUILD_RUSTC
}
