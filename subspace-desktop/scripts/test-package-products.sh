#!/usr/bin/env bash
# Archive and bundle fixtures use real product metadata, stub builds/signing.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product-fixture.sh
source "$root/scripts/product-fixture.sh"
setup_product_fixture ""
# Exercise the real Make target without installing tools or touching host apps.
cat >"$fixture/bin/mise" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" = run && "$2" = install-macos && "$3" = -- ]]
shift 3
printf '%s\n' "${MISE_ENV:-}" >>"$FIXTURE/mise.log"
exec bash "$FIXTURE/scripts/install-macos.sh" "$@"
EOF
chmod 755 "$fixture/bin/mise"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=product.sh
source "$root/scripts/product.sh"
test "$(resolve_target_dir /var/tmp/subspace-target)" = /var/tmp/subspace-target
test "$(resolve_target_dir relative-target)" = "$root/relative-target"
test "$(resolve_target_dir '\\fixture\share\target')" = "$fixture/target"
grep -Fxq -- '\\fixture\share\target' "$fixture/cygpath.log"

: >"$fixture/actions.log"
metadata="$fixture/metadata/subspace.json"
binary="$(jq -r .binary_name "$metadata")"
name="$(jq -r .display_name "$metadata")"
version="$(jq -r .version "$metadata")"
identifier="$(jq -r .bundle_id "$metadata")"
export FAKE_RUST_HOST=x86_64-pc-windows-msvc
for path_source in metadata environment; do
    : >"$fixture/binary.log"
    : >"$fixture/cargo.log"
    : >"$fixture/cygpath.log"
    if [[ "$path_source" = metadata ]]; then
        export FAKE_TARGET_DIRECTORY='Z:\fixture\target'
        unset CARGO_TARGET_DIR
        expected_path='Z:\fixture\target'
    else
        export FAKE_TARGET_DIRECTORY="$fixture/unused-target"
        export CARGO_TARGET_DIR='Z:/fixture/target'
        expected_path='Z:/fixture/target'
    fi
    bash "$fixture/scripts/package-release.sh" x86_64-pc-windows-msvc "$version"
    grep -Fxq -- "$expected_path" "$fixture/cygpath.log"
    grep -Fq -- "build --release --locked -p $binary --target x86_64-pc-windows-msvc" "$fixture/cargo.log"
    grep -Fxq -- '__product_metadata' "$fixture/binary.log"
    test -f "$fixture/dist/$binary-$version-x86_64-pc-windows-msvc.zip"
done
unset CARGO_TARGET_DIR FAKE_RUST_HOST FAKE_TARGET_DIRECTORY
for target in x86_64-unknown-linux-gnu x86_64-pc-windows-msvc; do
    bash "$fixture/scripts/package-release.sh" "$target" "$version"
    prefix="$binary-$version-$target"
    mkdir "$fixture/unpacked-$target"
    if [[ "$target" == *windows* ]]; then
        unzip -q "$fixture/dist/$prefix.zip" -d "$fixture/unpacked-$target"
        test -f "$fixture/unpacked-$target/$binary.exe"
    else
        tar -xzf "$fixture/dist/$prefix.tar.gz" -C "$fixture/unpacked-$target"
        test -x "$fixture/unpacked-$target/$binary"
    fi
    cmp "$metadata" "$fixture/unpacked-$target/product.json"
    test -f "$fixture/unpacked-$target/README.md"
    cmp "$root/licenses/charts/README.md" "$fixture/unpacked-$target/licenses/charts/README.md"
done
host_target=aarch64-apple-darwin
host_archive="$fixture/dist/$binary-$version-$host_target.tar.gz"
: >"$fixture/binary.log"
: >"$fixture/cargo.log"
SUBSPACE_BUILD_FEATURES=terminal bash "$fixture/scripts/package-release.sh" "$host_target" "$version"
grep -Fq -- "build --release --locked -p $binary --target $host_target --features terminal --message-format=json-render-diagnostics " "$fixture/cargo.log"
grep -Eq "^about -L debug generate --locked --fail --manifest-path products/subspace/Cargo.toml .*--target $host_target --features terminal " "$fixture/cargo.log"
grep -Eq "^tree .*--target $host_target --features terminal$" "$fixture/cargo.log"
bash "$fixture/scripts/package-release.sh" "$host_target" "$version"
grep -Fxq -- '__product_metadata' "$fixture/binary.log"
cp "$host_archive" "$fixture/previous.tar.gz"
cp "$host_archive.sha256" "$fixture/previous.tar.gz.sha256"
export FAKE_BINARY_VERSION=9.9.9
if bash "$fixture/scripts/package-release.sh" "$host_target" "$version"; then
    echo 'expected binary version mismatch' >&2
    exit 1
