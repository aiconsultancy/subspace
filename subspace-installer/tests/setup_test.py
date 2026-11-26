"""Offline checks that subspace-setup's generated inputs satisfy the installer.

Run with Python 3.10 or later: python3 -B subspace-installer/tests/setup_test.py
"""

import argparse
import base64
import contextlib
import hashlib
import io
import json
import os
import re
import shutil
import socket
import ssl
import subprocess
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "vm"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import fixture  # noqa: E402
import fleet_records  # noqa: E402
import fleet_setup  # noqa: E402
import irc_accounts  # noqa: E402
import roles  # noqa: E402
import runtime  # noqa: E402
from runtime import irc_officer, validate  # noqa: E402
import sessions  # noqa: E402
from standard_channels import COMMAND_SUFFIX  # noqa: E402


# Tests never reach the network: setup's outbound check is replaced here, and OutboundTests checks the real one.
check_outbound = fleet_setup.check_outbound
fleet_setup.check_outbound = lambda answers: None
# Nor do they read a fleet's tracker as its rear admiral: TrackerOwnershipTests checks the real reader.
agent_reader = fleet_setup.agent_reader
fleet_setup.agent_reader = lambda answers, members: (None, "the tests reach no Subspace API")
# Nor do they look up Stargate's newest release: a run without --stargate-ref installs this one here, and
# OutboundTests checks the real lookup.
latest_stargate = fleet_setup.latest_stargate
fleet_setup.latest_stargate = lambda: ("v0.0.0", "d" * 40)
# Guided setup preflights the explicit source before saving answers. Use the real check in its dedicated test.
check_usage_guard_source = fleet_setup.check_usage_guard_source
fleet_setup.check_usage_guard_source = lambda *_args: None
check_officer_tool_source = fleet_setup.check_officer_tool_source
fleet_setup.check_officer_tool_source = lambda *_args: None


def fleet_installer(root, output=None):
    """subspace-setup's 'installer' as the fleet installer's checks, in this process, on a VM with nothing installed
    but what the test puts under root (etc, state, sessions): install.sh --dry-run runs 'runtime.py plan', and a
    real run's 'prepare' makes the same checks before it installs anything, which here it never does. Git's reading
    of public repositories and the fixture Stargate's feature checks are left out. Each run's (dry_run, quiet, plan
    text) goes to output."""
    def installer(config_path, dry_run=False, secrets_dir=None, quiet=False):
        if secrets_dir is None and (beside := Path(config_path).with_name("fleet.secrets")).is_dir():
            secrets_dir = beside  # As install.sh takes the secrets directory beside the configuration.
        text = io.StringIO()
        with patch.object(runtime, "ETC", root / "etc"), patch.object(runtime, "STATE", root / "state"), \
                patch.object(runtime, "SESSIONS", root / "sessions"), patch.object(runtime, "IRC_CERTS", root / "certs"), \
                patch.object(runtime, "readable_without_credentials", return_value=True), \
                patch.object(runtime, "require_usage_guard_source"), \
                patch.object(runtime, "require_officer_tool_sources"), \
                patch.object(runtime, "stargate_crew_names_free", return_value=True), contextlib.redirect_stdout(text):
            try:
                runtime.main(["plan", str(config_path), str(Path(config_path).with_name("fleet.secrets.json")),
                              str(secrets_dir or ""), "false"])
            except runtime.Invalid as error:
                raise fleet_setup.SetupError(f"subspace-fleet: {error}") from None
        if output is not None:
            output.append((dry_run, quiet, text.getvalue()))
    return patch.object(fleet_setup, "installer", side_effect=installer)


def fake_api(api):
    """Send subspace-setup's API calls to the fixture's in-memory API."""
    return patch.object(fleet_setup, "api_call", side_effect=lambda _token, method, path, body=None: api.call(method, path, body))


def answers_for(argv):
    args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", *argv])
    with patch.object(fleet_setup.pwd, "getpwnam"):
        answers = fleet_setup.gather(args, {})
        rear, captains = fleet_setup.check_answers(answers)
    return answers, rear, captains


# A new fleet names every crew member: here the example captain's four, Claude-family first.
CREW = ["--crew-names", "Orion=Rigel,Deneb,Mira,Sirius"]


