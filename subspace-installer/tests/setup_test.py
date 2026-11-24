"""Offline checks that subspace-setup's generated inputs satisfy the installer.

Run with Python 3.10 or later: python3 -B subspace-installer/tests/setup_test.py
"""

import argparse
import base64
import hashlib
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
from unittest.mock import patch
from urllib.error import HTTPError, URLError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "vm"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import fixture  # noqa: E402
import fleet_setup  # noqa: E402
import irc_accounts  # noqa: E402
import runtime  # noqa: E402
from runtime import irc_officer, validate  # noqa: E402
from standard_channels import COMMAND_SUFFIX  # noqa: E402


# Tests never reach the network: setup's outbound check is replaced here, and OutboundTests checks the real one.
check_outbound = fleet_setup.check_outbound
fleet_setup.check_outbound = lambda answers: None


def fake_api(api):
    """Send subspace-setup's API calls to the fixture's in-memory API."""
    return patch.object(fleet_setup, "api_call", side_effect=lambda _token, method, path, body=None: api.call(method, path, body))


def answers_for(argv):
    args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", *argv])
    with patch.object(fleet_setup.pwd, "getpwnam"):
        answers = fleet_setup.gather(args, {})
        rear, captains = fleet_setup.check_answers(answers)
    return answers, rear, captains


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
        self.assertEqual([m["account"] for m in members], ["example-fleet-atlas", "example-fleet-orion", "example-fleet-vega"])
        # Crew have no IRC accounts; without repositories they have no worktrees either.
        self.assertEqual(set(secrets["ircPasswords"]), {"example-fleet-atlas", "example-fleet-orion", "example-fleet-vega", "admiral"})
        self.assertEqual(config["crew"], {"claude": {"count": 2}, "codex": {"count": 2}})
        self.assertEqual(config["repositories"], [])
        # Setup names no channels: the installer makes the standard set, with a separate command channel.
        self.assertNotIn("starbaseChannel", config["irc"])
        self.assertNotIn("commandChannel", config["irc"])
        command = f"#example-fleet-{COMMAND_SUFFIX}"
        self.assertEqual(cfg["irc"]["starbaseChannel"], "#example-fleet-starbase")
        self.assertEqual(cfg["irc"]["commandChannel"], command)
        standard = ["#example-fleet-starbase", command, "#example-fleet-orion", "#example-fleet-vega"]
        self.assertEqual(cfg["irc"]["channels"], standard)
        self.assertEqual(fleet_setup.fleet_channels(answers, members), standard)
        rear = irc_officer(cfg, cfg["members"][0], {"account": "example-fleet-atlas", "backend": {"directory": "/w"}})
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
        self.assertEqual({row["name"] for row in bundle["accounts"]}, {"fleet-one-atlas", "fleet-one-orion"})
        self.assertEqual({row["name"] for row in bundle["accounts"]}, set(secrets["ircPasswords"]))
        self.assertEqual([row["name"] for row in bundle["channels"]],
                         ["#fleet-one-starbase", f"#fleet-one-{COMMAND_SUFFIX}", "#fleet-one-orion"])
        self.assertEqual(cfg["irc"]["channels"], [row["name"] for row in bundle["channels"]])
        self.assertIn("dispatch", bundle["channels"][1]["topic"])
        self.assertEqual(bundle["humans"], ["admiral"])

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
        secrets = fleet_setup.fleet_secrets(store, members, answers["admin"])
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
                "--repositories", "https://github.com/example/project.git"]
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

    def test_repositories_answer_fills_the_fleet_list_and_gives_captains_crew(self):
        argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion", "--no-demo",
                "--repositories", "https://github.com/example/project.git, library=https://github.com/example/lib.git"]
        _, members, config, secrets, _ = self.generate(argv)
        self.assertEqual(config["repositories"], [
            {"id": "project", "url": "https://github.com/example/project.git"},
            {"id": "library", "url": "https://github.com/example/lib.git"},
        ])
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual([worker["id"] for worker in cfg["members"][1]["crew"]], ["claude-1", "claude-2", "codex-1", "codex-2"])
        self.assertEqual(set(secrets["ircPasswords"]), {"example-fleet-atlas", "example-fleet-orion", "admiral"})
        # The list lives in fleet.json: a later run without the flag keeps it, including hand edits.
        config["repositories"][1]["private"] = True
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.json", config)
        answers, rear, captains = answers_for(argv[:7])
        kept = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(kept["repositories"], config["repositories"])
        # A new answer replaces the list; a repository that stays keeps its ID and visibility.
        answers, rear, captains = answers_for([*argv[:7], "--repositories", "https://github.com/example/lib.git"])
        replaced = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(replaced["repositories"], [config["repositories"][1]])
        fleet_setup.save_answers(answers)
        self.assertNotIn("repositories", fleet_setup.read_json(fleet_setup.INPUTS / "answers.json"))
        # An explicit ID=URL answer renames a known repository and keeps its visibility.
        answers, rear, captains = answers_for([*argv[:7], "--repositories", "shared=https://github.com/example/lib.git"])
        renamed = fleet_setup.fleet_config(answers, fleet_setup.roster(answers, rear, captains), "a" * 40)
        self.assertEqual(renamed["repositories"], [{"id": "shared", "url": "https://github.com/example/lib.git",
                                                    "private": True}])

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

    def test_rejects_unsafe_or_ambiguous_answers(self):
        cases = [
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Atlas"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--admin", "x-orion"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Orion", "--admin", "NickServ"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", f"Orion, {COMMAND_SUFFIX.title()}"],
            ["--name", "X", "--rear-admiral", "Atlas", "--captains", "Starbase"],
            ["--name", "A very long fleet name indeed", "--rear-admiral", "Atlas", "--captains", "Andromeda Galaxy"],
            ["--name", "X", "--rear-admiral", "9lives", "--captains", "Orion"],
            # The rear admiral's name is its API agent name, and a ship's name is at most 64 characters.
            ["--name", "X", "--rear-admiral", "Atlas Prime", "--captains", "Orion"],
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
        self.assertEqual(registration["rearAdmiral"], {
            "name": "Nova", "ircNick": "fleet-two-nova", "defaultModel": "gpt-5.4",
            "serviceTokenClientId": "fleet-two-nova@agents.example.com"})
        self.assertEqual([(row["ship"], row["channel"], row["ircNick"]) for row in registration["captains"]],
                         [("Lyra", "#fleet-two-lyra", "fleet-two-lyra"), ("Vega", "#fleet-two-vega", "fleet-two-vega")])
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
        self.assertEqual(api.summary()["ships"], [("Lyra", "#fleet-two-lyra"), ("Vega", "#fleet-two-vega")])
        self.assertEqual([row[:3] for row in api.summary()["agents"]], [
            ("Nova", "rearAdmiral", "fleet-two-nova"), ("fleet-two-lyra", "captain", "fleet-two-lyra"),
            ("fleet-two-vega", "captain", "fleet-two-vega")])
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
            {**registration, "captains": [{**captain, "channel": "#lyra"}]},
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
        self.assertEqual(records["rearAdmiral"]["ircNick"], "example-fleet-atlas")
        self.assertEqual(api.summary()["fleets"]["example-fleet"]["starbaseChannel"], "#example-fleet-starbase")
        self.assertEqual(api.summary()["agents"], [
            ("Atlas", "rearAdmiral", "example-fleet-atlas", None, True),
            ("example-fleet-orion", "captain", "example-fleet-orion", "Orion", True)])
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


