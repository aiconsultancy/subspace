#!/usr/bin/env python3
"""Stage Cargo's release executable and publish release archives under locks."""

import argparse
from contextlib import ExitStack, contextmanager
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory, TemporaryFile, mkdtemp
import time
from uuid import uuid4


OAUTH_SECRET = "SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET"
# ort-sys 2.0.0-rc.15 reads these in build/ and src/internal/dirs.rs.
ORT_OVERRIDES = {
    "ORT_LIB_PATH", "ORT_LIB_LOCATION", "ORT_LIB_PROFILE", "ORT_VCPKG_TARGET",
    "ORT_IOS_XCFWK_PATH", "ORT_IOS_XCFWK_LOCATION", "ORT_EXT_IOS_XCFWK_PATH",
    "ORT_EXT_IOS_XCFWK_LOCATION", "ORT_PREFER_DYNAMIC_LINK", "ORT_SKIP_DOWNLOAD",
    "ORT_OFFLINE", "ORT_CXX_STDLIB", "CXXSTDLIB", "ORT_CUDA_VERSION", "CUDA_HOME",
    "NV_CUDA_CUDART_VERSION", "ORT_CACHE_DIR", "DOCS_RS",
}


def try_lock(lock):
    if os.name == "nt":
        import msvcrt

        lock.seek(0)
        msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
    else:
        import fcntl

        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)


@contextmanager
def exclusive_lock(path, timeout=30 * 60):
    path.parent.mkdir(parents=True, exist_ok=True)
    # Keep the file so that waiters and new arrivals always lock the same inode.
    with path.open("a+b") as lock:
        deadline = time.monotonic() + timeout
        waiting = False
        while True:
            try:
                try_lock(lock)
                break
            except OSError as exc:
                if exc.errno not in {errno.EAGAIN, errno.EACCES}:
                    raise
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(f"timed out after {timeout:g}s waiting for release lock {path}") from exc
                if not waiting:
                    print(f"waiting up to {timeout:g}s for release lock {path}", file=sys.stderr)
                    waiting = True
                time.sleep(min(0.1, remaining))
        yield


def cargo_config(options, root, env):
    # Bootstrap is needed only for Cargo's read-only inspection command.
    # Target and feature selection do not change Cargo's config discovery.
    result = subprocess.run(
        ["cargo", "-Z", "unstable-options", "config", "get", "--format=json", *options],
        cwd=root, env={**env, "RUSTC_BOOTSTRAP": "1"},
        capture_output=True, text=True, encoding="utf-8",
    )
    if result.returncode:
        diagnostic = result.stderr.strip()
        if secret := env.get(OAUTH_SECRET):
            diagnostic = diagnostic.replace(secret, "[redacted]")
        raise ValueError(f"could not inspect the resolved Cargo config:\n{diagnostic}")
    return json.loads(result.stdout)


def config_env(config, name):
    value = config.get("env", {}).get(name, "")
    return value["value"] if isinstance(value, dict) else value


def verify_onnx_config(config, env, target):
    env_keys = env.keys() | config.get("env", {}).keys()
    overrides = ORT_OVERRIDES.intersection(env_keys)
    if sys.platform == "linux" and "XDG_CACHE_HOME" in env_keys:
        overrides.add("XDG_CACHE_HOME")
    # A true value tells ort-sys to skip downloading the pinned archive.
    offline_key = "CARGO_NET_OFFLINE"
    offline = config_env(config, offline_key)
    if not isinstance(offline, str):
        raise ValueError(f"invalid {offline_key} in Cargo's [env] configuration")
    if any(value.lower() in {"true", "1"} for value in (env.get(offline_key, ""), offline)):
        overrides.add(offline_key)
    if "onnxruntime" in config.get("target", {}).get(target, {}):
        overrides.add(f"target.{target}.onnxruntime")
    if overrides:
        raise ValueError("ONNX Runtime release notices require the pinned download without "
                         "linkage overrides: " + ", ".join(sorted(overrides)))


def cargo_executable(messages, binary, onnx):
    executables = set()
    onnx_scripts = []
    for line in messages:
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            target = message["target"]
            if target["name"] == binary and target["kind"] == ["bin"]:
                executables.add(Path(message["executable"]))
        if onnx and message.get("reason") == "build-script-executed":
            package = message["package_id"]
            if package.startswith("registry+https://github.com/rust-lang/crates.io-index#"
                                  + onnx["crate"] + "@"):
                onnx_scripts.append(message)
    if len(executables) != 1:
        raise ValueError(f"expected one Cargo executable for {binary}, found {len(executables)}")
    if onnx:
        # The pinned build script emits this cfg only in its download branch.
        # Check the actual build too, including fresh Cargo artefacts and
        # overrides introduced by a wrapper only when invoking `cargo build`.
        version = onnx["crate_version"]
        if (len(onnx_scripts) != 1
                or not onnx_scripts[0]["package_id"].endswith(f"@{version}")
                or "pyke" not in onnx_scripts[0]["cfgs"]
                or "static=onnxruntime" not in onnx_scripts[0]["linked_libs"]):
            raise ValueError("Cargo did not link the pinned ONNX Runtime download")
    return executables.pop()