class UsageGuardAnswersTests(unittest.TestCase):
    def test_an_incompatible_pin_without_a_local_api_is_refused_before_saving(self):
        with tempfile.TemporaryDirectory() as root:
            args = fleet_setup.parse_args(["--non-interactive", "--inputs", root, "--operator", "operator",
                                           "--name", "Example Fleet", "--rear-admiral", "Atlas",
                                           "--captains", "Orion", "--no-api", "--stargate-ref", "a" * 40])
            with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                    patch.object(fleet_setup, "check_usage_guard_source", check_usage_guard_source), \
                    patch.object(runtime, "require_usage_guard_source",
                                 side_effect=runtime.Invalid("Usage guard requires a newer Stargate")) as check, \
                    patch.object(fleet_setup, "run") as run, \
                    self.assertRaisesRegex(fleet_setup.SetupError, "requires a newer Stargate"):
                fleet_setup.setup(args)
            self.assertEqual(check.call_args.args[0]["stargate"]["ref"], "a" * 40)
            run.assert_not_called()
            self.assertEqual(list(Path(root).iterdir()), [])

    def test_stale_officer_pin_is_refused_before_local_registration(self):
        with tempfile.TemporaryDirectory() as root:
            args = fleet_setup.parse_args(["--non-interactive", "--inputs", root, "--operator", "operator",
                                           "--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                                           *CREW, "--stargate-ref", "a" * 40])
            with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                    patch.object(fleet_setup, "check_officer_tool_source", check_officer_tool_source), \
                    patch.object(runtime.subprocess, "run", return_value=subprocess.CompletedProcess(
                        [], 0, stdout="", stderr="")) as git, \
                    patch.object(fleet_setup, "api_up") as api, patch.object(fleet_setup, "register_own") as register, \
                    self.assertRaisesRegex(fleet_setup.SetupError, "lacks scripts/officer-files.ts"):
                fleet_setup.setup(args)
            fetch = next(call.args[0] for call in git.call_args_list if "fetch" in call.args[0])
            self.assertEqual(fetch[-2:], [fleet_setup.STARGATE_REPO, "a" * 40])
            api.assert_not_called()
            register.assert_not_called()
            self.assertEqual(list(Path(root).iterdir()), [])

    def test_default_custom_and_off_options(self):
        with tempfile.TemporaryDirectory() as root, patch.object(fleet_setup, "INPUTS", Path(root)):
            defaults = fleet_setup.usage_guard_answers(fleet_setup.parse_args([]), {})
            self.assertEqual(defaults, {"enabled": True, "claude": {"warning": 20, "cutoff": 10},
                                        "codex": {"warning": 20, "cutoff": 10}})
            args = fleet_setup.parse_args(["--usage-guard-warning", "35", "--usage-guard-cutoff", "codex=15",
                                           "--no-usage-guard"])
            off = fleet_setup.usage_guard_answers(args, {})
            self.assertEqual(off, {"enabled": False, "claude": {"warning": 35, "cutoff": 10},
                                   "codex": {"warning": 35, "cutoff": 15}})
            (Path(root) / "fleet.json").write_text(json.dumps({"usageGuard": off}))
            on = fleet_setup.usage_guard_answers(fleet_setup.parse_args(["--usage-guard"]), {})
            self.assertEqual(on, {**off, "enabled": True})
            self.assertIn("Codex 35% warning / 15% cutoff", runtime.usage_guard_line(on, {"account": "atlas"}))

    def test_cli_overrides_can_repair_a_saved_partial_policy(self):
        with tempfile.TemporaryDirectory() as root, patch.object(fleet_setup, "INPUTS", Path(root)):
            (Path(root) / "fleet.json").write_text(json.dumps({"usageGuard": {"claude": {"cutoff": 22}}}))
            with self.assertRaisesRegex(fleet_setup.SetupError, "must be lower"):
                fleet_setup.usage_guard_answers(fleet_setup.parse_args([]), {})
            args = fleet_setup.parse_args(["--usage-guard-warning", "claude=25"])
            limits = fleet_setup.usage_guard_answers(args, {})
            self.assertEqual(limits["claude"], {"warning": 25, "cutoff": 22})
            self.assertEqual(limits["codex"], {"warning": 20, "cutoff": 10})

    def test_bad_provider_range_and_order_are_rejected(self):
        with tempfile.TemporaryDirectory() as root, patch.object(fleet_setup, "INPUTS", Path(root)):
            for argument, value, message in (
                ("--usage-guard-warning", "other=30", "expects"),
                ("--usage-guard-warning", "100", "expects"),
                ("--usage-guard-cutoff", "claude=20", "must be lower"),
            ):
                args = fleet_setup.parse_args([argument, value])
                with self.subTest(value=value), self.assertRaisesRegex(fleet_setup.SetupError, message):
                    fleet_setup.usage_guard_answers(args, {})


class SetupInputsTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        fleet_setup.INPUTS = Path(self.directory.name)

    def tearDown(self):
        self.directory.cleanup()

    def generate(self, argv, api_block=None, stargate_api=False):
        answers, rear, captains = answers_for(argv)
        store = fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None)
        members = fleet_setup.roster(answers, rear, captains)
        local = answers["irc"] == "local"
        config = fleet_setup.fleet_config(answers, members, "a" * 40, api_block, stargate_api, "b" * 40)
        token = "header.payload.signature" if api_block else ""
        secrets = fleet_setup.fleet_secrets(store, members, answers["admin"], token, local,
                                            certificates=answers.get("ircAuth") == "certificate")
        return answers, members, config, secrets, store

    def test_local_fleet_validates_with_api_and_demo_channels(self):
        api = {"url": "http://127.0.0.1:3000", "initiatives": ["00000000-0000-4000-8000-000000000001"]}
        answers, members, config, secrets, _ = self.generate(
            ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion, Vega",
             "--admin", "admiral", "--demo"], api_block=api)
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(answers["slug"], "example-fleet")
        # A new fleet's officers have plain names: each one's ID is its IRC account, API name and Stargate ID.
        self.assertEqual(config["officerNames"], "plain")
        self.assertEqual([m["account"] for m in members], ["atlas", "orion", "vega"])
        # Crew have no IRC accounts; without repositories they have no worktrees either.
        self.assertEqual(set(secrets["ircPasswords"]), {"admiral"})
        self.assertEqual(config["crew"], {"claude": {"count": 2}, "codex": {"count": 2}})
        self.assertEqual(config["repositories"], [])
        # Setup names no channels: the installer makes the standard set, with a separate command channel.
        self.assertNotIn("starbaseChannel", config["irc"])
        self.assertNotIn("commandChannel", config["irc"])
        command = f"#example-fleet-{COMMAND_SUFFIX}"
        self.assertEqual(cfg["irc"]["starbaseChannel"], "#example-fleet-starbase")
        self.assertEqual(cfg["irc"]["commandChannel"], command)
        # Starbase and fleet command keep the fleet's name; a ship channel is named after its captain.
        standard = ["#example-fleet-starbase", command, "#orion", "#vega"]
        self.assertEqual(cfg["irc"]["channels"], standard)
        self.assertEqual(fleet_setup.fleet_channels(answers, members), standard)
        rear = irc_officer(cfg, cfg["members"][0], {"account": "atlas", "backend": {"directory": "/w"}})
        self.assertEqual(rear["channels"], standard)
        self.assertEqual(rear["access"]["humanWakeChannels"], [{"channel": "#example-fleet-starbase", "replies": "all"}])
        self.assertIn("#aurora-lyra", config["irc"]["compose"]["channels"])
        self.assertIn("admiral", secrets["ircPasswords"])
        self.assertEqual(cfg["operators"], ["operator"])
        self.assertEqual(len(set(secrets["ircPasswords"].values())), len(secrets["ircPasswords"]))

    def test_shared_irc_fleet_validates_without_human_passwords(self):
        answers, _, config, secrets, store = self.generate(
            ["--name", "Fleet One", "--rear-admiral", "Atlas", "--captains", "Orion", "--admin", "admiral",
             "--no-demo", "--irc", "remote", "--irc-host", "irc.example.ts.net", "--irc-auth", "password",
             "--crew-claude", "1", "--crew-codex", "0"])
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(cfg["irc"]["mode"], "remote")
        self.assertEqual((cfg["irc"]["host"], cfg["irc"]["port"], cfg["irc"]["tls"]), ("irc.example.ts.net", 6697, True))
        # The shared server seeds officers' accounts with passwords, so setup keeps password sign-in explicitly.
        self.assertEqual(cfg["irc"]["auth"], "password")
        self.assertNotIn("admiral", secrets["ircPasswords"])
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        bundle = fleet_setup.irc_bundle(answers, members, store)
        self.assertEqual({row["name"] for row in bundle["accounts"]}, {"atlas", "orion"})
        self.assertEqual({row["name"] for row in bundle["accounts"]}, set(secrets["ircPasswords"]))
        self.assertEqual([row["name"] for row in bundle["channels"]],
                         ["#fleet-one-starbase", f"#fleet-one-{COMMAND_SUFFIX}", "#orion"])
        self.assertEqual(cfg["irc"]["channels"], [row["name"] for row in bundle["channels"]])
        self.assertIn("dispatch", bundle["channels"][1]["topic"])
        self.assertEqual(bundle["humans"], ["admiral"])

    def test_the_sizing_counts_the_api_setup_runs_on_this_vm_whatever_its_sign_in(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        local = runtime.memory.reserve(api_here=True, irc_here=True)
        # With development sign-in setup writes the API's block, and without it (WorkOS or Entra) none: either way
        # the API runs here, and the engines leave it its room.
        for block in ({"url": "http://127.0.0.1:3000", "initiatives": []}, None):
            with self.subTest(block=block):
                _, _, config, secrets, _ = self.generate(base, api_block=block)
                self.assertEqual(config["memory"], {"localApi": True})
                cfg = validate(config, secrets, check_files=False)
                self.assertEqual(runtime.memory_plan(cfg, 16036)["reserve"], local)
        # A hosted API, or none: nothing runs here for it.
        for flags in (["--api-url", "https://subspace.example.com"], ["--no-api"]):
            with self.subTest(flags=flags):
                _, _, config, secrets, _ = self.generate([*base, *flags])
                self.assertNotIn("memory", config)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {"memory": {"localApi": True, "swap": "4G"}})
        _, _, config, _, _ = self.generate([*base, "--no-api"])
        self.assertEqual(config["memory"], {"swap": "4G"})

    def test_memory_options_reach_fleet_json_and_are_kept(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion,Vega", "--no-demo"]
        answers, members, config, secrets, _ = self.generate(
            [*base, "--captain-memory", "24G", "--rear-admiral-memory", "none", "--swap", "8G"])
        self.assertEqual(config["memory"], {"captain": {"max": "24G"}, "rearAdmiral": {"max": "none"}, "swap": "8G",
                                            "localApi": True})
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(cfg["memory"]["captain"], {"max": 24 * 1024})
        self.assertEqual(cfg["memory"]["rear_admiral"], {"max": "none"})
        plan = runtime.memory_plan(cfg, 64154)["limits"]
        self.assertEqual((plan["orion"]["max"], plan["orion"]["high"], plan["atlas"]["max"]), (24 * 1024, 21 * 1024,
                                                                                            "none"))
        # Later runs keep the answers; what the operator added to fleet.json by hand stays beside them.
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {
            **config, "memory": {**config["memory"], "tasksMax": 2048, "captain": {"max": "16G", "high": "15G"}}})
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            kept = fleet_setup.gather(args, answers)
        config = fleet_setup.fleet_config(kept, members, "a" * 40)
        self.assertEqual(config["memory"], {"captain": {"max": "24G", "high": "15G"}, "rearAdmiral": {"max": "none"},
                                            "swap": "8G", "tasksMax": 2048, "localApi": True})
        # auto returns to the installer's own sizing, changing only what the option sets: a role's hard cap is
        # written as auto, and swap's default needs no entry.
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--captain-memory", "auto",
                                       "--rear-admiral-memory", "", "--swap", "auto"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            reset = fleet_setup.gather(args, kept)
        self.assertEqual(fleet_setup.fleet_config(reset, members, "a" * 40)["memory"],
                         {"captain": {"max": "auto", "high": "15G"}, "rearAdmiral": {"max": "auto"}, "tasksMax": 2048,
                          "localApi": True})
        # A fleet-wide cap set by hand reaches neither role, whose auto is the sizing for this VM, while an
        # officer's own cap still wins.
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {
            **config, "memory": {"max": "16G", "captain": {"max": "12G"}},
            "captains": [{**config["captains"][0], "memory": {"max": "28G"}}, *config["captains"][1:]]})
        auto = fleet_setup.fleet_config(reset, members, "a" * 40)
        self.assertEqual(auto["memory"], {"max": "16G", "captain": {"max": "auto"}, "rearAdmiral": {"max": "auto"},
                                          "localApi": True})
        result = runtime.memory_plan(validate(auto, secrets, check_files=False), 64154)
        plan = result["limits"]
        self.assertEqual((plan["orion"]["max"], plan["vega"]["max"], plan["atlas"]["max"]),
                         (28 * 1024, result["computed"]["captain"], result["computed"]["rear_admiral"]))
        self.assertEqual((plan["vega"]["max"], plan["atlas"]["max"]), (31 * 1024, 4 * 1024))
        # Bad values stop setup before it saves anything.
        for flag, value, message in (("--captain-memory", "lots", "--captain-memory must be a size such as 20G"),
                                     ("--rear-admiral-memory", "100M", "--rear-admiral-memory must be at least 512M"),
                                     ("--swap", "big", "--swap must be a size")):
            with self.subTest(flag), self.assertRaisesRegex(fleet_setup.SetupError, message):
                answers_for([*base, flag, value])

    def test_stargate_api_and_secrets_are_stable(self):
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        _, _, config, secrets, _ = self.generate(argv, stargate_api=True)
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(cfg["stargate"]["api"]["port"], 8484)
        self.assertEqual(cfg["stargate"]["ref"], "b" * 40)
        _, _, _, again, _ = self.generate(argv)
        self.assertEqual(secrets["ircPasswords"], again["ircPasswords"])
        self.assertEqual(secrets["proxyKey"], again["proxyKey"])

    def test_single_host_keeps_irc_history_in_postgresql(self):
        answers, rear, captains = answers_for(["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"])
        store = fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None)
        members = fleet_setup.roster(answers, rear, captains)
        history = {"database": "postgresql", "host": "postgres", "port": 5432, "user": "ergo", "name": "ergo_history",
                   "network": "subspace-api_default"}
        config = fleet_setup.fleet_config(answers, members, "a" * 40, None, False, "b" * 40, history)
        secrets = fleet_setup.fleet_secrets(store, members, answers["admin"], certificates=True)
        with self.assertRaises(Exception):
            validate(config, secrets, check_files=False)
        secrets["ircHistoryPassword"] = store.password("ergo-history")
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(cfg["irc"]["compose"]["history"]["network"], "subspace-api_default")
        self.assertEqual(cfg["historyPassword"], secrets["ircHistoryPassword"])
        config["irc"]["compose"].pop("history")
        with self.assertRaises(Exception):
            validate(config, secrets, check_files=False)
        secrets.pop("ircHistoryPassword")
        self.assertIsNone(validate(config, secrets, check_files=False)["historyPassword"])

    def test_crew_models_are_set_by_option_and_kept_on_a_rerun(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo",
                "--repositories", "https://github.com/example/project.git", *CREW]
        answers, members, config, secrets, _ = self.generate(
            [*base, "--crew-claude-model", "claude-opus-5-5", "--crew-codex-model", "gpt-6.2-sol"])
        self.assertEqual(config["crew"], {"claude": {"count": 2, "model": "claude-opus-5-5"},
                                          "codex": {"count": 2, "model": "gpt-6.2-sol"}})
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(sorted({worker["model"] for worker in cfg["members"][1]["crew"]}),
                         ["claude-opus-5-5", "gpt-6.2-sol"])
        # A rerun without the options keeps them, as saved answers.
        fleet_setup.save_answers(answers)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            again = fleet_setup.gather(args, fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))
        self.assertEqual(fleet_setup.fleet_config(again, members, "a" * 40)["crew"], config["crew"])
        # A model set in fleet.json by hand, with no answer for it, is kept too; '' returns to the default.
        answers, rear, captains = answers_for([*base, "--crew-codex-model", ""])
        rebuilt = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(rebuilt["crew"], {"claude": {"count": 2, "model": "claude-opus-5-5"}, "codex": {"count": 2}})
        with self.assertRaisesRegex(fleet_setup.SetupError, "--crew-codex-model must be a model ID"):
            answers_for([*base, "--crew-codex-model", "gpt 6"])

    def officers(self, config, secrets):
        cfg = validate(config, secrets, check_files=False)
        return [(person["harness"]["family"], person["harness"]["model"], person["harness"]["effort"])
                for person in cfg["members"]]

    def rerun(self, answers, config, *argv):
        fleet_setup.save_answers(answers)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", *argv])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            return fleet_setup.gather(args, fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))

    def test_a_new_fleet_runs_its_officers_on_the_default_models(self):
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion,Vega", "--no-demo"]
        answers, _, config, secrets, _ = self.generate(argv)
        self.assertNotIn("harness", config)
        self.assertEqual(answers["officerModels"], {"captain": {}, "rearAdmiral": {}})
        self.assertEqual(self.officers(config, secrets), [("claude", "claude-fable-5-2", "high"),
                                                          ("claude", "claude-opus-5-5", "xhigh"),
                                                          ("claude", "claude-opus-5-5", "xhigh")])

    def test_captain_delegate_policy_grants_pull_requests_and_leaves_coding_to_crew(self):
        args = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                "--no-demo", "--repositories", "https://github.com/example/project.git", "--captain-tools", "delegate",
                *CREW]
        answers, members, config, secrets, _ = self.generate(args)
        cfg = validate(config, secrets, check_files=False)
        rear, captain = cfg["members"]
        self.assertFalse(rear.get("delegatesCoding"))
        self.assertTrue(captain["delegatesCoding"])
        rules = captain["harness"]["permissions"]
        for command in ("subspace-github gh pr create --fill", "git diff main", "git fetch origin", "make test", "pwd"):
            self.assertEqual(fixture.opencode_decision(rules, "shell", [command]), "allow", command)
        for command in ("git commit -m wip", "git push origin HEAD"):
            self.assertEqual(fixture.opencode_decision(rules, "shell", [command]), "deny", command)
        self.assertEqual(fixture.opencode_decision(rules, "edit", ["src/main.ts"]), "deny")
        again = self.rerun(answers, config)
        self.assertEqual(fleet_setup.fleet_config(again, members, "a" * 40)["captainTools"], "delegate")

    def test_a_new_fleet_delegates_and_an_existing_one_keeps_its_captain_tools(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        repositories = ["--repositories", "https://github.com/example/project.git", *CREW]
        answers, members, config, secrets, _ = self.generate([*base, *repositories])
        self.assertEqual((answers["captainTools"], config["captainTools"]), ("delegate", "delegate"))
        self.assertTrue(validate(config, secrets, check_files=False)["members"][1]["delegatesCoding"])
        # Without repositories the captains have no crew, so they keep the standard policy until crew come.
        _, _, bare, bare_secrets, _ = self.generate(base)
        self.assertEqual(bare["captainTools"], "delegate")
        captain = validate(bare, bare_secrets, check_files=False)["members"][1]
        self.assertFalse(captain.get("delegatesCoding"))
        self.assertEqual(captain["harness"]["permissions"], roles.default_permissions("captain", None))
        # A rerun keeps the answer, and --captain-tools changes it.
        self.assertEqual(fleet_setup.fleet_config(self.rerun(answers, config), members, "a" * 40)["captainTools"],
                         "delegate")
        again = self.rerun(answers, config, "--captain-tools", "standard")
        self.assertEqual(fleet_setup.fleet_config(again, members, "a" * 40)["captainTools"], "standard")
        # A fleet set up before captainTools existed has neither the answer nor the key, and gets neither: a
        # fleet.json without captainTools means standard, as its captains were installed to code.
        legacy = {key: value for key, value in answers.items() if key != "captainTools"}
        config.pop("captainTools")
        again = self.rerun(legacy, config)
        self.assertNotIn("captainTools", again)
        rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertNotIn("captainTools", rebuilt)
        self.assertEqual(validate(rebuilt, secrets, check_files=False)["captainTools"], "standard")
        # So does a fleet the fleet installer set up alone, which setup has no answers or inputs for.
        (fleet_setup.INPUTS / "answers.json").unlink()
        (fleet_setup.INPUTS / "fleet.json").unlink()
        etc = Path(self.directory.name) / "etc"
        etc.mkdir()
        (etc / "fleet.json").write_text("{}")
        with patch.object(fleet_setup, "FLEET_ETC", etc):
            self.assertNotIn("captainTools", answers_for([*base, *repositories])[0])

    def test_peer_message_selection_survives_setup_and_can_be_disabled(self):
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion,Vega", "--no-demo",
                "--repositories", "https://github.com/example/project.git", "--captain-peer-messages",
                "--crew-names", "Orion=Rigel,Deneb,Mira,Sirius;Vega=Arden,Brook,Cedar,Dale"]
        answers, members, config, _, _ = self.generate(argv)
        self.assertTrue(config["captainPeerMessages"])
        kept = self.rerun(answers, config)
        self.assertTrue(fleet_setup.fleet_config(kept, members, "a" * 40)["captainPeerMessages"])
        off = self.rerun(kept, config, "--no-captain-peer-messages")
        self.assertNotIn("captainPeerMessages", fleet_setup.fleet_config(off, members, "a" * 40))

    def test_captain_merge_selection_survives_setup_and_can_be_cleared(self):
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo",
                "--repositories", "https://github.com/example/project.git",
                "--captain-merge-repositories", "example/project", *CREW]
        answers, members, config, _, _ = self.generate(argv)
        self.assertEqual(config["githubPolicy"]["captainMergeRepositories"], ["example/project"])
        config["githubPolicy"]["protectedBranches"] = ["main"]
        kept = self.rerun(answers, config)
        self.assertNotIn("captainMergeRepositories", kept)
        self.assertEqual(fleet_setup.fleet_config(kept, members, "a" * 40)["githubPolicy"], config["githubPolicy"])
        cleared = self.rerun(kept, config, "--captain-merge-repositories", "none")
        self.assertEqual(fleet_setup.fleet_config(cleared, members, "a" * 40)["githubPolicy"],
                         {"protectedBranches": ["main"], "captainMergeRepositories": []})

    def test_extra_permissions_written_by_hand_are_kept_on_a_rerun(self):
        args = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo",
                "--repositories", "https://github.com/example/project.git", *CREW]
        answers, members, config, _, _ = self.generate(args)
        rule = {"action": "shell", "resource": "kubectl get *", "effect": "allow"}
        config["harness"] = {"extraPermissions": [rule], "captain": {"extraPermissions": [rule]}}
        config["captains"][0]["harness"] = {"extraPermissions": [rule]}
        # A rerun that also changes the captains' model keeps every level's rules.
        again = self.rerun(answers, config, "--captain-model", "gpt-6.2-sol")
        rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(rebuilt["harness"]["extraPermissions"], [rule])
        self.assertEqual(rebuilt["harness"]["captain"], {"extraPermissions": [rule], "model": "gpt-6.2-sol",
                                                         "family": "codex"})
        self.assertEqual(rebuilt["captains"][0]["harness"], {"extraPermissions": [rule]})

    def test_officer_models_are_set_by_option_and_kept_on_a_rerun(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        answers, members, config, secrets, _ = self.generate(
            [*base, "--captain-model", "gpt-6.2-sol", "--captain-effort", "high", "--rear-admiral-effort", "max"])
        self.assertEqual(config["harness"], {"captain": {"model": "gpt-6.2-sol", "family": "codex", "effort": "high"},
                                             "rearAdmiral": {"effort": "max"}})
        self.assertEqual(self.officers(config, secrets), [("claude", "claude-fable-5-2", "max"),
                                                          ("codex", "gpt-6.2-sol", "high")])
        # A rerun without the options keeps them, with what else fleet.json's harness holds.
        config["harness"]["type"] = "opencode"
        again = self.rerun(answers, config)
        rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(rebuilt["harness"], config["harness"])
        # A new model starts at its provider's default effort unless the run names one.
        again = self.rerun(again, rebuilt, "--captain-model", "claude-sonnet-5-5")
        rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(rebuilt["harness"]["captain"], {"model": "claude-sonnet-5-5", "family": "claude"})
        self.assertEqual(self.officers(rebuilt, secrets)[1], ("claude", "claude-sonnet-5-5", None))
        # '' returns to the default, even over a model set in fleet.json by hand.
        rebuilt["harness"]["rearAdmiral"]["model"] = "claude-opus-5-5"
        again = self.rerun(again, rebuilt, "--captain-model", "", "--rear-admiral-model", "")
        rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(rebuilt["harness"], {"type": "opencode"})
        self.assertEqual(self.officers(rebuilt, secrets), [("claude", "claude-fable-5-2", "high"),
                                                           ("claude", "claude-opus-5-5", "xhigh")])
        for option, value, message in (("--captain-effort", "extreme", "--captain-effort must be one of low, medium"),
                                       ("--rear-admiral-model", "claude-opus-5-5#xhigh", "--rear-admiral-model must be "
                                        "a model ID")):
            with self.subTest(option), self.assertRaisesRegex(fleet_setup.SetupError, re.escape(message)):
                answers_for([*base, option, value])
        # The fleet installer checks the effort against the model.
        answers, _, config, secrets, _ = self.generate([*base, "--captain-model", "gpt-5.4", "--captain-effort", "max"])
        with self.assertRaisesRegex(runtime.Invalid, "'max' is not available for gpt-5.4"):
            self.officers(config, secrets)

    def test_an_empty_model_returns_one_role_to_its_default_over_a_fleet_wide_one(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo",
                "--crew-codex-model", "gpt-6.2-sol"]
        answers, members, config, secrets, _ = self.generate(base)
        config["harness"] = {"type": "opencode", "family": "codex", "model": "gpt-6.2-sol"}
        self.assertEqual(self.officers(config, secrets), [("codex", "gpt-6.2-sol", None)] * 2)
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            again = self.rerun(answers, config, "--captain-model", "")
            rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        # The captains return to their default; the rear admiral and the crew keep what they ran.
        self.assertEqual(self.officers(rebuilt, secrets), [("codex", "gpt-6.2-sol", None),
                                                           ("claude", "claude-opus-5-5", "xhigh")])
        self.assertEqual(rebuilt["harness"], {"type": "opencode",
                                              "rearAdmiral": {"family": "codex", "model": "gpt-6.2-sol"}})
        self.assertEqual(rebuilt["crew"], config["crew"])
        self.assertIn("fleet-wide harness family codex, model gpt-6.2-sol no longer applies to the captains; the "
                      "rear admiral keeps it, now as harness.rearAdmiral.", output.getvalue())
        # Nothing moves again on a later run.
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            fleet_setup.fleet_config(self.rerun(again, rebuilt), members, "a" * 40)
        self.assertNotIn("fleet-wide", output.getvalue())
        # An effort alone: the rear admiral keeps the model at its default effort, and the captains keep both.
        config["harness"] = {"model": "claude-fable-5-2", "effort": "max"}
        again = self.rerun(answers, config, "--rear-admiral-effort", "")
        with contextlib.redirect_stdout(io.StringIO()):
            rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(self.officers(rebuilt, secrets), [("claude", "claude-fable-5-2", "high"),
                                                           ("claude", "claude-fable-5-2", "max")])

    def test_a_fleet_set_up_before_the_role_defaults_keeps_its_model_until_it_chooses(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        answers, members, config, secrets, _ = self.generate(base)
        del answers["officerModels"]
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            again = self.rerun(answers, config)
        self.assertIn("officers defaulted to gpt-5.4, so they keep it", output.getvalue())
        self.assertIn("subspace-setup --captain-model '' --rear-admiral-model ''", output.getvalue())
        rebuilt = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(self.officers(rebuilt, secrets), [("codex", "gpt-5.4", None), ("codex", "gpt-5.4", None)])
        # Its later runs keep it quietly, and the documented command moves it to the defaults.
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            kept = self.rerun(again, rebuilt)
        self.assertEqual((output.getvalue(), kept["officerModels"]), ("", again["officerModels"]))
        moved = self.rerun(kept, rebuilt, "--captain-model", "", "--rear-admiral-model", "")
        # Naming both models on the first run moves it without the note.
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            direct = self.rerun(answers, config, "--captain-model", "", "--rear-admiral-model", "")
        self.assertEqual((output.getvalue(), direct["officerModels"]), ("", moved["officerModels"]))
        self.assertEqual(self.officers(fleet_setup.fleet_config(moved, members, "a" * 40), secrets),
                         [("claude", "claude-fable-5-2", "high"), ("claude", "claude-opus-5-5", "xhigh")])
        # A fleet whose fleet.json already names the officers' model keeps that instead.
        config["harness"] = {"type": "opencode", "family": "codex", "model": "gpt-6.2-sol"}
        again = self.rerun(answers, config)
        self.assertEqual(again["officerModels"], {"captain": {}, "rearAdmiral": {}})
        self.assertEqual(self.officers(fleet_setup.fleet_config(again, members, "a" * 40), secrets),
                         [("codex", "gpt-6.2-sol", None), ("codex", "gpt-6.2-sol", None)])

    def test_repositories_answer_fills_the_fleet_list_and_gives_captains_crew(self):
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo", *CREW,
                "--repositories", "https://github.com/example/project.git, library=https://github.com/example/lib.git"]
        _, members, config, secrets, _ = self.generate(argv)
        self.assertEqual(config["repositories"], [
            {"id": "project", "url": "https://github.com/example/project.git"},
            {"id": "library", "url": "https://github.com/example/lib.git"},
        ])
        cfg = validate(config, secrets, check_files=False)
        # Every crew member has the name it was given; its family and slot stay beside it.
        self.assertEqual([(worker["id"], worker["name"], worker["slot"]) for worker in cfg["members"][1]["crew"]],
                         [("rigel", "Rigel", "claude-1"), ("deneb", "Deneb", "claude-2"), ("mira", "Mira", "codex-1"),
                          ("sirius", "Sirius", "codex-2")])
        self.assertEqual(set(secrets["ircPasswords"]), {"admiral"})
        # The list lives in fleet.json: a later run without the flag keeps it, including hand edits.
        config["repositories"][1]["private"] = True
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        answers, rear, captains = answers_for(argv[:9])
        kept = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(kept["repositories"], config["repositories"])
        # A new answer replaces the list; a repository that stays keeps its ID and visibility.
        answers, rear, captains = answers_for([*argv[:9], "--repositories", "https://github.com/example/lib.git"])
        replaced = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(replaced["repositories"], [config["repositories"][1]])
        fleet_setup.save_answers(answers)
        self.assertNotIn("repositories", fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))
        # An explicit ID=URL answer renames a known repository and keeps its visibility.
        answers, rear, captains = answers_for([*argv[:9], "--repositories", "shared=https://github.com/example/lib.git"])
        renamed = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(renamed["repositories"], [{"id": "shared", "url": "https://github.com/example/lib.git",
                                                    "private": True}])

    def test_private_repositories_and_github_apps_added_by_hand_survive_a_rerun(self):
        # The documented way to add private repositories after the first install: edit fleet.json, then re-run
        # setup without --repositories. Nothing is dropped or reordered.
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion,Vega", "--no-demo"]
        answers, members, config, _, _ = self.generate(argv)
        apps = {member["id"]: {"appId": index + 1, "installationId": index + 10, "clientId": f"client-{index}",
                               "botLogin": f"example-{member['id']}[bot]", "botId": index + 100}
                for index, member in enumerate(members)}
        for person in [config["rearAdmiral"], *config["captains"]]:
            person["githubApp"] = apps[person["id"]]
        config["repositories"] = [
            {"id": "service", "url": "https://github.com/example/service.git", "private": True},
            {"id": "library", "url": "https://github.com/example/library.git", "private": True},
            {"id": "docs", "url": "https://github.com/example/docs.git"},
        ]
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        fleet_setup.save_answers(answers)
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            again = fleet_setup.gather(args, fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))
        rerun = fleet_setup.fleet_config(again, members, "a" * 40)
        self.assertEqual(rerun["repositories"], config["repositories"])
        self.assertEqual([person.get("githubApp") for person in [rerun["rearAdmiral"], *rerun["captains"]]],
                         [apps["atlas"], apps["orion"], apps["vega"]])
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, again["slug"], None), members,
                                            again["admin"], certificates=True)
        cfg = validate(rerun, secrets, check_files=False)
        self.assertEqual([name for name, repo in cfg["repositories"].items() if repo["private"]], ["service", "library"])

    def test_a_rejected_repository_url_is_not_echoed(self):
        fixture = "fixture-not-a-secret"
        for url in (f"https://user:{fixture}@example.com/a.git", f"https://example.com/a.git?token={fixture}",
                    f"http://{fixture}@example.com/a.git"):
            argv = ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion",
                    "--repositories", f"https://github.com/example/ok.git,{url}"]
            with self.subTest(url=url), self.assertRaises(fleet_setup.SetupError) as raised:
                answers_for(argv)
            self.assertNotIn(fixture, str(raised.exception))
            self.assertIn("Repository 2 (example.com)", str(raised.exception))

    def test_every_new_crew_member_is_named(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion, Vega",
                "--repositories", "https://github.com/example/project.git"]
        with self.assertRaisesRegex(fleet_setup.SetupError, "Name every crew member of orion, vega: pass --crew-names "
                                    "with 4 names per captain, in slot order \\(Claude 1, Claude 2, Codex 1, Codex 2\\)"):
            answers_for(base)
        for names, message in (("Orion=Rigel,Deneb", "gives Orion 2 crew names for 4 crew"),
                               ("Lyra=Rigel,Deneb,Mira,Sirius", "not one of the captains"),
                               ("Orion=Rigel,Deneb,Mira,Sirius;Vega=rigel,Castor,Pollux,Spica",
                                "orion's Rigel and vega's rigel are the same"),
                               ("Orion=Rigel,Deneb,Mira,Vega;Vega=Altair,Castor,Pollux,Spica", "is an officer's name"),
                               ("Orion=Rigel,Deneb,Mira,Si/rius;Vega=Altair,Castor,Pollux,Spica", "use ASCII")):
            with self.subTest(names), self.assertRaisesRegex(fleet_setup.SetupError, message):
                answers_for([*base, "--crew-names", names])
        answers, rear, captains = answers_for([*base, "--crew-names",
                                               "Orion=Rigel,Deneb,Mira,Sirius;Vega=Altair,Castor,Pollux,Spica"])
        members = fleet_setup.roster(answers, rear, captains)
        self.assertEqual(members[2]["crewNames"], {"claude-1": "Altair", "claude-2": "Castor", "codex-1": "Pollux",
                                                   "codex-2": "Spica"})
        # A captain already installed keeps its crew as installed, named or not.
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {"captains": [{"id": "orion"}, {"id": "vega"}],
                                                                  "repositories": [{"id": "project"}]})
        answers_for(base[:6])

    def test_saved_crew_names_follow_the_roster_and_installed_crew_keep_their_ids(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion, Vega", "--no-demo",
                "--repositories", "https://github.com/example/project.git"]
        answers, _, config, secrets, _ = self.generate(
            [*base, "--crew-names", "Orion=Rigel,Deneb,Mira,Sirius;Vega=Altair,Castor,Pollux,Spica"])

        def rebuilt(*argv):
            again = self.rerun(answers, config, *argv)
            with patch.object(fleet_setup.pwd, "getpwnam"):
                rear, captains = fleet_setup.check_answers(again)
            return again, fleet_setup.fleet_config(again, fleet_setup.roster(again, rear, captains), "a" * 40)
        # A captain taken out of the roster takes its crew's names along, so later runs still pass.
        again, fewer = rebuilt("--captains", "Orion")
        self.assertEqual(set(again["crewNames"]), {"orion"})
        # With fewer crew, only the remaining slots' names reach fleet.json, which the fleet installer takes.
        again, fewer = rebuilt("--crew-codex", "0")
        self.assertEqual(fewer["captains"][1]["crewNames"], {"claude-1": "Altair", "claude-2": "Castor"})
        validate(fewer, secrets, check_files=False)
        self.assertEqual(again["crewNames"]["vega"]["codex-1"], "Pollux")  # Kept for when the slot comes back.
        # Names that would give installed crew new IDs are refused before anything is saved, pointing to the
        # rename; the names they have already are accepted.
        with self.assertRaisesRegex(fleet_setup.SetupError, "would give installed crew new IDs: vega's Claude 1 "
                                    "\\(altair would become lyra\\).*rename-officers --crew-names.*Nothing was "
                                    "changed"):
            self.rerun(answers, config, "--crew-names", "Vega=Lyra,Castor,Pollux,Spica")
        self.rerun(answers, config, "--crew-names", "Vega=ALTAIR,Castor,Pollux,Spica")
        # A fleet installed before crew names keeps its slot names, which it may also give.
        for captain in config["captains"]:
            captain.pop("crewNames")
        with self.assertRaisesRegex(fleet_setup.SetupError, "orion's Claude 1 \\(claude-1 would become rigel\\)"):
            self.rerun({**answers, "crewNames": {}}, config, "--crew-names", "Orion=Rigel,Deneb,Mira,Sirius")
        self.rerun({**answers, "crewNames": {}}, config, "--crew-names", "Orion=Claude 1,Claude 2,Codex 1,Codex 2")
        # Crew named by rename-officers keep their installed IDs under their names (crewIds), and take the same
        # names again, as any later run may pass them; another name is still refused.
        slots = ["claude-1", "claude-2", "codex-1", "codex-2"]
        config["captains"][0].update(crewNames=dict(zip(slots, ["Rigel", "Deneb", "Mira", "Sirius"], strict=True)),
                                     crewIds={slot: slot for slot in slots})
        renamed = {**answers, "crewNames": {"orion": config["captains"][0]["crewNames"]},
                   "crewIds": {"orion": config["captains"][0]["crewIds"]}}
        again = self.rerun(renamed, config, "--crew-names", "Orion=rigel,Deneb,Mira,Sirius")
        self.assertEqual((again["crewNames"]["orion"]["claude-1"], again["crewIds"]["orion"]["claude-1"]),
                         ("rigel", "claude-1"))
        with self.assertRaisesRegex(fleet_setup.SetupError, "orion's Claude 1 \\(claude-1 would become canopus\\)"):
            self.rerun(renamed, config, "--crew-names", "Orion=Canopus,Deneb,Mira,Sirius")

    def test_rejects_unsafe_or_ambiguous_answers(self):
        cases = [
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Atlas"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--admin", "orion"],
            # Names that differ only by case are one name on IRC and in the API.
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion, ORION"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--admin", "NickServ"],
            # A ship channel, #<captain-id>, may not be one of the fleet's own standard channels.
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", f"Orion, X {COMMAND_SUFFIX.title()}"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "X Starbase"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "NickServ"],
            ["--name", "X", "--rear-admiral", "9lives", "--captains", "Orion"],
            # A ship's name is at most 64 characters.
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion" + "!" * 60],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--irc", "remote", "--irc-host", "localhost"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--ssh-from", "0.0.0.0/0; rm"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--repositories", "http://example.com/a.git"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion",
             "--repositories", "https://user:secret@example.com/a.git"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--repositories", "https://example.com/9lives.git"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion",
             "--repositories", "https://example.com/a/web.git,https://example.com/b/web.git"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion",
             "--repositories", ",".join(f"https://example.com/r{index}.git" for index in range(10))],
        ]
        for argv in cases:
            with self.subTest(argv=argv), self.assertRaises(fleet_setup.SetupError):
                answers_for(argv)
        with self.assertRaises(fleet_setup.SetupError):
            answers_for(["--rear-admiral", "Atlas", "--captains", "Orion"])

    def test_operator_sign_in_settings_survive_a_rerun(self):
        api_env = fleet_setup.INPUTS / "api.env"
        api_env.write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\n")
        self.assertTrue(fleet_setup.dev_auth_enabled())
        api_env.write_text("AUTH_PROVIDERS=workos\nENABLE_DEV_AUTH=false\nWORKOS_CLIENT_ID=client_example\n")
        self.assertFalse(fleet_setup.dev_auth_enabled())
        answers, _, _ = answers_for(["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion"])
        store = fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None)
        payload = Path(self.directory.name) / "payload"
        (payload / "subspace-api").mkdir(parents=True)
        (payload / "subspace-api" / "compose.yaml").write_text("services: {}\n")
        with patch.object(fleet_setup, "PAYLOAD", payload), \
                patch.object(fleet_setup, "API_DIR", Path(self.directory.name) / "api"), \
                patch.object(fleet_setup, "compose"):
            self.assertFalse(fleet_setup.api_up(answers, store))
        written = api_env.read_text()
        self.assertIn("AUTH_PROVIDERS=workos\n", written)
        self.assertIn("ENABLE_DEV_AUTH=false\n", written)
        self.assertIn("WORKOS_CLIENT_ID=client_example\n", written)
        self.assertIn("POSTGRES_PASSWORD=", written)

    def test_the_api_runs_an_agent_listener_for_the_sign_in_organisation(self):
        answers, _, _ = answers_for(["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion"])
        store = fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None)
        payload = Path(self.directory.name) / "payload"
        (payload / "subspace-api").mkdir(parents=True)
        (payload / "subspace-api" / "compose.yaml").write_text("services: {}\n")
        api_env = fleet_setup.INPUTS / "api.env"

        def up():
            with patch.object(fleet_setup, "PAYLOAD", payload), \
                    patch.object(fleet_setup, "API_DIR", Path(self.directory.name) / "api"), \
                    patch.object(fleet_setup, "compose"):
                return fleet_setup.api_up(answers, store)

        def env():
            return dict(line.partition("=")[::2] for line in api_env.read_text().splitlines()
                        if not line.startswith("#"))

        def published():
            return '"127.0.0.1:3001:3001"' in (fleet_setup.INPUTS / "api.override.yaml").read_text()

        # The first run offers the listener and publishes it on loopback.
        self.assertTrue(up())
        self.assertEqual(env()["AGENT_PORT"], "3001")
        self.assertEqual(env()["AGENT_ORGANISATION"], f"dev:{answers['organisation']}")
        self.assertIn(fleet_setup.AGENT_OFFERED, api_env.read_text())
        self.assertTrue(fleet_setup.agent_listener_enabled() and published())
        override = (fleet_setup.INPUTS / "api.override.yaml").read_text()
        self.assertIn('"127.0.0.1:3000:3000"', override)
        self.assertNotIn("0.0.0.0", override)
        up()
        self.assertEqual(env()["AGENT_PORT"], "3001")
        # A stale organisation in the installer's own form (dev:<organisation>) follows the fleet's.
        api_env.write_text(api_env.read_text().replace(
            f"AGENT_ORGANISATION=dev:{answers['organisation']}", "AGENT_ORGANISATION=dev:old-organisation"))
        up()
        self.assertEqual(env()["AGENT_ORGANISATION"], f"dev:{answers['organisation']}")
        # Turned off by the operator, with AGENT_PORT= or by deleting it, it stays off on every later run.
        for off in ("AGENT_PORT=\n", ""):
            api_env.write_text(api_env.read_text().replace(f"AGENT_PORT={env()['AGENT_PORT']}\n", off))
            for _ in range(2):
                up()
                self.assertEqual(env()["AGENT_PORT"], "")
                self.assertFalse(fleet_setup.agent_listener_enabled() or published())
        # The operator turns it back on by setting the port.
        api_env.write_text(api_env.read_text().replace("AGENT_PORT=\n", "AGENT_PORT=3001\n"))
        up()
        self.assertTrue(fleet_setup.agent_listener_enabled() and published())
        # Production never gets one.
        api_env.write_text(api_env.read_text() + "NODE_ENV=production\n")
        api_env.write_text(api_env.read_text().replace("NODE_ENV=development\n", ""))
        up()
        self.assertFalse(fleet_setup.agent_listener_enabled() or published())
        # Switching development sign-in off turns it off; a fresh api.env with WorkOS never offers it.
        api_env.write_text("AUTH_PROVIDERS=workos\nENABLE_DEV_AUTH=false\nWORKOS_CLIENT_ID=client_example\n")
        self.assertFalse(up())
        self.assertEqual(env()["AGENT_PORT"], "")
        self.assertNotIn("AGENT_ORGANISATION", env())
        self.assertFalse(published())

    def test_the_dry_run_previews_the_api_settings_the_real_run_uses(self):
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40, "--no-demo"]
        payload = Path(self.directory.name) / "payload"
        (payload / "subspace-api").mkdir(parents=True)
        (payload / "subspace-api" / "compose.yaml").write_text("services: {}\n")
        api_env = fleet_setup.INPUTS / "api.env"

        def dry_run():
            seen = []

            def installer(config_path, dry_run=False, secrets_dir=None, quiet=False):
                self.assertTrue(dry_run)
                folder = Path(config_path).parent
                seen.append((json.loads(Path(config_path).read_text()),
                             json.loads((folder / "fleet.secrets.json").read_text())))
            with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "say"), \
                    patch.object(fleet_setup, "installer", side_effect=installer):
                fleet_setup.setup(fleet_setup.parse_args([*argv, "--dry-run"]))
            config, secrets = seen[-1]
            return config, validate(config, secrets, check_files=False)

        def mcp(cfg):
            return [runtime.SUBSPACE_MCP_GRANT in runtime.harness_permissions(cfg, person) for person in cfg["members"]]

        # A new fleet: the real run starts the API with development sign-in and the agent listener.
        config, cfg = dry_run()
        self.assertEqual(config["api"], {"url": "http://127.0.0.1:3000", "initiatives": [],
                                         "agentUrl": "http://127.0.0.1:3001"})
        self.assertEqual(mcp(cfg), [True, True])
        # An installed fleet, unchanged: its saved initiative and the api.env the real run keeps.
        answers, _, _ = answers_for(argv[3:])
        answers["initiativeId"] = "00000000-0000-4000-8000-000000000001"
        fleet_setup.save_answers(answers)
        with patch.object(fleet_setup, "PAYLOAD", payload), \
                patch.object(fleet_setup, "API_DIR", Path(self.directory.name) / "api"), \
                patch.object(fleet_setup, "compose"):
            fleet_setup.api_up(answers, fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None))
        config, cfg = dry_run()
        self.assertEqual(config["api"]["initiatives"], [answers["initiativeId"]])
        real = runtime.validate(*self.generate(argv[3:], fleet_setup.local_api_block(
            answers, [answers["initiativeId"]], fleet_setup.agent_listener_enabled()))[2:4], check_files=False)
        for planned, person in zip(cfg["members"], real["members"]):
            self.assertEqual(runtime.harness_permissions(cfg, planned), runtime.harness_permissions(real, person))
        self.assertEqual(mcp(cfg), [True, True])
        # The operator turned the listener off: the officers lose the API's tools in both runs.
        api_env.write_text(api_env.read_text().replace("AGENT_PORT=3001\n", "AGENT_PORT=\n"))
        config, cfg = dry_run()
        self.assertNotIn("agentUrl", config["api"])
        self.assertEqual(mcp(cfg), [False, False])
        # Without development sign-in the real run writes no API settings, nor does the dry run.
        api_env.write_text("AUTH_PROVIDERS=workos\nENABLE_DEV_AUTH=false\n")
        config, cfg = dry_run()
        self.assertNotIn("api", config)

    def test_further_fleet_registration_carries_its_records_and_is_checked(self):
        argv = ["--name", "Fleet Two", "--rear-admiral", "Nova", "--captains", "Lyra, Vega",
                "--irc", "remote", "--irc-host", "irc.example.com", "--no-api"]
        answers, members, config, secrets, _ = self.generate(argv)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.secrets.json", secrets)
        registration = fleet_setup.fleet_registration(answers)
        self.assertEqual((registration["ircHost"], registration["ircPort"], registration["ircTls"]),
                         ("irc.example.com", 6697, True))
        self.assertEqual((registration["starbaseChannel"], registration["commandChannel"]),
                         ("#fleet-two-starbase", f"#fleet-two-{COMMAND_SUFFIX}"))
        # One name per officer: its ID is its agent name and IRC nick, and a ship's channel is its captain's.
        self.assertEqual(registration["rearAdmiral"], {
            "name": "nova", "ircNick": "nova", "defaultModel": "claude-fable-5-2",
            "serviceTokenClientId": "nova@agents.example.com"})
        self.assertEqual([(row["name"], row["ship"], row["channel"], row["ircNick"], row["defaultModel"])
                          for row in registration["captains"]],
                         [("lyra", "Lyra", "#lyra", "lyra", "claude-opus-5-5"),
                          ("vega", "Vega", "#vega", "vega", "claude-opus-5-5")])
        self.assertNotIn("Password", fleet_setup.json.dumps(registration))
        # Its Fleet API is off, so the bundle says it has no address.
        self.assertIsNone(registration["fleetApiUrl"])
        # The first fleet's VM registers it, with its initiative, and a re-run changes nothing.
        fleet_setup.write_json(fleet_setup.INPUTS / "answers.json", {"api": True, "slug": "fleet-one", "initiativeId": "own"})
        (fleet_setup.INPUTS / "api.env").write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\n")
        api = fixture.Subspace()
        with fake_api(api), patch.object(fleet_setup, "api_token", return_value="admin"):
            fleet_setup.register_fleet(registration)
            writes = len(api.writes)
            fleet_setup.register_fleet(registration)
        self.assertEqual(len(api.writes), writes)
        self.assertEqual(api.summary()["fleets"]["fleet-two"]["commandChannel"], f"#fleet-two-{COMMAND_SUFFIX}")
        self.assertEqual(api.summary()["ships"], [("Lyra", "#lyra"), ("Vega", "#vega")])
        self.assertEqual([row[:3] for row in api.summary()["agents"]], [
            ("lyra", "captain", "lyra"), ("nova", "rearAdmiral", "nova"), ("vega", "captain", "vega")])
        [initiative] = [row for row in api.initiatives.values() if row["name"] == "Fleet Two"]
        self.assertEqual((initiative["channels"], initiative["fleetId"]), ("", api.fleet_by("fleet-two")["id"]))
        captain = registration["captains"][0]
        bad = [
            {**registration, "slug": "fleet-one"},
            {**registration, "slug": "Fleet Two"},
            {**registration, "ircPort": "6697"},
            {**registration, "starbaseChannel": "#fleet-one-starbase"},
            {**registration, "commandChannel": None},
            {**registration, "captains": [{**captain, "channel": "#fleet-two-command"}]},
            {**registration, "captains": [{**captain, "channel": "#vega"}]},
            {**registration, "captains": [{**captain, "name": "Lyra-Prime"}]},
            {**registration, "captains": [captain, captain]},
            {**registration, "rearAdmiral": {**registration["rearAdmiral"], "name": "Rear Admiral Nova"}},
            {**registration, "rearAdmiral": {**registration["rearAdmiral"], "ircNick": "9nova"}},
            {**registration, "fleetApiUrl": "http://fleet-two.example.com:8484"},
            {**registration, "fleetApiUrl": "https://operator@fleet-two.example.com"},
            # The desktop's loopback ports carry the first fleet's tunnel.
            {**registration, "fleetApiUrl": "http://127.0.0.1:8484"},
            # A registration from an older installer, with names but no records, is refused.
            {**{key: value for key, value in registration.items() if key not in ("rearAdmiral", "captains")},
             "rearAdmiralName": "Nova", "captainNames": ["Lyra"]},
        ]
        for value in bad:
            with self.subTest(value=value), patch.object(fleet_setup, "api_token") as token:
                with self.assertRaises(fleet_setup.SetupError):
                    fleet_setup.register_fleet(value)
                token.assert_not_called()

    def test_further_fleet_registration_carries_only_a_fleet_api_address_it_knows(self):
        argv = ["--name", "Fleet Two", "--rear-admiral", "Nova", "--captains", "Lyra",
                "--irc", "remote", "--irc-host", "irc.example.com", "--no-api"]
        fleet_setup.write_json(fleet_setup.INPUTS / "api.env", {})
        (fleet_setup.INPUTS / "api.env").write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\n")

        def bundle(extra):
            answers, _, config, secrets, _ = self.generate(argv + extra, stargate_api=True)
            fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
            fleet_setup.write_json(fleet_setup.INPUTS / "fleet.secrets.json", secrets)
            return fleet_setup.fleet_registration(answers)

        # Without an address of its own, the bundle leaves it out: the tunnel's loopback end is the first fleet's.
        self.assertNotIn("fleetApiUrl", bundle([]))
        registration = bundle(["--fleet-api-url", "HTTPS://Fleet-Two.Example.ts.net/"])
        self.assertEqual(registration["fleetApiUrl"], "https://fleet-two.example.ts.net")
        fleet_setup.write_json(fleet_setup.INPUTS / "answers.json", {"api": True, "slug": "fleet-one", "initiativeId": "own"})
        api = fixture.Subspace()
        with fake_api(api), patch.object(fleet_setup, "api_token", return_value="admin"):
            fleet_setup.register_fleet(registration)
            writes = len(api.writes)
            fleet_setup.register_fleet(registration)
            self.assertEqual(len(api.writes), writes, "a re-run changes nothing")
            self.assertEqual(api.fleet_by("fleet-two")["fleetApiUrl"], "https://fleet-two.example.ts.net")
            # A bundle without one keeps what the API has; one from a fleet whose Fleet API is off clears it.
            fleet_setup.register_fleet({key: value for key, value in registration.items() if key != "fleetApiUrl"})
            self.assertEqual(len(api.writes), writes)
            fleet_setup.register_fleet({**registration, "fleetApiUrl": None})
            self.assertIsNone(api.fleet_by("fleet-two")["fleetApiUrl"])

    def test_the_fleet_api_address_is_the_tunnel_s_end_unless_given(self):
        api_block = {"url": "http://127.0.0.1:3000", "initiatives": ["00000000-0000-4000-8000-000000000001"]}
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        api = fixture.Subspace()
        for extra, stargate_api, address in (
                ([], True, "http://127.0.0.1:8484"),
                (["--fleet-api-url", "https://fleet.example.ts.net:8484/"], True, "https://fleet.example.ts.net:8484"),
                (["--fleet-api-url", "tunnel"], True, "http://127.0.0.1:8484"),
                (["--fleet-api-url", "https://fleet.example.ts.net:8484/"], False, None)):
            _, _, config, secrets, _ = self.generate(argv + extra, api_block, stargate_api)
            with self.subTest(extra=extra, stargate_api=stargate_api), fake_api(api):
                fleet_setup.register_own("admin", config, secrets)
                writes = len(api.writes)
                fleet_setup.register_own("admin", config, secrets)
                self.assertEqual(len(api.writes), writes, "a re-run changes nothing")
                self.assertEqual(api.fleet_by("example-fleet")["fleetApiUrl"], address)
        # A later run keeps the saved answer; 'tunnel' goes back to the tunnel's end.
        saved = {"fleetApiUrl": "https://fleet.example.ts.net"}
        self.assertEqual(fleet_setup.gather(fleet_setup.parse_args(["--non-interactive", *argv]), saved)["fleetApiUrl"],
                         "https://fleet.example.ts.net")
        self.assertNotIn("fleetApiUrl", fleet_setup.gather(
            fleet_setup.parse_args(["--non-interactive", *argv, "--fleet-api-url", "tunnel"]), saved))
        for extra in (["--fleet-api-url", "http://fleet.example.ts.net:8484"],
                      ["--fleet-api-url", "https://fleet.example.ts.net/?token=x"],
                      ["--fleet-api-url", "https://fleet.example.ts.net", "--stargate-api", "off"]):
            with self.subTest(extra=extra), self.assertRaises(fleet_setup.SetupError):
                answers_for(argv + extra)

    def test_setup_registers_the_fleet_its_ships_and_officers_without_dispatch(self):
        api_block = {"url": "http://127.0.0.1:3000", "initiatives": ["00000000-0000-4000-8000-000000000001"]}
        _, _, config, secrets, _ = self.generate(
            ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"], api_block)
        api = fixture.Subspace()
        with fake_api(api):
            records = fleet_setup.register_own("admin", config, secrets)
            writes = len(api.writes)
            fleet_setup.register_own("admin", config, secrets)
        self.assertEqual(len(api.writes), writes)
        self.assertEqual(records["rearAdmiral"]["ircNick"], "atlas")
        self.assertEqual(api.summary()["fleets"]["example-fleet"]["starbaseChannel"], "#example-fleet-starbase")
        self.assertEqual(api.summary()["agents"], [
            ("atlas", "rearAdmiral", "atlas", None, True), ("orion", "captain", "orion", "Orion", True)])
        # The desktop's local IRC endpoint is what the fleet records.
        self.assertEqual((api.fleet_by("example-fleet")["ircHost"], api.fleet_by("example-fleet")["ircTls"]),
                         ("127.0.0.1", False))

    def test_initiative_is_found_or_created_without_channels(self):
        api = fixture.Subspace()
        with fake_api(api):
            created = fleet_setup.fleet_initiative("t", "Fleet", "f")
            self.assertEqual(api.initiatives[created]["channels"], "")
            # Found again by its saved ID or by name; another fleet's initiative of that name is never taken.
            self.assertEqual(fleet_setup.fleet_initiative("t", "Fleet", "f", created), created)
            self.assertEqual(fleet_setup.fleet_initiative("t", "Fleet", "f"), created)
            api.call("POST", "/fleets", {"slug": "other", "name": "Other"})
            api.initiatives[created]["fleetId"] = api.fleet_by("other")["id"]
            self.assertNotEqual(fleet_setup.fleet_initiative("t", "Fleet", "f"), created)
        self.assertNotIn("PATCH", api.counts)

    def test_an_adopted_initiative_is_kept_as_the_fleets_by_its_id(self):
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo"]
        adopted = "0000000A-0000-4000-8000-00000000000B"
        answers, _, _ = answers_for([*base, "--initiative", adopted])
        self.assertEqual(answers["initiativeId"], adopted.lower())
        with self.assertRaisesRegex(fleet_setup.SetupError, "must be an initiative's UUID"):
            answers_for([*base, "--initiative", "Local workspace"])
        # Kept on later runs, and '' goes back to finding one by the fleet's name.
        fleet_setup.save_answers(answers)
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
            self.assertEqual(fleet_setup.gather(args, saved)["initiativeId"], adopted.lower())
            args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--initiative", ""])
            self.assertNotIn("initiativeId", fleet_setup.gather(args, saved))
        # Only --initiative on the run adopts: setup takes the initiative it names though the admin is not a member
        # and another fleet has it, and moves it once the fleet is registered. A recorded one that another fleet
        # has since taken stays there, with a warning; one that is gone is replaced by the name search.
        api = fixture.Subspace()
        api.call("POST", "/fleets", {"slug": "local", "name": "Local fleet"})
        api.call("POST", "/fleets", {"slug": "example-fleet", "name": "Example Fleet"})
        adopted, local = adopted.lower(), api.fleet_by("local")["id"]
        api.initiatives[adopted] = {"id": adopted, "name": "Local workspace", "fleetId": local, "channels": ""}

        def call(_token, method, path, body=None):
            if path == "/me/initiatives":
                return {"items": []}
            try:
                return api.call(method, path, body)
            except fixture.Refusal as refusal:
                raise fleet_setup.ApiError("refused", refusal.status) from None

        said = []
        with patch.object(fleet_setup, "api_call", side_effect=call), \
                patch.object(fleet_setup, "say", side_effect=said.append):
            self.assertEqual(fleet_setup.fleet_initiative("t", "Example Fleet", "example-fleet", adopted, adopt=True),
                             adopted)
            self.assertIsNone(fleet_setup.fleet_initiative("t", "Example Fleet", "example-fleet", adopted,
                                                           hint="; or not"))
            self.assertEqual(said[-1], f"WARNING: initiative Local workspace ({adopted}), recorded as fleet "
                             "example-fleet's, now belongs to fleet Local fleet (local). Setup leaves it there and "
                             "registers no initiative for example-fleet on this run: only an explicit adoption moves "
                             f"one. To move it back, run 'sudo subspace-fleetctl assign {adopted} example-fleet'; or "
                             "not.")
            self.assertEqual((api.initiatives[adopted]["fleetId"], api.counts["PUT"]), (local, 0))
            fleet_setup.adopt_initiative("t", "example-fleet", adopted)
            self.assertEqual(api.initiatives[adopted]["fleetId"], ours := api.fleet_by("example-fleet")["id"])
            self.assertEqual(said[-1], f"assigned initiative Local workspace ({adopted}) to this fleet, moving it "
                             "from fleet Local fleet (local)")
            fleet_setup.adopt_initiative("t", "example-fleet", adopted)
            self.assertEqual(api.counts["PUT"], 1)
            # Now the fleet's own, the recorded ID stands, with no move.
            self.assertEqual(fleet_setup.fleet_initiative("t", "Example Fleet", "example-fleet", adopted), adopted)
            self.assertEqual(api.initiatives[adopted]["fleetId"], ours)
            gone = "00000000-0000-4000-8000-00000000dead"
            self.assertNotIn(fleet_setup.fleet_initiative("t", "Example Fleet", "example-fleet", gone),
                             (gone, adopted))

    def test_only_initiative_on_the_run_adopts_and_only_one_that_exists(self):
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40,
                "--irc-auth", "password"]
        api = fixture.Subspace()
        api.call("POST", "/fleets", {"slug": "local", "name": "Local fleet"})
        local, workspace = api.fleet_by("local")["id"], "00000000-0000-4000-8000-0000000000a1"
        missing = "00000000-0000-4000-8000-00000000dead"
        api.initiatives[workspace] = {"id": workspace, "name": "Local workspace", "fleetId": local, "channels": ""}

        def call(_token, method, path, body=None):
            try:
                return api.call(method, path, body)
            except fixture.Refusal as refusal:
                raise fleet_setup.ApiError("refused", refusal.status) from None

        def setup(*extra):
            """A real run, with the API, the installer and the VM's commands stubbed, up to its summary."""
            said = []
            with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                    patch.object(fleet_setup, "installer"), patch.object(fleet_setup, "run"), \
                    patch.object(fleet_setup, "api_up", return_value=True), \
                    patch.object(fleet_setup, "history_database", return_value=None), \
                    patch.object(fleet_setup, "agent_listener_enabled", return_value=False), \
                    patch.object(fleet_setup, "api_token", return_value="header.payload.signature"), \
                    patch.object(fleet_setup, "api_call", side_effect=call), \
                    patch.object(fleet_setup, "seed_demo") as seed, \
                    patch.object(fleet_setup, "irc_source", return_value="a" * 40), \
                    patch.object(fleet_setup, "say", side_effect=said.append), \
                    patch.object(fleet_setup, "summary", side_effect=fleet_setup.SetupError("stop here")), \
                    self.assertRaises(fleet_setup.SetupError) as stopped:
                fleet_setup.setup(fleet_setup.parse_args([*argv, *extra]))
            answers = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
            return str(stopped.exception), answers.get("initiativeId"), said, seed.called

        # An --initiative that does not exist stops the run before any tracker record changes, and is not recorded.
        writes = list(api.writes)
        error, recorded, _, seeded = setup("--initiative", missing)
        self.assertIn(f"There is no initiative {missing} in the development organisation", error)
        self.assertEqual((recorded, seeded, api.writes), (None, False, writes))
        # One that exists is adopted, moved from the fleet that had it once this fleet is registered.
        error, recorded, said, _ = setup("--initiative", workspace)
        ours = api.fleet_by("example-fleet")["id"]
        self.assertEqual((error, recorded, api.initiatives[workspace]["fleetId"]), ("stop here", workspace, ours))
        self.assertIn(f"assigned initiative Local workspace ({workspace}) to this fleet, moving it from fleet Local "
                      "fleet (local)", said)
        self.assertEqual(fleet_setup.read_json(fleet_setup.INPUTS / "fleet.json")["api"]["initiatives"], [workspace])
        # Taken back by the other fleet, a routine rerun leaves it there, registers none and keeps the answer.
        api.initiatives[workspace]["fleetId"] = local
        puts = api.counts["PUT"]
        error, recorded, said, _ = setup()
        self.assertEqual((error, recorded, api.initiatives[workspace]["fleetId"], api.counts["PUT"]),
                         ("stop here", workspace, local, puts))
        self.assertEqual(fleet_setup.read_json(fleet_setup.INPUTS / "fleet.json")["api"]["initiatives"], [])
        self.assertTrue(any(line.startswith(f"WARNING: initiative Local workspace ({workspace}), recorded as fleet "
                                            "example-fleet's, now belongs to fleet Local fleet (local).")
                            and f"or setup with --initiative {workspace}; to have setup find or create one by the "
                            "fleet's name instead, pass --initiative ''." in line for line in said))
        # A mistyped --initiative on a fleet with one keeps the recorded one.
        error, recorded, _, _ = setup("--initiative", missing)
        self.assertEqual((error.startswith(f"There is no initiative {missing}"), recorded), (True, workspace))

    def test_teams_with_key_vault_needs_a_separate_gateway_user(self):
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {"stakeholders": {"teams": {"botClientId": "x"}}})
        etc = Path(self.directory.name) / "etc"
        etc.mkdir()
        answers = {"metadataUsers": [], "keyVault": "example-vault"}
        with patch.object(fleet_setup, "FLEET_ETC", etc), patch.object(fleet_setup.pwd, "getpwnam"):
            fleet_setup.write_json(etc / "gateway-checks.json", [["ss-atlas", "/var/lib/x/gateway.json"]])
            with self.assertRaises(fleet_setup.SetupError):
                fleet_setup.metadata_users(answers)
            fleet_setup.write_json(etc / "gateway-checks.json", [["ss-fleet-gateway", "/var/lib/x/gateway.json"]])
            self.assertEqual(fleet_setup.metadata_users(answers), ["ss-fleet-gateway"])
            self.assertEqual(fleet_setup.metadata_users({"metadataUsers": []}), ["ss-fleet-gateway"])