fi
unset FAKE_BINARY_VERSION
cmp "$fixture/previous.tar.gz" "$host_archive"
cmp "$fixture/previous.tar.gz.sha256" "$host_archive.sha256"
cp "$fixture/cargo.log" "$fixture/before-toolchain-check.log"
rust_copyright="$fixture/fake sysroot/share/doc/rust/COPYRIGHT-library.html"
mv "$rust_copyright" "$rust_copyright.bak"
if bash "$fixture/scripts/package-release.sh" "$host_target" "$version" >"$fixture/rust-notice.log" 2>&1; then
    echo 'expected a missing Rust standard library notice to stop packaging' >&2
    exit 1
fi
grep -Fq "missing or unreadable Rust standard library notice: $rust_copyright" "$fixture/rust-notice.log"
mv "$rust_copyright.bak" "$rust_copyright"
if FAKE_RUST_VERSION=1.77.0-nightly bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile' >"$fixture/rust-version.log" 2>&1; then
    echo 'expected a Rust version mismatch to stop packaging' >&2
    exit 1
fi
grep -Fq 'packaging requires Rust 1.77.0 from mise.toml; rustc reports rustc 1.77.0-nightly' "$fixture/rust-version.log"
cmp "$fixture/before-toolchain-check.log" "$fixture/cargo.log"
builds_before="$(grep -c '^build ' "$fixture/cargo.log")"
if FAKE_LICENCE_FAIL=1 bash "$fixture/scripts/package-release.sh" "$host_target" "$version"; then
    echo 'expected failed licence policy to stop packaging' >&2
    exit 1
fi
if FAKE_MISSING_LICENCE=1 bash "$fixture/scripts/package-release.sh" "$host_target" "$version"; then
    echo 'expected incomplete licence report to stop packaging' >&2
    exit 1
fi
if FAKE_FALLBACK_LOG_CHANGED=1 bash "$fixture/scripts/package-release.sh" "$host_target" "$version" >"$fixture/fallback.log" 2>&1; then
    echo 'expected unrecognised fallback diagnostics to stop packaging' >&2
    exit 1
fi
grep -Fq 'missing cargo-about fallback diagnostics for fixture-generic v1.0.0 (MIT,' "$fixture/fallback.log"
if FAKE_CLARIFICATION_FAIL=1 bash "$fixture/scripts/package-release.sh" "$host_target" "$version" >"$fixture/clarification.log" 2>&1; then
    echo 'expected a failed clarification checksum to stop packaging' >&2
    exit 1
fi
grep -Fq 'clarification failed its source validation' "$fixture/clarification.log"
mv "$fixture/linked-licence.txt" "$fixture/linked-licence.bak"
if bash "$fixture/scripts/package-release.sh" "$host_target" "$version" >"$fixture/source-notice.log" 2>&1; then
    echo 'expected a missing source-backed notice to stop packaging' >&2
    exit 1
fi
grep -Fq 'cannot read source-backed licence' "$fixture/source-notice.log"
mv "$fixture/linked-licence.bak" "$fixture/linked-licence.txt"
test "$(grep -c '^build ' "$fixture/cargo.log")" = "$builds_before"
cmp "$fixture/previous.tar.gz" "$host_archive"
cmp "$fixture/previous.tar.gz.sha256" "$host_archive.sha256"
builds_before="$(grep -c '^build ' "$fixture/cargo.log")"
if bash "$fixture/scripts/package-release.sh" "$host_target" 9.9.9; then
    echo 'expected metadata version mismatch' >&2
    exit 1
