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
import select
import shlex
import shutil
import signal
import socket
import subprocess
import sys
from tempfile import TemporaryDirectory, TemporaryFile, mkdtemp
import time
from uuid import uuid4


OAUTH_SECRET = "SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET"
LOCK_TIMEOUT = "SUBSPACE_RELEASE_LOCK_TIMEOUT"
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


def cargo_config(config_flags, build_flags, root, env):
    # Bootstrap is needed only for Cargo's read-only inspection command.
    # Target and feature selection do not change Cargo's config discovery.
    command = ["cargo", "-Z", "unstable-options", "config", "get", "--format=json"]
    try:
        result = subprocess.run(
            [*command, *config_flags, *build_flags],
            cwd=root, env={**env, "RUSTC_BOOTSTRAP": "1"},
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True,
        )
        return json.loads(result.stdout)
    except (OSError, subprocess.CalledProcessError, ValueError):
        pass
    # An explicit inline override can itself contain a credential. Reference
    # the caller's original value through a shell variable in that case.
    overrides = [
        f'--config "${{RELEASE_CARGO_CONFIG_{index}:?set to original --config argument}}"'
        if "=" in value else f"--config {shlex.quote(value)}"
        for index, value in enumerate(config_flags[1::2], 1)
    ]
    target_dir = shlex.quote(env["CARGO_TARGET_DIR"])
    arguments = " ".join([shlex.join(command), *overrides, shlex.join(build_flags)])
    raise ValueError("could not inspect the resolved Cargo config. Run locally to see Cargo's output:\n"
                     f"cd {shlex.quote(root.as_posix())} && "
                     f"CARGO_TARGET_DIR={target_dir} RUSTC_BOOTSTRAP=1 {arguments}")


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
    # A Linux container's init may leave orphaned zombies indefinitely.
    # Check for live writers while the leader still holds the group ID.
    while sys.platform == "linux" and linux_group_has_live_members(process.pid):
        time.sleep(0.1)
    process.wait()


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
                yield process.wait
        finally:
            process.stdin.close()
            process.wait()
    else:
        # The session leader holds its PID until the parent sees Cargo's
        # result and either closes the pipe or signals the whole group.
        launcher = ("import socket, subprocess, sys\n"
                    "try:\n"
                    "    status = subprocess.call(sys.argv[2:])\n"
                    "except OSError as error:\n"
                    "    print(error, file=sys.stderr)\n"
                    "    status = 1\n"
                    "with socket.socket(fileno=int(sys.argv[1])) as control:\n"
                    "    control.sendall(f'{status}\\n'.encode())\n"
                    "    control.recv(1)\n")
        control, child = socket.socketpair()
        with control, child, control.makefile("rb") as result:
            fd = child.fileno()
            with subprocess.Popen([sys.executable, "-c", launcher, str(fd), *command],
                                  cwd=root, env=env, pass_fds=(fd,), start_new_session=True,
                                  stdout=messages) as process:
                child.close()
                status = None

                def wait_for_cargo(timeout):
                    nonlocal status
                    if not select.select([control], [], [], timeout)[0]:
                        raise subprocess.TimeoutExpired(command, timeout)
                    status = int(result.readline() or b"1")
                    return status

                try:
                    yield wait_for_cargo
                finally:
                    if status == 0:
                        control.shutdown(socket.SHUT_WR)
                    else:
                        stop_posix_group(process)


def run_cargo(command, root, env, messages):
    interrupted = None

    def record_interrupt(signum, _frame):
        nonlocal interrupted
        interrupted = signum

    # Defer asynchronous exceptions during launch and teardown. Leave the
    # cache lock held until Cargo has finished or its interrupted group stops.
    with ExitStack() as handlers:
        for name in ("SIGINT", "SIGTERM", "SIGHUP", "SIGBREAK"):
            if (signum := getattr(signal, name, None)) is not None:
                previous = signal.signal(signum, record_interrupt)
                handlers.callback(signal.signal, signum, previous)
        with cargo_process(command, root, env, messages) as wait_for_cargo:
            while interrupted is None:
                try:
                    status = wait_for_cargo(timeout=0.1)
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
    config_flags = [flag for value in args.config for flag in ("--config", value)]
    build_flags = [
        "--config", f'env.SUBSPACE_PACKAGE_BUILD.value="{uuid4()}"',
        "--config", "env.SUBSPACE_PACKAGE_BUILD.force=true",
    ]
    env = {**os.environ, "CARGO_TARGET_DIR": str(cache)}
    with ExitStack() as cleanup:
        config = cargo_config(config_flags, build_flags, root, env)
        if env.get(OAUTH_SECRET) or config_env(config, OAUTH_SECRET):
            # Rustc dep-info, fingerprints and executable copies can all
            # contain the secret. Keep both Cargo directories private.
            private = cleanup.enter_context(TemporaryDirectory(prefix="subspace-release-"))
            cache = Path(private)
            env["CARGO_TARGET_DIR"] = private
            for key in ("target-dir", "build-dir"):
                build_flags.extend(["--config", f"build.{key}={json.dumps(cache.as_posix(), ensure_ascii=False)}"])
            config = cargo_config(config_flags, build_flags, root, env)
        if onnx:
            verify_onnx_config(config, env, args.target)
        cleanup.enter_context(exclusive_lock(cache / ".package.lock"))
        messages = cleanup.enter_context(TemporaryFile(mode="w+", encoding="utf-8"))
        # The supervisor owns the lock. Cargo and its descendants never inherit it.
        run_cargo([*command, *config_flags, *build_flags], root, env, messages)
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
        with publication_signals():
            replace_pair(args.archive, staged_checksum, final, checksum, locks)


@contextmanager
def publication_signals():
    interrupted = False

    def interrupt(signum, _frame):
        nonlocal interrupted
        if not interrupted:
            interrupted = True
            raise InterruptedError(f"release publication interrupted by {signal.Signals(signum).name}")

    # Keep the handler through rollback so repeated cancellation cannot interrupt it.
    with ExitStack() as handlers:
        for name in ("SIGTERM", "SIGHUP"):
            if (signum := getattr(signal, name, None)) is not None:
                previous = signal.signal(signum, interrupt)
                handlers.callback(signal.signal, signum, previous)
        yield


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
    except BaseException:
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
                print(f"could not restore the release files: {restore_error!r}\n"
                      f"recovery files: {backup}", file=sys.stderr)
        if not publishing:
            try:
                shutil.rmtree(backup, ignore_errors=True)
            except BaseException as cleanup_error:
                print(f"could not remove release backups at {backup}: {cleanup_error!r}", file=sys.stderr)
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
        sys.exit(str(exc))


if __name__ == "__main__":
    main()