class TrackerOwnershipTests(unittest.TestCase):
    """The warning when a fleet's tracker projects belong to another fleet, whose officers alone may assign them."""

    FLEET, LOCAL = "00000000-0000-4000-8000-0000000000f1", "00000000-0000-4000-8000-0000000000f2"
    WORKSPACE, CHILD, OWN = ("00000000-0000-4000-8000-0000000000a1", "00000000-0000-4000-8000-0000000000a2",
                             "00000000-0000-4000-8000-0000000000a3")

    def api(self, projects, kind="agent"):
        """GETs as the API answers them to the fleet's rear admiral (subspace-api src/api.ts, src/github.ts)."""
        fleets = {self.FLEET: {"slug": "example-fleet", "name": "Example Fleet"},
                  self.LOCAL: {"slug": "local", "name": "Local fleet"}}
        initiatives = {
            self.WORKSPACE: {"name": "Local workspace", "fleetId": self.LOCAL, "parentId": None},
            self.CHILD: {"name": "Gateway work", "fleetId": None, "parentId": self.WORKSPACE},
            self.OWN: {"name": "Example Fleet", "fleetId": self.FLEET, "parentId": None},
        }
        calls = []

        def get(path):
            calls.append(path)
            route, _, query = path.partition("?")
            if route == "/me":
                return {"kind": kind, "fleetId": self.FLEET if kind == "agent" else None}
            if route == "/repositories/projects":
                repository = parse_qs(query)["repository"][0]
                return {"items": [row for row in projects if row["repository"] == repository], "nextCursor": None}
            initiative = route.removeprefix("/initiatives/")
            row = initiatives[initiative]
            return {"id": initiative, **row, "fleet": fleets.get(row["fleetId"])}
        return get, calls

    def project(self, key, repository, initiative):
        return {"id": f"project-{key}", "key": key, "repository": repository, "initiativeId": initiative}

    def test_another_fleets_projects_are_named_with_the_initiative_to_adopt(self):
        projects = [self.project("WEB", "example/project", self.WORKSPACE),
                    self.project("GATE", "example/gateway", self.CHILD),
                    self.project("WEB", "example/library", self.WORKSPACE),
                    self.project("DOC", "example/library", None),
                    self.project("OWN", "example/library", self.OWN)]
        get, calls = self.api(projects)
        repositories = ["example/project", "example/gateway", "example/library"]
        lines = fleet_setup.ownership_lines(get, repositories, assigns=True)
        warning = ("WARNING: tracker project {} ({}) belongs to fleet Local fleet (local) through initiative Local "
                   "workspace ({}), so this fleet's officers are refused when they assign its issues or mark them "
                   "Ready for agents. To make it this fleet's, run setup again with --initiative {}, which assigns "
                   "it, and all below it, to this fleet.")
        # GATE's own initiative has no fleet, so its nearest ancestor with one decides; WEB is named once.
        self.assertEqual(lines, [
            warning.format("WEB", "example/project", self.WORKSPACE, self.WORKSPACE),
            warning.format("GATE", "example/gateway", self.WORKSPACE, self.WORKSPACE),
            "tracker projects DOC, OWN belong to this fleet or to none, so its officers may assign their issues",
        ])
        self.assertTrue(all(path.startswith(("/me", "/repositories/projects?repository=example%2F", "/initiatives/"))
                            for path in calls))
        # Adopted with --initiative on this run, setup assigns it; the parent's adoption also covers the child's
        # project.
        lines = fleet_setup.ownership_lines(get, repositories, self.WORKSPACE, assigns=True, adopt=True)
        self.assertIn(f"tracker project GATE (example/gateway) belongs to fleet Local fleet (local) through initiative "
                      f"Local workspace ({self.WORKSPACE}); setup assigns the fleet's initiative {self.WORKSPACE}, on "
                      "that path, to this fleet, which makes it this fleet's.", lines)
        self.assertIn(f"setup assigns the fleet's initiative Local workspace ({self.WORKSPACE}) to this fleet, moving "
                      "it from fleet Local fleet (local).", lines)
        self.assertFalse(any(line.startswith("WARNING") for line in lines))
        # Recorded on an earlier run, it stays with the fleet that has it now: only --initiative moves it.
        lines = fleet_setup.ownership_lines(get, repositories, self.WORKSPACE, assigns=True)
        self.assertEqual(lines[:2], [warning.format("WEB", "example/project", self.WORKSPACE, self.WORKSPACE),
                                     warning.format("GATE", "example/gateway", self.WORKSPACE, self.WORKSPACE)])
        self.assertEqual(lines[2], f"WARNING: the fleet's initiative Local workspace ({self.WORKSPACE}) belongs to "
                         "fleet Local fleet (local), and setup leaves it there. To make it this fleet's, run setup "
                         f"again with --initiative {self.WORKSPACE}, which assigns it, and all below it, to this fleet.")
        # One of no fleet the installer assigns, which makes the child's project this fleet's.
        lines = fleet_setup.ownership_lines(get, ["example/gateway"], self.CHILD, assigns=True)
        self.assertEqual(lines, [
            f"tracker project GATE (example/gateway) belongs to fleet Local fleet (local) through initiative Local "
            f"workspace ({self.WORKSPACE}); setup assigns the fleet's initiative {self.CHILD}, on that path, to this "
            "fleet, which makes it this fleet's.",
            f"setup assigns the fleet's initiative Gateway work ({self.CHILD}) to this fleet."])
        # A hosted API, or one without development sign-in: only its administrator can assign the initiative.
        lines = fleet_setup.ownership_lines(get, repositories, self.WORKSPACE, assigns=False)
        put = f'PUT /api/v1/initiatives/{self.WORKSPACE}/fleet with {{"fleetId": "{self.FLEET}"}}'
        self.assertIn(f"ask an administrator of the Subspace API to assign it to this fleet ({put}), and pass "
                      f"--initiative {self.WORKSPACE} to record it as the fleet's.", lines[0])
        self.assertEqual(lines[-2], f"WARNING: the fleet's initiative Local workspace ({self.WORKSPACE}) is not "
                         f"assigned to it but to fleet Local fleet (local): ask an administrator of the Subspace API "
                         f"to assign it to this fleet ({put}).")

    def test_an_initiative_that_does_not_exist_is_named(self):
        get, _ = self.api([self.project("OWN", "example/project", self.OWN)])

        def missing(path):
            if path.startswith("/initiatives/00000000-0000-4000-8000-00000000dead"):
                raise fleet_setup.ApiError("The Subspace API returned HTTP 404.", 404)
            return get(path)
        gone = "00000000-0000-4000-8000-00000000dead"
        self.assertEqual(fleet_setup.ownership_lines(missing, ["example/project"], gone, assigns=True, adopt=True), [
            f"WARNING: the tracker has no initiative {gone}: check the ID given with --initiative; a real run stops "
            "before changing any tracker record."])
        self.assertEqual(fleet_setup.ownership_lines(missing, ["example/project"], gone, assigns=True), [
            f"WARNING: the tracker has no initiative {gone}, which setup recorded as the fleet's; a real run finds or "
            "creates one by the fleet's name."])

    def test_an_adoption_setup_cannot_make_prints_the_administrators_request(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        inputs = Path(directory.name)
        members = [{"id": "atlas", "name": "Atlas", "account": "example-fleet-atlas"}]
        initiative = self.WORKSPACE
        said = []
        request = (f"PUT /api/v1/initiatives/{initiative}/fleet with {{\"fleetId\": \"<the fleet's ID>\"}}, where "
                   "the fleet's ID is the \"id\" that GET /api/v1/fleets/example-fleet returns.")

        def check(answers, adopt=True):
            said.clear()
            fleet_setup.ownership_check({"slug": "example-fleet", "initiativeId": initiative, **answers}, members, {},
                                        adopt=adopt)
            return said

        with patch.object(fleet_setup, "INPUTS", inputs), patch.object(fleet_setup, "say", side_effect=said.append), \
                patch.object(fleet_setup, "agent_reader", agent_reader):
            # Development sign-in off turns the agent listener off too, so no check runs: the request still follows.
            (inputs / "api.env").write_text("AUTH_PROVIDERS=workos\nENABLE_DEV_AUTH=false\nAGENT_PORT=\n")
            self.assertEqual(check({"api": True}), [
                "not checking which fleet owns this fleet's tracker projects: this VM's API has no agent listener",
                f"WARNING: setup cannot assign the initiative given with --initiative ({initiative}) to this fleet, as "
                "this VM's API no longer offers development sign-in, through which setup acts as its administrator. "
                f"Ask an administrator of the Subspace API to assign it: {request}"])
            # A hosted API's officers that have not enrolled yet.
            with patch.object(runtime, "AGENT_SOCKETS", inputs / "agents"), \
                    patch.object(fleet_setup, "installed_stargate_source", return_value=inputs / "stargate"):
                self.assertIn("as a hosted API's administrators keep its records. Ask an administrator of the "
                              f"Subspace API to assign it: {request}",
                              check({"api": False, "apiUrl": "https://subspace.example.com"})[-1])
            # Without --initiative on this run, or with development sign-in, which setup assigns through, none.
            self.assertEqual(len(check({"api": True}, adopt=False)), 1)
            (inputs / "api.env").write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\nAGENT_PORT=\n")
            self.assertEqual(len(check({"api": True})), 1)

    def test_nothing_to_warn_about_or_nothing_to_check(self):
        get, _ = self.api([self.project("OWN", "example/project", self.OWN)])
        self.assertEqual(fleet_setup.ownership_lines(get, ["example/project"], self.OWN, assigns=False),
                         ["tracker projects OWN belong to this fleet or to none, so its officers may assign their issues"])
        get, _ = self.api([])
        self.assertEqual(fleet_setup.ownership_lines(get, ["example/project"]), [
            "no tracker project lists this fleet's GitHub repositories, so setup cannot tell which fleet owns its work"])
        get, calls = self.api([], kind="human")
        self.assertIn("is not yet a registered agent", fleet_setup.ownership_lines(get, ["example/project"])[0])
        self.assertEqual(calls, ["/me"])

    def test_the_check_reads_as_the_rear_admiral_and_never_stops_setup(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        inputs = Path(directory.name)
        members = [{"id": "atlas", "name": "Atlas", "account": "example-fleet-atlas"}]
        config = {"repositories": [{"id": "project", "url": "https://github.com/example/project.git"},
                                   {"id": "tool", "url": "https://git.example.com/team/tool.git"}]}
        said = []
        with patch.object(fleet_setup, "INPUTS", inputs), patch.object(fleet_setup, "say", side_effect=said.append), \
                patch.object(fleet_setup, "agent_reader", agent_reader):
            # An API on this VM is read through its agent listener, naming the rear admiral in its header.
            (inputs / "api.env").write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\nAGENT_PORT=3001\n")
            seen = []

            def urlopen(request, timeout):
                seen.append((request.full_url, request.get_header("X-subspace-agent")))
                raise URLError("refused")
            with patch.object(fleet_setup, "urlopen", side_effect=urlopen):
                fleet_setup.ownership_check({"api": True}, members, config)
            self.assertEqual(seen, [("http://127.0.0.1:3001/api/v1/me", "example-fleet-atlas")])
            self.assertEqual(said, ["could not check which fleet owns this fleet's tracker projects: Could not reach "
                                    "the Subspace API's agent listener for GET /me."])
            (inputs / "api.env").write_text("AGENT_PORT=\n")
            said.clear()
            fleet_setup.ownership_check({"api": True}, members, config)
            self.assertEqual(said, ["not checking which fleet owns this fleet's tracker projects: this VM's API has no "
                                    "agent listener"])
            # A hosted API is read through the rear admiral's signing-proxy socket, once it has enrolled.
            hosted = {"apiUrl": "https://subspace.example.com", "api": False}
            sockets, source = inputs / "agents", inputs / "stargate"
            said.clear()
            with patch.object(runtime, "AGENT_SOCKETS", sockets), \
                    patch.object(fleet_setup, "installed_stargate_source", return_value=source):
                fleet_setup.ownership_check(hosted, members, config)
                self.assertEqual(said, ["not checking which fleet owns this fleet's tracker projects: the officers "
                                        "have not enrolled with the hosted API yet"])
                (sockets / "atlas").mkdir(parents=True)
                listener = socket.socket(socket.AF_UNIX)
                self.addCleanup(listener.close)
                listener.bind(str(sockets / "atlas/api.sock"))
                answers = {"/api/v1/me": {"kind": "agent", "fleetId": self.FLEET},
                           "/api/v1/repositories/projects?repository=example%2Fproject&limit=100": {"items": []}}
                commands = []

                def run(command, **_options):
                    commands.append(command)
                    return subprocess.CompletedProcess(command, 0, json.dumps(answers[command[-1]]), "")
                said.clear()
                with patch.object(fleet_setup.subprocess, "run", side_effect=run):
                    fleet_setup.ownership_check(hosted, members, config)
            self.assertEqual(commands[0], [fleet_setup.FLEET_APP / "bin/bun", source / "src/main.ts", "subspace",
                                           "request", "--socket", sockets / "atlas/api.sock", "/api/v1/me"])
            self.assertEqual(said, ["no tracker project lists this fleet's GitHub repositories, so setup cannot tell "
                                    "which fleet owns its work"])


class AgentDispatchSetupTests(unittest.TestCase):
    """Setup asks whether to switch on agent dispatch; unattended, it stays off."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        fleet_setup.INPUTS = Path(self.directory.name)
        self.argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                     "--repositories", "https://github.com/example/project.git", "--no-demo", *CREW]
        self.api = {"url": "http://127.0.0.1:3000", "initiatives": ["00000000-0000-4000-8000-000000000001"]}

    def config(self, argv, api_block):
        answers, rear, captains = answers_for(argv)
        members = fleet_setup.roster(answers, rear, captains)
        config = fleet_setup.fleet_config(answers, members, "a" * 40, api_block, False, "b" * 40)
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"], "header.payload.admin", True,
                                            "header.payload.rear" if api_block else "", certificates=True)
        return answers, config, secrets

    def test_unattended_setup_leaves_dispatch_off(self):
        answers, config, _ = self.config(self.argv, self.api)
        self.assertIs(answers["agentDispatch"], False)
        self.assertNotIn("agentDispatch", config)
        prompts = []
        args = fleet_setup.parse_args(["--operator", "operator", *self.argv])
        with patch.object(fleet_setup.sys.stdin, "isatty", return_value=True), \
                patch("builtins.input", side_effect=lambda text: prompts.append(text) or ""):
            answers = fleet_setup.gather(args, {})
        # Asked once, with what switching it on does, and an empty answer keeps the default: off.
        [question] = [text for text in prompts if "agent dispatch" in text]
        self.assertIn("Ready for agents", question)
        self.assertIn("(y/n) [n]", question)
        self.assertIs(answers["agentDispatch"], False)

    def test_dispatch_on_writes_a_valid_fleet_and_is_saved(self):
        answers, config, secrets = self.config([*self.argv, "--agent-dispatch"], self.api)
        self.assertIs(answers["agentDispatch"], True)
        self.assertEqual(config["agentDispatch"], {"enabled": True})
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(cfg["agentDispatch"]["pollIntervalSeconds"], 120)
        # The answer is kept for the next run, which keeps it unless a flag changes it.
        fleet_setup.save_answers(answers)
        saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
        self.assertIs(saved["agentDispatch"], True)
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator"])
        self.assertIs(fleet_setup.gather(args, saved)["agentDispatch"], True)
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--no-agent-dispatch"])
        self.assertIs(fleet_setup.gather(args, saved)["agentDispatch"], False)
        # Hand-set timings survive a rerun.
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {"agentDispatch": {"enabled": True, "answerTimeoutSeconds": 600}})
        _, config, _ = self.config([*self.argv, "--agent-dispatch"], self.api)
        self.assertEqual(config["agentDispatch"], {"enabled": True, "answerTimeoutSeconds": 600})
        _, config, _ = self.config([*self.argv, "--no-agent-dispatch"], self.api)
        self.assertEqual(config["agentDispatch"], {"enabled": False, "answerTimeoutSeconds": 600})

    def test_dispatch_needs_the_api(self):
        with self.assertRaisesRegex(fleet_setup.SetupError, "needs the API on this VM"):
            answers_for([*self.argv, "--agent-dispatch", "--no-api"])

    def test_the_rear_admiral_signs_in_as_its_own_agent_record(self):
        answers, config, secrets = self.config([*self.argv, "--agent-dispatch"], self.api)
        secrets.pop("rearAdmiralApiToken")
        api = fixture.Subspace()
        # Setup registers the officers before the installer runs, whether or not dispatch is on, and leaves
        # dispatch out of that check: its token is what this registration makes possible.
        with fake_api(api), patch.object(fleet_setup, "api_token",
                                         side_effect=lambda _answers, email=None: f"token-for-{email}"):
            records = fleet_setup.register_own("header.payload.admin", config, secrets)
            self.assertEqual(fleet_setup.rear_admiral_token(answers, records),
                             "token-for-atlas@agents.example.com")
        rear = next(row for row in api.agents.values() if row["rank"] == "rearAdmiral")
        self.assertEqual((rear["serviceTokenClientId"], rear["fleetId"], rear["ircNick"]),
                         ("atlas@agents.example.com", fixture.FLEET, "atlas"))
        captains = [row for row in api.agents.values() if row["rank"] == "captain"]
        self.assertEqual([(row["name"], row["fleetId"]) for row in captains], [("orion", fixture.FLEET)])
        rear["fleetId"] = "another-fleet"
        with fake_api(api), self.assertRaisesRegex(fleet_setup.SetupError, "belongs to another fleet"):
            fleet_setup.register_own("header.payload.admin", config, secrets)

    def test_a_rerun_keeps_a_token_with_most_of_its_life_left(self):
        import base64
        import json

        def jwt(exp):
            payload = base64.urlsafe_b64encode(json.dumps({"exp": exp}).encode()).decode().rstrip("=")
            return f"header.{payload}.signature"

        self.assertEqual(fleet_setup.token_seconds_left(jwt(1000), now=400), 600)
        self.assertEqual(fleet_setup.token_seconds_left(jwt(100), now=400), 0)
        self.assertEqual(fleet_setup.token_seconds_left("not-a-jwt"), 0)
        answers, _, _ = answers_for([*self.argv, "--agent-dispatch"])
        records = {"rearAdmiral": {"serviceTokenClientId": "example-fleet-atlas@agents.example.com"}}
        fresh = jwt(int(fleet_setup.time.time()) + 8 * 3600)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.secrets.json", {"rearAdmiralApiToken": fresh})
        with patch.object(fleet_setup, "api_token", return_value="minted") as mint:
            self.assertEqual(fleet_setup.rear_admiral_token(answers, records), fresh)
            mint.assert_not_called()
            fleet_setup.write_json(fleet_setup.INPUTS / "fleet.secrets.json",
                                   {"rearAdmiralApiToken": jwt(int(fleet_setup.time.time()) + 3600)})
            self.assertEqual(fleet_setup.rear_admiral_token(answers, records), "minted")
            mint.assert_called_once_with(answers, "example-fleet-atlas@agents.example.com")

    def test_only_the_gateway_user_may_also_reach_the_api(self):
        answers = {"operator": "operator", "api": True, "agentDispatch": True, "metadataUsers": []}
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "metadata_users", return_value=[]):
            self.assertIn("3000:ss-fleet-gateway", fleet_setup.isolation_args(answers))
            self.assertNotIn("--port-user", fleet_setup.isolation_args({**answers, "agentDispatch": False}))
        with patch.object(fleet_setup.pwd, "getpwnam", side_effect=KeyError), \
                patch.object(fleet_setup, "metadata_users", return_value=[]):
            self.assertNotIn("--port-user", fleet_setup.isolation_args(answers))

    def test_only_the_officers_may_reach_the_agent_listener(self):
        answers = {"operator": "operator", "api": True, "agentDispatch": True, "metadataUsers": []}
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {
            "rearAdmiral": {"id": "atlas"}, "captains": [{"id": "orion"}, {"id": "vega"}]})
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "metadata_users", return_value=[]):
            args = fleet_setup.isolation_args(answers)
        protected = [args[i + 1] for i, arg in enumerate(args) if arg == "--operator-only-port"]
        self.assertEqual(protected, [str(fleet_setup.API_PORT), "5432", str(fleet_setup.AGENT_API_PORT)])
        exceptions = [args[i + 1] for i, arg in enumerate(args) if arg == "--port-user"]
        self.assertEqual(exceptions, ["3000:ss-fleet-gateway", "3001:ss-atlas", "3001:ss-orion", "3001:ss-vega"])
        # Officers not yet created get no exception; setup isolates again after the installer has run.
        with patch.object(fleet_setup.pwd, "getpwnam", side_effect=KeyError), \
                patch.object(fleet_setup, "metadata_users", return_value=[]):
            args = fleet_setup.isolation_args(answers)
        self.assertIn(str(fleet_setup.AGENT_API_PORT), args)
        self.assertNotIn("--port-user", args)

    def test_lockdown_admits_only_the_named_users_to_each_protected_port(self):
        script = Path(fleet_setup.HERE) / "lockdown.sh"
        source = script.read_text()
        functions = re.findall(r"^(?:unique_list|isolation_wanted)\(\).*$", source, re.M)
        render = re.search(r"^render_isolation\(\) \{.*?^\}$", source, re.M | re.S).group(0)
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "isolation.nft"
            program = "\n".join([
                *functions, render,
                "protected=(3000 5432 3001) uids=(0 1000) port_uids=(3000:900 3001:901 3001:902)",
                "metadata_uids=() protect_metadata=false", f"render_isolation {out}",
            ])
            subprocess.run(["bash", "-c", program], check=True)
            rules = [line.strip() for line in out.read_text().splitlines()]
        accepts = [rule for rule in rules if rule.endswith("accept") and "dport" in rule]
        self.assertEqual(accepts, [
            'oifname "lo" tcp dport 3000 meta skuid 900 accept',
            'oifname "lo" tcp dport 3001 meta skuid 901 accept',
            'oifname "lo" tcp dport 3001 meta skuid 902 accept',
        ])
        reject = 'oifname "lo" tcp dport { 3000,5432,3001 } meta skuid != { 0,1000 } reject with tcp reset'
        self.assertIn(reject, rules)
        # The exceptions come before the reject, so they win; container addresses get no exceptions.
        self.assertLess(rules.index(accepts[-1]), rules.index(reject))
        self.assertIn('oifname { "docker0", "br-*" } tcp dport { 3000,5432,3001 } meta skuid != { 0,1000 } '
                      'reject with tcp reset', rules)


class ExternalDatabaseTests(unittest.TestCase):
    """The API on the fleet VM against an existing PostgreSQL (--database-url-file)."""

    PASSWORD = "s3cret$word%23x"
    URL = f"postgresql://tracker:{PASSWORD}@tracker-db.example.org:5432/tracker?sslmode=verify-full"

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        fleet_setup.INPUTS = self.root / "inputs"
        fleet_setup.INPUTS.mkdir(mode=0o700)
        self.owner = patch.object(fleet_setup, "SECRET_OWNER", fleet_setup.os.getuid())
        self.owner.start()
        self.url_file = self.private("database-url", self.URL + "\n")
        self.payload = self.root / "payload"
        (self.payload / "subspace-api").mkdir(parents=True)
        (self.payload / "subspace-api" / "compose.yaml").write_text("services: {}\n")

    def tearDown(self):
        self.owner.stop()
        self.directory.cleanup()

    def private(self, name, text, mode=0o600):
        path = fleet_setup.INPUTS / name
        path.write_text(text)
        path.chmod(mode)
        return str(path)

    def answers(self, *extra, saved=None):
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", *extra])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            answers = fleet_setup.gather(args, saved or {})
            fleet_setup.check_answers(answers)
        return answers, args

    def up(self, answers, migrate=False, statuses=(0,)):
        """api_up with Compose stubbed: the migration check answers with statuses, other commands succeed."""
        calls, results = [], list(statuses)

        def compose(*args, **_kwargs):
            calls.append(args)
            return results.pop(0) if "--check" in args and results else 0
        store = fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None)
        with patch.object(fleet_setup, "PAYLOAD", self.payload), \
                patch.object(fleet_setup, "API_DIR", self.root / "opt" / "api"), \
                patch.object(fleet_setup, "compose", side_effect=compose):
            fleet_setup.api_up(answers, store, migrate)
        return calls

    def test_the_url_file_is_root_only_and_holds_one_server_url(self):
        self.assertEqual(fleet_setup.database_url(self.url_file), self.URL)
        refusals = {
            "world-readable": self.private("loose", self.URL, 0o644),
            "loopback": self.private("loopback", self.URL.replace("tracker-db.example.org", "127.0.0.1")),
            "no database": self.private("nodb", self.URL.replace("/tracker?", "/?")),
            "a quote": self.private("quote", self.URL.replace("s3cret", "it's")),
            "two lines": self.private("lines", self.URL + "\n" + self.URL),
            "another scheme": self.private("scheme", self.URL.replace("postgresql:", "mysql:")),
            "an unknown sslmode": self.private("mode", self.URL.replace("verify-full", "strict")),
            "relative": "database-url",
            "missing": str(fleet_setup.INPUTS / "absent"),
        }
        link = fleet_setup.INPUTS / "link"
        link.symlink_to(self.url_file)
        refusals["a link"] = str(link)
        for reason, path in refusals.items():
            with self.subTest(reason), self.assertRaises(fleet_setup.SetupError) as refused:
                fleet_setup.database_url(path)
            self.assertNotIn(self.PASSWORD, str(refused.exception))
            self.assertNotIn("tracker-db.example.org", str(refused.exception))
        with patch.object(fleet_setup, "SECRET_OWNER", fleet_setup.os.getuid() + 1), \
                self.assertRaisesRegex(fleet_setup.SetupError, "owned by root"):
            fleet_setup.database_url(self.url_file)

    def test_the_setting_combines_only_with_this_vm_s_api_and_is_kept(self):
        answers, args = self.answers("--database-url-file", self.url_file)
        # The path is kept, never the URL; demo data is off without asking.
        self.assertEqual(answers["databaseUrlFile"], self.url_file)
        self.assertFalse(answers["demo"])
        self.assertFalse(args.migrate)
        fleet_setup.save_answers(answers)
        self.assertNotIn(self.PASSWORD, (fleet_setup.INPUTS / "answers.json").read_text())
        later, _ = self.answers(saved=answers)
        self.assertEqual(later["databaseUrlFile"], self.url_file)
        for extra, message in [
            (["--database-url-file", self.url_file, "--no-api"], "--no-api"),
            (["--migrate"], "--migrate is for an existing PostgreSQL"),
            (["--no-api", "--migrate"], "--migrate is for an existing PostgreSQL"),
            (["--database-url-file", self.url_file, "--demo"], "Demo data is fictional"),
            (["--database-ca-file", self.url_file], "pass --database-url-file too"),
        ]:
            with self.subTest(extra), self.assertRaisesRegex(fleet_setup.SetupError, message):
                self.answers(*extra)

    def test_the_api_uses_the_existing_server_without_a_postgresql_container(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        calls = self.up(answers)
        api_env = fleet_setup.INPUTS / "api.env"
        self.assertEqual(api_env.stat().st_mode & 0o777, 0o600)
        environment = api_env.read_text()
        # Single-quoted, so Compose does not expand the $ in the password.
        self.assertIn(f"DATABASE_URL='{self.URL}'\n", environment)
        self.assertNotIn("POSTGRES_PASSWORD", environment)
        self.assertNotIn("postgres", fleet_setup.read_json(fleet_setup.INPUTS / "generated-secrets.json", {}))
        override = (fleet_setup.INPUTS / "api.override.yaml").read_text()
        self.assertIn('  postgres:\n    profiles: ["subspace-local-database"]\n', override)
        self.assertIn("    depends_on: !reset {}\n", override)
        self.assertIn('    command: ["bun", "src/main.ts"]\n', override)
        # The API and its agent listener are published on loopback exactly as with the API's own PostgreSQL.
        self.assertIn('      - "127.0.0.1:3000:3000"\n      - "127.0.0.1:3001:3001"\n', override)
        self.assertNotIn("5432", override)
        self.assertNotIn(self.PASSWORD, override)
        self.assertEqual(calls, [
            ("build", "api"),
            ("run", "--rm", "--no-deps", "-T", "api", "bun", "scripts/migrate.ts", "--check"),
            ("up", "--detach", "--wait", "--wait-timeout", "300", "--build"),
        ])
        self.assertFalse(any(self.PASSWORD in str(arg) for call in calls for arg in call))

    def test_migrations_are_checked_and_applied_only_when_asked(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        migrate = ("run", "--rm", "--no-deps", "-T", "api", "bun", "scripts/migrate.ts")
        with self.assertRaisesRegex(fleet_setup.SetupError, "run setup again with --migrate") as refused:
            self.up(answers, statuses=[fleet_setup.MIGRATIONS_PENDING])
        self.assertNotIn(self.PASSWORD, str(refused.exception))
        self.assertIn(migrate, self.up(answers, migrate=True, statuses=[fleet_setup.MIGRATIONS_PENDING]))
        self.assertNotIn(migrate, self.up(answers, migrate=True, statuses=[0]))
        with self.assertRaisesRegex(fleet_setup.SetupError, "Could not read the existing PostgreSQL"):
            self.up(answers, statuses=[1])

    def test_a_ca_certificate_is_trusted_through_a_read_only_mount(self):
        ca = self.private("ca.pem", "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n", 0o644)
        answers, _ = self.answers("--database-url-file", self.url_file, "--database-ca-file", ca)
        self.up(answers)
        copy = self.root / "opt" / "database-ca.pem"
        self.assertEqual(copy.read_text(), Path(ca).read_text())
        self.assertEqual(copy.stat().st_mode & 0o777, 0o644)
        override = (fleet_setup.INPUTS / "api.override.yaml").read_text()
        self.assertIn(f'      - "{copy}:/etc/subspace/database-ca.pem:ro"\n', override)
        self.assertIn("      NODE_EXTRA_CA_CERTS: /etc/subspace/database-ca.pem\n", override)
        digest = fleet_setup.hashlib.sha256(Path(ca).read_bytes()).hexdigest()
        self.assertIn(f"      SUBSPACE_DATABASE_CA_SHA256: {digest}\n", override)
        # A private key is never taken for a CA certificate, and '' removes the setting.
        key = self.private("key.pem", "-----BEGIN CERTIFICATE-----\n-----BEGIN " + "PRIVATE KEY-----\n")
        with self.assertRaisesRegex(fleet_setup.SetupError, "no private key"):
            self.answers("--database-url-file", self.url_file, "--database-ca-file", key)
        cleared, _ = self.answers("--database-ca-file", "", saved=answers)
        self.assertNotIn("databaseCaFile", cleared)

    def test_sign_in_in_an_existing_database_adds_no_organisation_or_person(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        with patch.object(fleet_setup, "compose", return_value="header.payload.signature") as compose:
            fleet_setup.api_token(answers)
        self.assertEqual(compose.call_args.args[:4], ("exec", "-T", "-e", "DEV_TOKEN_EXISTING_ONLY=true"))
        with patch.object(fleet_setup, "compose", side_effect=fleet_setup.SetupError("exit 4")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "adds no organisation or person"):
            fleet_setup.api_token(answers)
        local, _ = self.answers()
        with patch.object(fleet_setup, "compose", return_value="header.payload.signature") as compose:
            fleet_setup.api_token(local)
        self.assertNotIn("DEV_TOKEN_EXISTING_ONLY=true", compose.call_args.args)
        # A member who is not an administrator is stopped before setup writes anything.
        with patch.object(fleet_setup, "api_call", return_value={"isAdmin": False}) as call, \
                self.assertRaisesRegex(fleet_setup.SetupError, "not an administrator"):
            fleet_setup.existing_administrator(answers, "token")
        call.assert_called_once_with("token", "GET", "/me")
        with patch.object(fleet_setup, "api_call", return_value={"isAdmin": True}):
            fleet_setup.existing_administrator(answers, "token")

    def test_the_lockdown_keeps_the_api_ports_and_drops_the_postgresql_one(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {"rearAdmiral": {"id": "atlas"}})
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "metadata_users", return_value=[]):
            args = fleet_setup.isolation_args(answers)
        protected = [args[i + 1] for i, arg in enumerate(args) if arg == "--operator-only-port"]
        self.assertEqual(protected, [str(fleet_setup.API_PORT), str(fleet_setup.AGENT_API_PORT)])
        self.assertIn(f"{fleet_setup.AGENT_API_PORT}:ss-atlas", args)

    def test_the_plan_says_what_happens_without_the_url(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        plan = "\n".join(fleet_setup.external_plan(answers, migrate=False))
        self.assertIn(self.url_file, plan)
        self.assertIn("sslmode=verify-full", plan)
        self.assertIn("applies them only with --migrate", plan)
        # The installer's dry run that follows talks of IRC sign-in: this line says it means the API's.
        self.assertIn("\nAPI sign-in: setup acts as", plan)
        self.assertNotIn(self.PASSWORD, plan)
        self.assertNotIn("tracker-db.example.org", plan)
        self.assertNotIn("WARNING", plan)
        plain = self.private("plain", self.URL.replace("?sslmode=verify-full", ""))
        answers, _ = self.answers("--database-url-file", plain)
        self.assertIn("WARNING: the connection can be unencrypted",
                      "\n".join(fleet_setup.external_plan(answers, migrate=True)))

    def test_the_dry_run_validates_and_never_prints_the_url(self):
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40,
                "--database-url-file", self.url_file, "--dry-run"]
        output, written, vm_inputs = [], [], fleet_setup.INPUTS

        def dry_installer(config_path, dry_run=False):
            # The installer validates inputs written to a private copy, never to the VM's inputs.
            self.assertTrue(dry_run)
            self.assertFalse(Path(config_path).is_relative_to(vm_inputs))
            written.extend(path.read_text() for path in Path(config_path).parent.iterdir()
                           if path.is_file() and path.name != "database-url")
        before = sorted(path.name for path in fleet_setup.INPUTS.iterdir())
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "installer", side_effect=dry_installer), \
                patch.object(fleet_setup, "database_checks") as checks, \
                patch.object(fleet_setup, "say", side_effect=output.append):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertTrue(written)
        checks.assert_called_once()
        self.assertTrue(any("this VM runs no PostgreSQL container" in line for line in output))
        self.assertNotIn(self.PASSWORD, "\n".join(output) + "".join(written))
        self.assertEqual(sorted(path.name for path in fleet_setup.INPUTS.iterdir()), before)
        Path(self.url_file).chmod(0o640)
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "installer") as installer, \
                self.assertRaisesRegex(fleet_setup.SetupError, "chmod 600"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        installer.assert_not_called()

    def test_a_dry_run_with_another_database_changes_nothing_on_the_vm(self):
        # An installed VM: its answers name one URL file; a dry run tries another.
        answers, _ = self.answers("--database-url-file", self.url_file)
        fleet_setup.save_answers(answers)
        self.up(answers)
        (fleet_setup.INPUTS / "fleet.json").write_text('{"installed": true}\n')
        other = self.private("other-database-url", self.URL.replace("tracker-db", "other-db"))

        def snapshot():
            return {path.name: path.read_bytes() for path in fleet_setup.INPUTS.iterdir() if path.is_file()}
        before = snapshot()
        argv = ["--non-interactive", "--operator", "operator", "--inputs", str(fleet_setup.INPUTS),
                "--stargate-ref", "c" * 40, "--database-url-file", other, "--dry-run"]
        output = []
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "installer"), \
                patch.object(fleet_setup, "database_checks"), \
                patch.object(fleet_setup, "say", side_effect=output.append):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertTrue(any(other in line for line in output))
        self.assertEqual(fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")["databaseUrlFile"], self.url_file)
        self.assertEqual(snapshot(), before)
        self.assertIn("dry run complete; nothing on this VM changed", output)

    def test_the_host_installers_keep_a_dry_run_s_files_apart_from_the_vm_s_inputs(self):
        # Without a test harness for SSH, check the text: in a dry run the host installers send the database
        # URL and the CA files to a scratch directory they remove afterwards, and stop before reading anything back.
        root = Path(fleet_setup.HERE).parents[1]
        cases = (("install.sh", "inputs=/root/subspace-setup-dry-run", 'say "Running subspace-setup --dry-run',
                  ['transient+=("$inputs")', "trap remove_transient EXIT"], 'name=$operator_dir/$option'),
                 ("install.ps1", "'/root/subspace-setup-dry-run'", 'Say "Running subspace-setup --dry-run',
                  ["$transient.Add($inputs)", "} finally {", "'rm', '-rf'"], "$operatorDir/$option"))
        for name, scratch, branch, cleanup, operator_copy in cases:
            text = (root / name).read_text()
            with self.subTest(name):
                self.assertIn(scratch, text)
                for file in ("database-url", "database-ca.pem", "irc-ca-"):
                    self.assertNotIn(f"/root/fleet/{file}", text)
                    self.assertIn(f"$inputs/{file}", text)
                dry = text[text.index(branch):]
                dry = dry[:dry.index("return") + len("return")]
                self.assertNotIn("summary.env", dry)
                self.assertLess(text.index(branch), text.index("summary.env", text.index(branch)))
                # The dry run's installer copy goes in the scratch directory too, never over
                # /opt/subspace/bootstrap, which only the real run replaces.
                self.assertRegex(text, r'(send_payload|Send-Payload) "\$inputs/bootstrap" *;? *\}? *else *\{? *'
                                       r'(send_payload|Send-Payload)')
                self.assertIn("$inputs/bootstrap/subspace-installer/vm/setup.sh", dry)
                # The scratch directory and the operator's password files are removed however the run ends, and
                # the passwords reach the VM only after the installer.
                for part in cleanup:
                    self.assertIn(part, text)
                self.assertLess(text.index('"$inputs/bootstrap"'), text.index(operator_copy))
        # Run from that copy, setup keeps the subspace-setup command pointing at the installed one.
        setup = (root / "subspace-installer/vm/setup.sh").read_text()
        self.assertIn('if ! "$dry_run" && [[ $(readlink /usr/local/sbin/subspace-setup', setup)

    def test_a_run_the_installer_refuses_migrates_nothing(self):
        # A fleet configuration the installer refuses, such as a Slack bot without its tokens, is found before
        # the lockdown, the API or --migrate change anything.
        (fleet_setup.INPUTS / "fleet.secrets").mkdir(mode=0o700)
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40,
                "--database-url-file", self.url_file, "--migrate", "--irc-auth", "password"]
        checks = []

        def refuse(config_path, dry_run=False, secrets_dir=None, quiet=False):
            checks.append((dry_run, secrets_dir, json.loads(Path(config_path).read_text())["slug"]))
            raise fleet_setup.SetupError("Supply slack-app-token in captain orion's secrets directory.")
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                patch.object(fleet_setup, "installer", side_effect=refuse), \
                patch.object(fleet_setup, "run") as run, patch.object(fleet_setup, "api_up") as api_up, \
                patch.object(fleet_setup, "compose") as compose, patch.object(fleet_setup, "irc_source"), \
                patch.object(fleet_setup, "say"), \
                self.assertRaisesRegex(fleet_setup.SetupError, "slack-app-token"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertEqual(checks, [(True, fleet_setup.INPUTS / "fleet.secrets", "example-fleet")])
        run.assert_not_called()
        api_up.assert_not_called()
        compose.assert_not_called()

        # Accepted, the check comes before the lockdown and the API, which then migrates as asked.
        order = []
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                patch.object(fleet_setup, "installer", side_effect=lambda *a, dry_run=False, **k: order.append(
                    "check" if dry_run else "install")), \
                patch.object(fleet_setup, "run", side_effect=lambda *a, **k: order.append(Path(str(a[0])).name)), \
                patch.object(fleet_setup, "api_up", side_effect=lambda *a: order.append(("api", a[2])) or False), \
                patch.object(fleet_setup, "irc_source", return_value="a" * 40), patch.object(fleet_setup, "say"), \
                patch.object(fleet_setup, "summary", side_effect=fleet_setup.SetupError("stop here")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "stop here"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertEqual(order[:4], ["check", "lockdown.sh", "docker.sh", ("api", True)])

    def test_the_dry_run_checks_the_database_read_only_and_says_what_it_found(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        payload = self.root / "payload"
        for name in ("20260101000000_first", "20260102000000_second"):
            (payload / "subspace-api/drizzle" / name).mkdir(parents=True)
            (payload / "subspace-api/drizzle" / name / "migration.sql").write_text("SELECT 1;\n")

        class Fake:
            def __init__(self, url, ca_file=None, applied=(), standing="admin"):
                self.tls, self.closed, self.applied, self.standing = True, False, set(applied), standing

            def close(self):
                self.closed = True

        def check(migrate=False, **fake):
            output, made = [], []

            def connect(url, ca_file=None):
                made.append(Fake(url, ca_file, **fake))
                return made[-1]
            with patch.object(fleet_setup, "PAYLOAD", payload), \
                    patch.object(fleet_setup.postgres_check, "Connection", side_effect=connect), \
                    patch.object(fleet_setup.postgres_check, "applied_migrations", side_effect=lambda c: c.applied), \
                    patch.object(fleet_setup.postgres_check, "administrator", side_effect=lambda c, o, e: c.standing), \
                    patch.object(fleet_setup, "say", side_effect=output.append):
                try:
                    fleet_setup.database_checks(answers, migrate)
                finally:
                    self.assertTrue(made[0].closed)
            return "\n".join(output)
        both = ("20260101000000_first", "20260102000000_second")
        text = check(applied=both)
        self.assertIn("connected read-only to the existing PostgreSQL named in", text)
        self.assertIn("over TLS", text)
        self.assertIn("it has all 2 of this API's migrations", text)
        self.assertIn("developer@example.com in the development organisation local is an administrator", text)
        self.assertIn("lacks 1 of this API's 2 migrations, which the real run applies",
                      check(migrate=True, applied=both[:1]))
        with self.assertRaisesRegex(fleet_setup.SetupError, "lacks 2 of this API's 2 migrations.*add --migrate"):
            check(applied=())
        self.assertIn("no organisation yet", check(migrate=True, applied=()))
        with self.assertRaisesRegex(fleet_setup.SetupError, "is not in the database"):
            check(applied=both, standing="missing")
        with self.assertRaisesRegex(fleet_setup.SetupError, "is not an administrator"):
            check(applied=both, standing="member")
        with patch.object(fleet_setup.postgres_check, "Connection",
                          side_effect=fleet_setup.postgres_check.DatabaseError("cannot reach the server (timeout)")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "could not check the existing PostgreSQL") as refused:
            fleet_setup.database_checks(answers, False)
        self.assertNotIn(self.PASSWORD, str(refused.exception))
        self.assertNotIn("tracker-db.example.org", str(refused.exception))

    def test_the_database_check_speaks_scram_as_rfc_7677_does(self):
        final, server = fleet_setup.postgres_check.scram_proof(
            "pencil", "rOprNGfwEbeRWgbNEkqO",
            "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096",
            "n=user,r=rOprNGfwEbeRWgbNEkqO")
        self.assertEqual(final, "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,"
                                "p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=")
        self.assertEqual(server, "6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=")
        self.assertEqual(fleet_setup.postgres_check.parameters(self.URL), {
            "host": "tracker-db.example.org", "port": 5432, "database": "tracker", "user": "tracker",
            "password": "s3cret$word#x", "sslmode": "verify-full"})

    def test_the_other_new_settings_survive_a_run_against_an_existing_postgresql(self):
        # Slack bots, a captain's sessionRoot and repository notes are added to fleet.json by hand; setup keeps them
        # on a run with --database-url-file, and the installer accepts the result with password IRC sign-in.
        url = "https://github.com/example/project.git"
        slack = {"appId": "AFIXTURE1", "users": ["UFIXTURE1"], "channels": ["CFIXTURE1"]}
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {
            "rearAdmiral": {"id": "atlas", "slack": slack},
            "captains": [{"id": "orion", "sessionRoot": "workspace", "slack": {**slack, "appId": "AFIXTURE2"}}],
            "repositories": [{"id": "project", "url": url, "description": "The example service.",
                              "tracker": "Example issues"}],
        })
        answers, _ = self.answers("--database-url-file", self.url_file, "--repositories", url)
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        api = {"url": f"http://{fleet_setup.LOOPBACK}:{fleet_setup.API_PORT}",
               "initiatives": ["00000000-0000-4000-8000-000000000001"],
               "agentUrl": f"http://{fleet_setup.LOOPBACK}:{fleet_setup.AGENT_API_PORT}"}
        config = fleet_setup.fleet_config(answers, members, "a" * 40, api)
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"], "header.payload.signature", certificates=True)
        cfg = validate(config, secrets, check_files=False)
        rear, orion = cfg["members"]
        self.assertEqual((rear["slack"]["appId"], orion["slack"]["appId"], orion["sessionRoot"]),
                         ("AFIXTURE1", "AFIXTURE2", "workspace"))
        self.assertEqual(cfg["repositoryNotes"]["project"], {"description": "The example service.",
                                                             "tracker": "Example issues"})
        self.assertEqual(cfg["irc"]["auth"], "certificate")
        self.assertTrue(runtime.rear_root(cfg))
        self.assertNotIn(self.PASSWORD, str(config) + str(secrets))

    def test_there_is_no_way_back_to_a_postgresql_on_this_vm(self):
        answers, _ = self.answers("--database-url-file", self.url_file)
        self.up(answers)
        with self.assertRaisesRegex(fleet_setup.SetupError, "no way back to a PostgreSQL on this VM") as refused:
            self.answers("--database-url-file", "", saved=answers)
        self.assertNotIn(self.PASSWORD, str(refused.exception))
        # On a VM that never had the setting, an empty value changes nothing.
        local, _ = self.answers("--database-url-file", "")
        self.assertNotIn("databaseUrlFile", local)
        # Whatever api.env holds, the API's own PostgreSQL never gets an external URL, so the image's automatic
        # migration on start can only reach the database on this VM.
        self.assertIn("DATABASE_URL=", (fleet_setup.INPUTS / "api.env").read_text())
        self.up(local)
        environment = (fleet_setup.INPUTS / "api.env").read_text()
        self.assertNotIn("DATABASE_URL", environment)
        self.assertNotIn(self.PASSWORD, environment)
        self.assertIn("POSTGRES_PASSWORD=", environment)

    def test_an_existing_postgresql_turns_the_api_of_a_no_api_fleet_back_on(self):
        stopped, _ = self.answers("--no-api")
        self.assertFalse(stopped["api"])
        answers, _ = self.answers("--database-url-file", self.url_file, saved=stopped)
        self.assertTrue(answers["api"])
        self.assertTrue(fleet_setup.external_database(answers))
        # Asking for both on one run is still refused rather than one being ignored.
        with self.assertRaisesRegex(fleet_setup.SetupError, "--no-api leaves out"):
            self.answers("--database-url-file", self.url_file, "--no-api", saved=stopped)

    def test_setup_does_not_move_the_api_s_own_postgresql(self):
        local, _ = self.answers()
        fleet_setup.save_answers(local)
        (fleet_setup.INPUTS / "api.env").write_text("POSTGRES_PASSWORD=example\n")
        argv = ["--non-interactive", "--operator", "operator", "--inputs", str(fleet_setup.INPUTS),
                "--database-url-file", self.url_file, "--dry-run"]
        with patch.object(fleet_setup.pwd, "getpwnam"), \
                self.assertRaisesRegex(fleet_setup.SetupError, "does not move"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertNotIn("databaseUrlFile", fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))

    def test_stopping_or_uninstalling_never_touches_the_database(self):
        # Stopping the API (--no-api) removes its containers, never a volume; nothing setup runs drops anything.
        (fleet_setup.INPUTS / "api.env").write_text(f"DATABASE_URL='{self.URL}'\n")
        with patch.object(fleet_setup.shutil, "which", return_value="/usr/bin/docker"), \
                patch.object(fleet_setup, "run", return_value="container"), \
                patch.object(fleet_setup, "compose") as compose:
            fleet_setup.api_down()
        compose.assert_called_once_with("down")
        source = Path(fleet_setup.__file__).read_text()
        self.assertNotRegex(source, r'(?i)\b(drop|truncate) +(table|schema|database|owned|role)\b|delete +from'
                            r'|"down"[^)\n]*"(-v|--volumes)"|volume", *"rm')
        # The fleet installer's uninstall stops fleet units and removes its programs; it never runs Docker
        # Compose, psql or anything with the API's database.
        installer = (Path(fleet_setup.HERE).parent / "install.sh").read_text()
        uninstall = re.search(r"^uninstall\(\) \{.*?^\}$", installer, re.M | re.S).group(0)
        self.assertNotRegex(uninstall, r"(?i)docker|compose|psql|postgres|database|/root/fleet")


class FakeIrcServer:
    """A TLS IRC server with an operator, NickServ's SAREGISTER and CERT ADD, and SASL EXTERNAL by certificate."""

    def __init__(self, work, client_bundles, operator=("ircop", "fixture-operator-password")):
        self.accounts, self.operator, self.requests = {}, operator, []
        # An account whose SAREGISTER the server answers, then drops the connection before anything else.
        self.drop_after = None
        # Ergo's require-sasl: only connections signed in to an account (passwords here) get past registration.
        self.require_sasl, self.passwords = False, {}
        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
                        "-nodes", "-days", "1", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost",
                        "-keyout", str(work / "server.key"), "-out", str(work / "server.pem")],
                       check=True, capture_output=True)
        self.ca = str(work / "server.pem")
        clients = work / "clients.pem"
        clients.write_text("".join(bundle[:bundle.index("-----END CERTIFICATE-----") + 26] for bundle in client_bundles))
        self.context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.context.load_cert_chain(work / "server.pem", work / "server.key")
        self.context.verify_mode = ssl.CERT_OPTIONAL
        self.context.load_verify_locations(clients)
        self.listener = socket.create_server(("127.0.0.1", 0))
        self.port = self.listener.getsockname()[1]
        threading.Thread(target=self.serve, daemon=True).start()

    def serve(self):
        while True:
            try:
                raw, _ = self.listener.accept()
            except OSError:
                return
            threading.Thread(target=self.session, args=(raw,), daemon=True).start()

    def session(self, raw):
        try:
            connection = self.context.wrap_socket(raw, server_side=True)
        except (ssl.SSLError, OSError):
            raw.close()
            return
        peer = connection.getpeercert(binary_form=True)
        fingerprint = hashlib.sha256(peer).hexdigest() if peer else None
        stream, oper = connection.makefile("rwb"), False
        state = {"capabilities": False, "user": False, "account": None, "mechanism": None}

        def send(line):
            stream.write(line.encode() + b"\r\n")
            stream.flush()

        def welcome():
            if self.require_sasl and not state["account"]:
                send("FAIL * ACCOUNT_REQUIRED :You must log in with SASL to join this server")
                return False
            send(":irc.example.test 001 you :Welcome")
            return True
        for raw_line in stream:
            line = raw_line.decode().rstrip("\r\n")
            words = line.split(" ")
            command = words[0].upper()
            if command == "USER":
                state["user"] = True
                if not state["capabilities"] and not welcome():
                    break
            elif line == "CAP END":
                state["capabilities"] = False
                if state["user"] and not welcome():
                    break
            elif line == "AUTHENTICATE PLAIN":
                state["mechanism"] = "PLAIN"
                send("AUTHENTICATE +")
            elif command == "AUTHENTICATE" and state["mechanism"] == "PLAIN":
                _, name, secret = base64.b64decode(words[1]).decode().split("\0")
                if self.passwords.get(name) == secret:
                    state["account"] = name
                    send(f":irc.example.test 900 you you!u@h {name} :You are now logged in")
                    send(":irc.example.test 903 you :SASL authentication successful")
                else:
                    send(":irc.example.test 904 you :SASL authentication failed")
            elif command == "OPER":
                oper = tuple(words[1:3]) == self.operator
                send(":irc.example.test 381 you :You are now an operator" if oper else
                     ":irc.example.test 464 you :Password incorrect")
            elif command == "PING":
                send(f":irc.example.test PONG irc.example.test :{words[1].lstrip(':')}")
            elif command == "PRIVMSG" and words[1] == "NickServ" and line.split(" :", 1)[1].split()[0] == "INFO":
                # Anyone may ask; Ergo answers with several lines for a registered account.
                name = line.split(" :", 1)[1].split()[1]
                self.requests.append(["INFO", name])
                if name.lower() in {account.lower() for account in self.accounts}:
                    send(f":NickServ!NickServ@services NOTICE you :Account: {name}")
                    send(":NickServ!NickServ@services NOTICE you :Registered at: 2026-01-01")
                else:
                    send(":NickServ!NickServ@services NOTICE you :Account does not exist")
            elif command == "PRIVMSG" and words[1] == "NickServ":
                request = line.split(" :", 1)[1].split()
                self.requests.append(request)
                if not oper:
                    send(":NickServ!NickServ@services NOTICE you :Insufficient privileges")
                elif request[0] == "SAREGISTER" and request[1] in self.accounts:
                    send(":NickServ!NickServ@services NOTICE you :Name reserved due to a prior registration")
                elif request[0] == "SAREGISTER":
                    send(f":NickServ!NickServ@services NOTICE you :Successfully registered account {request[1]}")
                    self.accounts[request[1]] = set()
                    if request[1] == self.drop_after:
                        self.drop_after = None
                        break
                elif request[:2] == ["CERT", "ADD"] and request[2] in self.accounts:
                    self.accounts[request[2]].add(request[3])
                    send(":NickServ!NickServ@services NOTICE you :Certificate fingerprint successfully added")
                else:
                    send(":NickServ!NickServ@services NOTICE you :No such account")
            elif line == "CAP REQ :sasl":
                state["capabilities"] = True
                send(":irc.example.test CAP * ACK :sasl")
            elif line == "AUTHENTICATE EXTERNAL":
                send("AUTHENTICATE +")
            elif line == "AUTHENTICATE +":
                account = next((name for name, prints in self.accounts.items() if fingerprint in prints), None)
                state["account"] = account or state["account"]
                send(f":irc.example.test 900 you you!u@h {account} :You are now logged in" if account else
                     ":irc.example.test 904 you :SASL authentication failed")
            elif command == "QUIT":
                break
        connection.close()


