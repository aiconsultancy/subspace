"""scripts/publish.sh against a fake cargo and a fake registry; uploads nothing."""

import importlib.util
import json
import os
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SCRIPT = Path(__file__).with_name("publish.sh")
SPEC = importlib.util.spec_from_file_location("publish", Path(__file__).with_name("publish.py"))
PUBLISH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PUBLISH)

FAKE_CARGO = r'''#!/usr/bin/env python3
import json, os, sys
state = os.environ["FAKE_STATE"]
with open(os.path.join(state, "cargo.log"), "a") as log:
    log.write(" ".join(sys.argv[1:]) + "\n")
if sys.argv[1] == "metadata":
    print(open(os.path.join(state, "metadata.json")).read())
elif sys.argv[1:3] == ["publish", "--workspace"]:
    print("warning: aborting upload due to dry run")
elif sys.argv[1:3] == ["publish", "-p"]:
    name = sys.argv[3]
    limited = os.path.join(state, "limit-" + name)
    if os.path.exists(limited):
        os.remove(limited)
        sys.stderr.write("error: the remote server responded with an error (status 429 Too Many Requests)\n")
        sys.exit(101)
    published = json.load(open(os.path.join(state, "registry.json")))
    published.setdefault(name, []).append("0.1.0")
    json.dump(published, open(os.path.join(state, "registry.json"), "w"))
'''


def package(name: str, *dependencies: tuple[str, str | None]) -> dict:
    return {"name": name, "version": "0.1.0", "publish": None,
            "dependencies": [{"name": dep, "kind": kind} for dep, kind in dependencies]}


class Registry(BaseHTTPRequestHandler):
    state: Path

    def log_message(self, *args) -> None:
        pass

    def do_GET(self) -> None:
        published = json.loads((self.state / "registry.json").read_text())
        name = self.path.rstrip("/").rsplit("/", 1)[-1]
        if name not in published:
            self.send_response(404)
            self.end_headers()
            return
        versions = published[name]
        if self.path.startswith("/api/"):
            body = json.dumps({"versions": [{"num": version} for version in versions]})
        else:
            body = "\n".join(json.dumps({"name": name, "vers": v}) for v in versions)
        self.send_response(200)
        self.end_headers()
        self.wfile.write(body.encode())