fi
test "$(grep -c '^build ' "$fixture/cargo.log")" = "$builds_before"
cmp "$fixture/previous.tar.gz" "$host_archive"
cmp "$fixture/previous.tar.gz.sha256" "$host_archive.sha256"
export FAKE_BUILD_FAIL=1
if bash "$fixture/scripts/package-release.sh" "$host_target" "$version"; then
    echo 'expected failed archive build' >&2
    exit 1
fi
unset FAKE_BUILD_FAIL
cmp "$fixture/previous.tar.gz" "$host_archive"
cmp "$fixture/previous.tar.gz.sha256" "$host_archive.sha256"
# An inherited feature list never puts an unreleased Space in a release.
: >"$fixture/cargo.log"
# Cargo reads every whitespace character as a separator, so must the guard. A
# character it cannot place (a no-break space here) stops the build too.
refused='release builds (must not include unreleased Spaces|refuse an unexpected character)'
for features in unreleased-spaces 'terminal board-lab' 'terminal,subspace/component-gallery' \
    $'terminal\nsubspace-framework/component-gallery' $'terminal,board-lab\r' $'terminal\tboard-lab' \
    $'terminal\vboard-lab' $'terminal\fsubspace/board-lab' $'terminal\xc2\xa0board-lab'; do
    if SUBSPACE_BUILD_FEATURES="$features" bash "$fixture/scripts/package-release.sh" "$host_target" "$version" 2>"$fixture/unreleased.err"; then
        echo "expected SUBSPACE_BUILD_FEATURES='$features' to stop the release archive" >&2
        exit 1
    fi
    grep -Eq "$refused" "$fixture/unreleased.err"
    if SUBSPACE_BUILD_FEATURES="$features" bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile' 2>"$fixture/unreleased.err"; then
        echo "expected SUBSPACE_BUILD_FEATURES='$features' to stop the macOS app" >&2
        exit 1
    fi
    grep -Eq "$refused" "$fixture/unreleased.err"
done
test ! -s "$fixture/cargo.log"
cmp "$fixture/previous.tar.gz" "$host_archive"
cmp "$fixture/previous.tar.gz.sha256" "$host_archive.sha256"
: >"$fixture/cargo.log"
SUBSPACE_BUILD_FEATURES=terminal bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile'
grep -Fq -- "build --release --locked -p $binary --target $host_target --features terminal --message-format=json-render-diagnostics " "$fixture/cargo.log"
grep -Eq "^about -L debug generate .*--target $host_target --features terminal " "$fixture/cargo.log"
app="$fixture/dist/$name.app"
test -x "$app/Contents/MacOS/$binary"
grep -Fxq '# features: terminal' "$app/Contents/MacOS/$binary"
grep -Fxq -- '- fixture-terminal v1.0.0' "$app/Contents/Resources/THIRD_PARTY_LICENSES"
test ! -e "$fixture/target/$host_target/release/$binary"
test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = "$identifier"
test "$(plutil -extract CFBundleDisplayName raw -o - "$app/Contents/Info.plist")" = "$name"
test "$(plutil -extract LSMinimumSystemVersion raw -o - "$app/Contents/Info.plist")" = 15.0
icon_name="$(plutil -extract CFBundleIconFile raw -o - "$app/Contents/Info.plist")"
test -f "$app/Contents/Resources/$icon_name.icns"
cmp "$metadata" "$app/Contents/Resources/product.json"
cmp "$root/licenses/charts/README.md" "$app/Contents/Resources/licenses/charts/README.md"
test "$(command -v codesign)" = "$fixture/bin/codesign"
test -x "$app/Contents/Resources/bin/mise"
test -f "$app/Contents/Resources/licenses/mise/LICENSE"
grep -Fxq "mise 2026.10.17 macos-arm64 (fixture)" <("$app/Contents/Resources/bin/mise")
staged_app="$(sed -n 's/^bundle-mise //p' "$fixture/actions.log" | tail -1)"
[[ "$staged_app" == "$fixture/dist/.subspace-macos."*"/$name.app" ]]
mise_sign_line="$(grep -Fn -- "--sign fixture signing identity $staged_app/Contents/Resources/bin/mise" "$fixture/actions.log" | head -1 | cut -d: -f1)"
app_sign_line="$(grep -Fn -- "--sign fixture signing identity $staged_app" "$fixture/actions.log" | grep -Fv '/Contents/Resources/bin/mise' | head -1 | cut -d: -f1)"
test -n "$mise_sign_line"
test -n "$app_sign_line"
test "$mise_sign_line" -lt "$app_sign_line"
grep -Fq "codesign --force --options runtime --timestamp --sign fixture signing identity" "$fixture/actions.log"
grep -Fq 'xcrun notarytool submit' "$fixture/actions.log"
grep -Fq 'xcrun stapler validate' "$fixture/actions.log"
# Without a keychain item the app still packages, signing in with a code.
if grep -Fxq 'oauth secret' "$fixture/cargo.log"; then echo 'no OAuth secret was expected' >&2; exit 1; fi
grep -Fxq 'security find-generic-password' "$fixture/security.log"
# The keychain's OAuth secret reaches only the build, and is never printed.
: >"$fixture/cargo.log"
FAKE_OAUTH_SECRET=fixture-oauth-secret bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile' >"$fixture/package.out" 2>&1
grep -Fxq 'oauth secret' "$fixture/cargo.log"
secret_cache="$(tail -n 1 "$fixture/release-cache.log")"
[[ "$secret_cache" != "$fixture/target/release-pkg/"* ]]
test ! -e "$secret_cache"
grep -Fq fixture-oauth-secret "$app/Contents/MacOS/$binary"
if grep -Fq fixture-oauth-secret "$fixture/package.out" "$fixture/actions.log"; then echo 'the OAuth secret was printed' >&2; exit 1; fi
if FAKE_OAUTH_SECRET=fixture-oauth-secret FAKE_DROP_OAUTH_SECRET=1 bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile' 2>"$fixture/package.err"; then
    echo 'an app without the OAuth secret must not be packaged' >&2; exit 1
