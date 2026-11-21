#!/usr/bin/env python3
"""Exercise release publication failures and stamp refresh in a small Cargo project."""

from contextlib import redirect_stderr
import hashlib
import importlib.util
import io
import lzma
import os
from pathlib import Path
import signal
import subprocess
import sys
import tarfile
from tempfile import TemporaryDirectory
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parent
HELPER = SCRIPTS / "release-files.py"
SPEC = importlib.util.spec_from_file_location("release_files", HELPER)
release_files = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_files)


class ReleaseFiles(unittest.TestCase):
    def setUp(self):
        temporary = TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_lock_times_out(self):
        lock = self.root / ".package.lock"
        with patch.dict(os.environ, {release_files.LOCK_TIMEOUT: "0.2"}):
            with release_files.exclusive_lock(lock):
                with self.assertRaisesRegex(
                    TimeoutError, r"0\.2s waiting for release lock .*\.package\.lock .*SUBSPACE_RELEASE_LOCK_TIMEOUT"
                ):
                    with release_files.exclusive_lock(lock):
                        self.fail("acquired a held lock")
            with patch.dict(os.environ, {release_files.LOCK_TIMEOUT: "NaN"}):
                with self.assertRaisesRegex(ValueError, "SUBSPACE_RELEASE_LOCK_TIMEOUT"):
                    with release_files.exclusive_lock(lock):
                        self.fail("accepted an invalid lock timeout")

        with patch.dict(os.environ):
            os.environ.pop(release_files.LOCK_TIMEOUT, None)
            with release_files.exclusive_lock(lock):
                with patch.object(time, "monotonic", side_effect=[0, 1800]):
                    with self.assertRaisesRegex(TimeoutError, r"1800s waiting for release lock"):
                        with release_files.exclusive_lock(lock):
                            self.fail("acquired a held lock")

    def test_cargo_config_errors_suppress_credentials(self):
        workspace = self.root / "fixture workspace"
        workspace.mkdir()
        (workspace / "native-notices.json").write_text('{"onnxruntime":{"targets":[]}}')
        config = workspace / "broken config.toml"
        credential = "fixture-deploy-auth-value"
        config.write_text(f'[env]\nDEPLOY_AUTH = "{credential}" trailing\n')
        inline_override = 'env.BUILD_AUTH="fixture-build-auth-value"'
        env = {**os.environ, "CARGO_HOME": str(self.root / "cargo-home")}
        failed = subprocess.run(
            [sys.executable, str(HELPER), "build", "target", "fixture", "stage",
             "--target", "fixture-target", "--config", config.name, "--config", inline_override],
            cwd=workspace, env=env, capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(failed.returncode, 1)
        self.assertEqual(failed.stdout, "")
        message, reproduction = failed.stderr.splitlines()
        self.assertIn("could not inspect the resolved Cargo config", message)
        self.assertIn("cargo -Z unstable-options config get --format=json", reproduction)
        self.assertNotIn(credential, failed.stderr)
        self.assertNotIn("fixture-build-auth-value", failed.stderr)
        local = subprocess.run(
            ["bash", "-c", reproduction], cwd=self.root,
            env={**env, "RELEASE_CARGO_CONFIG_2": inline_override},
            capture_output=True, text=True, timeout=10,
        )
        self.assertNotEqual(local.returncode, 0)
        self.assertIn("TOML parse error", local.stderr)
        self.assertIn(credential, local.stderr)

    def test_onnx_uses_verified_archive_instead_of_cached_extraction(self):
        native = self.root / "ort-cache"
        cache = self.root / "build-cache"
        native.mkdir()
        cache.mkdir()
        original = b"pinned ONNX Runtime library"
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w") as archive:
            member = tarfile.TarInfo("libonnxruntime.a")
            member.size = len(original)
            archive.addfile(member, io.BytesIO(original))
        filters = [{"id": lzma.FILTER_LZMA2, "dict_size": 1 << 26}]
        compressed = lzma.compress(buffer.getvalue(), format=lzma.FORMAT_RAW, filters=filters)
        digest = hashlib.sha256(compressed).hexdigest()
        extracted = native / digest / "libonnxruntime.a"
        extracted.parent.mkdir()
        extracted.write_bytes(b"tampered extracted library")
        upstream = native / f"{digest}.tar.lzma2"
        upstream.write_bytes(b"tampered cached archive")
        with patch.object(release_files, "ort_download_cache", return_value=native), \
                patch.object(release_files, "urlopen") as download:
            with TemporaryDirectory(dir=cache) as temporary:
                private = Path(temporary)
                with self.assertRaisesRegex(ValueError, "ONNX Runtime archive checksum mismatch"):
                    release_files.prepare_onnx_archive(cache, private, "fixture", "", digest)
                self.assertFalse((private / "lib").exists())
            upstream.write_bytes(compressed)
            with TemporaryDirectory(dir=cache) as temporary:
                library = release_files.prepare_onnx_archive(cache, Path(temporary), "fixture", "", digest)
                self.assertEqual(library.parent, Path(temporary))
                self.assertEqual((library / "libonnxruntime.a").read_bytes(), original)
            download.assert_not_called()
        self.assertEqual(extracted.read_bytes(), b"tampered extracted library")
        self.assertEqual((cache / upstream.name).read_bytes(), compressed)

    @unittest.skipIf(os.name == "nt", "uses Unix signals and advisory locks")
    def test_cargo_group_stops_before_unlock(self):
        worker = self.root / "cargo-wrapper.py"
        worker.write_text("""
import pathlib, subprocess, sys, time

code = '''
import fcntl, pathlib, time
with open("child.lock", "w") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    pathlib.Path("child-ready").touch()
    while not pathlib.Path("overwrite").exists():
        time.sleep(0.02)
    pathlib.Path("cached-executable").write_text("orphan")
'''
child = subprocess.Popen([sys.executable, "-c", code])
while not pathlib.Path("child-ready").exists():
    time.sleep(0.02)
if sys.argv[1] == "error":
    sys.exit(23)
if sys.argv[1] == "success":
    sys.exit(0)
child.wait()
""")
        supervisor = f"""
import os, runpy, subprocess, sys
from pathlib import Path

helper = runpy.run_path({str(HELPER)!r})
with helper["exclusive_lock"](Path(".package.lock")), open("messages", "w") as messages:
    try:
        helper["run_cargo"]([sys.executable, {str(worker)!r}, sys.argv[1]], Path.cwd(), os.environ, messages)
    except KeyboardInterrupt:
        sys.exit(42)
    except subprocess.CalledProcessError as exc:
        sys.exit(exc.returncode)
"""
        for mode, expected in (("success", 0), ("error", 23), ("SIGINT", 42), ("SIGTERM", 42)):
            with self.subTest(mode=mode):
                cwd = self.root / mode
                cwd.mkdir()
                ready = cwd / "child-ready"
                with subprocess.Popen([sys.executable, "-c", supervisor, mode], cwd=cwd) as process:
                    try:
                        deadline = time.monotonic() + 10
                        while not ready.exists():
                            self.assertLess(time.monotonic(), deadline, "Cargo child never started")
                            time.sleep(0.02)
                        if mode not in {"success", "error"}:
                            process.send_signal(getattr(signal, mode))
                        self.assertEqual(process.wait(timeout=10), expected)
                        with patch.dict(os.environ, {release_files.LOCK_TIMEOUT: "0"}), \
                                release_files.exclusive_lock(cwd / ".package.lock"):
                            if mode == "success":
                                with self.assertRaises(TimeoutError):
                                    with release_files.exclusive_lock(cwd / "child.lock"):
                                        self.fail("successful Cargo build stopped its child")
                            else:
                                with release_files.exclusive_lock(cwd / "child.lock"):
                                    pass
                            self.assertFalse((cwd / "cached-executable").exists())
                    finally:
                        # Release a surviving child even when the fixture fails.
                        (cwd / "overwrite").touch()
                        if process.poll() is None:
                            process.terminate()
                        elif mode == "success":
                            with patch.dict(os.environ, {release_files.LOCK_TIMEOUT: "10"}), \
                                    release_files.exclusive_lock(cwd / "child.lock"):
                                pass

    @unittest.skipIf(os.name == "nt", "uses Unix process groups")
    def test_exited_group_leader_on_macos_counts_as_stopped(self):
        # macOS reports EPERM, not ESRCH, when only the unreaped leader is left.
        refused = PermissionError("fixture EPERM")
        for platform in ("darwin", "linux"):
            with self.subTest(platform=platform), \
                    subprocess.Popen([sys.executable, "-c", ""], start_new_session=True) as process, \
                    patch.object(release_files.os, "killpg", side_effect=refused) as killpg, \
                    patch.object(release_files.sys, "platform", platform):
                if platform == "darwin":
                    release_files.stop_posix_group(process)
                    self.assertEqual(process.returncode, 0)
                else:
                    with self.assertRaisesRegex(PermissionError, "fixture EPERM"):
                        release_files.stop_posix_group(process)
                killpg.assert_called_once_with(process.pid, signal.SIGKILL)

    def test_failed_checksum_rename_restores_previous_pair(self):
        dist = self.root / "dist"
        stage = dist / "stage"
        stage.mkdir(parents=True)
        archive = stage / "subspace.tar.gz"
        final = dist / archive.name
        checksum = final.with_suffix(".gz.sha256")
        staged_checksum = stage / checksum.name
        replace = Path.replace

        def fail_checksum(path, destination):
            if path == staged_checksum:
                raise PermissionError("fixture checksum contention")
            return replace(path, destination)

        args = SimpleNamespace(archive=archive, dist=dist, command=[
            sys.executable, "-c", f"from pathlib import Path; Path({archive.name!r}).write_bytes(b'new')",
        ])
        with patch.object(Path, "replace", fail_checksum):
            # A failed first publication leaves neither file behind.
            with self.assertRaisesRegex(PermissionError, "fixture checksum contention"):
                release_files.publish(args)
            self.assertFalse(final.exists())
            self.assertFalse(checksum.exists())
            final.write_bytes(b"previous archive")
            checksum.write_bytes(b"previous checksum")
            with self.assertRaisesRegex(PermissionError, "fixture checksum contention"):
                release_files.publish(args)
        self.assertEqual(final.read_bytes(), b"previous archive")
        self.assertEqual(checksum.read_bytes(), b"previous checksum")
        self.assertFalse(list((dist / ".locks").glob("previous-*")))

    def test_interrupt_during_publication_restores_both_files(self):
        dist = self.root / "dist"
        stage = dist / "stage"
        locks = dist / ".locks"
        stage.mkdir(parents=True)
        locks.mkdir()
        archive, staged_checksum = stage / "app.zip", stage / "app.zip.sha256"
        final, checksum = dist / archive.name, dist / staged_checksum.name
        unlink, replace = Path.unlink, Path.replace

        for interrupted_path in (checksum, archive, staged_checksum):
            with self.subTest(interrupted_path=interrupted_path):
                archive.write_bytes(b"new archive")
                staged_checksum.write_bytes(b"new checksum")
                final.write_bytes(b"previous archive")
                checksum.write_bytes(b"previous checksum")
                original = KeyboardInterrupt("fixture interrupt")
                fired = False

                def interrupt_after(operation):
                    def run(path, *args, **kwargs):
                        nonlocal fired
                        result = operation(path, *args, **kwargs)
                        if path == interrupted_path and not fired:
                            fired = True
                            raise original
                        return result
                    return run

                with patch.object(Path, "unlink", interrupt_after(unlink)), \
                        patch.object(Path, "replace", interrupt_after(replace)):
                    with self.assertRaises(KeyboardInterrupt) as raised:
                        release_files.replace_pair(archive, staged_checksum, final, checksum, locks)
                self.assertIs(raised.exception, original)
                self.assertEqual(final.read_bytes(), b"previous archive")
                self.assertEqual(checksum.read_bytes(), b"previous checksum")
                self.assertFalse(list(locks.glob("previous-*")))

    @unittest.skipIf(os.name == "nt", "uses Unix signals")
    def test_termination_during_publication_restores_both_files(self):
        publisher = f"""
import os, runpy, signal, sys
from pathlib import Path

helper = runpy.run_path({str(HELPER)!r})
replace = Path.replace

def terminate_after_archive(path, destination):
    result = replace(path, destination)
    if path == Path(sys.argv[2]):
        os.kill(os.getpid(), signum)
    return result

signum = getattr(signal, sys.argv.pop(1))
previous = signal.getsignal(signum)
Path.replace = terminate_after_archive
try:
    helper["main"]()
finally:
    assert signal.getsignal(signum) == previous
"""
        for name in ("SIGTERM", "SIGHUP"):
            if not hasattr(signal, name):
                continue
            with self.subTest(signal=name):
                dist = self.root / name
                stage = dist / "stage"
                stage.mkdir(parents=True)
                archive = stage / "app.zip"
                final = dist / archive.name
                checksum = dist / "app.zip.sha256"
                final.write_bytes(b"previous archive")
                checksum.write_bytes(b"previous checksum")
                result = subprocess.run(
                    [sys.executable, "-c", publisher, name, "publish", str(archive), str(dist),
                     sys.executable, "-c", "from pathlib import Path; Path('app.zip').write_bytes(b'new')"],
                    capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"release publication interrupted by {name}", result.stderr)
                self.assertEqual(final.read_bytes(), b"previous archive")
                self.assertEqual(checksum.read_bytes(), b"previous checksum")
                self.assertFalse(list((dist / ".locks").glob("previous-*")))

    def test_failed_restore_keeps_recovery_copies_and_original_exception(self):
        stage, locks = self.root / "stage", self.root / ".locks"
        stage.mkdir()
        locks.mkdir()
        archive, staged_checksum = stage / "app.zip", stage / "app.zip.sha256"
        final, checksum = self.root / archive.name, self.root / staged_checksum.name
        archive.write_bytes(b"new archive")
        staged_checksum.write_bytes(b"new checksum")
        final.write_bytes(b"previous archive")
        checksum.write_bytes(b"previous checksum")
        original = KeyboardInterrupt("fixture interrupt")
        replace = Path.replace

        def fail_checksum(path, destination):
            if path == staged_checksum:
                raise original
            if destination == checksum:
                raise PermissionError("fixture rollback contention")
            return replace(path, destination)

        stderr = io.StringIO()
        with patch.object(Path, "replace", fail_checksum), redirect_stderr(stderr):
            with self.assertRaises(KeyboardInterrupt) as raised:
                release_files.replace_pair(archive, staged_checksum, final, checksum, locks)
        self.assertIs(raised.exception, original)
        backup, = locks.glob("previous-*")
        self.assertIn(f"recovery files: {backup}", stderr.getvalue())
        self.assertEqual((backup / final.name).read_bytes(), b"previous archive")
        self.assertEqual((backup / checksum.name).read_bytes(), b"previous checksum")

    def test_cleanup_failure_keeps_original_exception_after_restore(self):
        stage, locks = self.root / "stage", self.root / ".locks"
        stage.mkdir()
        locks.mkdir()
        archive, staged_checksum = stage / "app.zip", stage / "app.zip.sha256"
        final, checksum = self.root / archive.name, self.root / staged_checksum.name
        archive.write_bytes(b"new archive")
        staged_checksum.write_bytes(b"new checksum")
        final.write_bytes(b"previous archive")
        checksum.write_bytes(b"previous checksum")
        original = KeyboardInterrupt("fixture interrupt")
        replace = Path.replace

        def fail_checksum(path, destination):
            if path == staged_checksum:
                raise original
            return replace(path, destination)

        stderr = io.StringIO()
        with patch.object(Path, "replace", fail_checksum), redirect_stderr(stderr), \
                patch.object(release_files.shutil, "rmtree", side_effect=OSError("fixture cleanup failure")):
            with self.assertRaises(KeyboardInterrupt) as raised:
                release_files.replace_pair(archive, staged_checksum, final, checksum, locks)
        self.assertIs(raised.exception, original)
        self.assertIn("fixture cleanup failure", stderr.getvalue())
        self.assertEqual(final.read_bytes(), b"previous archive")
        self.assertEqual(checksum.read_bytes(), b"previous checksum")

    def test_release_cache_refreshes_the_build_stamp(self):
        for directory in ("src", "scripts", "spaces", "clients", "products", "terminal", "stage"):
            (self.root / directory).mkdir()
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "fixture-stamp"\nversion = "0.1.0"\nedition = "2024"\n'
        )
        (self.root / "native-notices.json").write_text('{"onnxruntime":{"targets":[]}}')
        (self.root / "stamp.rs").write_bytes(
            (SCRIPTS.parent / "products/subspace/build/stamp.rs").read_bytes()
        )
        (self.root / "build.rs").write_text("""
mod stamp;
fn main() {
    let build = stamp::BuildEnv {
        workspace: std::env::current_dir().unwrap(), source_commit: None,
        target: String::new(), profile: "release".into(), capture: false,
        machine: stamp::machine_name(), unix_time: stamp::timestamp::now(),
    };
    for line in stamp::directives(&build) { println!("{line}"); }
}
""")
        (self.root / "src/main.rs").write_text("""
fn main() {
    println!("{} {} {}", env!("SUBSPACE_BUILD_UNIX"),
        env!("SUBSPACE_BUILD_TIMESTAMP"), env!("SUBSPACE_BUILD_DIRTY"));
}
""")
        script = self.root / "scripts/fixture"
        script.write_text("clean\n")
        env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        env.pop(release_files.OAUTH_SECRET, None)

        def run(*command):
            return subprocess.check_output(command, cwd=self.root, env=env, text=True).strip()

        run("git", "init", "--quiet", "--template=")
        run("git", "add", "scripts/fixture")
        tree = run("git", "write-tree")
        commit = run("git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                     "commit-tree", tree, "-m", "fixture")
        run("git", "update-ref", "HEAD", commit)
        run("cargo", "generate-lockfile", "--offline")
        host = next(line[6:] for line in run("rustc", "-vV").splitlines() if line.startswith("host: "))
        stage = self.root / "stage"
        binary = stage / ("fixture-stamp.exe" if "-windows-" in host else "fixture-stamp")

        def build_stamp():
            run(sys.executable, str(HELPER), "build", str(self.root / "target"),
                "fixture-stamp", str(stage), "--target", host)
            return run(str(binary)).split()

        first_time, first_stamp, first_dirty = build_stamp()
        self.assertEqual(first_dirty, "false")
        script.write_text("dirty\n")
        time.sleep(1.05)
        next_time, next_stamp, next_dirty = build_stamp()
        self.assertGreater(int(next_time), int(first_time))
        self.assertNotEqual(first_stamp, next_stamp)
        self.assertEqual(next_dirty, "true")
        self.assertEqual(len(list((self.root / "target/release-pkg").iterdir())), 1)


if __name__ == "__main__":
    unittest.main()