class PublishScriptTests(unittest.TestCase):
    def setUp(self) -> None:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        base = Path(temp.name)
        self.root, self.state, bin_dir = base / "framework", base / "state", base / "bin"
        for directory in (self.root, self.state, bin_dir):
            directory.mkdir()
        # Hooks export repository-local variables. Remove them before creating
        # the fixture, and use the same environment for Git and the script.
        local = subprocess.check_output(
            ["git", "rev-parse", "--local-env-vars"], text=True).splitlines()
        self.env = {name: value for name, value in os.environ.items() if name not in local}
        # Fixture commits must never use the developer's signing configuration.
        self.env.update({"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"})
        (bin_dir / "cargo").write_text(FAKE_CARGO)
        (bin_dir / "cargo").chmod(0o755)
        self.packages = [
            package("subspace-framework", ("subspace-shell", None)),
            package("subspace-shell", ("subspace-core", None), ("subspace-test", "dev")),
            package("subspace-core"),
            package("subspace-test"),
        ]
        (self.state / "metadata.json").write_text(json.dumps({"packages": self.packages}))
        (self.state / "registry.json").write_text(json.dumps({"subspace-core": ["0.1.0"]}))
        self.git_repository(base / "remote.git")
        handler = type("Handler", (Registry,), {"state": self.state})
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        url = f"http://127.0.0.1:{server.server_address[1]}"
        self.env.update({"PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "FAKE_STATE": str(self.state), "SUBSPACE_PUBLISH_ROOT": str(self.root),
                    "SUBSPACE_PUBLISH_API": url, "SUBSPACE_PUBLISH_INDEX": url,
                    "SUBSPACE_PUBLISH_TIME_SCALE": "0"})
        for name in ("CARGO_REGISTRY_TOKEN", "GITHUB_ACTIONS", "GITHUB_REF_NAME"):
            self.env.pop(name, None)

    def git(self, *args: str, cwd: Path | None = None) -> str:
        return subprocess.run(["git", *args], cwd=cwd or self.root, check=True, text=True,
                              capture_output=True, env=self.env).stdout.strip()

    def git_repository(self, remote: Path) -> None:
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "fixture@example.test")
        self.git("config", "user.name", "Fixture")
        (self.root / ".gitignore").write_text(".local/\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.git("init", "-q", "--bare", str(remote), cwd=self.root.parent)
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "-q", "origin", "main")

    def stamp(self) -> None:
        sha = self.git("rev-parse", "HEAD")
        (self.root / ".local/checks").mkdir(parents=True)
        for name in ("check", "publish-check"):
            (self.root / f".local/checks/{name}-{sha}").touch()

    def publish(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["bash", str(SCRIPT), *args], env=self.env, text=True,
                              capture_output=True)

    def cargo_calls(self) -> list[str]:
        log = self.state / "cargo.log"
        return log.read_text().splitlines() if log.exists() else []

    def test_the_order_follows_every_dependency_kind_and_breaks_ties_by_name(self) -> None:
        order = [package["name"] for package in PUBLISH.publish_order(self.packages)]
        self.assertEqual(order, ["subspace-core", "subspace-test", "subspace-shell",
                                 "subspace-framework"])
        cycle = [package("a", ("b", None)), package("b", ("a", "dev"))]
        with self.assertRaises(SystemExit):
            PUBLISH.publish_order(cycle)

    def test_the_estimate_follows_the_new_crate_and_update_limits(self) -> None:
        crates = [PUBLISH.Crate(str(n), "0.1.0", "new") for n in range(24)]
        self.assertEqual(PUBLISH.estimate_minutes(crates), 190)
        updates = [PUBLISH.Crate(str(n), "0.2.0", "update") for n in range(32)]
        self.assertEqual(PUBLISH.estimate_minutes(updates), 2)
        self.assertEqual(PUBLISH.index_path("ab"), "2/ab")
        self.assertEqual(PUBLISH.index_path("abc"), "3/a/abc")
        self.assertEqual(PUBLISH.index_path("subspace-core"), "su/bs/subspace-core")

    def test_a_dry_run_reports_refusals_and_uploads_nothing(self) -> None:
        (self.root / "dirty.txt").write_text("uncommitted")
        result = self.publish("--dry-run")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("3 new", result.stdout)
        self.assertIn("A real run would refuse: the tree has uncommitted changes", result.stdout)
        self.assertIn("CARGO_REGISTRY_TOKEN is not set", result.stdout)
        self.assertIn("subspace-core 0.1.0: already on crates.io", result.stdout)
        self.assertIn("Dry run passed; nothing was uploaded.", result.stdout)
        calls = self.cargo_calls()
        self.assertIn("publish --workspace --dry-run --no-verify --allow-dirty --locked", calls)
        self.assertFalse([call for call in calls if call.startswith("publish -p")])

    def test_a_real_run_refuses_without_the_checks_or_a_token(self) -> None:
        result = self.publish()
        self.assertEqual(result.returncode, 1)
        self.assertIn("Refusing: `make check` has not passed", result.stdout)
        self.assertIn("Refusing: CARGO_REGISTRY_TOKEN is not set", result.stdout)
        self.assertFalse([call for call in self.cargo_calls() if call.startswith("publish")])

    def test_a_real_run_skips_present_crates_waits_out_rate_limits_and_resumes(self) -> None:
        self.stamp()
        (self.state / "limit-subspace-shell").touch()
        self.env["CARGO_REGISTRY_TOKEN"] = "fixture-token-not-real"
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("fixture-token-not-real", result.stdout + result.stderr)
        self.assertIn("subspace-core 0.1.0: already on crates.io, skipped", result.stdout)
        self.assertIn("subspace-shell: crates.io rate limit, attempt 1; retrying in 10 min",
                      result.stdout)
        uploads = [call.split()[2] for call in self.cargo_calls() if call.startswith("publish -p")]
        self.assertEqual(uploads, ["subspace-test", "subspace-shell", "subspace-shell",
                                   "subspace-framework"])
        self.assertIn("subspace-framework 0.1.0: published", result.stdout)
        rerun = self.publish()
        self.assertEqual(rerun.returncode, 0)
        self.assertIn("0 new, 0 new versions, 4 already on crates.io", rerun.stdout)

    def test_ci_publishes_from_the_release_tag_without_local_stamps(self) -> None:
        self.env.update({"CARGO_REGISTRY_TOKEN": "fixture-token-not-real",
                         "GITHUB_ACTIONS": "true",
                         "GITHUB_REF_NAME": "subspace-framework-v0.1.0"})
        result = self.publish("--ci")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("Refusing", result.stdout)
        self.assertNotIn("fixture-token-not-real", result.stdout + result.stderr)
        self.assertIn("subspace-framework 0.1.0: published", result.stdout)

    def test_ci_refuses_outside_actions_or_off_the_release_tag(self) -> None:
        self.env["CARGO_REGISTRY_TOKEN"] = "fixture"
        outside = self.publish("--ci")
        self.assertEqual(outside.returncode, 1)
        self.assertIn("Refusing: --ci runs only in GitHub Actions", outside.stdout)
        self.env.update({"GITHUB_ACTIONS": "true", "GITHUB_REF_NAME": "main"})
        branch = self.publish("--ci")
        self.assertEqual(branch.returncode, 1)
        self.assertIn("Refusing: the workflow ran on `main`, not the release tag "
                      "`subspace-framework-v0.1.0`", branch.stdout)
        self.assertFalse([call for call in self.cargo_calls() if call.startswith("publish -p")])
        dry = self.publish("--ci", "--dry-run")
        self.assertEqual(dry.returncode, 0, dry.stdout + dry.stderr)
        self.assertIn("A real run would refuse: the workflow ran on `main`", dry.stdout)
        self.assertEqual(self.publish("--ci", "--force").returncode, 2)

    def test_a_failed_upload_stops_before_its_dependents(self) -> None:
        self.stamp()
        self.env["CARGO_REGISTRY_TOKEN"] = "fixture"
        cargo = Path(self.env["PATH"].split(":")[0]) / "cargo"
        cargo.write_text(cargo.read_text().replace(
            'limited = os.path.join', 'sys.exit(1) if name == "subspace-test" else None\n    limited = os.path.join'))
        result = self.publish()
        self.assertEqual(result.returncode, 1)
        self.assertIn("subspace-test 0.1.0: failed", result.stdout)
        self.assertIn("subspace-framework 0.1.0: not reached", result.stdout)


if __name__ == "__main__":
    unittest.main()
