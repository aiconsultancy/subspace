#!/usr/bin/env python3
"""Stage Cargo's release executable and publish release archives under locks."""

import argparse
from contextlib import ExitStack, contextmanager
import errno
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
from tempfile import TemporaryDirectory, TemporaryFile, mkdtemp
import time
from uuid import uuid4


OAUTH_SECRET = "SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET"
LOCK_TIMEOUT = "SUBSPACE_RELEASE_LOCK_TIMEOUT"
URL_USERINFO = re.compile(r"(\b[a-z][a-z0-9+.-]*://)[^\s/?#'\"<>]+@", re.IGNORECASE)
CREDENTIAL_VALUE = re.compile(
    r"""(?<![\w-])((?P<quote>['"]?)[\w-]*(?:token|password|passphrase|secret|(?:api|private)[-_]?key)"""
    r"""[\w-]*(?P=quote)[ \t]*[:=][ \t]*)[^\r\n]*""",
    re.IGNORECASE,
)
CARGO_SOURCE_LINE = re.compile(r"^([ \t]*\d+ \| )([^\r\n]+)", re.MULTILINE)
TOML_ASSIGNMENT = re.compile(r"""\s*(?:[\w.-]+|'[^']+'|"[^"]+")\s*=""")
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
def exclusive_lock(path):
    try:
        timeout = float(os.environ.get(LOCK_TIMEOUT, "1800"))
        if timeout < 0 or not math.isfinite(timeout):
            raise ValueError
    except ValueError:
        raise ValueError(f"{LOCK_TIMEOUT} must be a finite, non-negative number of seconds") from None
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
                    raise TimeoutError(f"timed out after {timeout:g}s waiting for release lock {path} "
                                       f"(adjust {LOCK_TIMEOUT} to wait longer)") from exc
                if not waiting:
                    print(f"waiting up to {timeout:g}s for release lock {path}", file=sys.stderr)
                    waiting = True
                time.sleep(min(0.1, remaining))
        yield


def redact_cargo_error(diagnostic, env):
    if secret := env.get(OAUTH_SECRET):
        diagnostic = diagnostic.replace(secret, "[redacted]")
    diagnostic = URL_USERINFO.sub(r"\1", diagnostic)
    diagnostic = CREDENTIAL_VALUE.sub(r"\1[redacted]", diagnostic)

    def redact_continuation(match):
        # Cargo can quote a continuation line of a malformed multiline value
        # without showing its key. Keep the line number and parser diagnostic.
        if TOML_ASSIGNMENT.match(match[2]) and not any(triple in match[2] for triple in ('"""', "'''")):
            return match[0]
        return match[1] + "[source omitted]"

    return CARGO_SOURCE_LINE.sub(redact_continuation, diagnostic)


def cargo_config(options, root, env):
    # Bootstrap is needed only for Cargo's read-only inspection command.
    # Target and feature selection do not change Cargo's config discovery.
    result = subprocess.run(
        ["cargo", "-Z", "unstable-options", "config", "get", "--format=json", *options],
        cwd=root, env={**env, "RUSTC_BOOTSTRAP": "1"},
        capture_output=True, text=True, encoding="utf-8",
    )
    if result.returncode:
        diagnostic = redact_cargo_error(result.stderr.strip(), env)
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


def verify_onnx_archive(script, target, expected):
    # The pinned build script reports the extracted distribution directory
    # as a native search path, including when Cargo reuses a fresh build.
    archives = set()
    for link_path in script.get("linked_paths", []):
        kind, _, directory = link_path.partition("=")
        if kind == "native":
            archive = Path(directory) / "libonnxruntime.a"
            if archive.is_file():
                archives.add(archive.resolve())
    if len(archives) != 1:
        raise ValueError(f"expected one libonnxruntime.a in Cargo's ONNX Runtime link paths for {target}")
    archive = archives.pop()
    with archive.open("rb") as library:
        actual = hashlib.file_digest(library, "sha256").hexdigest()
    if actual != expected:
        raise ValueError(f"ONNX Runtime static archive checksum mismatch for {target}: {archive}\n"
                         f"expected SHA-256 {expected}, got {actual}")


def cargo_executable(messages, binary, target, onnx):
    executables = set()
    onnx_scripts = []
    for line in messages:
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            artefact = message["target"]
            if artefact["name"] == binary and artefact["kind"] == ["bin"]:
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
        verify_onnx_archive(onnx_scripts[0], target, onnx["static_archives"][target])
    return executables.pop()


def linux_group_has_live_members(group):
    for stat in Path("/proc").glob("[0-9]*/stat"):
        try:
            fields = stat.read_bytes().rpartition(b") ")[2].split()
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
        if fields and int(fields[2]) == group and fields[0] not in {b"Z", b"X"}:
            return True
    return False


def stop_posix_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()
    while True:
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return
        # A Linux container's init may leave orphaned zombies in the
        # process table indefinitely. They can no longer write to the cache.
        if sys.platform == "linux" and not linux_group_has_live_members(process.pid):
            return
        time.sleep(0.1)


