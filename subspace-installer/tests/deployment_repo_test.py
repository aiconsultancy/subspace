"""Offline tests for deployment-repo/scaffold.py and the scripts it puts in a new deployment repository."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


scaffold = load("scaffold", ROOT / "deployment-repo/scaffold.py")
KIT = {
    "agent-skills/subspace-irc/SKILL.md": "irc skill\n",
    "agent-skills/claude/subspace-tracker/SKILL.md": "claude tracker skill\n",
    "agent-skills/codex/subspace-tracker/SKILL.md": "codex tracker skill\n",
    "agent-skills/bin/subspace_token.py": "# token helper\n",
    "agent-skills/bin/subspace_irc.py": "# irc helper\n",
}


def git(*args, cwd):
    return subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.com",
                           "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", *args],
                          cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def repository(path, files, tags):
    path.mkdir(parents=True)
    git("init", "-q", cwd=path)
    git("config", "uploadpack.allowAnySHA1InWant", "true", cwd=path)
    commits = {}
    for tag in tags:
        for name, text in files.items():
            (path / name).parent.mkdir(parents=True, exist_ok=True)
            (path / name).write_text(f"{text}{tag}\n")
        git("add", ".", cwd=path)
        git("commit", "-q", "-m", tag, cwd=path)
        git("tag", "-a", "-m", tag, tag, cwd=path)
        commits[tag] = git("rev-parse", "HEAD", cwd=path)
    return commits


class ScaffoldTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.subspace = self.base / "subspace"
        self.commits = repository(self.subspace, KIT, ["subspace-desktop-v1.2.0", "subspace-desktop-v1.10.0",
                                                       "subspace-framework-v9.0.0"])
        self.stargate = self.base / "stargate"
        self.stargate_commits = repository(self.stargate, {"README.md": "stargate "}, ["v0.9.0", "v0.10.0"])
        environment = {**os.environ, "UPDATE_PINS_URL_SUBSPACE": self.subspace.as_uri(),
                       "UPDATE_PINS_URL_STARGATE": self.stargate.as_uri(), "AGENT_KIT_UPSTREAM_URL": self.subspace.as_uri()}
        for fixture in (patch.dict(os.environ, environment, clear=True),
                        patch.object(scaffold, "UPSTREAM", self.subspace)):
            fixture.start()
            self.addCleanup(fixture.stop)

    def run_scaffold(self, *args):
        output = io.StringIO()
        with patch.object(sys, "argv", ["scaffold.py", *args]), contextlib.redirect_stdout(output):
            try:
                scaffold.main()
                code = 0
            except SystemExit as exit:
                code = exit.code
        return code, output.getvalue()

    def test_dry_run_pins_the_latest_releases_and_writes_nothing(self):
        target = self.base / "example-deployment"
        code, output = self.run_scaffold("--dir", str(target), "--name", "Example Org", "--dry-run",
                                         "--api-url", "https://api.example.com", "--workos-client-id", "client_EXAMPLE")
        self.assertEqual(code, 0, output)
        self.assertIn(f"subspace: aiconsultancy/subspace subspace-desktop-v1.10.0 {self.commits['subspace-desktop-v1.10.0']}", output)
        self.assertIn(f"stargate: aiconsultancy/stargate v0.10.0 {self.stargate_commits['v0.10.0']}", output)
        for path in ("pins.json", ".mcp.json", ".codex/config.toml", ".gitignore", "AGENTS.md",
                     ".claude/skills/subspace-irc/SKILL.md", "agent-kit/bin/subspace_irc.py", "would run git init"):
            self.assertIn(path, output)
        self.assertIn("warning: that Subspace commit has no WorkOS sign-in for local agents and no IRC sign-in", output)
        self.assertFalse(target.exists())

    def test_scaffold_writes_a_repository_whose_kit_matches_its_pin_and_whose_scripts_work(self):
        target = self.base / "example-deployment"
        code, output = self.run_scaffold("--dir", str(target), "--name", "Example Org", "--subspace",
                                         "subspace-desktop-v1.2.0", "--api-url", "https://api.example.com/",
                                         "--workos-client-id", "client_EXAMPLE")
        self.assertEqual(code, 0, output)
        self.assertTrue((target / ".git").is_dir())
        for path in target.rglob("*"):
            if path.is_file() and ".git" not in path.relative_to(target).parts:
                self.assertIsNone(re.search(r"@@[A-Z_]+@@", path.read_text()), path)
        pins = json.loads((target / "pins.json").read_text())
        self.assertEqual(pins["subspace"]["release"], "subspace-desktop-v1.2.0")
        self.assertEqual(pins["subspace"]["commit"], self.commits["subspace-desktop-v1.2.0"])
        self.assertEqual(pins["stargate"]["release"], "v0.10.0")
        self.assertEqual(json.loads((target / "agent-kit/agent.json").read_text())["api_url"], "https://api.example.com")
        mcp = json.loads((target / ".mcp.json").read_text())["mcpServers"]["subspace"]
        self.assertEqual(mcp["url"], "https://api.example.com/mcp")
        self.assertIn("--config agent-kit/agent.json --header --mcp-endpoint https://api.example.com/mcp",
                      mcp["headersHelper"])
        for path in (".codex/config.toml", "opencode.json"):
            self.assertIn('--config \\"$top/agent-kit/agent.json\\" --mcp-stdio', (target / path).read_text(), path)
        self.assertEqual((target / ".agents/skills/subspace-tracker/SKILL.md").read_text(),
                         "codex tracker skill\nsubspace-desktop-v1.2.0\n")
        self.assertTrue(os.access(target / "scripts/update-pins.py", os.X_OK))
        self.assertIn("*.pem", (target / ".gitignore").read_text())
        self.assertIn("Example Org", (target / "README.md").read_text())

        def run(*command):
            return subprocess.run([sys.executable, "-B", *command], cwd=target, capture_output=True, text=True)

        self.assertEqual(run("scripts/agent-kit.py", "check").returncode, 0)  # fetched from the pinned upstream
        result = run("scripts/test-agent-kit.py")
        self.assertEqual(result.returncode, 0, result.stderr)
        # A flag moves only the project it names.
        moved = run("scripts/update-pins.py", "--stargate", "v0.9.0")
        self.assertEqual(moved.returncode, 0, moved.stderr)
        self.assertIn("stargate: v0.10.0 -> v0.9.0", moved.stdout)
        self.assertNotIn("subspace", moved.stdout)
        self.assertEqual(json.loads((target / "pins.json").read_text())["subspace"]["release"], "subspace-desktop-v1.2.0")
        moved = run("scripts/update-pins.py")
        self.assertIn("subspace: subspace-desktop-v1.2.0 -> subspace-desktop-v1.10.0", moved.stdout)
        self.assertIn("stargate: v0.9.0 -> v0.10.0", moved.stdout)
        self.assertIn("Next: python3 scripts/agent-kit.py sync", moved.stdout)
        stale = run("scripts/agent-kit.py", "check")
        self.assertNotEqual(stale.returncode, 0)
        self.assertIn("does not match the pin", stale.stderr)
        self.assertEqual(run("scripts/agent-kit.py", "sync").returncode, 0)
        self.assertEqual(run("scripts/agent-kit.py", "check").returncode, 0)

    def test_the_mcp_registration_uses_the_repositorys_own_api_and_session(self):
        target = self.base / "example-deployment"
        self.assertEqual(self.run_scaffold("--dir", str(target), "--name", "Example Org", "--api-url",
                                           "https://api.example.com", "--workos-client-id", "client_EXAMPLE")[0], 0)
        # The real helper in place of the fixture's copy, and a computer set up for another deployment.
        shutil.copyfile(ROOT / "agent-skills/bin/subspace_token.py", target / "agent-kit/bin/subspace_token.py")
        home = self.base / "home"
        home.mkdir()
        (home / "agent.json").write_text(json.dumps({"identity_provider": "workos", "api_url": "https://other.example.com",
                                                     "workos_client_id": "client_OTHER"}))
        helper = load("subspace_token_copy", target / "agent-kit/bin/subspace_token.py")
        with patch.object(helper, "HOME", home):
            store = helper.SessionStore(helper.load_config(str(target / "agent-kit/agent.json")))
            with store.locked():
                store.write_file(b"null")  # signed out for this API, and off any real Keychain
        environment = {**os.environ, "SUBSPACE_HOME": str(home), "SUBSPACE_API_URL": "https://elsewhere.example.com"}
        claude = json.loads((target / ".mcp.json").read_text())["mcpServers"]["subspace"]["headersHelper"]
        result = subprocess.run(["sh", "-c", claude], cwd=target, env=environment, capture_output=True, text=True)
        self.assertEqual(result.stderr, "subspace: WorkOS session unavailable. Run subspace_token.py --config "
                                        "agent-kit/agent.json --login.\n")
        command = json.loads((target / "opencode.json").read_text())["mcp"]["subspace"]["command"]
        request = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}) + "\n"
        result = subprocess.run(command, cwd=target / "fleet", env=environment, input=request, capture_output=True,
                                text=True)
        message = json.loads(result.stdout)["error"]["message"]
        self.assertTrue(message.startswith("WorkOS session unavailable. Run subspace_token.py --config "), message)
        self.assertTrue(message.endswith("/agent-kit/agent.json --login."), message)

    def test_the_gitignore_keeps_the_installers_secret_files_out(self):
        target = self.base / "example-deployment"
        self.assertEqual(self.run_scaffold("--dir", str(target), "--name", "Example Org")[0], 0)
        secret = ["fleet/vm/fleet.secrets.json", "fleet/vm/fleet.secrets/atlas/github-app.pem",
                  "fleet/vm/fleet.secrets/subspace-enrolment-token", "fleet/vm/generated-secrets.json",
                  "fleet/vm/extra-secrets.json", "fleet/vm/api.env", "fleet/vm/enrolment-token", "fleet/vm/database-url",
                  "fleet/vm/database-ca.pem", "fleet/vm/irc-bundle.json", "fleet/irc/operator-password",
                  "fleet/irc/compose.env", "fleet/irc/generated-secrets.json", "fleet/irc/privkey.pem", "admin-token",
                  "fleet/api-admin-token", "fleet/local/irc.password", "fleet/local/api-token.json",
                  "fleet/local/example-person.password", "hosting/terraform.tfstate", "hosting/.env.production"]
        kept = ["fleet/vm/fleet.json", "fleet/vm/answers.json", "fleet/vm/summary.example.env", "fleet/install.md"]
        scaffolded = {path.relative_to(target).as_posix() for path in target.rglob("*")
                      if path.is_file() and ".git" not in path.relative_to(target).parts}
        for path in secret + kept:
            (target / path).parent.mkdir(parents=True, exist_ok=True)
            (target / path).write_text("fixture\n")
        staged = {line[len("add '"):-1] for line in git("add", "--dry-run", "-A", cwd=target).splitlines()}
        self.assertEqual(sorted(set(secret) & staged), [])
        self.assertEqual(sorted((scaffolded | set(kept)) - staged), [])

    def test_a_failed_fetch_leaves_nothing_behind_and_a_rerun_works(self):
        target = self.base / "example-deployment"
        target.mkdir()  # an empty folder is fine
        missing = "0123456789abcdef0123456789abcdef01234567"
        with patch.dict(os.environ, AGENT_KIT_UPSTREAM_URL=(self.base / "nowhere").as_uri()):
            code, output = self.run_scaffold("--dir", str(target), "--name", "Example Org", "--subspace", missing)
        self.assertIn("nothing was created", str(code))
        self.assertIn("run the scaffold again", str(code))
        self.assertEqual(list(target.iterdir()), [])
        self.assertEqual(sorted(path.name for path in self.base.iterdir()), ["example-deployment", "stargate", "subspace"])
        code, output = self.run_scaffold("--dir", str(target), "--name", "Example Org")
        self.assertEqual(code, 0, output)
        self.assertTrue((target / ".git").is_dir())
        self.assertTrue((target / "agent-kit/bin/subspace_irc.py").is_file())
        self.assertEqual(sorted(path.name for path in self.base.iterdir()), ["example-deployment", "stargate", "subspace"])
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE(target.stat().st_mode) & 0o755, 0o755)

    def test_without_a_hosted_api_there_is_no_mcp_registration(self):
        target = self.base / "local-deployment"
        code, output = self.run_scaffold("--dir", str(target), "--name", "Example Org", "--no-git",
                                         "--subspace", self.commits["subspace-desktop-v1.2.0"], "--stargate", "v0.9.0")
        self.assertEqual(code, 0, output)
        self.assertEqual(json.loads((target / "pins.json").read_text())["subspace"]["release"], "")
        for path in (".mcp.json", ".codex/config.toml", "opencode.json", "agent-kit/agent.json", ".git"):
            self.assertFalse((target / path).exists(), path)
        self.assertTrue((target / ".claude/skills/subspace-irc/SKILL.md").is_file())

    def test_refusals(self):
        occupied = self.base / "occupied"
        occupied.mkdir()
        (occupied / "notes.md").write_text("mine\n")
        for args, message in (
            (["--dir", str(occupied), "--name", "Example Org"], "not an empty folder"),
            (["--dir", str(self.base / "new"), "--name", "Example Org", "--api-url", "https://api.example.com"],
             "together"),
            (["--dir", str(self.base / "new"), "--name", "Example Org", "--api-url", "http://api.example.com",
              "--workos-client-id", "client_EXAMPLE"], "HTTPS origin"),
            (["--dir", str(self.base / "new"), "--name", "@@NAME@@"], "one line"),
            (["--dir", str(self.base / "new"), "--name", "Example Org", "--stargate", "v9.9.9"], "has no tag v9.9.9"),
        ):
            with self.subTest(message):
                code, _ = self.run_scaffold(*args)
                self.assertIn(message, str(code))
        self.assertFalse((self.base / "new").exists())
        self.assertEqual([path.name for path in occupied.iterdir()], ["notes.md"])


if __name__ == "__main__":
    unittest.main()
