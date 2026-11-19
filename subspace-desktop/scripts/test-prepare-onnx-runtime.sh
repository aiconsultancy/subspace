#!/usr/bin/env bash
# Verify the ONNX CI helper with a controlled Cargo fixture.
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
helper="$root/scripts/prepare-onnx-runtime.sh"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-onnx-fixture.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/caller target" "$fixture/native cache" "$fixture/runner temp" "$fixture/invocation cwd"
printf 'caller target must survive\n' >"$fixture/caller target/sentinel"
printf 'native cache must survive\n' >"$fixture/native cache/sentinel"

cat >"$fixture/bin/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$CARGO_TARGET_DIR" >"$FAKE_CARGO_TARGET_RECORD"
[[ "${MBX_BUILD_SCRIPT_EXECUTION:-}" == 0 ]]
if [[ "${FAKE_CARGO_MODE:-valid}" == cargo-fail ]]; then
	exit 37
fi
if [[ "${FAKE_CARGO_MODE:-valid}" == missing-output ]]; then
	exit 0
fi
[[ "$*" == 'check --locked -p subspace-search-onnx' ]]
native="$FAKE_NATIVE_CACHE_DIR"
if [[ "${FAKE_CARGO_MODE:-valid}" == temporary-archive ]]; then
	native="$CARGO_TARGET_DIR/native path"
fi
other_native="$CARGO_TARGET_DIR/other native path"
mkdir -p "$CARGO_TARGET_DIR/debug/build/ort-sys-fixture" "$native"
printf 'cargo:rustc-link-search=native=%s\ncargo:rustc-link-search=native=%s\n' "$other_native" "$native" >"$CARGO_TARGET_DIR/debug/build/ort-sys-fixture/output"
if [[ "${FAKE_CARGO_MODE:-valid}" == valid || "${FAKE_CARGO_MODE:-valid}" == temporary-archive ]]; then
	printf 'fixture archive\n' >"$native/libonnxruntime.a"
fi
if [[ "${FAKE_CARGO_MODE:-valid}" == empty-archive ]]; then
	: >"$native/libonnxruntime.a"
fi
SH
chmod 755 "$fixture/bin/cargo"

export PATH="$fixture/bin:$PATH"
export CARGO_TARGET_DIR="$fixture/caller target"
export FAKE_CARGO_TARGET_RECORD="$fixture/cargo target record"
export FAKE_NATIVE_CACHE_DIR="$fixture/native cache"
export RUNNER_TEMP="$fixture/runner temp"
# The helper must bypass execution replay even when its caller enables it.
export MBX_BUILD_SCRIPT_EXECUTION=1

assert_failure() {
	local mode="$1" expected="$2" expected_status="${3:-1}" output="$fixture/$1.output" status
	export FAKE_CARGO_MODE="$mode"
	if (cd "$fixture/invocation cwd" && "$helper") >"$output" 2>&1; then
		status=0
	else
		status=$?
	fi
	[[ "$status" == "$expected_status" ]]
	grep -Fq "$expected" "$output"
	[[ ! -e "$(cat "$FAKE_CARGO_TARGET_RECORD")" ]]
}

unset FAKE_CARGO_MODE
valid_output="$fixture/valid output"
(cd "$fixture/invocation cwd" && "$helper") >"$valid_output" 2>&1
grep -Fq 'verified ONNX Runtime archive:' "$valid_output"
stage="$(cat "$FAKE_CARGO_TARGET_RECORD")"
[[ "$stage" != "$CARGO_TARGET_DIR" ]]
[[ ! -e "$stage" ]]
[[ -f "$fixture/caller target/sentinel" ]]
[[ -f "$fixture/native cache/sentinel" ]]
[[ -s "$fixture/native cache/libonnxruntime.a" ]]
[[ "$CARGO_TARGET_DIR" == "$fixture/caller target" ]]
[[ "$MBX_BUILD_SCRIPT_EXECUTION" == 1 ]]

rm "$fixture/native cache/libonnxruntime.a"
assert_failure missing-archive 'libonnxruntime.a was not found in an emitted native directory'
assert_failure empty-archive 'libonnxruntime.a was not found in an emitted native directory'
assert_failure missing-output 'no ort-sys build output files were found'
assert_failure cargo-fail 'Cargo check failed' 37
assert_failure temporary-archive 'native archive must survive temporary-target cleanup'
[[ "$(cat "$FAKE_CARGO_TARGET_RECORD")" != "$CARGO_TARGET_DIR" ]]
[[ ! -e "$(cat "$FAKE_CARGO_TARGET_RECORD")" ]]
[[ -f "$fixture/caller target/sentinel" ]]
[[ -f "$fixture/native cache/sentinel" ]]
[[ "$CARGO_TARGET_DIR" == "$fixture/caller target" ]]

echo 'ONNX helper fixtures passed: exact native archive, missing archive/output, Cargo status, cleanup and whitespace paths'
