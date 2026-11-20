#!/usr/bin/env bash
# Isolated command fixtures shared by run and package tests. No host app or
# signing/cloud command is delivered to the real executable.

setup_product_fixture() {
    local root="${root:?}"
    local checksum temporary="${TMPDIR:-/tmp}"
    fixture_repo="$(mktemp -d "${temporary%/}/subspace-products.XXXXXX")"
    trap 'rm -rf "${fixture_repo:?}"' EXIT
    fixture="$fixture_repo/subspace-desktop"
    mkdir -p "$fixture/scripts" "$fixture/bin" "$fixture/metadata" "$fixture/spaces/actions/assets"
    cp "$root/scripts/"{product.sh,prerelease-common.sh,run-app.sh,package-release.sh,package-macos.sh,package-macos-ci.sh,build-macos-app.sh,install-macos.sh,github-oauth-secret.sh,resolve-licence-notices.py} "$fixture/scripts/"
    cp "$root/"{README.md,NOTICE,about.jq,about.toml,native-notices.json} "$fixture/"
    cp "$root/../"{LICENSE,NOTICE} "$fixture_repo/"
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
    mkdir -p "$fixture/native/ort/build/download" "$fixture/native/webview2/x64" \
        "$fixture/git-fork" "$fixture_repo/resolved-framework/host/update"
    printf 'Fixture ONNX Runtime distribution table\n' >"$fixture/native/ort/build/download/dist.tsv"
    printf 'Fixture WebView2 loader\n' >"$fixture/native/webview2/x64/WebView2LoaderStatic.lib"
    printf 'Fixture Git fork licence\n' >"$fixture/git-fork/LICENSE"
    printf 'Fixture Framework project licence\n' >"$fixture_repo/resolved-framework/LICENSE"
    local ort_pin webview_pin
    ort_pin="$(shasum -a 256 "$fixture/native/ort/build/download/dist.tsv" | cut -d ' ' -f1)"
    webview_pin="$(shasum -a 256 "$fixture/native/webview2/x64/WebView2LoaderStatic.lib" | cut -d ' ' -f1)"
    jq --arg ort "$ort_pin" --arg webview "$webview_pin" '
      .onnxruntime.artifacts["build/download/dist.tsv"] = $ort |
      .webview2.artifacts["x64/WebView2LoaderStatic.lib"] = $webview
    ' "$fixture/native-notices.json" >"$fixture/native-notices.tmp"
    mv "$fixture/native-notices.tmp" "$fixture/native-notices.json"
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
target=""; features=""
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
    case "${args[i]}" in
        --target) target="${args[i+1]}" ;;
        --features) features="${args[i+1]}" ;;
    esac
done
component=onnxruntime
native_dir="$FIXTURE/native/ort"
if [[ "$target" == *-windows-* ]]; then
    component=webview2
    native_dir="$FIXTURE/native/webview2"