class IrcCertificateSetupTests(unittest.TestCase):
    """A remote IRC server: officers sign in by certificate unless told otherwise, and setup can bind them."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        fleet_setup.INPUTS = self.root / "inputs"
        fleet_setup.INPUTS.mkdir(mode=0o700)
        owner = patch.object(fleet_setup, "SECRET_OWNER", os.getuid())
        owner.start()
        self.addCleanup(owner.stop)
        # No installed fleet unless a test writes one.
        installed = patch.object(fleet_setup, "FLEET_ETC", self.root / "etc")
        installed.start()
        self.addCleanup(installed.stop)

    def answers(self, *extra, saved=None):
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", "--irc", "remote",
                                       "--irc-host", "irc.example.com", *extra])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            answers = fleet_setup.gather(args, saved or {})
            fleet_setup.check_answers(answers)
        return answers, args

    def test_managed_shared_irc_bundle_carries_fingerprints(self):
        answers, _ = self.answers("--managed-shared-irc")
        self.assertTrue(answers["managedSharedIrc"])
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        fingerprints = {person["account"]: (None, str(index) * 64) for index, person in enumerate(members)}
        with patch.object(fleet_setup.Secrets, "password", side_effect=AssertionError("unexpected password")):
            bundle = fleet_setup.irc_bundle(
                answers, members, fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), fingerprints)
        self.assertEqual(bundle["accounts"],
                         [{"name": name, "fingerprint": fingerprint} for name, (_, fingerprint) in fingerprints.items()])
        self.assertEqual(fleet_setup.read_json(fleet_setup.INPUTS / "irc-bundle.json"), bundle)

    def test_a_remote_server_signs_officers_in_by_certificate_unless_told_otherwise(self):
        answers, _ = self.answers()
        self.assertEqual(answers["ircAuth"], "certificate")
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        config = fleet_setup.fleet_config(answers, members, "a" * 40)
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"], local_irc=False, certificates=True)
        self.assertEqual(config["irc"]["auth"], "certificate")
        self.assertEqual(secrets["ircPasswords"], {})
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual((cfg["irc"]["auth"], cfg["passwords"]), ("certificate", {}))
        # Passwords stay an explicit choice and are kept; a remote fleet set up before stays on passwords.
        self.assertEqual(self.answers("--irc-auth", "password")[0]["ircAuth"], "password")
        self.assertEqual(self.answers(saved={"irc": "remote", "slug": "example-fleet"})[0]["ircAuth"], "password")
        self.assertEqual(self.answers(saved={**answers})[0]["ircAuth"], "certificate")
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", "--irc", "local",
                                       "--irc-auth", "certificate"])
        self.assertEqual(fleet_setup.gather(args, {})["ircAuth"], "certificate")
        with self.assertRaisesRegex(fleet_setup.SetupError, "retain its IRC sign-in mode"):
            fleet_setup.gather(args, {"irc": "local"})
        for extra, message in ((["--irc-operator", "ircop"], "together"),
                               (["--irc-auth", "password", "--irc-operator", "ircop",
                                 "--irc-operator-password-file", "/x"], "for certificate sign-in")):
            with self.subTest(extra), self.assertRaisesRegex(fleet_setup.SetupError, message):
                self.answers(*extra)

    def test_the_installed_fleets_sign_in_and_certificates_are_kept(self):
        # A remote fleet that setup set up with passwords, moved to certificates with the fleet installer alone.
        etc = self.root / "etc"
        (etc / "irc-certs").mkdir(parents=True)
        (etc / "fleet.json").write_text(json.dumps({"irc": {"mode": "remote", "auth": "certificate"}}))
        bundle = runtime.new_irc_bundle("example-fleet-orion")
        (etc / "irc-certs/orion.pem").write_text(bundle)
        saved = {"irc": "remote", "slug": "example-fleet", "ircAuth": "password"}
        answers, _ = self.answers(saved=saved)
        self.assertEqual(answers["ircAuth"], "certificate")
        self.assertEqual(self.answers("--irc-auth", "password", saved=saved)[0]["ircAuth"], "password")
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        fingerprint = runtime.irc_fingerprint(bundle, "orion")
        # The dry run reads the installed certificate; the real run keeps it, so its bound fingerprint holds.
        planned = fleet_setup.officer_certificates(members, generate=False)
        self.assertEqual(planned["example-fleet-orion"][1], fingerprint)
        self.assertIsNone(planned["example-fleet-atlas"][1])
        made = fleet_setup.officer_certificates(members, generate=True)
        self.assertEqual(made["example-fleet-orion"][0].read_text(), bundle)
        self.assertNotEqual(made["example-fleet-atlas"][1], None)
        # Without an installed method, an older remote fleet keeps passwords and a new one gets certificates.
        (etc / "fleet.json").unlink()
        self.assertEqual(self.answers(saved=saved)[0]["ircAuth"], "password")
        self.assertEqual(self.answers()[0]["ircAuth"], "certificate")

    def test_certificates_live_in_the_secrets_directory_and_the_lines_bind_them(self):
        answers, _ = self.answers()
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        # A dry run generates nothing and says the real run will.
        planned = fleet_setup.officer_certificates(members, generate=False)
        self.assertEqual({fingerprint for _, fingerprint in planned.values()}, {None})
        self.assertFalse((fleet_setup.INPUTS / "fleet.secrets").exists())
        self.assertIn("the real run generates its certificate", "\n".join(fleet_setup.certificate_lines(answers, planned)))
        made = fleet_setup.officer_certificates(members, generate=True)
        self.assertEqual(fleet_setup.officer_certificates(members, generate=True), made)
        lines = "\n".join(fleet_setup.certificate_lines(answers, made))
        for person in members:
            path, fingerprint = made[person["account"]]
            self.assertEqual(path, fleet_setup.INPUTS / "fleet.secrets" / person["id"] / "irc-client.pem")
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(path.parent.stat().st_mode & 0o777, 0o700)
            self.assertIn(f"  {person['account']}: SHA-256 {fingerprint}", lines)
            self.assertIn(f"/NS SAREGISTER {person['account']}\n", lines)
            self.assertIn(f"/NS CERT ADD {person['account']} {fingerprint}", lines)
        self.assertNotIn("PRIVATE", lines)
        # The fleet installer takes them from there as supplied bundles.
        config = fleet_setup.fleet_config(answers, members, "a" * 40)
        cfg = validate(config, {"proxyKey": "fixture-model-proxy-key"}, check_files=False)
        self.assertEqual(set(runtime.irc_bundles(cfg, fleet_setup.INPUTS / "fleet.secrets")), {"atlas", "orion"})

    def test_the_operator_password_is_read_from_a_root_only_file_and_never_shown(self):
        secret = "operator-secret-value"
        path = fleet_setup.INPUTS / "operator"
        path.write_text(secret + "\n")
        path.chmod(0o600)
        _, args = self.answers("--irc-operator", "ircop", "--irc-operator-password-file", str(path))
        self.assertEqual(fleet_setup.irc_operator(args), ("ircop", secret, None))
        account = fleet_setup.INPUTS / "account"
        account.write_text("account-secret-value\n")
        account.chmod(0o600)
        _, args = self.answers("--irc-operator", "ircop", "--irc-operator-password-file", str(path),
                               "--irc-operator-account", "opsuser", "--irc-operator-account-password-file", str(account))
        self.assertEqual(fleet_setup.irc_operator(args), ("ircop", secret, ("opsuser", "account-secret-value")))
        account.chmod(0o640)
        with self.assertRaisesRegex(fleet_setup.SetupError, "IRC account password file .* mode 0600") as refused:
            fleet_setup.irc_operator(args)
        self.assertNotIn("account-secret-value", str(refused.exception))
        account.chmod(0o600)
        for extra, message in ((["--irc-operator-account", "opsuser"], "together"),
                               (["--irc-operator-account", "opsuser", "--irc-operator-account-password-file",
                                 str(account)], "pass --irc-operator too")):
            with self.subTest(extra), self.assertRaisesRegex(fleet_setup.SetupError, message):
                self.answers(*extra)
        path.chmod(0o644)
        with self.assertRaisesRegex(fleet_setup.SetupError, "mode 0600") as refused:
            fleet_setup.irc_operator(args)
        self.assertNotIn(secret, str(refused.exception))
        path.chmod(0o600)
        _, args = self.answers("--irc-operator", "ircop", "--irc-operator-password-file", "operator")
        with self.assertRaisesRegex(fleet_setup.SetupError, "absolute path"):
            fleet_setup.irc_operator(args)
        # setup keeps neither in its answers.
        answers, _ = self.answers("--irc-operator", "ircop", "--irc-operator-password-file", str(path))
        self.assertFalse({"ircOperator", "ircOperatorPasswordFile"} & answers.keys())
        self.assertNotIn(secret, json.dumps(answers))

    def test_names_taken_on_the_irc_server_stop_setup_before_it_changes_anything(self):
        answers, _ = self.answers()
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        certificates = fleet_setup.officer_certificates(members, generate=True)
        server = FakeIrcServer(self.root, [path.read_text() for path, _ in certificates.values()])
        self.addCleanup(server.listener.close)
        answers.update(ircHost="localhost", ircPort=server.port, ircCa=server.ca)
        # orion is someone else's account; atlas is the officer's own, as its certificate signs in to it.
        server.accounts = {"orion": {"0" * 64}, "atlas": {certificates["atlas"][1]}}
        with patch.object(fleet_setup, "say"):
            problems = fleet_setup.irc_name_problems(answers, members, certificates, None)
        self.assertEqual(problems, ["orion is taken on localhost: NickServ knows it as account orion, which the "
                                    "officer's certificate does not sign in to"])
        self.assertIn(["INFO", "orion"], server.requests)
        server.accounts.pop("orion")
        with patch.object(fleet_setup, "say"):
            self.assertEqual(fleet_setup.irc_name_problems(answers, members, certificates, None), [])
        # A server that admits only signed-in connections is asked through an officer certificate that signs in.
        server.require_sasl = True
        server.accounts["orion"] = {"1" * 64}
        with patch.object(fleet_setup, "say"):
            self.assertEqual(len(fleet_setup.irc_name_problems(answers, members, certificates, None)), 1)

    def installed_prefixed(self, server=None):
        """A fleet subspace-setup installed with fleet-prefixed names on a remote server with certificates."""
        answers, _ = self.answers(saved={"slug": "example-fleet"})
        self.assertEqual(answers["officerNames"], "prefixed")
        if server:
            answers.update(ircHost="localhost", ircPort=server.port, ircCa=server.ca)
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        self.assertEqual([person["account"] for person in members], ["example-fleet-atlas", "example-fleet-orion"])
        certificates = fleet_setup.officer_certificates(members, generate=True)
        config = fleet_setup.fleet_config(answers, members, "a" * 40)
        self.assertEqual(config["officerNames"], "prefixed")
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        fleet_setup.save_answers(answers)
        return answers, members, certificates

    def test_an_installed_fleet_keeps_its_prefixed_names_on_a_rerun(self):
        answers, members, _ = self.installed_prefixed()
        again, _ = self.answers(saved=fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))
        self.assertEqual(again["officerNames"], "prefixed")
        self.assertEqual(fleet_setup.fleet_channels(again, members),
                         ["#example-fleet-starbase", "#example-fleet-command", "#example-fleet-orion"])

    def test_rename_officers_moves_a_prefixed_fleet_to_plain_names(self):
        answers, members, certificates = self.installed_prefixed()
        # The new certificates the rename makes, made here so that the test server trusts them.
        staged = {}
        for agent in ("atlas", "orion"):
            staged[agent] = fleet_setup.INPUTS / "rename" / agent / "irc-client.pem"
            staged[agent].parent.mkdir(mode=0o700, parents=True)
            fleet_setup.write_private(staged[agent], runtime.new_irc_bundle(agent))
        server = FakeIrcServer(self.root, [path.read_text() for path, _ in certificates.values()]
                               + [path.read_text() for path in staged.values()])
        self.addCleanup(server.listener.close)
        answers.update(ircHost="localhost", ircPort=server.port, ircCa=server.ca)
        fleet_setup.save_answers(answers)
        server.accounts = {account: {fingerprint} for account, (_, fingerprint) in certificates.items()}
        # The API on this VM, with the fleet registered under its prefixed names.
        (fleet_setup.INPUTS / "api.env").write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\n")
        api = fixture.Subspace()
        config = {**fleet_setup.read_json(fleet_setup.INPUTS / "fleet.json"),
                  "api": {"url": "http://127.0.0.1:3000", "initiatives": []}}
        cfg = validate(config, {"proxyKey": "fixture-model-proxy-key", "apiToken": "a.b.c"}, check_files=False)
        fleet_records.register(api.call, fleet_records.fleet_fields(cfg), fleet_records.roster(cfg), lambda _: None)
        ids = sorted(api.agents)
        output = []
        # The fleet's answers are checked as setup reads them, with the test server's name standing in for a DNS one.
        check_answers = fleet_setup.check_answers
        patches = [patch.object(fleet_setup, "say", side_effect=output.append), fake_api(api),
                   patch.object(fleet_setup.os, "geteuid", return_value=0),
                   patch.object(fleet_setup, "api_token", return_value="admin"),
                   patch.object(fleet_setup, "api_healthy", return_value=True),
                   patch.object(fleet_setup.pwd, "getpwnam"), fleet_installer(self.root),
                   patch.object(fleet_setup, "check_answers",
                                side_effect=lambda answers: check_answers({**answers, "ircHost": "irc.example.com"}))]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)
        # The dry run checks the new names on the IRC server and in the API and changes nothing. A server that
        # admits only signed-in connections is asked through the officers' current certificates.
        server.require_sasl = True
        fleet_setup.rename_officers(["--dry-run", "--inputs", str(fleet_setup.INPUTS)])
        self.assertIn("dry run: the names are free", "\n".join(output))
        self.assertIn(["INFO", "orion"], server.requests)
        server.require_sasl = False
        self.assertEqual(sorted(server.accounts), ["example-fleet-atlas", "example-fleet-orion"])
        self.assertEqual(sorted(path.name for path in (fleet_setup.INPUTS / "rename").rglob("*.pem")),
                         ["irc-client.pem", "irc-client.pem"])
        # Without an operator, the real run makes the new certificates, prints the lines that bind them and stops.
        with self.assertRaisesRegex(fleet_setup.SetupError, "not registered yet.*keep signing in to their old"):
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS)])
        self.assertTrue(any(line.strip().startswith("/NS CERT ADD orion ") for line in output))
        self.assertEqual(fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")["officerNames"], "prefixed")
        self.assertEqual({row["ircNick"] for row in api.agents.values()}, {"example-fleet-atlas", "example-fleet-orion"})
        # With an operator it registers them. The connection drops after atlas's SAREGISTER, before its CERT ADD:
        # the run stops, and the next one recognises the account it made and only binds the certificate.
        operator = self.root / "operator-password"
        operator.write_text("fixture-operator-password\n")
        operator.chmod(0o600)
        rename = ["--inputs", str(fleet_setup.INPUTS), "--irc-operator", "ircop", "--irc-operator-password-file",
                  str(operator)]
        fingerprints = {agent: runtime.irc_fingerprint(path.read_text(), agent) for agent, path in staged.items()}
        server.drop_after = "atlas"
        with patch.object(fleet_setup, "configure") as configure, \
                self.assertRaisesRegex(fleet_setup.SetupError, "closed the connection"):
            fleet_setup.rename_officers(rename)
        configure.assert_not_called()
        self.assertEqual(server.accounts["atlas"], set())
        self.assertEqual(fleet_setup.registered_accounts(), {"atlas": fingerprints["atlas"]})
        # Setup itself then stops part-way through the switch to the new names.
        output.clear()
        with patch.object(fleet_setup, "configure", side_effect=fleet_setup.SetupError("setup stopped")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "setup stopped"):
            fleet_setup.rename_officers(rename)
        self.assertIn(f"/NS CERT ADD atlas {fingerprints['atlas']}", "\n".join(output))
        self.assertEqual(server.requests.count(["SAREGISTER", "atlas"]), 1)
        self.assertEqual(server.requests.count(["CERT", "ADD", "atlas", fingerprints["atlas"]]), 1)
        self.assertEqual(fleet_setup.registered_accounts(), {})
        self.assertEqual(sorted(server.accounts), ["atlas", "example-fleet-atlas", "example-fleet-orion", "orion"])
        self.assertEqual(server.accounts["example-fleet-orion"], {certificates["example-fleet-orion"][1]})
        # The API's records are renamed in place: the same IDs, account subjects kept, the ship's channel moved.
        self.assertEqual(sorted(api.agents), ids)
        self.assertEqual(api.summary()["agents"], [("atlas", "rearAdmiral", "atlas", None, True),
                                                   ("orion", "captain", "orion", "Orion", True)])
        self.assertEqual(api.summary()["ships"], [("Orion", "#orion")])
        saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
        self.assertEqual(saved["officerNames"], "plain")
        self.assertEqual(saved["officerRename"]["from"], {"atlas": "example-fleet-atlas", "orion": "example-fleet-orion"})
        self.assertNotIn("switched", saved["officerRename"])
        self.assertEqual(saved["officerRename"]["subjects"]["orion"], "example-fleet-orion@agents.example.com")
        renamed = fleet_setup.roster(saved, "atlas", ["orion"])
        self.assertEqual([(person["account"], person["serviceTokenClientId"]) for person in renamed],
                         [("atlas", "example-fleet-atlas@agents.example.com"),
                          ("orion", "example-fleet-orion@agents.example.com")])
        # The new certificate is the fleet's, and the old one is kept beside it.
        for agent in ("atlas", "orion"):
            self.assertEqual((fleet_setup.INPUTS / "fleet.secrets" / agent / "irc-client.pem").read_text(),
                             (fleet_setup.INPUTS / "rename" / agent / "irc-client.pem").read_text())
            self.assertTrue((fleet_setup.INPUTS / "rename" / agent / "irc-client.previous.pem").is_file())
        self.assertNotIn("/NS ERASE", "\n".join(output))
        # The next run resumes at the switch. While the installed certificates do not sign in as the new accounts,
        # the rename is not finished, and nothing says to drop the old ones.
        output.clear()
        with patch.object(fleet_setup, "configure") as configure, \
                self.assertRaisesRegex(fleet_setup.SetupError, "atlas, orion does not sign in as the new account.*"
                                       "not finished"):
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS)])
        configure.assert_called_once()
        self.assertIn("have their plain names on IRC and in the API, but setup has not yet finished",
                      "\n".join(output))
        self.assertNotIn("/NS ERASE", "\n".join(output))
        self.assertNotIn("switched", fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")["officerRename"])
        # Once setup has installed the new certificates, the rename is done and says what is left.

        def install(_args):
            (fleet_setup.FLEET_ETC / "irc-certs").mkdir(parents=True, exist_ok=True)
            for agent in ("atlas", "orion"):
                shutil.copy(fleet_setup.INPUTS / "fleet.secrets" / agent / "irc-client.pem",
                            fleet_setup.FLEET_ETC / "irc-certs" / f"{agent}.pem")
        output.clear()
        fleet_setup.rename_officers(["--dry-run", "--inputs", str(fleet_setup.INPUTS)])
        self.assertIn("dry run: the real run runs setup again with the new names", "\n".join(output))
        with patch.object(fleet_setup, "configure", side_effect=install) as configure:
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS)])
        configure.assert_called_once()
        self.assertTrue(fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")["officerRename"]["switched"])
        cleanup = "\n".join(output)
        self.assertIn("atlas: the installed certificate signs in as atlas", cleanup)
        self.assertIn("/NS ERASE example-fleet-atlas; /NS ERASE example-fleet-orion", cleanup)
        self.assertIn("IRC_AUTH_LOCAL_ACCOUNTS), add atlas orion", cleanup)
        self.assertIn("old ship channels (#example-fleet-orion)", cleanup)
        # Run again, it says the fleet is renamed and repeats what is left.
        output.clear()
        with patch.object(fleet_setup, "configure") as configure:
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS)])
        configure.assert_not_called()
        self.assertIn("already have plain names (renamed from example-fleet-atlas, example-fleet-orion)",
                      "\n".join(output))

    def test_the_rename_checks_the_plain_names_before_it_changes_anything(self):
        # A captain whose fleet-prefixed account was fine, but whose plain name the IRC server keeps for itself.
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Global", "--irc", "remote",
                                       "--irc-host", "irc.example.com"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            answers = fleet_setup.gather(args, {"slug": "example-fleet"})
            fleet_setup.check_answers(answers)
        members = fleet_setup.roster(answers, "atlas", ["global"])
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", fleet_setup.fleet_config(answers, members, "a" * 40))
        fleet_setup.save_answers({**answers, "api": False})
        before = {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}
        with patch.object(fleet_setup, "say"), patch.object(fleet_setup.pwd, "getpwnam"), \
                patch.object(fleet_setup.os, "geteuid", return_value=0), \
                patch.object(fleet_setup, "irc_name_problems") as irc, \
                self.assertRaisesRegex(fleet_setup.SetupError, "global is a name the IRC server"):
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS), "--no-api-records"])
        irc.assert_not_called()
        self.assertEqual({path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}, before)

    def test_a_fleet_whose_records_are_in_another_fleets_api_is_not_renamed(self):
        # Installed with --api-target: the host keeps the API target, and this VM has no API of its own.
        answers, _, _ = self.installed_prefixed()
        fleet_setup.save_answers({**answers, "api": False})
        before = {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}
        for dry in ([], ["--dry-run"]):
            with self.subTest(dry=bool(dry)), patch.object(fleet_setup, "say"), \
                    patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                    patch.object(fleet_setup, "irc_name_problems") as irc, \
                    self.assertRaisesRegex(fleet_setup.SetupError, "no Subspace API on this VM and no hosted API.*"
                                           "--api-target.*keep this fleet's fleet-prefixed names.*--no-api-records"):
                fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS), *dry])
            irc.assert_not_called()
            self.assertEqual({path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}, before)
        # A fleet that no API keeps records of says so, and its IRC names are renamed.
        with patch.object(fleet_setup, "say") as say, patch.object(fleet_setup.pwd, "getpwnam"), \
                patch.object(fleet_setup, "irc_name_problems", return_value=[]), fleet_installer(self.root):
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS), "--dry-run", "--no-api-records"])
        self.assertIn("dry run: the names are free", "\n".join(call.args[0] for call in say.call_args_list))

    def test_installed_crew_are_named_only_where_stargate_allows(self):
        self.installed_prefixed()
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {
            **fleet_setup.read_json(fleet_setup.INPUTS / "fleet.json"),
            "repositories": [{"id": "project", "url": "https://github.com/example/project.git"}]})
        with patch.object(fleet_setup, "say"), patch.object(fleet_setup, "installed_stargate_source", return_value=None), \
                self.assertRaisesRegex(fleet_setup.SetupError, "crew plugin requires a crew member's name to match "
                                       "its ID.*orion claude-1.*Nothing was changed"):
            fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS), "--dry-run",
                                         "--crew-names", "Orion=Rigel,Deneb,Mira,Sirius"])
        with patch.object(fleet_setup, "crew_names_supported", return_value=True):
            crew = fleet_setup.rename_crew(fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"),
                                           "Orion=Rigel,Deneb,Mira,Sirius")
        # Each crew member keeps its installed ID, so its sessions, worktrees and queued tasks stay.
        self.assertEqual(crew["crewIds"], {"orion": {"claude-1": "claude-1", "claude-2": "claude-2",
                                                     "codex-1": "codex-1", "codex-2": "codex-2"}})
        self.assertEqual(crew["crewNames"]["orion"]["claude-1"], "Rigel")

    def test_with_an_operator_setup_registers_binds_and_checks_each_certificate(self):
        answers, _ = self.answers()
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        certificates = fleet_setup.officer_certificates(members, generate=True)
        server = FakeIrcServer(self.root, [path.read_text() for path, _ in certificates.values()])
        self.addCleanup(server.listener.close)
        answers.update(ircHost="localhost", ircPort=server.port, ircCa=server.ca)
        output = []
        operator = ("ircop", "fixture-operator-password", None)
        with patch.object(fleet_setup, "say", side_effect=output.append):
            # A dry run only checks the operator and reports that no certificate signs in yet.
            fleet_setup.check_certificates(answers, members, certificates, operator, register=False)
            self.assertEqual(server.accounts, {})
            fleet_setup.check_certificates(answers, members, certificates, operator, register=True)
        accounts = [person["account"] for person in members]
        self.assertEqual(server.accounts, {account: {certificates[account][1]} for account in accounts})
        self.assertEqual(server.requests, [request for account in accounts for request in
                                           (["SAREGISTER", account], ["CERT", "ADD", account, certificates[account][1]])])
        self.assertIn(f"{accounts[0]}: its certificate does not sign in yet", output)
        self.assertIn(f"{accounts[1]}: its certificate signs in", output)
        self.assertNotIn(operator[1], "\n".join(output))
        self.assertEqual(irc_accounts.check_login("localhost", server.port, server.ca,
                                                  str(certificates[accounts[1]][0])), accounts[1])
        # A rerun finds every certificate signing in and asks NickServ for nothing.
        requests = len(server.requests)
        with patch.object(fleet_setup, "say"):
            fleet_setup.check_certificates(answers, members, certificates, operator, register=True)
        self.assertEqual(len(server.requests), requests)
        # An account that already exists without this officer's certificate is never changed: the run stops.
        server.accounts[accounts[0]] = {"0" * 64}
        with patch.object(fleet_setup, "say"), \
                self.assertRaisesRegex(fleet_setup.SetupError, f"did not register {accounts[0]} .*Name reserved.*"
                                       "never changes an existing account's certificates"):
            fleet_setup.check_certificates(answers, members, certificates, operator, register=True)
        self.assertEqual(server.accounts[accounts[0]], {"0" * 64})
        self.assertEqual(server.requests[requests:], [["SAREGISTER", accounts[0]]])
        server.accounts[accounts[0]] = {certificates[accounts[0]][1]}
        # A wrong operator password is refused without repeating it.
        with self.assertRaisesRegex(fleet_setup.SetupError, "refused the operator credential for ircop") as refused:
            fleet_setup.check_certificates(answers, members, certificates, ("ircop", "wrong-password", None),
                                           register=True)
        self.assertNotIn("wrong-password", str(refused.exception))
        # A server that admits only signed-in connections: without an account the run stops, naming the options,
        # and with one the operator's connection signs in first, then opers up.
        server.require_sasl, server.passwords, server.accounts = True, {"opsuser": "fixture-account-password"}, {}
        with self.assertRaisesRegex(fleet_setup.SetupError, "--irc-operator-account and "
                                    "--irc-operator-account-password-file"):
            fleet_setup.check_certificates(answers, members, certificates, operator, register=True)
        with self.assertRaisesRegex(fleet_setup.SetupError, "refused the sign-in of the account opsuser"):
            fleet_setup.check_certificates(answers, members, certificates,
                                           (*operator[:2], ("opsuser", "wrong")), register=True)
        with patch.object(fleet_setup, "say"):
            fleet_setup.check_certificates(answers, members, certificates,
                                           (*operator[:2], ("opsuser", "fixture-account-password")), register=True)
        self.assertEqual(server.accounts, {account: {certificates[account][1]} for account in accounts})
        # A certificate the server does not accept after registration stops the run.
        server.operator = ("ircop", "other")
        with patch.object(fleet_setup, "say"), \
                patch.object(irc_accounts, "register", return_value={}), \
                patch.object(irc_accounts, "check_login", return_value=None), \
                self.assertRaisesRegex(fleet_setup.SetupError, "cannot sign in to localhost"):
            fleet_setup.check_certificates(answers, members, certificates, operator, register=True)


class HostedApi(BaseHTTPRequestHandler):
    """A hosted Subspace API's health route and enrolment route, as the merged API answers them."""

    token = "senr1." + "B" * 64

    def reply(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    redirect = None  # A Location to answer every request with, as a misconfigured proxy might.

    def do_GET(self):
        if self.redirect:
            return self.moved()
        self.reply(200, {"status": "ok"}) if self.path == "/health" else self.reply(404, {})

    def moved(self):
        self.send_response(302)
        self.send_header("Location", self.redirect)
        self.send_header("Content-Length", "0")
        self.end_headers()

    # The enrolment check's answer by agent name, or None for an API that predates the route.
    checks = None

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if self.path == "/api/v1/agents/enrol/check":
            if self.checks is None:
                return self.reply(404, {"detail": "Not found."})
            if self.headers.get("Authorization") != f"Bearer {self.token}":
                return self.reply(401, {"detail": "This enrolment token has expired."})
            self.server.checked = body["agents"]
            return self.reply(200, {"items": [{"name": row["name"], **self.checks.get(
                row["name"], {"outcome": "new", "detail": None})} for row in body["agents"]]})
        self.server.enrolments += 1
        if self.redirect:
            return self.moved()
        if self.headers.get("Authorization") != f"Bearer {self.token}":
            self.reply(401, {"detail": "A valid enrolment token is required."})
        else:
            self.reply(400, {"detail": "The request contains invalid fields."})

    def log_message(self, *_args):
        pass


class HostedApiSetupTests(unittest.TestCase):
    """subspace-setup with a Subspace API on another host (--api-url)."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        fleet_setup.INPUTS = self.root / "inputs"
        fleet_setup.INPUTS.mkdir(mode=0o700)
        for name, value in (("SECRET_OWNER", os.getuid()), ("FLEET_ETC", self.root / "etc")):
            patcher = patch.object(fleet_setup, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.token_file = fleet_setup.INPUTS / "enrolment-token"
        self.token_file.write_text(HostedApi.token + "\n")
        self.token_file.chmod(0o600)

    def answers(self, *extra, saved=None):
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", *extra])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            answers = fleet_setup.gather(args, saved or {})
            fleet_setup.check_answers(answers)
        return answers, args

    def test_the_hosted_api_replaces_one_on_this_vm_and_is_kept(self):
        answers, _ = self.answers("--api-url", "https://Subspace.Example.com/")
        self.assertEqual((answers["apiUrl"], answers["api"], answers["demo"], answers["agentDispatch"]),
                         ("https://subspace.example.com", False, False, False))
        later, _ = self.answers(saved=answers)
        self.assertEqual((later["apiUrl"], later["api"]), ("https://subspace.example.com", False))
        for extra, message in (
                (["--api-url", "http://subspace.example.com"], "HTTPS origin"),
                (["--api-url", "https://subspace.example.com/tracker"], "HTTPS origin"),
                (["--api-url", ""], "no way back"),
                (["--api-url", "https://subspace.example.com", "--database-url-file", "/root/fleet/database-url"],
                 "leave out --database-url-file"),
                (["--api-url", "https://subspace.example.com", "--demo"], "drop --demo"),
                (["--api-url", "https://subspace.example.com", "--agent-dispatch"], "leave out --agent-dispatch"),
                (["--api-enrolment-token-file", str(self.token_file)], "pass --api-url too")):
            with self.subTest(extra), self.assertRaisesRegex(fleet_setup.SetupError, message):
                self.answers(*extra)
        with self.assertRaisesRegex(fleet_setup.SetupError, "leave out --database-url-file"):
            self.answers("--database-url-file", "/root/fleet/database-url", saved=answers)

    def test_a_fleet_whose_tracker_is_here_switches_only_when_asked(self):
        for saved in ({"api": True, "slug": "example-fleet"},
                      {"api": True, "slug": "example-fleet", "databaseUrlFile": "/root/fleet/database-url"}):
            (fleet_setup.INPUTS / "api.env").write_text("POSTGRES_PASSWORD=example\n")
            with self.subTest(saved), self.assertRaisesRegex(fleet_setup.SetupError, "--switch-to-hosted-api"):
                self.answers("--api-url", "https://subspace.example.com", saved=saved)
            switched, _ = self.answers("--api-url", "https://subspace.example.com", "--switch-to-hosted-api",
                                       saved=saved)
            self.assertNotIn("databaseUrlFile", switched)
            self.assertFalse(switched["api"])
        # A fleet with no API here has no tracker to leave behind.
        (fleet_setup.INPUTS / "api.env").unlink()
        self.assertTrue(self.answers("--api-url", "https://subspace.example.com", saved={"api": False})[0]["apiUrl"])

    def test_the_enrolment_token_goes_to_the_secrets_directory_and_is_never_shown(self):
        _, args = self.answers("--api-url", "https://subspace.example.com",
                               "--api-enrolment-token-file", str(self.token_file))
        self.assertEqual(fleet_setup.enrolment_token(args, {}), HostedApi.token)
        stored = fleet_setup.INPUTS / "fleet.secrets" / runtime.ENROLMENT_TOKEN
        self.assertFalse(stored.exists(), "nothing is kept before the run's checks pass")
        fleet_setup.store_enrolment_token(HostedApi.token)
        self.assertEqual((stored.read_text(), stored.stat().st_mode & 0o777), (HostedApi.token + "\n", 0o600))
        # A later run of a fleet saved as hosted checks no token: the stored one may have expired, and enrolled
        # officers need none. A run that makes a fleet hosted without a token stops, whatever files exist.
        _, later = self.answers("--api-url", "https://subspace.example.com")
        self.assertIsNone(fleet_setup.enrolment_token(later, {"apiUrl": "https://subspace.example.com"}))
        (fleet_setup.FLEET_ETC).mkdir()
        (fleet_setup.FLEET_ETC / "extra-secrets.json").write_text("{}")
        for saved in ({}, {"api": True, "slug": "example-fleet"}):
            with self.subTest(saved), self.assertRaisesRegex(fleet_setup.SetupError, "need the fleet's enrolment token"):
                fleet_setup.enrolment_token(later, saved)
        for text, mode, message in ((HostedApi.token, 0o644, "mode 0600"), ("not-a-token", 0o600, "senr1")):
            self.token_file.write_text(text)
            self.token_file.chmod(mode)
            with self.subTest(message), self.assertRaisesRegex(fleet_setup.SetupError, message) as refused:
                fleet_setup.enrolment_token(args, {})
            self.assertNotIn(HostedApi.token, str(refused.exception))

    def test_the_dry_run_checks_the_api_and_the_token_without_using_it(self):
        server = ThreadingHTTPServer(("127.0.0.1", 0), HostedApi)
        server.enrolments = 0
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        answers = {"apiUrl": f"http://127.0.0.1:{server.server_port}"}
        output = []
        with patch.object(fleet_setup, "say", side_effect=output.append):
            fleet_setup.hosted_checks(answers, HostedApi.token)
        self.assertIn("checked without using it", "\n".join(output))
        with self.assertRaisesRegex(fleet_setup.SetupError, "refused the enrolment token \\(401: A valid "
                                    "enrolment token is required.\\)") as refused:
            fleet_setup.hosted_checks(answers, "senr1." + "C" * 64)
        self.assertNotIn("senr1.", str(refused.exception))
        self.assertEqual(server.enrolments, 2)
        # Without a token given on this run (a rerun, its stored token maybe expired) only health is checked.
        output.clear()
        with patch.object(fleet_setup, "say", side_effect=output.append):
            fleet_setup.hosted_checks(answers, None)
        self.assertEqual(server.enrolments, 2)
        self.assertIn("Officers already enrolled keep working without one; a new officer", "\n".join(output))
        with self.assertRaisesRegex(fleet_setup.SetupError, "does not answer GET /health"):
            fleet_setup.hosted_checks({"apiUrl": "http://127.0.0.1:9"}, HostedApi.token)
        # A redirect is never followed with the token, and the message names where it pointed.
        HostedApi.redirect = "https://elsewhere.example.net/api/v1/agents/enrol"
        self.addCleanup(setattr, HostedApi, "redirect", None)
        for token in (None, HostedApi.token):
            with self.subTest(token=bool(token)), \
                    self.assertRaisesRegex(fleet_setup.SetupError, "redirect \\(302\\) to elsewhere.example.net"):
                fleet_setup.hosted_checks(answers, token)

    def test_the_officers_names_are_checked_with_the_enrolment_token(self):
        server = ThreadingHTTPServer(("127.0.0.1", 0), HostedApi)
        server.enrolments = 0
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        answers, _ = self.answers("--api-url", "https://subspace.example.com")
        answers["apiUrl"] = f"http://127.0.0.1:{server.server_port}"
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        HostedApi.checks = {"orion": {"outcome": "refused", "detail": "The name orion is taken: agent orion of "
                                      "fleet other-fleet has it, and agent names are unique in an organisation."}}
        self.addCleanup(setattr, HostedApi, "checks", None)
        output = []
        with patch.object(fleet_setup, "say", side_effect=output.append):
            problems = fleet_setup.hosted_name_problems(answers, members, HostedApi.token)
        self.assertEqual(server.checked, [
            {"name": "atlas", "rank": "rearAdmiral", "ircNick": "atlas"},
            {"name": "orion", "rank": "captain", "ircNick": "orion", "ship": "Orion"}])
        self.assertEqual(server.enrolments, 0)
        self.assertEqual(problems, ["orion: The name orion is taken: agent orion of fleet other-fleet has it, and "
                                    "agent names are unique in an organisation."])
        with self.assertRaisesRegex(fleet_setup.SetupError, "Pick another name for that officer.*Nothing was changed"):
            fleet_setup.require_names(problems)
        # A spent token, or an API without the route, checks nothing and says so; enrolment still refuses.
        with patch.object(fleet_setup, "say", side_effect=output.append):
            self.assertEqual(fleet_setup.hosted_name_problems(answers, members, "senr1." + "C" * 64), [])
            HostedApi.checks = None
            self.assertEqual(fleet_setup.hosted_name_problems(answers, members, HostedApi.token), [])
        self.assertIn("This enrolment token has expired.", "\n".join(output))
        self.assertIn("predates POST /api/v1/agents/enrol/check", "\n".join(output))

    def test_a_hosted_fleet_is_renamed_with_an_administrators_access_token(self):
        # A fleet installed with fleet-prefixed names against a hosted API, its IRC on this VM. Enrolment made its
        # records: the rear admiral by its name, the captain by its account, the ship with no channel.
        answers, _ = self.answers("--api-url", "https://subspace.example.com", "--irc-auth", "password",
                                  saved={"slug": "example-fleet"})
        self.assertEqual(answers["officerNames"], "prefixed")
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        config = fleet_setup.fleet_config(answers, members, "a" * 40, stargate_ref=runtime.SUBSPACE_AGENTS_COMMIT)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        fleet_setup.save_answers(answers)
        fleet_setup.store_enrolment_token(HostedApi.token)
        api = fixture.Subspace()
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"])
        cfg = validate({key: value for key, value in config.items() if key != "api"}, secrets, check_files=False)
        fleet_records.register(api.call, fleet_records.fleet_fields(cfg), fleet_records.roster(cfg), lambda _: None)
        for ship in api.ships.values():
            ship["channel"] = None
        ids = sorted(api.agents)
        api.people = ["orion"]  # A person whose username is the captain's plain name.
        keys = self.root / "subspace-agents"
        keys.mkdir()
        for name in ("Atlas", "example-fleet-orion"):
            (keys / f"{name}.pem").write_text(f"{name} key\n")
            (keys / f"{name}.pem").chmod(0o600)
        # An identity provider's access token, longer than any password the installer reads.
        token = "eyJ" + "x" * 1997
        token_file = self.root / "admin-token"
        token_file.write_text(token + "\n")
        token_file.chmod(0o600)

        def hosted(_origin, bearer, method, path, body=None):
            self.assertEqual(bearer, token)
            return api.call(method, path, body)
        output = []
        for patcher in (patch.object(fleet_setup, "say", side_effect=output.append),
                        patch.object(fleet_setup, "hosted_call", side_effect=hosted),
                        patch.object(fleet_setup, "GATEWAY_KEYS", keys), patch.object(fleet_setup.pwd, "getpwnam"),
                        patch.object(fleet_setup.os, "geteuid", return_value=0), fleet_installer(self.root)):
            patcher.start()
            self.addCleanup(patcher.stop)
        rename = ["--inputs", str(fleet_setup.INPUTS)]
        # Without an administrator's token the dry run cannot ask the API, and says so; the real run stops.
        fleet_setup.rename_officers([*rename, "--dry-run"])
        text = "\n".join(output)
        self.assertIn("without --api-admin-token-file the hosted Subspace API was not asked", text)
        self.assertIn("dry run: the API was not asked, so its names are unchecked", text)
        self.assertNotIn("the names are free", text)
        with self.assertRaisesRegex(fleet_setup.SetupError, "pass --api-admin-token-file"):
            fleet_setup.rename_officers(rename)
        # With it, a person's username is found before anything changes, and no person is named.
        rename += ["--api-admin-token-file", str(token_file)]
        writes = list(api.writes)
        with self.assertRaisesRegex(fleet_setup.SetupError, "orion is taken in the Subspace API: a person in the "
                                    "organisation has it as their username.*Nothing was changed"):
            fleet_setup.rename_officers(rename)
        self.assertEqual(api.writes, writes)
        api.people = []
        with patch.object(fleet_setup, "configure") as configure:
            fleet_setup.rename_officers(rename)
        configure.assert_called_once()
        # The records are renamed in place, and the ship enrolment left without a channel gets its new one.
        self.assertEqual(sorted(api.agents), ids)
        self.assertEqual(api.summary()["agents"], [("atlas", "rearAdmiral", "atlas", None, True),
                                                   ("orion", "captain", "orion", "Orion", True)])
        self.assertEqual(api.summary()["ships"], [("Orion", "#orion")])
        # Stargate's enrolled keys are kept under the new names (atlas.pem is Atlas.pem on a case-blind disk).
        self.assertEqual(((keys / "atlas.pem").read_text(), (keys / "orion.pem").read_text()),
                         ("Atlas key\n", "example-fleet-orion key\n"))
        self.assertTrue(fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")["officerRename"]["switched"])
        self.assertIn("Stargate kept the old key files", "\n".join(output))
        self.assertNotIn(token, "\n".join(output))
        # The token file is checked as a password file is, and may be as long as the API reads.
        for text, mode, message in ((token, 0o644, "mode 0600"), ("two words", 0o600, "one bearer token"),
                                    ("x" * (fleet_setup.API_TOKEN_LIMIT + 1), 0o600, "at most 16,384")):
            token_file.write_text(text)
            token_file.chmod(mode)
            with self.subTest(message), self.assertRaisesRegex(fleet_setup.SetupError, message) as refused:
                fleet_setup.bearer_token_file(str(token_file), "--api-admin-token-file", "API administrator token")
            self.assertNotIn(text, str(refused.exception))
        token_file.write_text("x" * fleet_setup.API_TOKEN_LIMIT)
        self.assertEqual(len(fleet_setup.bearer_token_file(str(token_file), "--api-admin-token-file", "token")),
                         fleet_setup.API_TOKEN_LIMIT)

    CAPTAINS = ["draco", "cygnus", "pavo"]
    SLOTS = ["claude-1", "claude-2", "codex-1", "codex-2"]
    CREW = {"draco": ["Kestrel", "Heron", "Wren", "Finch"], "cygnus": ["Osprey", "Plover", "Linnet", "Swift"],
            "pavo": ["Merlin", "Egret", "Robin", "Siskin"]}
    CREW_NAMES = ";".join(f"{captain.title()}={','.join(names)}" for captain, names in CREW.items())

    def installed_crew_fleet(self):
        """A fleet installed before plain officer names and crew names, enrolled with a hosted API, its IRC on this
        VM: three captains, each with the default four crew under their slot names, so that every ship has a
        claude-1, a codex-1 and so on, and each captain's root session pinned with that roster. Returns the API,
        the setup run's patches and what they record: setup's lines, and each fleet installer run."""
        repositories = [{"id": "project", "url": "https://github.com/example/project.git"}]
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", {
            "captains": [{"id": captain} for captain in self.CAPTAINS], "repositories": repositories})
        answers, _ = self.answers("--api-url", "https://subspace.example.com", "--irc-auth", "password",
                                  "--captains", "Draco,Cygnus,Pavo",
                                  "--stargate-ref", runtime.SUBSPACE_AGENTS_COMMIT, saved={"slug": "example-fleet"})
        self.assertEqual(answers["officerNames"], "prefixed")
        members = fleet_setup.roster(answers, "atlas", self.CAPTAINS)
        config = fleet_setup.fleet_config(answers, members, "a" * 40)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        fleet_setup.save_answers(answers)
        fleet_setup.store_enrolment_token(HostedApi.token)
        # The fleet installer's record of the installed fleet, and each captain's pinned root session.
        fleet_setup.FLEET_ETC.mkdir()
        fleet_setup.write_json(fleet_setup.FLEET_ETC / "fleet.json", config)
        for captain in self.CAPTAINS:
            workspace = self.root / "state/agents" / captain / "workspace"
            sessions.SessionRecord(self.root / "sessions", captain).save({
                "sessionID": "ses_fixture", "directory": str(workspace), "project": str(workspace / "project"),
                "crew": self.SLOTS, "confirmed": True})
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"])
        cfg = validate({key: value for key, value in config.items() if key != "api"}, secrets, check_files=False)
        self.assertEqual([[worker["id"] for worker in person["crew"]] for person in cfg["members"][1:]],
                         [self.SLOTS] * 3)
        api = fixture.Subspace()
        fleet_records.register(api.call, fleet_records.fleet_fields(cfg), fleet_records.roster(cfg), lambda _: None)
        token = "eyJ" + "x" * 97
        token_file = self.root / "admin-token"
        token_file.write_text(token + "\n")
        token_file.chmod(0o600)
        self.rename = ["--inputs", str(fleet_setup.INPUTS), "--api-admin-token-file", str(token_file)]
        output, plans = [], []

        def hosted(_origin, bearer, method, path, body=None):
            self.assertEqual(bearer, token)
            return api.call(method, path, body)
        patches = [
            patch.object(fleet_setup, "say", side_effect=output.append),
            patch.object(fleet_setup, "hosted_call", side_effect=hosted), fleet_installer(self.root, plans),
            patch.object(fleet_setup, "GATEWAY_KEYS", self.root / "subspace-agents"),
            patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0),
            patch.object(fleet_setup, "crew_names_supported", return_value=True),
            # The switch is setup's real run, with the VM's commands and the hosted API's own checks stubbed.
            patch.object(fleet_setup, "run", return_value=""), patch.object(fleet_setup, "irc_source", return_value="b" * 40),
            patch.object(fleet_setup, "api_down"), patch.object(fleet_setup, "stargate_agents", return_value=True),
            patch.object(fleet_setup, "hosted_checks"), patch.object(fleet_setup, "hosted_name_problems", return_value=[])]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)
        return api, output, plans

    def crew_after_the_switch(self):
        """Each captain's crew as (ID, name) in the fleet.json that setup last handed the fleet installer."""
        config = fleet_setup.read_json(fleet_setup.INPUTS / "fleet.json")
        cfg = validate(config, fleet_setup.read_json(fleet_setup.INPUTS / "fleet.secrets.json"), check_files=False)
        return {person["id"]: [(worker["id"], worker["name"]) for worker in person["crew"]]
                for person in cfg["members"][1:]}

    def test_a_fleet_whose_crew_share_ids_across_ships_is_renamed_with_names_unique_in_the_fleet(self):
        api, output, plans = self.installed_crew_fleet()
        snapshot = {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}
        writes = list(api.writes)
        # The dry run checks what the switch hands the fleet installer, crew names included, and shows each ship's
        # crew as the switch installs them: the new names, each under its installed ID.
        fleet_setup.rename_officers([*self.rename, "--crew-names", self.CREW_NAMES, "--dry-run"])
        text = "\n".join(output)
        self.assertIn("crew after the switch, by ship (name, ID, model family and slot):\n"
                      "  Draco: Kestrel (claude-1, Claude 1); Heron (claude-2, Claude 2); Wren (codex-1, Codex 1); "
                      "Finch (codex-2, Codex 2)\n  Cygnus: Osprey (claude-1, Claude 1); ", text)
        self.assertIn("dry run: the names are free", text)
        # The fleet installer's dry run takes the switch's fleet.json, and keeps its plan to itself.
        self.assertEqual([run[:2] for run in plans], [(True, True)])
        self.assertIn("crew: Merlin (Claude 1, claude-sonnet-5-5, ID claude-1); Egret (Claude 2", plans[0][2])
        self.assertEqual({path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}, snapshot)
        self.assertEqual(api.writes, writes)
        # A name the fleet installer refuses stops the dry run, and the real run before anything changes: here a
        # name that is another member's ID in the same crew, which Stargate's team refuses.
        clash = self.CREW_NAMES.replace("Kestrel", "Codex 1")
        for dry in (["--dry-run"], []):
            with self.subTest(dry=bool(dry)), self.assertRaisesRegex(
                    fleet_setup.SetupError, "Captain draco's crew member Codex 1 \\(claude-1\\) has the same name as "
                    "the ID of Wren \\(codex-1\\).*Nothing was changed"):
                fleet_setup.rename_officers([*self.rename, "--crew-names", clash, *dry])
            self.assertEqual({path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}, snapshot)
            self.assertEqual(api.writes, writes)
        # A name on two ships is refused, naming both.
        with self.assertRaisesRegex(fleet_setup.SetupError, "draco's Heron and pavo's heron are the same"):
            fleet_setup.rename_officers([*self.rename, "--crew-names", self.CREW_NAMES.replace("Siskin", "heron")])
        # The real run renames the officers, then switches: setup runs with the new names, and the fleet installer
        # takes every ship's claude-1 under its own name, its pinned session keeping its roster of IDs.
        plans.clear()
        fleet_setup.rename_officers([*self.rename, "--crew-names", self.CREW_NAMES])
        self.assertEqual([run[:2] for run in plans], [(True, True), (False, False)])
        saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
        self.assertEqual((saved["officerNames"], saved["officerRename"]["switched"]), ("plain", True))
        self.assertEqual(saved["crewIds"], {captain: {slot: slot for slot in self.SLOTS} for captain in self.CAPTAINS})
        self.assertEqual(self.crew_after_the_switch(),
                         {captain: list(zip(self.SLOTS, names, strict=True)) for captain, names in self.CREW.items()})
        self.assertEqual([row[0] for row in api.summary()["agents"]], ["atlas", "cygnus", "draco", "pavo"])

    def stuck_fleet(self):
        """The fleet as a rename-officers run with --crew-names left it before this fix: the officers renamed on
        IRC and in the API, the plain names and crew names saved, and the switch refused by the fleet installer,
        whose crew-name check then compared IDs, so that every ship's claude-1 had the same 'name'."""
        api, output, plans = self.installed_crew_fleet()

        def check_crew_names_by_id(agents):
            seen = {}
            for person in agents:
                for worker in (worker for worker in person["crew"] if worker["named"]):
                    other = seen.get(worker["id"])
                    runtime.require(other is None, f"Crew member {worker['name']} of captain {person['id']} has the "
                                    f"same name as {other}, ignoring case.")
                    seen[worker["id"]] = f"captain {person['id']}'s {worker['name']}"
        # That release had no check of the switch before the rename changed anything.
        with patch.object(runtime, "check_crew_names", side_effect=check_crew_names_by_id), \
                patch.object(fleet_setup, "check_switch", return_value=[]), \
                self.assertRaisesRegex(fleet_setup.SetupError, "Crew member Osprey of captain cygnus has the same "
                                       "name as captain draco's Kestrel, ignoring case"):
            fleet_setup.rename_officers([*self.rename, "--crew-names", self.CREW_NAMES])
        saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
        self.assertEqual(saved["officerNames"], "plain")
        self.assertEqual(saved["officerRename"]["from"], {
            officer: f"example-fleet-{officer}" for officer in ["atlas", *self.CAPTAINS]})
        self.assertNotIn("switched", saved["officerRename"])
        self.assertEqual(saved["crewNames"]["cygnus"], dict(zip(self.SLOTS, self.CREW["cygnus"], strict=True)))
        self.assertEqual([row[0] for row in api.summary()["agents"]], ["atlas", "cygnus", "draco", "pavo"])
        output.clear()
        plans.clear()
        return api, output, plans

    def test_a_rename_stopped_at_the_switch_resumes_there_with_no_options(self):
        api, output, plans = self.stuck_fleet()
        writes = list(api.writes)
        # The dry run now checks the switch, and passes.
        fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS), "--dry-run"])
        self.assertIn("  Pavo: Merlin (claude-1, Claude 1); Egret (claude-2, Claude 2); Robin (codex-1, Codex 1); "
                      "Siskin (codex-2, Codex 2)", output)
        self.assertIn("dry run: the real run runs setup again with the new names", "\n".join(output))
        # The real run resumes at the switch: it renames nothing again, runs setup, and records the rename as done.
        output.clear()
        fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS)])
        self.assertIn("have their plain names on IRC and in the API, but setup has not yet finished", "\n".join(output))
        self.assertEqual([run[0] for run in plans], [True, True, False])
        self.assertEqual(api.writes, writes)
        saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
        self.assertTrue(saved["officerRename"]["switched"])
        self.assertEqual(self.crew_after_the_switch()["cygnus"], list(zip(self.SLOTS, self.CREW["cygnus"], strict=True)))
        self.assertIn("desktops: run the host installer again", "\n".join(output))

    def test_an_ordinary_setup_run_on_a_stuck_rename_installs_the_same_and_leaves_the_rename_to_finish(self):
        api, output, plans = self.stuck_fleet()
        writes = list(api.writes)
        stuck = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
        # A plain re-run, and one that passes the crew's new names again, install what the switch installs: the
        # plain names and each crew member's new name under its installed ID. Neither renames anything, nor
        # records the rename as done, which only rename-officers checks and records.
        for extra in ([], ["--crew-names", self.CREW_NAMES]):
            with self.subTest(extra=bool(extra)):
                fleet_setup.setup(fleet_setup.parse_args(["--non-interactive", "--inputs", str(fleet_setup.INPUTS),
                                                          *extra]))
                self.assertEqual(plans[-1][0], False)
                self.assertEqual(self.crew_after_the_switch(),
                                 {captain: list(zip(self.SLOTS, names, strict=True))
                                  for captain, names in self.CREW.items()})
                saved = fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")
                self.assertEqual({key: saved[key] for key in ("officerNames", "officerRename", "crewNames", "crewIds")},
                                 {key: stuck[key] for key in ("officerNames", "officerRename", "crewNames", "crewIds")})
                self.assertEqual(api.writes, writes)
        # rename-officers then finishes it.
        fleet_setup.rename_officers(["--inputs", str(fleet_setup.INPUTS)])
        self.assertTrue(fleet_setup.read_json(fleet_setup.INPUTS / "answers.json")["officerRename"]["switched"])

    def test_the_hosted_paths_are_the_apis_own(self):
        # The health check, the enrolment check and the REST examples in the officers' instructions name paths
        # the API serves, as its OpenAPI document lists them.
        document = json.loads((Path(fleet_setup.HERE).parents[1] / "subspace-api/openapi.json").read_text())
        for path, method in (("/health", "get"), ("/api/v1/agents/enrol", "post"),
                             ("/api/v1/agents/enrol/check", "post"), ("/api/v1/me", "get"),
                             ("/api/v1/issues", "post"), ("/api/v1/fleets/{reference}/enrolment-tokens", "post")):
            self.assertIn(method, document["paths"].get(path, {}), path)
        source = Path(fleet_setup.__file__).read_text()
        self.assertIn('f"{origin}/health"', source)
        self.assertIn('f"{origin}/api/v1/agents/enrol"', source)
        guidance = roles.REQUEST_GUIDANCE
        self.assertIn("/api/v1/me", guidance)
        self.assertIn("/api/v1/issues", guidance)

    def test_stargate_is_checked_for_subspace_agents_at_the_chosen_commit(self):
        source = self.root / "stargate"
        git = ["git", "-C", str(source), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.com"]
        subprocess.run(["git", "init", "--quiet", str(source)], check=True)
        subprocess.run([*git, "config", "uploadpack.allowAnySHA1InWant", "true"], check=True)
        (source / "README.md").write_text("Stargate\n")
        subprocess.run([*git, "add", "."], check=True)
        subprocess.run([*git, "commit", "--quiet", "-m", "before"], check=True)
        before = subprocess.run([*git, "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip()
        (source / "src/subspace").mkdir(parents=True)
        (source / runtime.SUBSPACE_AGENTS_FILE).write_text("export {};\n")
        subprocess.run([*git, "add", "."], check=True)
        subprocess.run([*git, "commit", "--quiet", "-m", "subspaceAgents"], check=True)
        after = subprocess.run([*git, "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip()
        with patch.object(fleet_setup, "STARGATE_REPO", f"file://{source}"):
            self.assertFalse(fleet_setup.stargate_agents(before))
            self.assertTrue(fleet_setup.stargate_agents(after))
            with self.assertRaisesRegex(fleet_setup.SetupError, "Could not fetch Stargate"):
                fleet_setup.stargate_agents("0" * 40)

    def test_the_fleet_installer_gets_an_enrolment_api_and_no_administrator_token(self):
        answers, _ = self.answers("--api-url", "https://subspace.example.com", "--irc", "remote",
                                  "--irc-host", "irc.example.com")
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        config = fleet_setup.fleet_config(answers, members, "a" * 40, stargate_ref=runtime.SUBSPACE_AGENTS_COMMIT)
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"], local_irc=False, certificates=True)
        self.assertEqual(config["api"], {"url": "https://subspace.example.com", "enrolment": True})
        self.assertNotIn("apiToken", secrets)
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual((cfg["api"]["origin"], cfg["api"]["enrolment"], cfg["token"]),
                         ("https://subspace.example.com", True, ""))
        values = fleet_setup.summary(answers, members, False)
        self.assertEqual((values["SUBSPACE_API"], values["SUBSPACE_HOSTED_API"]), ("0", "https://subspace.example.com"))
        self.assertNotIn("SUBSPACE_HOSTED_API", fleet_setup.summary(self.answers()[0], members, False))

    def test_the_host_installers_copy_the_token_for_the_run_only(self):
        # Without an SSH harness, check the text: the token file goes with the operator's password files, which
        # reach the VM after the installer and are removed however the run ends, and --api-url excludes
        # --api-target and --database-url-file before anything is copied.
        root = Path(fleet_setup.HERE).parents[1]
        shell, powershell = (root / "install.sh").read_text(), (root / "install.ps1").read_text()
        self.assertEqual(shell.count("for option in irc-operator-password irc-operator-account-password "
                                     "api-enrolment-token; do"), 2)
        self.assertIn('api-enrolment-token) printf \'%s\' "$api_enrolment_token_file" ;;', shell)
        self.assertIn("@('api-enrolment-token', $ApiEnrolmentTokenFile)", powershell)
        for text, check, first_copy in ((shell, "--api-url|--api-url=*) hosted=true", "database-url.new"),
                                        (powershell, "if ($hosted) {\n        if ($ApiTarget)", "database-url.new")):
            self.assertLess(text.index(check), text.index(first_copy))

    def test_a_refused_switch_leaves_the_fleet_as_it_was(self):
        # An installed fleet whose API runs here, with an existing PostgreSQL, tries to switch and is refused.
        saved = {"api": True, "slug": "example-fleet", "name": "Example Fleet", "databaseUrlFile": "/root/fleet/db"}
        fleet_setup.write_json(fleet_setup.INPUTS / "answers.json", saved)
        (fleet_setup.INPUTS / "api.env").write_text("DATABASE_URL='postgresql://example'\n")
        before = {path.name: path.read_bytes() for path in fleet_setup.INPUTS.iterdir() if path.is_file()}
        argv = ["--non-interactive", "--operator", "operator", "--rear-admiral", "Atlas", "--captains", "Orion",
                "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40, "--api-url",
                "https://subspace.example.com", "--switch-to-hosted-api", "--api-enrolment-token-file",
                str(self.token_file)]
        for failing in ("stargate_agents", "hosted_checks"):
            with self.subTest(failing), patch.object(fleet_setup.pwd, "getpwnam"), \
                    patch.object(fleet_setup.os, "geteuid", return_value=0), \
                    patch.object(fleet_setup, "stargate_agents", return_value=failing != "stargate_agents"), \
                    patch.object(fleet_setup, "hosted_checks", side_effect=fleet_setup.SetupError("refused")
                                 if failing == "hosted_checks" else None), \
                    patch.object(fleet_setup, "installer") as installer, patch.object(fleet_setup, "say"), \
                    self.assertRaises(fleet_setup.SetupError):
                fleet_setup.setup(fleet_setup.parse_args(argv))
            installer.assert_not_called()
            self.assertEqual({path.name: path.read_bytes() for path in fleet_setup.INPUTS.iterdir() if path.is_file()},
                             before)
            self.assertFalse((fleet_setup.INPUTS / "fleet.secrets").exists())

    def test_a_saved_api_target_gives_way_to_a_hosted_api(self):
        # A fleet once set up with --api-target keeps that in its host state; --api-url sets it aside instead of
        # refusing, and a successful real run forgets it, after the dry-run branch has returned.
        root = Path(fleet_setup.HERE).parents[1]
        shell, powershell = (root / "install.sh").read_text(), (root / "install.ps1").read_text()
        for text, load, keep, forget, dry in (
                (shell, 'if "$hosted"; then\n            note "Leaving out the saved --api-target',
                 'api_target=$(cat "$state/api-target")', 'if "$hosted"; then rm -f "$state/api-target"; fi',
                 'say "Dry run complete.'),
                (powershell, 'if ($hosted) { Write-Host "    Leaving out the saved --api-target',
                 "else { $script:ApiTarget = (Get-Content $apiFile -Raw).Trim() }",
                 "if ($hosted) { Remove-Item -Force -ErrorAction SilentlyContinue $apiFile }",
                 "Say 'Dry run complete.")):
            self.assertLess(text.index(load), text.index(keep))
            self.assertLess(text.index(dry), text.index(forget))
        # The explicit option with --api-url is still refused.
        self.assertIn('[ -z "$api_target" ] || die "--api-url names the fleet\'s API, so --api-target does not apply."',
                      shell)

    def test_shared_api_target_checks_the_vms_own_saved_settings(self):
        root = Path(fleet_setup.HERE).parents[1]
        self.assertIn("passthrough+=(--shared-api-target)", (root / "install.sh").read_text())
        self.assertIn("$Passthrough.Add('--shared-api-target')", (root / "install.ps1").read_text())
        base = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                "--non-interactive", "--shared-api-target"]
        remote = {"irc": "remote", "ircHost": "irc.example.com"}
        for flags, saved in ((["--irc", "remote", "--irc-host", "irc.example.com"], {}), ([], remote)):
            answers = fleet_setup.gather(fleet_setup.parse_args(base + flags), saved)
            self.assertFalse(answers["api"])
            self.assertEqual(answers["ircHost"], "irc.example.com")

        for flags, saved in (
                ([], {"irc": "local", "ircHost": "irc.example.com"}),
                (["--irc", "remote"], {"irc": "local"}),
                (["--irc", "local"], remote),
                *[([], {**remote, "ircHost": host}) for host in
                  ("127.0.0.9", "127.1", "localhost.", "::ffff:127.0.0.1")]):
            with self.subTest(flags=flags, saved=saved), \
                    self.assertRaisesRegex(fleet_setup.SetupError, "--api-target needs remote IRC"):
                fleet_setup.gather(fleet_setup.parse_args(base + flags), saved)
        with self.assertRaisesRegex(fleet_setup.SetupError, "explicit or saved hosted API"):
            fleet_setup.gather(fleet_setup.parse_args(base), {**remote, "apiUrl": "https://api.example.com"})

    def test_a_switch_without_a_token_changes_nothing(self):
        # A fleet whose API runs here, installed (so the fleet installer's extra-secrets.json exists), switches
        # without --api-enrolment-token-file: refused before setup saves anything.
        saved = {"api": True, "slug": "example-fleet", "name": "Example Fleet"}
        fleet_setup.write_json(fleet_setup.INPUTS / "answers.json", saved)
        (fleet_setup.INPUTS / "api.env").write_text("POSTGRES_PASSWORD=example\n")
        fleet_setup.FLEET_ETC.mkdir()
        (fleet_setup.FLEET_ETC / "extra-secrets.json").write_text('{"tokens": {}, "cloud": {}}')
        before = {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}
        argv = ["--non-interactive", "--operator", "operator", "--rear-admiral", "Atlas", "--captains", "Orion",
                "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40, "--api-url",
                "https://subspace.example.com", "--switch-to-hosted-api"]
        for dry in ([], ["--dry-run"]):
            with self.subTest(dry=bool(dry)), patch.object(fleet_setup.pwd, "getpwnam"), \
                    patch.object(fleet_setup.os, "geteuid", return_value=0), \
                    patch.object(fleet_setup, "stargate_agents", return_value=True), \
                    patch.object(fleet_setup, "hosted_checks") as checks, \
                    patch.object(fleet_setup, "installer") as installer, patch.object(fleet_setup, "say"), \
                    self.assertRaisesRegex(fleet_setup.SetupError, "a switch to it, need the fleet's enrolment token"):
                fleet_setup.setup(fleet_setup.parse_args(argv + dry))
            checks.assert_not_called()
            installer.assert_not_called()
            self.assertEqual({path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}, before)

    def test_the_host_scripts_take_api_url_with_an_equals_sign(self):
        root = Path(fleet_setup.HERE).parents[1]
        self.assertIn("case $arg in --api-url|--api-url=*) hosted=true ;; esac", (root / "install.sh").read_text())
        self.assertIn("$_ -ceq '--api-url' -or $_ -clike '--api-url=*'", (root / "install.ps1").read_text())

    def test_a_dry_run_on_the_pinned_release_stops_before_anything_else(self):
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40,
                "--api-url", "https://subspace.example.com", "--api-enrolment-token-file", str(self.token_file),
                "--dry-run"]
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "stargate_agents",
                                                                    return_value=False), \
                patch.object(fleet_setup, "installer") as installer, patch.object(fleet_setup, "say"), \
                self.assertRaisesRegex(fleet_setup.SetupError, "needs a Stargate with its subspaceAgents section"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        installer.assert_not_called()
        written = []
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "stargate_agents", return_value=True), \
                patch.object(fleet_setup, "hosted_checks") as checks, patch.object(fleet_setup, "say"), \
                patch.object(fleet_setup, "installer", side_effect=lambda path, **_: written.append(
                    (json.loads(Path(path).read_text()), sorted(p.name for p in (Path(path).parent / "fleet.secrets")
                                                                .iterdir())))):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        checks.assert_called_once()
        self.assertEqual(checks.call_args.args[1], HostedApi.token)
        config, secrets = written[0]
        self.assertEqual(config["api"], {"url": "https://subspace.example.com", "enrolment": True})
        self.assertIn(runtime.ENROLMENT_TOKEN, secrets)
        # The dry run left nothing on the VM: the token went only to its private copy.
        self.assertFalse((fleet_setup.INPUTS / "fleet.secrets").exists())


class FirstFleetTests(unittest.TestCase):
    """What the first real fleet install found: the Fleet API, the fleet's record, IRC wording and stdin."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        fleet_setup.INPUTS = self.root / "inputs"
        fleet_setup.INPUTS.mkdir(mode=0o700)

    def checkout(self, relative):
        source = self.root / "stargate"
        (source / relative).parent.mkdir(parents=True, exist_ok=True)
        (source / relative).write_text("export {};\n")
        return source

    def test_the_fleet_api_is_found_where_each_supported_stargate_keeps_it(self):
        # v0.40 to v0.43 keep it in src/api/v1; v0.44, v0.45 and commit 5630a5c in src/observability/dashboard/api/v1.
        for relative in ("src/api/v1/server.ts", "src/observability/dashboard/api/v1/server.ts",
                         "src/somewhere/else/api/v1/server.ts"):
            with self.subTest(relative):
                source = self.checkout(relative)
                self.assertEqual(fleet_setup.fleet_api_server(source), relative)
                output = []
                with patch.object(fleet_setup, "say", side_effect=output.append):
                    self.assertTrue(fleet_setup.confirm_fleet_api(None, source))
                    self.assertTrue(fleet_setup.confirm_fleet_api(True, source))
                self.assertIn(f"Stargate's Fleet API is in {relative}; it is on", output)
                shutil.rmtree(source)
        source = self.checkout("src/main.ts")
        self.assertIsNone(fleet_setup.fleet_api_server(source))
        self.assertIsNone(fleet_setup.fleet_api_server(None))
        # Asked for but missing stops setup, naming what it looked for; auto but missing says so.
        with self.assertRaisesRegex(fleet_setup.SetupError, "--stargate-api on, but the installed Stargate has no "
                                    "Fleet API in .*src/observability/dashboard/api/v1/server.ts"):
            fleet_setup.confirm_fleet_api(True, source)
        output = []
        with patch.object(fleet_setup, "say", side_effect=output.append):
            self.assertFalse(fleet_setup.confirm_fleet_api(None, source))
        self.assertTrue(output[0].startswith("NOTE: the installed Stargate has no Fleet API"))
        self.assertIn("records no Fleet API address", output[0])

    def test_the_fleet_record_names_the_vm_and_its_fleet_api(self):
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", "--vm-host", "10.0.0.5"])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            answers = fleet_setup.gather(args, {})
        self.assertEqual(answers["vmHost"], "10.0.0.5")
        with patch.object(fleet_setup.pwd, "getpwnam"):
            kept = fleet_setup.gather(fleet_setup.parse_args(["--non-interactive", "--operator", "operator"]), answers)
            self.assertEqual(kept["vmHost"], "10.0.0.5")
            cleared = fleet_setup.gather(fleet_setup.parse_args(["--non-interactive", "--operator", "operator",
                                                                 "--vm-host", ""]), answers)
            self.assertNotIn("vmHost", cleared)
            with self.assertRaisesRegex(fleet_setup.SetupError, "--vm-host must be a DNS name"):
                fleet_setup.gather(fleet_setup.parse_args(["--non-interactive", "--operator", "operator",
                                                           "--vm-host", "user@host"]), answers)
        members = fleet_setup.roster(answers, "atlas", ["orion"])
        api_block = {"url": "http://127.0.0.1:3000", "initiatives": [], "vmHost": answers["vmHost"]}
        config = fleet_setup.fleet_config(answers, members, "a" * 40, api_block, stargate_api=True)
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"], "header.payload.signature", certificates=True)
        fields = runtime.fleet_records.fleet_fields(validate(config, secrets, check_files=False))
        self.assertEqual((fields["vmHost"], fields["fleetApiUrl"]), ("10.0.0.5", "http://127.0.0.1:8484"))
        # The host installers pass the --remote host to setup.
        root = Path(fleet_setup.HERE).parents[1]
        self.assertIn('if [ "$KIND" = ssh ]; then passthrough+=(--vm-host "${HOST#*@}"); fi',
                      (root / "install.sh").read_text())
        self.assertIn("$Passthrough.AddRange([string[]]@('--vm-host', ($targetEnv.HOST -split '@', 2)[-1]))",
                      (root / "install.ps1").read_text())

    def test_a_certificate_fleet_is_not_told_to_copy_a_password_bundle(self):
        answers = {"ircAuth": "certificate", "ircHost": "irc.example.com"}
        line = fleet_setup.remote_irc_next_step(answers, None)
        self.assertNotIn("irc-bundle.json", line)
        self.assertIn("/NS lines printed above", line)
        self.assertIn("registered and checked", fleet_setup.remote_irc_next_step(answers, ("ircop", "x", None)))
        self.assertIn("irc-bundle.json", fleet_setup.remote_irc_next_step({"ircAuth": "password"}, None))

    def test_docker_compose_never_reads_setups_standard_input(self):
        with patch.object(fleet_setup.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "x.y.z")) \
                as ran:
            fleet_setup.compose("exec", "-T", "api", "bun", "run", "auth:dev", capture=True)
            self.assertIs(ran.call_args.kwargs["stdin"], subprocess.DEVNULL)
            fleet_setup.compose("exec", "-T", "api", "psql", input_text="SELECT 1;")
            self.assertIsNone(ran.call_args.kwargs["stdin"])
            self.assertEqual(ran.call_args.kwargs["input"], "SELECT 1;")
            fleet_setup.run("true")
            self.assertIsNone(ran.call_args.kwargs["stdin"])
        # As 'subspace-setup token' runs it: a script piped after it over SSH stays unread.
        script = self.root / "rest.sh"
        script.write_text("echo rest\n")
        fake = self.root / "docker"
        fake.write_text("#!/bin/sh\ncat > /dev/null; echo header.payload.signature\n")
        fake.chmod(0o755)
        with open(script) as stdin, patch.dict(os.environ, {"PATH": f"{self.root}:{os.environ['PATH']}"}):
            saved = os.dup(0)
            os.dup2(stdin.fileno(), 0)
            try:
                fleet_setup.compose("exec", "-T", "api", "bun", "run", "auth:dev", capture=True)
                self.assertEqual(sys.stdin.read(), "echo rest\n")
            finally:
                os.dup2(saved, 0)
                os.close(saved)


class OutboundTests(unittest.TestCase):
    """setup stops at once, saying what to open, when the VM cannot reach what the install downloads."""

    def test_the_package_mirrors_come_from_both_apt_source_formats(self):
        with tempfile.TemporaryDirectory() as temp:
            apt = Path(temp)
            (apt / "sources.list.d").mkdir()
            (apt / "sources.list").write_text("# comment\ndeb [arch=amd64] http://mirror.example.com/ubuntu noble main\n"
                                              "deb cdrom:[Ubuntu]/ noble main\n")
            (apt / "sources.list.d/ubuntu.sources").write_text(
                "Types: deb\nURIs: http://archive.example.org/ubuntu/ http://mirror.example.com/ubuntu\n"
                "Suites: noble\n\nTypes: deb\nURIs: http://disabled.example.net/ubuntu\nEnabled: no\n")
            self.assertEqual(fleet_setup.apt_mirrors(apt), ["http://mirror.example.com/ubuntu",
                                                            "http://archive.example.org/ubuntu/"])

    def test_mirrors_are_probed_through_apts_own_proxy(self):
        dump = ('Acquire "";\nAcquire::http::Proxy "http://proxy.example.com:3128/";\n'
                'Acquire::https::Proxy "DIRECT";\n')
        with patch.object(fleet_setup.subprocess, "run",
                          return_value=subprocess.CompletedProcess([], 0, stdout=dump)):
            self.assertEqual(fleet_setup.apt_proxies(), {"http": "http://proxy.example.com:3128/"})
        opened = []

        class Opener:
            def open(self, request, timeout):
                opened.append(request.full_url)
                raise URLError("refused")
        with patch.object(fleet_setup, "build_opener", return_value=Opener()) as build, \
                patch.object(fleet_setup, "urlopen", side_effect=AssertionError("not through the environment")):
            self.assertFalse(fleet_setup.reachable("http://archive.example.org/ubuntu/", "http://proxy.example.com:3128/"))
        self.assertEqual(build.call_args.args[0].proxies, {"http": "http://proxy.example.com:3128/",
                                                           "https": "http://proxy.example.com:3128/"})
        with patch.object(fleet_setup, "apt_mirrors", return_value=["http://archive.example.org/ubuntu/"]), \
                patch.object(fleet_setup, "apt_proxies", return_value={"http": "http://proxy.example.com:3128/"}), \
                patch.object(fleet_setup, "docker_ready", return_value=True):
            self.assertEqual(fleet_setup.outbound_targets({"api": True}), [
                ("GitHub", "https://github.com", None),
                ("the package mirror", "http://archive.example.org/ubuntu/", "http://proxy.example.com:3128/")])

    def test_no_outbound_access_stops_at_once_and_says_what_to_open(self):
        targets = [("GitHub", "https://github.com", None),
                   ("the package mirror", "http://archive.example.org/ubuntu/", None)]
        with patch.object(fleet_setup, "outbound_targets", return_value=targets), \
                patch.object(fleet_setup, "urlopen", side_effect=URLError("timed out")) as opened, \
                self.assertRaisesRegex(fleet_setup.SetupError, r"no outbound access to GitHub \(github.com\), the "
                                       r"package mirror \(archive.example.org\).*Open outbound HTTPS \(TCP 443\)"):
            check_outbound({"api": True})
        self.assertTrue(all(call.kwargs["timeout"] == fleet_setup.OUTBOUND_TIMEOUT for call in opened.call_args_list))
        # An HTTP error is an answer: the host is reachable.
        refused = HTTPError("https://github.com", 403, "Forbidden", {}, None)
        with patch.object(fleet_setup, "outbound_targets", return_value=targets), \
                patch.object(fleet_setup, "urlopen", side_effect=refused):
            check_outbound({"api": True})
        # Docker's repository only matters when the VM runs containers and has no Docker Compose yet.
        def labels(answers, ready=False):
            with patch.object(fleet_setup, "apt_mirrors", return_value=[]), \
                    patch.object(fleet_setup, "docker_ready", return_value=ready):
                return [label for label, _, _ in fleet_setup.outbound_targets(answers)]
        self.assertNotIn("Docker's package repository", labels({"api": False, "irc": "remote"}))
        self.assertIn("Docker's package repository", labels({"api": True}))
        self.assertNotIn("Docker's package repository", labels({"api": True}, ready=True))

    def test_the_release_lookup_says_why_it_failed(self):
        with patch.object(fleet_setup, "run", side_effect=fleet_setup.SetupError("git ls-remote did not finish")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "no outbound access to GitHub.*--stargate-ref"):
            latest_stargate()


class HostInstallerTests(unittest.TestCase):
    """The host install.sh, run with a fake ssh that prints its arguments."""

    def test_ssh_puts_options_before_the_host_and_a_command_after_it(self):
        with tempfile.TemporaryDirectory() as temp:
            work = Path(temp)
            (work / "bin").mkdir()
            (work / "home/targets/example").mkdir(parents=True)
            (work / "home/targets/example/target.env").write_text("KIND=remote\n")
            fake = work / "bin/ssh"
            fake.write_text('#!/bin/sh\nfor a in "$@"; do printf "%s\\n" "$a"; done\n')
            fake.chmod(0o755)
            installer = Path(fleet_setup.HERE).parents[1] / "install.sh"
            environment = {**os.environ, "PATH": f"{work / 'bin'}:{os.environ['PATH']}", "SUBSPACE_HOME": str(work / "home")}

            def ssh(*argv):
                result = subprocess.run(["bash", str(installer), "ssh", "--target", "example", *argv],
                                        env=environment, capture_output=True, text=True, check=True)
                return result.stdout.splitlines()
            config = str(work / "home/targets/example/ssh_config")
            self.assertEqual(ssh("--", "sudo", "uptime"), ["-F", config, "subspace-target", "sudo", "uptime"])
            self.assertEqual(ssh("-L", "8080:127.0.0.1:8080"),
                             ["-L", "8080:127.0.0.1:8080", "-F", config, "subspace-target"])
            self.assertEqual(ssh(), ["-F", config, "subspace-target"])
            # Everything after -- is the remote command's, even words that look like the installer's options.
            self.assertEqual(ssh("--", "df", "-h", "--yes", "--target", "other", "--local"),
                             ["-F", config, "subspace-target", "df", "-h", "--yes", "--target", "other", "--local"])

    @unittest.skipUnless(shutil.which("pwsh"), "needs PowerShell (pwsh)")
    def test_install_ps1_ssh_runs_a_command_under_strict_mode(self):
        with tempfile.TemporaryDirectory() as temp:
            work = Path(temp)
            (work / "bin").mkdir()
            (work / "home/targets/example").mkdir(parents=True)
            (work / "home/targets/example/target.env").write_text(
                "KIND=remote\nHOST=ops@vm.example.com\nPORT=22\nIDENTITY=\nPROXY=\n")
            fake = work / "bin/ssh.exe"
            fake.write_text('#!/bin/sh\nfor a in "$@"; do printf "%s\\n" "$a"; done\n')
            fake.chmod(0o755)
            # Off Windows, the script's ACL helper cannot run; nothing else on this path needs Windows.
            script = (Path(fleet_setup.HERE).parents[1] / "install.ps1").read_text()
            script = re.sub(r"(?ms)^function Protect-File\b.*?^}\n", "function Protect-File([string]$Path) { }\n",
                            script, count=1)
            (work / "install.ps1").write_text(script)
            environment = {**os.environ, "PATH": f"{work / 'bin'}:{os.environ['PATH']}", "SUBSPACE_HOME": str(work / "home")}
            config = str(work / "home/targets/example/ssh_config")

            def ssh(words, file=False):
                command = ["pwsh", "-NoProfile", "-File", str(work / "install.ps1"), "ssh", "--target", "example",
                           *words] if file else ["pwsh", "-NoProfile", "-Command",
                                                 f"& '{work / 'install.ps1'}' ssh --target example {' '.join(words)}"]
                result = subprocess.run(command, env=environment, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                return result.stdout.splitlines()
            host = ["-F", config, "subspace-target"]
            self.assertEqual(ssh(["uptime"]), [*host, "uptime"])
            self.assertEqual(ssh(["--", "uptime"]), [*host, "uptime"])
            self.assertEqual(ssh(["df", "-h"]), [*host, "df", "-h"])
            self.assertEqual(ssh(["-L", "8080:127.0.0.1:8080", "sudo", "uptime", "--yes"]),
                             ["-F", config, "-L", "8080:127.0.0.1:8080", "subspace-target", "sudo", "uptime", "--yes"])
            self.assertEqual(ssh([]), host)
            self.assertEqual(ssh(["--"], file=True), host)
            self.assertEqual(ssh(["--", "df", "-h", "--yes"], file=True), [*host, "df", "-h", "--yes"])


class IrcServerTests(unittest.TestCase):
    def test_private_addresses_only(self):
        import irc_server
        for address in ("10.0.0.4", "192.168.64.2", "100.101.102.103", "fd7a:115c:a1e0::1"):
            self.assertTrue(irc_server.private_ip(address), address)
        for address in ("0.0.0.0", "::", "203.0.113.7", "8.8.8.8", "not-an-ip"):
            self.assertFalse(irc_server.private_ip(address), address)

    def test_fleet_bundles_cannot_take_or_change_accounts(self):
        import irc_server
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(irc_server, "STATE", Path(directory)):
                (Path(directory) / "fleets").mkdir()
                fleet_setup.write_json(Path(directory) / "fleets" / "one.json", {
                    "fleet": "one", "accounts": [{"name": "one-atlas", "password": "p1"}], "channels": []})
                irc_server.conflicts("one", {"accounts": [{"name": "one-atlas", "password": "p1"},
                                                          {"name": "one-orion", "password": "p2"}]})
                with self.assertRaises(fleet_setup.SetupError):
                    irc_server.conflicts("one", {"accounts": [{"name": "one-atlas", "password": "changed"}]})
                with self.assertRaises(fleet_setup.SetupError):
                    irc_server.conflicts("two", {"accounts": [{"name": "one-atlas", "password": "p1"}]})

    def test_fleet_bundles_carry_the_standard_channels(self):
        import irc_server
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            fleet_setup.write_json(state / "answers.json", {"humans": ["admiral"]})
            bundle = {"fleet": "two", "accounts": [{"name": "two-nova", "password": "p1"}],
                      "channels": [{"name": "#two-starbase"}, {"name": "#two-lyra"}], "humans": ["admiral"]}
            with patch.object(irc_server, "STATE", state), patch.object(irc_server, "render") as render, \
                    patch("sys.stdin") as stdin:
                stdin.read.return_value = fleet_setup.json.dumps(bundle)
                with self.assertRaisesRegex(fleet_setup.SetupError, f"#two-{COMMAND_SUFFIX}"):
                    irc_server.add("-")
                render.assert_not_called()


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__).parse_args()
    unittest.main()
