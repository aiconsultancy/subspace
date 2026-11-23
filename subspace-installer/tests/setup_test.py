"""Offline checks that subspace-setup's generated inputs satisfy the installer.

Run with Python 3.10 or later: python3 -B subspace-installer/tests/setup_test.py
"""

import argparse
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "vm"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import fixture  # noqa: E402
import fleet_setup  # noqa: E402
from runtime import irc_officer, validate  # noqa: E402
from standard_channels import COMMAND_SUFFIX  # noqa: E402


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
        secrets = fleet_setup.fleet_secrets(store, members, answers["admin"], token, local)
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
             "--no-demo", "--irc", "remote", "--irc-host", "irc.example.ts.net", "--crew-claude", "1", "--crew-codex", "0"])
        cfg = validate(config, secrets, check_files=False)
        self.assertEqual(cfg["irc"]["mode"], "remote")
        self.assertEqual((cfg["irc"]["host"], cfg["irc"]["port"], cfg["irc"]["tls"]), ("irc.example.ts.net", 6697, True))
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

        self.assertTrue(up())
        written = api_env.read_text()
        self.assertIn("AGENT_PORT=3001\n", written)
        self.assertIn(f"AGENT_ORGANISATION=dev:{answers['organisation']}\n", written)
        self.assertTrue(fleet_setup.agent_listener_enabled())
        override = (fleet_setup.INPUTS / "api.override.yaml").read_text()
        self.assertIn('"127.0.0.1:3000:3000"', override)
        self.assertIn('"127.0.0.1:3001:3001"', override)
        self.assertNotIn("0.0.0.0", override)
        # An operator who clears the port keeps it off, and production never gets one.
        api_env.write_text(written.replace("AGENT_PORT=3001", "AGENT_PORT="))
        up()
        self.assertFalse(fleet_setup.agent_listener_enabled())
        api_env.write_text("NODE_ENV=production\n")
        up()
        self.assertNotIn("AGENT_PORT", api_env.read_text())

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

    def test_agents_may_reach_the_agent_listener_and_not_the_api(self):
        answers = {"operator": "operator", "api": True, "agentDispatch": True, "metadataUsers": []}
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "metadata_users", return_value=[]):
            args = fleet_setup.isolation_args(answers)
        protected = [args[i + 1] for i, arg in enumerate(args) if arg == "--operator-only-port"]
        self.assertEqual(protected, [str(fleet_setup.API_PORT), "5432"])
        self.assertNotIn(str(fleet_setup.AGENT_API_PORT), " ".join(args))
        self.assertNotEqual(fleet_setup.AGENT_API_PORT, fleet_setup.API_PORT)


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
