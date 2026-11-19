#!/usr/bin/env bash
# Execute the real Windows build-script selection with a portable resource stub.
# Native resource compilation remains covered by the Windows build in CI.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-icon-resources.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
cat >"$fixture/embed_resource.rs" <<'RUST'
#[derive(Debug)]
pub struct CompilationResult;
pub const NONE: &[&str] = &[];
pub fn compile(path: &str, _: &[&str]) -> CompilationResult {
    println!("selected-resource={path}");
    CompilationResult
}
impl CompilationResult {
    pub fn manifest_required(self) -> Result<(), Self> {
        Ok(())
    }
}
RUST
rustc --edition 2024 --crate-name embed_resource --crate-type rlib \
    "$fixture/embed_resource.rs" -o "$fixture/libembed_resource.rlib"
cat >"$fixture/build-script.rs" <<'RUST'
mod windows_resources;

fn main() {
    windows_resources::embed().unwrap();
}
RUST
# Compile the same resource module used by the real build script.
cp "$root/products/subspace/build/windows_resources.rs" "$fixture/windows_resources.rs"
rustc --edition 2024 "$fixture/build-script.rs" \
    --extern "embed_resource=$fixture/libembed_resource.rlib" -o "$fixture/build-script.exe"
for channel in stable nightly alpha beta ''; do
    resource=app.rc
    stem=subspace
    if [[ "$channel" = nightly ]]; then
        resource=app-nightly.rc
        stem=subspace-nightly
    fi
    output="$(CARGO_CFG_TARGET_OS=windows SUBSPACE_RELEASE_CHANNEL="$channel" "$fixture/build-script.exe")"
    grep -Fxq 'cargo:rerun-if-env-changed=SUBSPACE_RELEASE_CHANNEL' <<<"$output"
    grep -Fxq 'cargo:rerun-if-changed=app.rc' <<<"$output"
    grep -Fxq 'cargo:rerun-if-changed=app-nightly.rc' <<<"$output"
    grep -Fxq "selected-resource=$resource" <<<"$output"
    [[ "$(awk -F '"' '/ICON/ {print $2}' "$root/products/subspace/$resource")" = "assets/$stem.ico" ]]
done
output="$(env -u SUBSPACE_RELEASE_CHANNEL CARGO_CFG_TARGET_OS=windows "$fixture/build-script.exe")"
grep -Fxq 'selected-resource=app.rc' <<<"$output"
output="$(CARGO_CFG_TARGET_OS=linux SUBSPACE_RELEASE_CHANNEL=nightly "$fixture/build-script.exe")"
if grep -q '^selected-resource=' <<<"$output"; then
    echo 'non-Windows builds must not compile a Windows resource' >&2
    exit 1
fi
echo 'portable Windows icon resource selection passed'