fi
registry=registry+https://github.com/rust-lang/crates.io-index
fork_url=https://example.com/fixture/fork
fork_revision=1234567890abcdef1234567890abcdef12345678
framework="$FIXTURE/../resolved-framework"
case "$1" in
    run) cat "$FIXTURE/metadata/subspace.json" ;;
    metadata)
        target_directory="${FAKE_TARGET_DIRECTORY:-$FIXTURE/target}"
        jq -c --arg target "$target_directory" '.target_directory = $target' "$FIXTURE/metadata/workspace.json"
        ;;
    tree)
        test -z "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}"
        printf 'subspace v%s (%s/products/subspace)\n' "$(jq -r .version "$FIXTURE/metadata/subspace.json")" "$FIXTURE"
        printf '%s\n' 'fixture-linked v1.0.0' 'fixture-generic v1.0.0' 'fixture-mpl v3.1.4'
        printf 'fixture-linked v1.0.0 (%s#%.8s)\n' "$fork_url" "$fork_revision"
        printf 'subspace-update v1.0.0 (%s/host/update)\n' "$framework"
        jq -r --arg component "$component" --arg version "${FAKE_NATIVE_VERSION:-}" '
          .[$component] | "\(.crate) v\(if $version == "" then .crate_version else $version end)"
        ' "$FIXTURE/native-notices.json"
        if [[ "$features" == *terminal* ]]; then echo 'fixture-terminal v1.0.0'; fi
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
            echo "[DEBUG] unable to find text for license 'MIT' for crate 'fixture-linked 1.0.0', falling back to canonical text" >&2
            if [[ "${FAKE_PARTIAL_FALLBACK:-0}" != 1 ]]; then
                echo "[DEBUG] unable to find text for license 'MIT' for crate 'fixture-generic 1.0.0', falling back to canonical text" >&2
            fi
        fi
        echo "[DEBUG] unable to find text for license 'Apache-2.0' for crate 'subspace-update 1.0.0', falling back to canonical text" >&2
        while [[ "$1" != --output-file ]]; do shift; done
        version="$(jq -r .version "$FIXTURE/metadata/subspace.json")"
        jq -n --arg version "$version" --arg root "$FIXTURE" --arg registry "$registry" \
          --arg fork "$fork_url" --arg rev "$fork_revision" --arg framework "$framework" \
          --arg native "$native_dir" --arg component "$component" --arg features "$features" \
          --arg native_version "${FAKE_NATIVE_VERSION:-}" --arg missing_fork "${FAKE_MISSING_FORK:-0}" \
          --slurpfile pins "$FIXTURE/native-notices.json" '
          def local_package($name; $version; $path):
            {name: $name, version: $version, source: null, id: ("path+file://" + $path + "#" + $name + "@" + $version),
             manifest_path: ($path + "/Cargo.toml")};
          def registry_package($name; $version):
            {name: $name, version: $version, source: $registry, id: ($registry + "#" + $name + "@" + $version),
             manifest_path: ($root + "/registry/" + $name + "/Cargo.toml")};
          local_package("subspace"; $version; ($root + "/products/subspace")) as $product |
          registry_package("fixture-linked"; "1.0.0") as $linked |
          registry_package("fixture-generic"; "1.0.0") as $generic |
          registry_package("fixture-mpl"; "3.1.4") as $mpl |
          registry_package("fixture-macro"; "2.0.0") as $macro |
          registry_package("fixture-terminal"; "1.0.0") as $terminal |
          ($pins[0][$component] | registry_package(.crate; (if $native_version == "" then .crate_version else $native_version end)) |
            .manifest_path = ($native + "/Cargo.toml")) as $native_crate |
          ($linked | .id = ("git+" + $fork + "#fixture-linked@1.0.0") |
            .source = ("git+" + $fork + "#" + $rev) | .manifest_path = ($root + "/git-fork/Cargo.toml")) as $git |
          local_package("subspace-framework"; "1.0.0"; $framework) as $facade |
          local_package("subspace-update"; "1.0.0"; ($framework + "/host/update")) as $update |
          {
            crates: ([$product, $linked, $generic, $mpl, $macro, $native_crate, $facade, $update]
              + (if $missing_fork == "1" then [] else [$git] end)
              + (if $features | contains("terminal") then [$terminal] else [] end) | map({package: .})),
            licenses: ([
              {id: "Apache-2.0", text: "Fixture project licence", source_path: "LICENSE",
               used_by: [{crate: $product}]},
              {id: "Apache-2.0", text: "Fixture generic Apache licence", source_path: null,
               used_by: [{crate: $update}]},
              {id: "MIT", text: "Fixture generic licence", source_path: null,
               used_by: [{crate: $linked}, {crate: $generic}]},
              {id: "MIT", text: "Fixture native crate licence", source_path: "LICENSE",
               used_by: [{crate: $native_crate}]},
              {id: "MPL-2.0", text: "Fixture MPL licence", source_path: "LICENSE",
               used_by: [{crate: $mpl}]},
              {id: "ISC", text: "Fixture build-only licence", source_path: null,
               used_by: [{crate: $macro}]}
            ] + (if $missing_fork == "1" then [] else
              [{id: "MIT", text: "Fixture Git fork licence\n", source_path: ($root + "/git-fork/LICENSE"),
                used_by: [{crate: $git}]}] end)
              + (if $features | contains("terminal") then
                [{id: "MIT", text: "Fixture Terminal licence", source_path: "LICENSE",
                  used_by: [{crate: $terminal}]}] else [] end))
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
        directory="${CARGO_TARGET_DIR:-$FIXTURE/target}${target:+/$target}/$release"
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
        printf '# features: %s\n' "$features" >>"$directory/$package$exe"
        chmod 755 "$directory/$package$exe"
        # A real build compiles the OAuth secret in; FAKE_DROP_OAUTH_SECRET
        # stands for a stale build cache that did not.
        if [ -n "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}" ] && [ "${FAKE_DROP_OAUTH_SECRET:-0}" != 1 ]; then
            printf '# %s\n' "$SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET" >>"$directory/$package$exe"
        fi
        if [[ -n "${FIXTURE_BUILD_GATE:-}" ]]; then
            touch "$FIXTURE_BUILD_GATE/built"
            for ((waited = 0; waited < 300; waited++)); do
                [[ ! -f "$FIXTURE_BUILD_GATE/resume" ]] || exit 0
                sleep 0.1
            done
            echo 'timed out waiting to stage the fixture executable' >&2
            exit 28
        fi
        ;;
    *) echo "unexpected cargo operation: $*" >&2; exit 24 ;;
esac
EOF
    # Packaging runs `cargo-about about ...`, which the fake cargo answers.
    cat >"$fixture/bin/cargo-about" <<'EOF'
#!/usr/bin/env bash
exec "$FIXTURE/bin/cargo" "$@"
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
