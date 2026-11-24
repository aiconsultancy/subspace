#!/usr/bin/env python3
"""Offline tests for scripts/agent-kit.py, against a throwaway upstream repository."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parent / "agent-kit.py"
spec = importlib.util.spec_from_file_location("agent_kit", SCRIPT)
kit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kit)
SETTINGS = {"identity_provider": "workos", "api_url": "https://api.example.com", "workos_client_id": "client_EXAMPLE"}


def git(*args, cwd):
    subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false",
                    *args], cwd=cwd, check=True, capture_output=True)


class AgentKitTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        base = Path(temporary.name)
        self.upstream, self.root, self.home = base / "upstream", base / "repo", base / "home"
        for source in kit.FILES.values():
            path = self.upstream / source
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"upstream {source}\n")
        git("init", "-q", cwd=self.upstream)
        git("config", "uploadpack.allowAnySHA1InWant", "true", cwd=self.upstream)
        git("add", ".", cwd=self.upstream)
        git("commit", "-q", "-m", "upstream", cwd=self.upstream)
        self.commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.upstream, capture_output=True,
                                     text=True, check=True).stdout.strip()
        self.write_pin(self.commit)
        (self.root / "agent-kit").mkdir(parents=True)
        (self.root / "agent-kit" / "agent.json").write_text(json.dumps(SETTINGS))
        environment = {key: value for key, value in os.environ.items() if key != "SUBSPACE_HOME"}
        environment.update(SUBSPACE_HOME=str(self.home), AGENT_KIT_UPSTREAM_URL=self.upstream.as_uri())
        for fixture in (patch.object(kit, "ROOT", self.root), patch.dict(os.environ, environment, clear=True)):
            fixture.start()
            self.addCleanup(fixture.stop)

    def write_pin(self, commit):
        pin = self.root / kit.PIN_FILE
        pin.parent.mkdir(parents=True, exist_ok=True)
        value = {"repository": "example/subspace", "branch": "main", "commit": commit}
        pin.write_text(json.dumps({kit.PIN_KEY: value} if kit.PIN_KEY else value))

    def run_kit(self, *args):
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(sys, "argv", ["agent-kit.py", *args]), contextlib.redirect_stdout(output), \
                contextlib.redirect_stderr(errors):
            try:
                kit.main()
                code = 0
            except SystemExit as exit:
                code = exit.code
        return code, output.getvalue(), errors.getvalue()

    def test_sync_copies_the_pinned_files_and_check_catches_a_hand_edit(self):
        code, _, _ = self.run_kit("check", "--source", str(self.upstream))
        self.assertIn("does not match the pin", str(code))
        self.assertEqual(self.run_kit("sync", "--source", str(self.upstream))[0], 0)
        for destination, source in kit.FILES.items():
            self.assertEqual((self.root / destination).read_text(), f"upstream {source}\n")
        helper = self.root / "agent-kit/bin/subspace_irc.py"
        if os.name != "nt":
            self.assertTrue(helper.stat().st_mode & stat.S_IXUSR)
        self.assertEqual(self.run_kit("check")[0], 0)  # a fresh fetch of the pin, not the given clone
        (self.root / ".claude/skills/subspace-irc/SKILL.md").write_text("edited here\n")
        code, _, errors = self.run_kit("check", "--source", str(self.upstream))
        self.assertIn("does not match the pin", str(code))
        self.assertIn(".claude/skills/subspace-irc/SKILL.md differs", errors)

    def test_the_pin_must_name_a_full_commit_that_the_source_has(self):
        self.write_pin(self.commit[:12])
        self.assertIn("full 40-character", str(self.run_kit("sync", "--source", str(self.upstream))[0]))
        self.write_pin("0" * 40)
        self.assertIn("does not have the pinned commit", str(self.run_kit("sync", "--source", str(self.upstream))[0]))

    def test_setup_installs_the_helpers_settings_and_name_and_keeps_another_apis_settings(self):
        self.run_kit("sync", "--source", str(self.upstream))
        self.home.mkdir()
        (self.home / "agents.env").write_text("SUBSPACE_TARGET=example-fleet\n")
        code, output, _ = self.run_kit("setup", "--name", "George", "--dry-run")
        self.assertEqual(code, 0)
        self.assertIn("would write", output)
        self.assertFalse((self.home / "agent.json").exists())
        self.assertEqual(self.run_kit("setup", "--name", "George")[0], 0)
        self.assertEqual(json.loads((self.home / "agent.json").read_text()), SETTINGS)
        self.assertEqual((self.home / "bin/subspace_token.py").read_text(), "upstream agent-skills/bin/subspace_token.py\n")
        self.assertEqual((self.home / "agents.env").read_text(), "SUBSPACE_TARGET=example-fleet\nSUBSPACE_HUMAN_NAME=George\n")
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE((self.home / "agent.json").stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE((self.home / "bin/subspace_irc.py").stat().st_mode), 0o755)
        self.assertIn("already set up", self.run_kit("setup")[1])
        (self.home / "agent.json").write_text(json.dumps({**SETTINGS, "api_url": "https://other.example.com"}))
        self.assertIn("set up for another API", str(self.run_kit("setup")[0]))
        self.assertEqual(self.run_kit("setup", "--force")[0], 0)
        self.assertEqual(json.loads((self.home / "agent.json").read_text()), SETTINGS)
        self.assertEqual(len(list(self.home.glob("agent.json.bak-*"))), 1)
        self.assertIn("pass --name", str(self.run_kit("setup", "--name", " ")[0]))


if __name__ == "__main__":
    unittest.main()
