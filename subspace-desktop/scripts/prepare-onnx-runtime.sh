#!/usr/bin/env bash
# Check ONNX Runtime with a fresh Cargo target and verify its emitted archive.
set -euo pipefail

script_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$script_root"

temp_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
stage="$(mktemp -d "$temp_root/subspace-onnx-target.XXXXXX")"
stage="$(cd -- "$stage" && pwd -P)"
cleanup() {
	rm -rf -- "$stage"
}
trap cleanup EXIT

# The native download lives outside the Cargo state restored by MBX.
# Use a fresh target and execute build scripts rather than replaying their output:
# MBX cannot restore a native archive stored outside Cargo's OUT_DIR.
set +e
MBX_BUILD_SCRIPT_EXECUTION=0 CARGO_TARGET_DIR="$stage" cargo check --locked -p subspace-search-onnx
cargo_status=$?
set -e
if ((cargo_status != 0)); then
	echo "Cargo check failed with exit status $cargo_status" >&2
	exit "$cargo_status"
fi

output_count=0
verified_archive=''
while IFS= read -r -d '' output_file; do
	output_count=$((output_count + 1))
	while IFS= read -r line || [[ -n "$line" ]]; do
		prefix='cargo:rustc-link-search=native='
		if [[ "$line" != "$prefix"* ]]; then
			continue
		fi
		native_dir="${line#"$prefix"}"
		printf 'checking native ONNX Runtime archive: %s/libonnxruntime.a\n' "$native_dir"
		if [[ -n "$native_dir" && -s "$native_dir/libonnxruntime.a" ]]; then
			native_dir="$(cd -- "$native_dir" && pwd -P)"
			if [[ "$native_dir" == "$stage" || "$native_dir" == "$stage/"* ]]; then
				echo 'ONNX Runtime native archive must survive temporary-target cleanup' >&2
				continue
			fi
			verified_archive="$native_dir/libonnxruntime.a"
			break 2
		fi
	done <"$output_file"
done < <(find "$stage" -path '*/build/ort-sys-*/output' -print0)

if [[ -z "$verified_archive" ]]; then
	if ((output_count == 0)); then
		echo 'ONNX Runtime verification failed: no ort-sys build output files were found' >&2
	else
		echo 'ONNX Runtime verification failed: libonnxruntime.a was not found in an emitted native directory' >&2
	fi
	exit 1
fi

echo "verified ONNX Runtime archive: $verified_archive"
