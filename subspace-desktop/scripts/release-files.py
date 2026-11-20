#!/usr/bin/env python3
"""Stage Cargo's release executable and publish release archives under locks."""

import argparse
from contextlib import contextmanager
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from tempfile import TemporaryFile
import time


@contextmanager
def exclusive_lock(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    # Keep the file so that waiters and new arrivals always lock the same inode.
    with path.open("a+b") as lock:
        if os.name == "nt":
            import msvcrt

            while True:
                lock.seek(0)
                try:
                    msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
                    break
                except OSError as exc:
                    if exc.errno != errno.EACCES:
                        raise
                    time.sleep(0.1)
        else:
            import fcntl

            fcntl.flock(lock, fcntl.LOCK_EX)
        yield lock.fileno()


def cargo_executable(messages, binary):
    executables = set()
    for line in messages:
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            target = message["target"]
            if target["name"] == binary and target["kind"] == ["bin"]:
                executables.add(Path(message["executable"]))
    if len(executables) != 1:
        raise ValueError(f"expected one Cargo executable for {binary}, found {len(executables)}")
    return executables.pop()


def build(args):
    features = sorted(set(re.findall(r"[^,\s]+", args.features)))
    # Keep room for dependency build paths on Windows.
    key = hashlib.sha256(",".join(features).encode()).hexdigest()[:24]
    cache = args.target_dir / "release-pkg" / f"{args.target}-{key}"
    command = ["cargo", "build", "--release", "--locked", "-p", args.binary,
               "--target", args.target]
    if args.features:
        command.extend(["--features", args.features])
    command.append("--message-format=json-render-diagnostics")
    env = {**os.environ, "CARGO_TARGET_DIR": str(cache)}
    cache_lock = cache / ".package.lock"
    with exclusive_lock(cache_lock) as lock, TemporaryFile(mode="w+", encoding="utf-8") as messages:
        # On Unix the child also retains the lock if its parent is killed.
        inherited = {"pass_fds": (lock,)} if os.name != "nt" else {}
        subprocess.run(command, env=env, stdout=messages, check=True, **inherited)
        messages.seek(0)
        executable = cargo_executable(messages, args.binary)
        suffix = ".exe" if "-windows-" in args.target else ""
        shutil.copy2(executable, args.stage / (args.binary + suffix))


def publish(args):
    final = args.dist / args.archive.name
    checksum = final.with_name(final.name + ".sha256")
    staged_checksum = args.archive.with_name(checksum.name)
    previous_checksum = args.archive.parent / ".previous.sha256"
    with exclusive_lock(args.dist / ".locks" / (final.name + ".lock")):
        subprocess.run(args.command, cwd=args.archive.parent, check=True)
        digest = hashlib.sha256()
        with args.archive.open("rb") as archive:
            while chunk := archive.read(1024 * 1024):
                digest.update(chunk)
        staged_checksum.write_bytes(f"{digest.hexdigest()}  {final.name}\n".encode())
        # Withdraw the old checksum before replacing the archive. A failed
        # archive rename leaves the old archive and its checksum in place.
        try:
            checksum.replace(previous_checksum)
        except FileNotFoundError:
            pass
        try:
            args.archive.replace(final)
        except OSError:
            if previous_checksum.exists():
                previous_checksum.replace(checksum)
            raise
        staged_checksum.replace(checksum)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(required=True)
    builder = commands.add_parser("build")
    builder.add_argument("target_dir", type=Path)
    builder.add_argument("binary")
    builder.add_argument("stage", type=Path)
    builder.add_argument("--target", required=True)
    builder.add_argument("--features", default="")
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