class AgentDispatchSetupTests(unittest.TestCase):
    """Setup asks whether to switch on agent dispatch; unattended, it stays off."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        fleet_setup.INPUTS = Path(self.directory.name)
        self.argv = ["--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                     "--repositories", "https://github.com/example/project.git", "--no-demo"]
        self.api = {"url": "http://127.0.0.1:3000", "initiatives": ["00000000-0000-4000-8000-000000000001"]}

    def config(self, argv, api_block):
        answers, rear, captains = answers_for(argv)
        members = fleet_setup.roster(answers, rear, captains)
        config = fleet_setup.fleet_config(answers, members, "a" * 40, api_block, False, "b" * 40)
        secrets = fleet_setup.fleet_secrets(fleet_setup.Secrets(fleet_setup.INPUTS, answers["slug"], None), members,
                                            answers["admin"], "header.payload.admin", True,
                                            "header.payload.rear" if api_block else "")
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
                             "token-for-example-fleet-atlas@agents.example.com")
        rear = next(row for row in api.agents.values() if row["rank"] == "rearAdmiral")
        self.assertEqual((rear["serviceTokenClientId"], rear["fleetId"], rear["ircNick"]),
                         ("example-fleet-atlas@agents.example.com", fixture.FLEET, "example-fleet-atlas"))
        captains = [row for row in api.agents.values() if row["rank"] == "captain"]
        self.assertEqual([(row["name"], row["fleetId"]) for row in captains], [("example-fleet-orion", fixture.FLEET)])
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
        for name, scratch, branch in (("install.sh", "inputs=/root/subspace-setup-dry-run",
                                       'if "$dry_run"; then\n        # The dry run'),
                                      ("install.ps1", "'/root/subspace-setup-dry-run'",
                                       "if ($dryRun) {\n        # The dry run")):
            text = (root / name).read_text()
            with self.subTest(name):
                self.assertIn(scratch, text)
                for file in ("database-url", "database-ca.pem", "irc-ca.pem"):
                    self.assertNotRegex(text, rf"/root/fleet/{re.escape(file)}\b")
                    self.assertRegex(text, rf"\$inputs/{re.escape(file)}")
                dry = text[text.index(branch):]
                dry = dry[:dry.index("return") + len("return")]
                self.assertIn("rm", dry)
                self.assertIn("-rf", dry)
                self.assertNotIn("summary.env", dry)
                self.assertLess(text.index(branch), text.index("summary.env", text.index(branch)))
                # The installer copy that the dry run runs goes in the scratch directory too, never over
                # /opt/subspace/bootstrap, which only the real run replaces, after the dry-run branch.
                self.assertRegex(dry, r'(send_payload|Send-Payload) "\$inputs/bootstrap"')
                self.assertIn("$inputs/bootstrap/subspace-installer/vm/setup.sh", dry)
                self.assertRegex(text[text.index(branch) + len(dry):], r"\n    (send_payload|Send-Payload)\n")
        # Run from that copy, setup keeps the subspace-setup command pointing at the installed one.
        setup = (root / "subspace-installer/vm/setup.sh").read_text()
        self.assertIn('if ! "$dry_run" && [[ $(readlink /usr/local/sbin/subspace-setup', setup)

    def test_a_run_the_installer_refuses_migrates_nothing(self):
        # A fleet configuration the installer refuses, such as a Slack bot without its tokens, is found before
        # the lockdown, the API or --migrate change anything.
        (fleet_setup.INPUTS / "fleet.secrets").mkdir(mode=0o700)
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--inputs", str(fleet_setup.INPUTS), "--stargate-ref", "c" * 40,
                "--database-url-file", self.url_file, "--migrate"]
        checks = []

        def refuse(config_path, dry_run=False, secrets_dir=None, strict=False):
            checks.append((dry_run, strict, secrets_dir, json.loads(Path(config_path).read_text())["slug"]))
            raise fleet_setup.SetupError("Supply slack-app-token in captain orion's secrets directory.")
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                patch.object(fleet_setup, "installer", side_effect=refuse), \
                patch.object(fleet_setup, "run") as run, patch.object(fleet_setup, "api_up") as api_up, \
                patch.object(fleet_setup, "compose") as compose, patch.object(fleet_setup, "irc_source"), \
                patch.object(fleet_setup, "say"), \
                self.assertRaisesRegex(fleet_setup.SetupError, "slack-app-token"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertEqual(checks, [(True, True, fleet_setup.INPUTS / "fleet.secrets", "example-fleet")])
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
                                            answers["admin"], "header.payload.signature")
        cfg = validate(config, secrets, check_files=False)
        rear, orion = cfg["members"]
        self.assertEqual((rear["slack"]["appId"], orion["slack"]["appId"], orion["sessionRoot"]),
                         ("AFIXTURE1", "AFIXTURE2", "workspace"))
        self.assertEqual(cfg["repositoryNotes"]["project"], {"description": "The example service.",
                                                             "tracker": "Example issues"})
        self.assertEqual(cfg["irc"]["auth"], "password")
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

    def answers(self, *extra, saved=None):
        args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", "--irc", "remote",
                                       "--irc-host", "irc.example.com", *extra])
        with patch.object(fleet_setup.pwd, "getpwnam"):
            answers = fleet_setup.gather(args, saved or {})
            fleet_setup.check_answers(answers)
        return answers, args

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
        with self.assertRaisesRegex(fleet_setup.SetupError, "needs a remote IRC server"):
            args = fleet_setup.parse_args(["--non-interactive", "--operator", "operator", "--name", "Example Fleet",
                                           "--rear-admiral", "Atlas", "--captains", "Orion", "--irc", "local",
                                           "--irc-auth", "certificate"])
            fleet_setup.gather(args, {})
        for extra, message in ((["--irc-operator", "ircop"], "together"),
                               (["--irc-auth", "password", "--irc-operator", "ircop",
                                 "--irc-operator-password-file", "/x"], "for certificate sign-in")):
            with self.subTest(extra), self.assertRaisesRegex(fleet_setup.SetupError, message):
                self.answers(*extra)

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
                "Suites: noble\n")
            self.assertEqual(fleet_setup.apt_mirrors(apt), ["http://mirror.example.com/ubuntu",
                                                            "http://archive.example.org/ubuntu/"])

    def test_no_outbound_access_stops_at_once_and_says_what_to_open(self):
        targets = [("GitHub", "https://github.com"), ("the package mirror", "http://archive.example.org/ubuntu/")]
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
        # Docker's repository only matters when the VM runs containers.
        with patch.object(fleet_setup, "apt_mirrors", return_value=[]):
            self.assertNotIn("Docker's package repository",
                             dict(fleet_setup.outbound_targets({"api": False, "irc": "remote"})))
            self.assertIn("Docker's package repository", dict(fleet_setup.outbound_targets({"api": True})))

    def test_the_release_lookup_says_why_it_failed(self):
        with patch.object(fleet_setup, "run", side_effect=fleet_setup.SetupError("git ls-remote did not finish")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "no outbound access to GitHub.*--stargate-ref"):
            fleet_setup.latest_stargate()


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
