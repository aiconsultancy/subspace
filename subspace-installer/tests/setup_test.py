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
import fleet_setup  # noqa: E402
from runtime import irc_officer, validate  # noqa: E402
from standard_channels import COMMAND_SUFFIX  # noqa: E402


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
        self.assertIn("#aurora-nav", config["irc"]["compose"]["channels"])
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

    def test_further_fleet_registration_is_checked(self):
        answers, rear, captains = answers_for(["--name", "Fleet Two", "--rear-admiral", "Nova", "--captains", "Lyra",
                                               "--irc", "remote", "--irc-host", "irc.example.com", "--no-api"])
        members = fleet_setup.roster(answers, rear, captains)
        registration = fleet_setup.fleet_registration(answers, members)
        self.assertEqual(registration["channels"], ["#fleet-two-starbase", f"#fleet-two-{COMMAND_SUFFIX}", "#fleet-two-lyra"])
        self.assertEqual((registration["ircHost"], registration["ircPort"], registration["ircTls"]),
                         ("irc.example.com", 6697, True))
        self.assertEqual(registration["captainNames"], ["Lyra"])
        fleet_setup.write_json(fleet_setup.INPUTS / "answers.json", {"api": True, "slug": "fleet-one"})
        (fleet_setup.INPUTS / "api.env").write_text("AUTH_PROVIDERS=dev\nENABLE_DEV_AUTH=true\n")
        bad = [
            {**registration, "slug": "fleet-one"},
            {**registration, "channels": ["#fleet-one-starbase"]},
            # Each standard channel is required: no command channel, or a captain without a ship channel.
            {**registration, "channels": ["#fleet-two-starbase", "#fleet-two-lyra"]},
            {**registration, "channels": registration["channels"][:2]},
            {**registration, "captainNames": "Lyra"},
            {**registration, "slug": "Fleet Two"},
            {**registration, "ircPort": "6697"},
        ]
        for value in bad:
            with self.subTest(value=value), patch.object(fleet_setup, "api_token") as token:
                with self.assertRaises(fleet_setup.SetupError):
                    fleet_setup.register_fleet(value)
                token.assert_not_called()

    def test_initiative_gains_new_channels_and_keeps_added_ones(self):
        calls = []

        def api(_token, method, path, body=None):
            calls.append((method, path, body))
            if path == "/me/initiatives":
                return {"items": [{"id": "i-1", "name": "Example Fleet", "channels": "#example-starbase #extra"}]}
            if path == "/initiatives/i-1" and method == "GET":
                return {"version": 4}
            return {}

        with patch.object(fleet_setup, "api_call", side_effect=api):
            found = fleet_setup.fleet_initiative("t", "Example Fleet", ["#example-starbase", "#example-vega"], "i-1")
        self.assertEqual(found, "i-1")
        self.assertEqual(calls[-1], ("PATCH", "/initiatives/i-1/channel-configuration",
                                     {"version": 4, "channels": "#example-starbase #extra #example-vega"}))

    def test_initiative_drops_retired_channels_and_skips_other_fleets(self):
        calls = []

        def api(_token, method, path, body=None):
            calls.append((method, path, body))
            if path == "/me/initiatives":
                return {"items": [{"id": "mine", "name": "Fleet", "channels": "#f-starbase #f-orion #extra"},
                                  {"id": "theirs", "name": "Fleet", "channels": "#o-starbase"}]}
            if path.startswith("/initiatives?"):
                return {"items": [{"id": "theirs", "name": "Fleet", "fleet": {"slug": "other"}},
                                  {"id": "mine", "name": "Fleet", "fleet": {"slug": "f"}}]}
            if path == "/initiatives/mine" and method == "GET":
                return {"version": 7}
            return {}

        with patch.object(fleet_setup, "api_call", side_effect=api):
            found = fleet_setup.fleet_initiative("t", "Fleet", ["#f-starbase", "#f-vega"], None,
                                                 ["#f-starbase", "#f-orion"], "f")
        self.assertEqual(found, "mine")
        self.assertEqual(calls[-1], ("PATCH", "/initiatives/mine/channel-configuration",
                                     {"version": 7, "channels": "#f-starbase #extra #f-vega"}))

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

    def test_agents_are_registered_once_and_the_rear_admiral_gets_its_own_token(self):
        answers, rear, captains = answers_for([*self.argv, "--agent-dispatch"])
        members = fleet_setup.roster(answers, rear, captains)
        calls = []
        agents = []

        def api(token, method, path, body=None):
            calls.append((method, path))
            if path == "/fleets/example-fleet":
                return {"id": "fleet-uuid"}
            if path.startswith("/agents?"):
                return {"items": agents, "nextCursor": None}
            if method == "POST" and path == "/agents":
                agents.append({**body, "isActive": True})
                return body
            raise AssertionError(path)

        with patch.object(fleet_setup, "api_call", side_effect=api), \
                patch.object(fleet_setup, "api_token", side_effect=lambda _answers, email=None: f"token-for-{email}"):
            token = fleet_setup.dispatch_agents(answers, members, "header.payload.admin")
            self.assertEqual(token, "token-for-example-fleet-atlas@agents.example.com")
            self.assertEqual([(agent["name"], agent["rank"], agent.get("ship")) for agent in agents], [
                ("example-fleet-atlas", "rearAdmiral", None), ("example-fleet-orion", "captain", "Orion")])
            self.assertTrue(all(agent["fleetId"] == "fleet-uuid" for agent in agents))
            fleet_setup.dispatch_agents(answers, members, "header.payload.admin")
        self.assertEqual(sum(1 for call in calls if call == ("POST", "/agents")), 2)
        agents[1]["fleetId"] = "another-fleet"
        with patch.object(fleet_setup, "api_call", side_effect=api), \
                self.assertRaisesRegex(fleet_setup.SetupError, "belongs to another fleet"):
            fleet_setup.dispatch_agents(answers, members, "header.payload.admin")

    def test_a_rerun_keeps_a_token_with_most_of_its_life_left(self):
        import base64
        import json

        def jwt(exp):
            payload = base64.urlsafe_b64encode(json.dumps({"exp": exp}).encode()).decode().rstrip("=")
            return f"header.{payload}.signature"

        self.assertEqual(fleet_setup.token_seconds_left(jwt(1000), now=400), 600)
        self.assertEqual(fleet_setup.token_seconds_left(jwt(100), now=400), 0)
        self.assertEqual(fleet_setup.token_seconds_left("not-a-jwt"), 0)
        answers, rear, captains = answers_for([*self.argv, "--agent-dispatch"])
        members = fleet_setup.roster(answers, rear, captains)
        agents = [{"serviceTokenClientId": fleet_setup.agent_subject(person["account"]), "isActive": True,
                   "rank": "captain" if index else "rearAdmiral", "fleetId": "fleet-uuid"}
                  for index, person in enumerate(members)]

        def api(_token, _method, path, _body=None):
            return {"id": "fleet-uuid"} if path.startswith("/fleets/") else {"items": agents}

        fresh = jwt(int(fleet_setup.time.time()) + 8 * 3600)
        fleet_setup.write_json(fleet_setup.INPUTS / "fleet.secrets.json", {"rearAdmiralApiToken": fresh})
        with patch.object(fleet_setup, "api_call", side_effect=api), \
                patch.object(fleet_setup, "api_token", return_value="minted") as mint:
            self.assertEqual(fleet_setup.dispatch_agents(answers, members, "admin"), fresh)
            mint.assert_not_called()
            fleet_setup.write_json(fleet_setup.INPUTS / "fleet.secrets.json",
                                   {"rearAdmiralApiToken": jwt(int(fleet_setup.time.time()) + 3600)})
            self.assertEqual(fleet_setup.dispatch_agents(answers, members, "admin"), "minted")

    def test_only_the_gateway_user_may_also_reach_the_api(self):
        answers = {"operator": "operator", "api": True, "agentDispatch": True, "metadataUsers": []}
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "metadata_users", return_value=[]):
            self.assertIn("3000:ss-fleet-gateway", fleet_setup.isolation_args(answers))
            self.assertNotIn("--port-user", fleet_setup.isolation_args({**answers, "agentDispatch": False}))
        with patch.object(fleet_setup.pwd, "getpwnam", side_effect=KeyError), \
                patch.object(fleet_setup, "metadata_users", return_value=[]):
            self.assertNotIn("--port-user", fleet_setup.isolation_args(answers))


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
