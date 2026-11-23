# ONNX Runtime 1.31.0

The macOS and Linux builds link the static ONNX Runtime selected by
`ort-sys` 2.0.0-rc.15. Its `build/download/dist.tsv` pins the native
downloads to ONNX Runtime 1.31.0. The [MIT licence](LICENSE) and
[third-party notices](ThirdPartyNotices.txt) are the unchanged files from
Microsoft's [1.31.0 release wheel](https://pypi.org/project/onnxruntime/1.31.0/#files)
for CPython 3.12 on Apple silicon (SHA-256
`aaab9b3af536b06ca27ab5e35e3d429c97457ce76cf298af103f687e8b9975c0`).

Packaging verifies the distribution archive against the SHA-256 in
`dist.tsv` before extracting and linking `libonnxruntime.a`. The macOS
target uses the `coreml` distribution. Both Linux targets use the `none`
distribution.

These distributions include MPL-2.0 Eigen 3.4.90 at revision
`1d8b82b0740839c0de7f1242a3585e3390ff5f33`. The
[Eigen source archive](https://github.com/eigen-mirror/eigen/archive/1d8b82b0740839c0de7f1242a3585e3390ff5f33/eigen-1d8b82b0740839c0de7f1242a3585e3390ff5f33.zip)
is pinned in [ONNX Runtime 1.31.0's dependency table](https://github.com/microsoft/onnxruntime/blob/v1.31.0/cmake/deps.txt)
with SHA-1 `05b19b49e6fbb91246be711d801160528c135e34`.