fi
grep -Fq 'verify the login keychain item or SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET, then package again' "$fixture/package.err"
test ! -e "$(tail -n 1 "$fixture/release-cache.log")"
if grep -Fq fixture-oauth-secret "$fixture/package.err"; then echo 'the OAuth secret was printed' >&2; exit 1; fi
: >"$fixture/actions.log"
export FAKE_BUILD_FAIL=1
if bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile'; then echo 'expected failed build' >&2; exit 1; fi
test ! -s "$fixture/actions.log" || ! grep -q '^codesign\|^xcrun notarytool submit' "$fixture/actions.log"
test -x "$app/Contents/MacOS/$binary"
unset FAKE_BUILD_FAIL
# Installer wiring runs only in fixture HOME with signing/process stubs.
: >"$fixture/cargo.log"
: >"$fixture/mise.log"
env -u FEATURES -u SUBSPACE_BUILD_FEATURES -u MISE_ENV HOME="$fixture/home" \
    make -f "$root/Makefile" install-macos
if ! grep -Fq -- "build --release --locked -p $binary --target $host_target --features terminal --message-format=json-render-diagnostics " "$fixture/cargo.log"; then
    echo "install-macos must enable Terminal by default" >&2
    exit 1
fi
grep -Fxq terminal "$fixture/mise.log"
installed="$fixture/home/Applications/$name.app"
test -x "$installed/Contents/MacOS/$binary"
cmp "$metadata" "$installed/Contents/Resources/product.json"
: >"$fixture/cargo.log"
HOME="$fixture/home" make -f "$root/Makefile" install-macos \
    FEATURES=terminal,capture TARGET="$host_target"
grep -Fq -- "build --release --locked -p $binary --target $host_target --features terminal,capture --message-format=json-render-diagnostics " "$fixture/cargo.log"
grep -Eq "^about -L debug generate .*--target $host_target --features terminal,capture " "$fixture/cargo.log"
: >"$fixture/cargo.log"
HOME="$fixture/home" make -f "$root/Makefile" install-macos FEATURES=
if grep -Fq -- --features "$fixture/cargo.log"; then
    echo "install-macos must preserve an explicit empty FEATURES override" >&2
    exit 1
fi
export FAKE_RUNNING_BINARY="$binary"
if HOME="$fixture/home" bash "$fixture/scripts/install-macos.sh"; then
    echo 'expected running product to block installation' >&2
    exit 1
fi
test -x "$installed/Contents/MacOS/$binary"
unset FAKE_RUNNING_BINARY
echo 'release archive and macOS bundle fixtures passed'