def build(args):
    root = Path.cwd()
    onnx = json.loads((root / "native-notices.json").read_text(encoding="utf-8"))["onnxruntime"]
    if args.target not in onnx["targets"]:
        onnx = None
    features = sorted(set(re.findall(r"[^,\s]+", args.features)))
    # Keep room for dependency build paths on Windows.
    key = hashlib.sha256(",".join(features).encode()).hexdigest()[:24]
    cache = args.target_dir / "release-pkg" / f"{args.target}-{key}"
    command = ["cargo", "build", "--release", "--locked", "-p", args.binary,
               "--target", args.target]
    if args.features:
        command.extend(["--features", args.features])
    command.append("--message-format=json-render-diagnostics")
    options = [flag for value in args.config for flag in ("--config", value)]
    options.extend([
        "--config", f'env.SUBSPACE_PACKAGE_BUILD.value="{uuid4()}"',
        "--config", "env.SUBSPACE_PACKAGE_BUILD.force=true",
    ])
    env = {**os.environ, "CARGO_TARGET_DIR": str(cache)}
    with ExitStack() as cleanup:
        config = cargo_config(options, root, env)
        if env.get(OAUTH_SECRET) or config_env(config, OAUTH_SECRET):
            # Rustc dep-info, fingerprints and executable copies can all
            # contain the secret. Keep both Cargo directories private.
            private = cleanup.enter_context(TemporaryDirectory(prefix="subspace-release-"))
            cache = Path(private)
            env["CARGO_TARGET_DIR"] = private
            for key in ("target-dir", "build-dir"):
                options.extend(["--config", f"build.{key}={json.dumps(cache.as_posix(), ensure_ascii=False)}"])
            config = cargo_config(options, root, env)
        if onnx:
            verify_onnx_config(config, env, args.target)
        cleanup.enter_context(exclusive_lock(cache / ".package.lock"))
        messages = cleanup.enter_context(TemporaryFile(mode="w+", encoding="utf-8"))
        # The supervisor owns the lock. Cargo and its descendants never inherit it.
        subprocess.run([*command, *options], cwd=root, env=env, stdout=messages, check=True)
        messages.seek(0)
        executable = cargo_executable(messages, args.binary, onnx)
        suffix = ".exe" if "-windows-" in args.target else ""
        shutil.copy2(executable, args.stage / (args.binary + suffix))


def publish(args):
    final = args.dist / args.archive.name
    checksum = final.with_name(final.name + ".sha256")
    staged_checksum = args.archive.with_name(checksum.name)
    locks = args.dist / ".locks"
    with exclusive_lock(locks / (final.name + ".lock")):
        subprocess.run(args.command, cwd=args.archive.parent, check=True)
        digest = hashlib.sha256()
        with args.archive.open("rb") as archive:
            while chunk := archive.read(1024 * 1024):
                digest.update(chunk)
        staged_checksum.write_bytes(f"{digest.hexdigest()}  {final.name}\n".encode())
        replace_pair(args.archive, staged_checksum, final, checksum, locks)


def replace_pair(archive, staged_checksum, final, checksum, locks):
    backup = Path(mkdtemp(prefix="previous-", dir=locks))
    old_archive = backup / final.name
    old_checksum = backup / checksum.name
    keep_backup = installed_archive = False
    try:
        if final.exists():
            try:
                os.link(final, old_archive)
            except OSError:
                shutil.copy2(final, old_archive)
        # Withdraw the checksum while the archive name is changing, so
        # readers never see the new archive next to the old checksum.
        try:
            checksum.replace(old_checksum)
        except FileNotFoundError:
            pass
        try:
            archive.replace(final)
            installed_archive = True
            staged_checksum.replace(checksum)
        except OSError:
            try:
                if installed_archive:
                    if old_archive.exists():
                        old_archive.replace(final)
                    else:
                        final.unlink()
                if old_checksum.exists():
                    old_checksum.replace(checksum)
            except OSError as exc:
                keep_backup = True
                raise OSError(f"could not restore the release files: {exc}; recovery files: {backup}") from exc
            raise
    finally:
        if not keep_backup:
            shutil.rmtree(backup)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(required=True)
    builder = commands.add_parser("build")
    builder.add_argument("target_dir", type=Path)
    builder.add_argument("binary")
    builder.add_argument("stage", type=Path)
    builder.add_argument("--target", required=True)
    builder.add_argument("--features", default="")
    builder.add_argument("--config", action="append", default=[])
    builder.set_defaults(run=build)
    publisher = commands.add_parser("publish")
    publisher.add_argument("archive", type=Path)
    publisher.add_argument("dist", type=Path)
    publisher.add_argument("command", nargs=argparse.REMAINDER)
    publisher.set_defaults(run=publish)
    args = parser.parse_args()
    try:
        args.run(args)
    except subprocess.CalledProcessError as exc:
        sys.exit(exc.returncode)
    except (OSError, ValueError) as exc:
        sys.exit(str(exc))


if __name__ == "__main__":
    main()
