#!/usr/bin/env python3
"""Exercise release publication failures and stamp refresh in a small Cargo project."""

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
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
        self.root = Path(self.enterContext(TemporaryDirectory()))

    def test_lock_times_out(self):
        lock = self.root / ".package.lock"
        with release_files.exclusive_lock(lock):
            with self.assertRaisesRegex(TimeoutError, r"0\.2s waiting for release lock .*\.package\.lock"):
                with release_files.exclusive_lock(lock, timeout=0.2):
                    self.fail("acquired a held lock")

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