@contextmanager
def windows_cargo_job():
    import ctypes as ct
    from ctypes import wintypes as win

    class Accounting(ct.Structure):
        _fields_ = [(name, ct.c_longlong) for name in ("user", "kernel", "period_user", "period_kernel")]
        _fields_ += [(name, win.DWORD) for name in ("faults", "total", "active", "terminated")]

    kernel = ct.WinDLL("kernel32", use_last_error=True)

    def api(name, result, *arguments):
        function = getattr(kernel, name)
        function.restype = result
        function.argtypes = arguments
        return function

    create = api("CreateJobObjectW", win.HANDLE, ct.c_void_p, win.LPCWSTR)
    close = api("CloseHandle", win.BOOL, win.HANDLE)
    assign = api("AssignProcessToJobObject", win.BOOL, win.HANDLE, win.HANDLE)
    terminate = api("TerminateJobObject", win.BOOL, win.HANDLE, win.UINT)
    query = api("QueryInformationJobObject", win.BOOL, win.HANDLE, ct.c_int,
                ct.c_void_p, win.DWORD, ct.c_void_p)
    job = create(None, None)
    if not job:
        raise ct.WinError(ct.get_last_error())
    try:
        def attach(process):
            if not assign(job, int(process._handle)):
                raise ct.WinError(ct.get_last_error())

        yield attach
    finally:
        try:
            if not terminate(job, 1):
                raise ct.WinError(ct.get_last_error())
            info = Accounting()
            while True:
                # JobObjectBasicAccountingInformation
                if not query(job, 1, ct.byref(info), ct.sizeof(info), None):
                    raise ct.WinError(ct.get_last_error())
                if not info.active:
                    break
                time.sleep(0.1)
        finally:
            close(job)


@contextmanager
def cargo_process(command, root, env, messages):
    if os.name == "nt":
        # Gate the launcher so Cargo cannot spawn before it joins the job.
        launcher = ("import subprocess, sys\n"
                    "if sys.stdin.buffer.read(1) == b'1':\n"
                    "    sys.exit(subprocess.call(sys.argv[1:]))\n")
        process = subprocess.Popen([sys.executable, "-c", launcher, *command], cwd=root, env=env,
                                   stdin=subprocess.PIPE, stdout=messages,
                                   creationflags=subprocess.CREATE_NEW_PROCESS_GROUP)
        try:
            with windows_cargo_job() as attach:
                attach(process)
                process.stdin.write(b"1")
                process.stdin.close()
                yield process
        finally:
            process.stdin.close()
            process.wait()
    else:
        process = subprocess.Popen(command, cwd=root, env=env, stdout=messages, start_new_session=True)
        try:
            yield process
        finally:
            stop_posix_group(process)


def run_cargo(command, root, env, messages):
    interrupted = None

    def record_interrupt(signum, _frame):
        nonlocal interrupted
        interrupted = signum

    # Defer asynchronous exceptions during launch and teardown. Leave the
    # cache lock held until Cargo and every member of its group have stopped.
    with ExitStack() as handlers:
        for name in ("SIGINT", "SIGTERM", "SIGHUP", "SIGBREAK"):
            if (signum := getattr(signal, name, None)) is not None:
                previous = signal.signal(signum, record_interrupt)
                handlers.callback(signal.signal, signum, previous)
        with cargo_process(command, root, env, messages) as process:
            while interrupted is None:
                try:
                    status = process.wait(timeout=0.1)
                    break
                except subprocess.TimeoutExpired:
                    continue
        if interrupted is not None:
            raise KeyboardInterrupt(signal.Signals(interrupted).name)
    if status:
        raise subprocess.CalledProcessError(status, command)


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
        run_cargo([*command, *options], root, env, messages)
        messages.seek(0)
        executable = cargo_executable(messages, args.binary, args.target, onnx)
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


def backup_file(source, destination):
    try:
        os.link(source, destination)
    except OSError:
        shutil.copy2(source, destination)


def replace_pair(archive, staged_checksum, final, checksum, locks):
    backup = Path(mkdtemp(prefix="previous-", dir=locks))
    old_archive = backup / final.name
    old_checksum = backup / checksum.name
    publishing = False
    try:
        for source, destination in ((final, old_archive), (checksum, old_checksum)):
            if source.exists():
                backup_file(source, destination)
        publishing = True
        # Withdraw the checksum while the archive name is changing, so
        # readers never see the new archive next to the old checksum.
        checksum.unlink(missing_ok=True)
        archive.replace(final)
        staged_checksum.replace(checksum)
    except BaseException as original:
        if publishing:
            try:
                checksum.unlink(missing_ok=True)
                for saved, destination in ((old_archive, final), (old_checksum, checksum)):
                    if saved.exists():
                        # Keep both backups intact in case the restore itself fails.
                        replacement = backup / ".restore"
                        backup_file(saved, replacement)
                        replacement.replace(destination)
                        replacement.unlink(missing_ok=True)  # A same-inode rename may do nothing.
                    else:
                        destination.unlink(missing_ok=True)
                publishing = False
            except BaseException as restore_error:
                original.add_note(f"could not restore the release files: {restore_error!r}\n"
                                  f"recovery files: {backup}")
        if not publishing:
            try:
                shutil.rmtree(backup, ignore_errors=True)
            except BaseException as cleanup_error:
                original.add_note(f"could not remove release backups at {backup}: {cleanup_error!r}")
        raise
    shutil.rmtree(backup, ignore_errors=True)


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
        sys.exit("\n".join([str(exc), *getattr(exc, "__notes__", [])]))


if __name__ == "__main__":
    main()
