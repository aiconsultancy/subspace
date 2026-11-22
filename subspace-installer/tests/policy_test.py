"""Offline checks for network, GitHub and workspace policy."""

import json
import subprocess
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from github_policy import PolicyError, check_push, cli_permissions
from network import rules
import roles
from roles import default_permissions
import runtime
from runtime import Invalid, repository_config, toolchain_config, validate
import workspaces


def example_config():
    return json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text())


def officer_secrets(*extra):
    """Passwords for the example's rear admiral and captain, plus any extra accounts."""
    return {"proxyKey": "fixture-model-proxy-key", "ircPasswords": {
        account: "fixture-password" for account in ("example-atlas", "example-orion", *extra)
    }}


class FleetPolicyTests(unittest.TestCase):
    def test_firewall_scopes_operator_ports_and_proxy(self):
        result = rules({"dashboard": 8070, "agentsview": 8080, "proxy": 8317}, [1700], [701, 702])
        self.assertEqual(result.splitlines()[:2], ["add table inet subspace_fleet", "delete table inet subspace_fleet"])
        self.assertIn('oifname "lo" tcp dport 8070 meta skuid != { 0, 1700 } reject', result)
        self.assertIn('oifname "lo" tcp dport 8080 meta skuid != { 0, 1700 } reject', result)
        self.assertIn('oifname "lo" tcp dport 8317 meta skuid != { 0, 701, 702, 1700 } reject', result)
        self.assertIn('iifname != "lo" tcp dport { 8070, 8080, 8317 } reject', result)

    def test_github_high_level_and_api_policy(self):
        self.assertEqual(cli_permissions(["issue", "close", "12", "-R", "example/project"]), {"issues": "write"})
        self.assertEqual(cli_permissions(["api", "repos/example/project/pulls/12/files", "--paginate"]), {"pull_requests": "read"})
        for args in (
            ["api", "orgs/example/members"], ["api", "repos/example/project/installations"],
            ["api", "repos/example/project/issues", "-X", "POST"], ["api", "graphql"],
            ["pr", "merge", "4", "--admin"], ["repo", "delete", "example/project"],
            ["issue", "view", "12", "-R", "host.example.com/example/project"],
        ):
            with self.subTest(args=args), self.assertRaises(PolicyError):
                cli_permissions(args)

    def test_only_fast_forward_updates_pass_for_protected_branches(self):
        url = "https://github.com/example/project.git"
        old, new = "a" * 40, "b" * 40
        update = f"refs/heads/work {new} refs/heads/release/current {old}"
        with patch("github_policy.subprocess.run", return_value=subprocess.CompletedProcess([], 0)) as git:
            check_push(url, ["main", "release/*"], [update])
            git.assert_called_once_with(["/usr/bin/git", "merge-base", "--is-ancestor", old, new], check=False)
        with patch("github_policy.subprocess.run", return_value=subprocess.CompletedProcess([], 1)):
            with self.assertRaises(PolicyError):
                check_push(url, ["main", "release/*"], [update])
            check_push(url, ["main"], [update])
        with self.assertRaises(PolicyError):
            check_push(url, ["main"], [f"(delete) {'0' * 40} refs/heads/main {old}"])

    def test_private_repository_bypasses_shared_mirror(self):
        source = "https://github.com/example/project.git"
        repos = repository_config([{"id": "project", "url": source, "private": True}])
        people = [{"id": "atlas", "role": "rear_admiral"}, {"id": "orion", "role": "captain"}]
        home = Path("/agents/orion")
        references = Path("/references/atlas")
        helper = Path("/opt/fixture")
        with patch.object(workspaces, "refresh_mirror") as public, patch.object(workspaces, "refresh_reference") as rear, patch.object(workspaces, "prepare_member") as member:
            workspaces.prepare_workspaces(repos, people, {"orion": home}, Path("/cache"), "repos", references, helper)
        public.assert_not_called()
        rear.assert_called_once_with("repos", references / "project", source, None, helper, "ss-atlas")
        member.assert_called_once_with(people[1], home, "project", source, None, "repos", helper)
        assert "credential-read" in workspaces.private_credentials(helper, source)["GIT_CONFIG_VALUE_1"]
        with patch.object(workspaces, "as_user", return_value="fixture-installation-token") as mint:
            env = workspaces.private_credentials(helper, source, "ss-atlas")
        mint.assert_called_once_with("ss-atlas", helper, "bootstrap-token", source, output=True)
        self.assertIn("SUBSPACE_GIT_TOKEN", env)
        self.assertNotIn(env["SUBSPACE_GIT_TOKEN"], env["GIT_CONFIG_VALUE_1"])
        with self.assertRaises(Invalid):
            repository_config([{"id": "project", "url": "https://example.com/project.git", "private": True}])

    def test_private_repositories_need_apps_for_officers_only(self):
        config = example_config()
        config["crew"] = {"claude": {"count": 0}, "codex": {"count": 1}}
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git", "private": True}]
        apps = [
            {"appId": index, "installationId": index, "clientId": f"fixture-{index}",
             "botId": index, "botLogin": f"fixture-{index}[bot]"}
            for index in range(1, 3)
        ]
        config["rearAdmiral"]["githubApp"] = apps[0]
        with self.assertRaisesRegex(Invalid, "rear admiral and every captain"):
            validate(config, officer_secrets())
        config["captains"][0]["githubApp"] = apps[0]
        with self.assertRaisesRegex(Invalid, "distinct GitHub App identity"):
            validate(config, officer_secrets())
        config["captains"][0]["githubApp"] = apps[1]
        config["captains"][0]["crewNames"] = {"codex-1": "Rigel"}
        captain = validate(config, officer_secrets())["members"][1]
        self.assertEqual(captain["githubApp"], apps[1])
        self.assertEqual([(worker["id"], worker["name"]) for worker in captain["crew"]], [("rigel", "Rigel")])
        self.assertNotIn("githubApp", captain["crew"][0])

    def test_crew_run_under_their_captain_without_irc_accounts(self):
        config = example_config()
        config["repositories"] = [
            {"id": "project", "url": "https://github.com/example/project.git"},
            {"id": "library", "url": "https://github.com/example/library.git"},
        ]
        cfg = validate(config, officer_secrets())
        self.assertEqual([person["id"] for person in cfg["members"]], ["atlas", "orion"])
        crew = cfg["members"][1]["crew"]
        self.assertEqual([worker["id"] for worker in crew], ["claude-1", "claude-2", "codex-1", "codex-2"])
        self.assertEqual({worker["family"]: worker["model"] for worker in crew},
                         {"claude": "claude-sonnet-5-5", "codex": "gpt-5.4"})
        self.assertEqual(crew[0]["agent"], "orion-crew-claude-1")
        self.assertEqual(crew[0]["branch"], "crew/example-fleet-orion-claude-1")
        self.assertEqual(cfg["members"][0]["crew"], [])
        # Operators supply IRC passwords for the rear admiral and captains only.
        with self.assertRaisesRegex(Invalid, "Crew have no IRC accounts"):
            validate(config, officer_secrets("example-orion-codex-1"))
        # Without repositories there is nothing for crew worktrees to come from.
        config["repositories"] = []
        self.assertEqual(validate(config, officer_secrets())["members"][1]["crew"], [])

    def test_unsupported_crew_combinations_are_refused(self):
        repositories = [{"id": "project", "url": "https://github.com/example/project.git"}]
        cases = [
            ({"crew": {"claude": {"type": "claude-sdk"}}}, "claude-sdk crew are not supported"),
            ({"crew": {"codex": {"type": "opencode"}}}, "Remove crew type"),
            ({"crew": {"codex": {"permissions": [{"action": "*", "resource": "*", "effect": "allow"}]}}},
             "Stargate's crew coding profile"),
            ({"captain": {"crewGitHubApps": {}}}, "Remove crewGitHubApps"),
            ({"captain": {"harness": {"type": "claude-sdk", "family": "claude"}}}, "cannot host crew"),
            ({"captain": {"crewNames": {"claude-1": "Codex 1"}}}, "distinct IDs"),
            ({"captain": {"crewNames": {"claude-1": "Orion"}}}, "distinct IDs"),
            ({"captain": {"crewNames": {"claude-1": "Ægir"}}}, "ASCII"),
            ({"captain": {"crewNames": {"claude-3": "Rigel"}}}, "only enabled crew slots"),
            ({"rearAdmiral": {"crew": {"codex": {"count": 1}}}}, "Only captains may declare crew"),
            ({"repositories": [{"id": "9lives", "url": "https://github.com/example/nine.git"}]}, "start with a lowercase letter"),
            ({"repositories": [{"id": "default", "url": "https://github.com/example/default.git"}]}, "reserved"),
            ({"repositories": [{"id": f"repo{index}", "url": f"https://github.com/example/r{index}.git"} for index in range(10)]},
             "at most 9 repositories"),
            ({"crew": {"claude": {"count": 10}, "codex": {"count": 10}},
              "repositories": [{"id": f"repo{index}", "url": f"https://github.com/example/r{index}.git"} for index in range(3)]},
             "must not exceed 50"),
        ]
        for change, message in cases:
            with self.subTest(message=message):
                config = example_config()
                config["repositories"] = change.get("repositories", repositories)
                if "crew" in change:
                    config["crew"] = change["crew"]
                config["captains"][0].update(change.get("captain", {}))
                config["rearAdmiral"].update(change.get("rearAdmiral", {}))
                with self.assertRaisesRegex(Invalid, message):
                    validate(config, officer_secrets())
        # A Claude Agent SDK captain is valid without crew.
        config = example_config()
        config["captains"][0].update(harness={"type": "claude-sdk", "family": "claude"},
                                     crew={"claude": {"count": 0}, "codex": {"count": 0}})
        self.assertEqual(validate(config, officer_secrets())["members"][1]["crew"], [])

    def test_crew_plugin_options_give_each_member_a_worktree_per_repository(self):
        config = example_config()
        config["crew"] = {"claude": {"count": 1}, "codex": {"count": 1, "model": "gpt-5.5"}}
        config["repositories"] = [
            {"id": "project", "url": "https://github.com/example/project.git"},
            {"id": "library", "url": "https://github.com/example/library.git"},
        ]
        cfg = validate(config, officer_secrets())
        captain = cfg["members"][1]
        home = Path("/var/lib/subspace-fleet/agents/orion")
        options = runtime.crew_options(cfg, captain, home, 4097, "ses_fixture")
        self.assertEqual(options["namespace"], "example-fleet-orion")
        self.assertEqual(options["captainDirectory"], f"{home}/workspace/project")
        self.assertEqual(options["projectDirectory"], options["captainDirectory"])
        self.assertEqual(options["worktrees"], {"claude-1": f"{home}/worktrees/claude-1/project",
                                                "codex-1": f"{home}/worktrees/codex-1/project"})
        self.assertEqual(options["projects"], {"library": {
            "projectDirectory": f"{home}/workspace/library",
            "worktrees": {"claude-1": f"{home}/worktrees/claude-1/library", "codex-1": f"{home}/worktrees/codex-1/library"},
        }})
        self.assertEqual(options["stateDirectory"], f"{home}/.local/stargate-crew")
        self.assertEqual(options["mesijCommand"], "/opt/subspace-fleet/bin/mesij")
        self.assertEqual(options["captainSessionID"], "ses_fixture")
        team = options["team"]
        self.assertEqual(team["captain"], {"id": "orion", "agent": "build", "model": "openai-proxy/gpt-5.4"})
        self.assertEqual(team["modelProviders"], ["claude-proxy", "openai-proxy"])
        self.assertEqual(team["historyAPI"], {"url": "http://127.0.0.1:4097",
                                              "headersEnv": {"authorization": "OPENCODE_AUTHORIZATION"}})
        self.assertEqual([(worker["id"], worker["agent"], worker["model"], worker["permissionProfile"]) for worker in team["crew"]], [
            ("claude-1", "orion-crew-claude-1", "claude-proxy/claude-sonnet-5-5", "coding"),
            ("codex-1", "orion-crew-codex-1", "openai-proxy/gpt-5.5", "coding"),
        ])
        # Stargate requires distinct, non-nested captain and worker directories.
        paths = [options["captainDirectory"], *options["worktrees"].values(),
                 *(path for project in options["projects"].values() for path in project["worktrees"].values())]
        for index, path in enumerate(paths):
            for other in paths[index + 1:]:
                self.assertFalse(Path(path).is_relative_to(other) or Path(other).is_relative_to(path))

    def test_captain_engine_hosts_crew_agents_and_the_pinned_plugin(self):
        config = example_config()
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        cfg = validate(config, officer_secrets())
        captain = cfg["members"][1]
        home = Path("/var/lib/subspace-fleet/agents/orion")
        documents = roles.role_documents(cfg, captain, Path("/references/atlas"), home)
        saved = {}

        def save(name, settings, _user):
            saved[name] = settings
            return Path(f"/etc/subspace-fleet/{name}.json")

        with patch.object(runtime, "directory"), patch.object(runtime, "save_settings", side_effect=save), \
                patch.object(runtime, "serve_unit", return_value="subspace-fleet-engine-orion.service"):
            _, backend, headers, _ = runtime.render_engine(
                cfg, captain, home, home / "workspace/project", 4097, b"seed", documents, None)
            engine = saved["opencode-orion"]
            self.assertNotIn("plugins", engine)
            self.assertNotIn("sessionID", backend)
            _, backend, _, _ = runtime.render_engine(
                cfg, captain, home, home / "workspace/project", 4097, b"seed", documents, "ses_fixture")
        engine = saved["opencode-orion"]
        self.assertEqual(engine["default_agent"], "build")
        self.assertEqual(set(engine["providers"]), {"claude-proxy", "openai-proxy"})
        self.assertIn("anthropic-compatible", engine["providers"]["claude-proxy"]["package"])
        self.assertIn("openai-compatible/responses", engine["providers"]["openai-proxy"]["package"])
        crew_agent = engine["agents"]["orion-crew-codex-2"]
        self.assertEqual((crew_agent["model"], crew_agent["mode"]), ("openai-proxy/gpt-5.4", "primary"))
        self.assertIn("Codex 2 (`codex-2`)", crew_agent["system"])
        self.assertIn(f"{home}/worktrees/codex-2/project", crew_agent["system"])
        self.assertNotIn("ship channel", crew_agent["system"])
        [plugin] = engine["plugins"]
        self.assertEqual(plugin["package"], f"/opt/subspace-fleet/src/stargate-{runtime.STARGATE_REF}")
        self.assertEqual(plugin["options"]["captainSessionID"], "ses_fixture")
        self.assertEqual(backend["sessionID"], "ses_fixture")
        self.assertEqual((backend["agent"], backend["enforcePermissions"]), ("build", True))
        self.assertEqual(backend["permissions"][-len(runtime.CREW_GRANTS):], runtime.CREW_GRANTS)
        self.assertEqual(engine["permissions"], captain["harness"]["permissions"])
        environment = saved["engine-orion"]["environment"]
        self.assertEqual(environment["OPENCODE_AUTHORIZATION"], next(iter(headers.values())))
        self.assertIn("fleet-crew", documents["skills"])
        self.assertIn("stargate_crew", documents["instructions"])

    def test_crew_worktrees_are_added_to_the_captain_checkout(self):
        captain = {"id": "orion", "role": "captain", "crew": [
            {"id": "claude-1", "branch": "crew/example-fleet-orion-claude-1"},
            {"id": "codex-1", "branch": "crew/example-fleet-orion-codex-1"},
        ]}
        home = Path("/agents/orion")
        checkout = home / "workspace/project"
        calls = []

        def git(user, *args, output=False, env=None, shared=False):
            calls.append((user, args))
            if args[-2:] == ("rev-parse", "HEAD"):
                return "a" * 40
            if "--git-common-dir" in args:
                return str(checkout / ".git")
            return "" if output else None

        source = "https://github.com/example/project.git"
        with patch.object(workspaces, "clone"), patch.object(workspaces, "check_origin"), \
                patch.object(workspaces, "as_user"), patch.object(workspaces, "git", side_effect=git):
            workspaces.prepare_member(captain, home, "project", source, Path("/cache/project.git"), "repos", Path("/opt/fixture/github_app.py"))
        added = [args for user, args in calls if "worktree" in args and "add" in args]
        self.assertEqual(added, [
            ("-C", checkout, "worktree", "add", "--quiet", "-b", "crew/example-fleet-orion-claude-1",
             home / "worktrees/claude-1/project", "a" * 40),
            ("-C", checkout, "worktree", "add", "--quiet", "-b", "crew/example-fleet-orion-codex-1",
             home / "worktrees/codex-1/project", "a" * 40),
        ])
        self.assertTrue(all(user == "ss-orion" for user, args in calls if args[:2] == ("-C", checkout)))

    def test_stargate_fleet_api_is_an_operator_listener(self):
        example = Path(__file__).resolve().parents[1] / "fleet.example.json"
        config = json.loads(example.read_text())
        config["crew"] = {"claude": {"count": 0}, "codex": {"count": 0}}
        secrets = {"proxyKey": "fixture-model-proxy-key", "ircPasswords": {
            account: "fixture-password" for account in ("example-atlas", "example-orion")
        }}
        self.assertIsNone(validate(config, secrets)["stargate"]["api"])
        config["stargate"] = {"api": {}}
        cfg = validate(config, secrets)
        self.assertEqual(cfg["stargate"]["api"], {"port": 8484})
        self.assertIn(8484, cfg["listeners"])
        config["stargate"] = {"api": {"port": 8070}}
        with self.assertRaisesRegex(Invalid, "distinct ports"):
            validate(config, secrets)
        config["stargate"] = {"api": {"port": 8484, "listen": "0.0.0.0:8484"}}
        with self.assertRaises(Invalid):
            validate(config, secrets)
        result = rules({"dashboard": 8070, "agentsview": 8080, "proxy": 8317, "stargate-api": 8484}, [1700], [701])
        self.assertIn('oifname "lo" tcp dport 8484 meta skuid != { 0, 1700 } reject', result)

    def test_toolchain_versions_are_data(self):
        self.assertEqual(toolchain_config({"node": False, "go": "1.26.1"}), {"go": "1.26.1"})
        with self.assertRaises(Invalid):
            toolchain_config({"rust": "latest\n[env]"})

    def test_role_policies_allow_reading_and_bound_writes(self):
        reference = Path("/references/atlas")
        rear = default_permissions("rear_admiral", reference)
        captain = default_permissions("captain", reference)
        self.assertEqual(rear[0], {"action": "*", "resource": "*", "effect": "deny"})
        self.assertIn({"action": "external_directory", "resource": f"{reference}/*", "effect": "allow"}, rear)
        self.assertFalse(any(rule["action"] == "edit" for rule in rear))
        self.assertIn({"action": "edit", "resource": "*", "effect": "allow"}, captain)
        self.assertFalse(any(rule["resource"].startswith("git push") for rule in captain))


if __name__ == "__main__":
    unittest.main()
