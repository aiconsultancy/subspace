"""Offline checks for network, GitHub and workspace policy."""

import contextlib
import hashlib
import io
import json
import os
import pwd
import re
import ssl
import stat
import subprocess
import tarfile
import tempfile
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import fixture  # noqa: E402
import fleet_records  # noqa: E402
import github_app
from github_policy import PolicyError, check_push, cli_permissions, merge_repository
from network import ip_address, rules
import roles
from roles import default_permissions
import runtime
from runtime import Invalid, repository_config, toolchain_config, validate
import standard_channels
import workspaces


# Tests never reach the network: Git's anonymous repository check is replaced here; RepositoryAccessTests
# checks the real one.
readable_without_credentials = runtime.readable_without_credentials
runtime.readable_without_credentials = lambda url: True


def stargate_team_problems(team):
    """Stargate's own check of a crew team (src/coordination/crew/team.ts, from its release that lets a crew name
    differ from its ID): crew IDs unique and none the captain's, captain or all; compared without case, a name is
    no other member's name or ID, nor captain or all. The members it would refuse."""
    problems, ids = [], {team["captain"]["id"], "captain", "all"}
    for member in team["crew"]:
        if member["id"] in ids:
            problems.append(member["id"])
        ids.add(member["id"])
    members = [team["captain"], *team["crew"]]
    owners = {"captain": -1, "all": -1, **{member["id"]: index for index, member in enumerate(members)}}
    for index, member in enumerate(members):
        name = member.get("name", "").strip().lower()
        if name and owners.setdefault(name, index) != index:
            problems.append(member.get("name"))
    return problems


def example_config():
    # Most of these tests name the officers' accounts as a fleet installed before plain names did; PlainNamesTests
    # covers plain names, the example's own.
    config = fixture.legacy_names(json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text()))
    # These tests sign officers in with passwords; IrcCertificateTests covers the default, certificates.
    config["irc"]["auth"] = "password"
    return config


def api_secrets():
    """The example's officer secrets plus the fixture API token that an API configuration needs."""
    return {**officer_secrets(), "apiToken": "eyJhbGciOiJub25lIn0.eyJzdWIiOiJhZG1pbiJ9.admin"}


def officer_secrets(*extra):
    """Passwords for the example's rear admiral and captain, plus any extra accounts."""
    return {"proxyKey": "fixture-model-proxy-key", "ircPasswords": {
        account: "fixture-password" for account in ("example-atlas", "example-orion", *extra)
    }}


class FleetPolicyTests(unittest.TestCase):
    def test_firewall_scopes_operator_ports_and_proxy(self):
        result = rules({"dashboard": 8070, "agentsview": 8080, "proxy": 8317}, [1700], [701, 702], 703)
        self.assertEqual(result.splitlines()[:2], ["add table inet subspace_fleet", "delete table inet subspace_fleet"])
        self.assertIn('oifname "lo" tcp dport 8070 meta skuid != { 0, 1700 } reject', result)
        self.assertIn('oifname "lo" tcp dport 8080 meta skuid != { 0, 703, 1700 } reject', result)
        self.assertIn('oifname "lo" tcp dport 8317 meta skuid != { 0, 701, 702, 1700 } reject', result)
        self.assertIn('iifname != "lo" tcp dport { 8070, 8080, 8317 } reject', result)

    def test_firewall_applies_private_egress_only_to_officers_before_dnat(self):
        result = rules({"dashboard": 8070}, [1700], [701, 702, 900], 703,
                       officers=[701, 702], dns={ip_address("10.0.0.53")},
                       allowed={(ip_address("fd00::44"), 443, None), (ip_address("10.0.0.42"), 6697, 702)})
        self.assertIn("type filter hook output priority -110; policy accept;", result)
        self.assertIn("meta skuid { 701, 702 } jump officer_destinations", result)
        reject = result.index("ip daddr { 10.0.0.0/8")
        for allowance in ("ip daddr 10.0.0.53 tcp dport 53 return", "ip daddr 10.0.0.53 udp dport 53 return",
                          "ip6 daddr fd00::44 tcp dport 443 return", "meta skuid 702 ip daddr 10.0.0.42 tcp dport 6697 return"):
            self.assertLess(result.index(allowance), reject)
        self.assertIn("ip6 daddr { fc00::/7, fe80::/10 } reject", result)
        self.assertEqual(runtime.agent_egress(["git.example.com:443", "[fd00::44]:8443"]),
                         [("git.example.com", 443), ("fd00::44", 8443)])
        for destination in ("git.example.com", "git.example.com:0", "git.example.com:80/path", "fd00::44:8443"):
            with self.subTest(destination), self.assertRaisesRegex(Invalid, "Invalid agent egress destination"):
                runtime.agent_egress([destination])

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

    def test_captain_merge_is_bound_to_the_checkout_and_full_reviewed_head(self):
        checkout = Path("/fleet/agents/orion/workspace/project")
        policy = {"enabled": True, "contents": "write", "captainMergeCheckouts": {str(checkout): "example/project"}}
        args = ["pr", "merge", "4", "--squash", "--match-head-commit", "a" * 40]
        self.assertEqual(merge_repository(args, policy, checkout), "example/project")
        for directory in (checkout.parent / "other", checkout.parent.parent / "worktrees/crew/project"):
            with self.subTest(directory=directory), self.assertRaises(PolicyError):
                merge_repository(args, policy, directory)
        for override in ({"contents": "read"}, {"captainMergeCheckouts": {}}):
            with self.subTest(override=override), self.assertRaises(PolicyError):
                merge_repository(args, {**policy, **override}, checkout)
        for invalid in (args[:-2], args[:-1] + ["abc"], args + ["--repo", "other/project"],
                        args + ["--admin"], args + ["--auto"], args + ["--delete-branch"]):
            with self.subTest(args=invalid), self.assertRaises(PolicyError):
                merge_repository(invalid, policy, checkout)
        with patch.object(Path, "cwd", return_value=checkout), patch.object(github_app, "active_policy", return_value=policy), \
                patch.object(github_app, "installation_token", return_value="fixture-token") as mint, \
                patch.object(github_app.os, "execve") as gh:
            github_app.main(["gh", *args])
        self.assertEqual(mint.call_args.args[1:], ({"contents": "write"}, ["project"]))
        self.assertEqual(gh.call_args.args[1], ["gh", "api", "--method", "PUT", "repos/example/project/pulls/4/merge",
                                                "-f", "sha=" + "a" * 40, "-f", "merge_method=squash"])

    def test_merge_repository_selection_only_grants_write_capable_captains(self):
        config = example_config()
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        config["githubPolicy"] = {"captainMergeRepositories": ["EXAMPLE/PROJECT"]}
        app = {"appId": 1, "installationId": 2, "clientId": "fixture-client", "botLogin": "fixture[bot]", "botId": 3}
        config["captains"][0]["githubApp"] = app
        cfg = validate(config, officer_secrets(), check_files=False)
        rear, captain = cfg["members"]
        self.assertNotIn("mergeCheckouts", rear)
        self.assertEqual(captain["mergeCheckouts"], {"project": "example/project"})
        command = "subspace-github gh pr merge 4 --squash --match-head-commit " + "a" * 40
        self.assertEqual(fixture.opencode_decision(runtime.harness_permissions(cfg, captain), "shell", [command]), "allow")
        self.assertEqual(fixture.opencode_decision(runtime.harness_permissions(cfg, rear), "shell", [command]), "deny")
        app["contents"] = "read"
        self.assertNotIn("mergeCheckouts", validate(config, officer_secrets(), check_files=False)["members"][1])
        config["githubPolicy"]["captainMergeRepositories"] = ["example/library"]
        with self.assertRaisesRegex(Invalid, "must name configured GitHub repositories"):
            validate(config, officer_secrets(), check_files=False)

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
            ({"captain": {"crewNames": {"claude-1": "R2.D2"}}}, "ASCII letters, digits, spaces, hyphens"),
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
        # The captain's default model with its reasoning effort, as OpenCode's model#variant.
        self.assertEqual(team["captain"], {"id": "orion", "agent": "build", "model": "claude-proxy/claude-opus-5-5#xhigh"})
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
        self.assertEqual([rule["resource"] for rule in backend["permissions"]
                          if rule["action"] == "stargate_model"],
                         ["inspect", "effort", "get"])
        self.assertEqual([variant["id"] for variant in
                          engine["providers"]["openai-proxy"]["models"]["gpt-5.4"]["variants"]],
                         ["low", "medium", "high", "xhigh"])
        self.assertEqual(engine["permissions"], captain["harness"]["permissions"])
        environment = saved["engine-orion"]["environment"]
        self.assertEqual(environment["OPENCODE_AUTHORIZATION"], next(iter(headers.values())))
        self.assertIn("fleet-crew", documents["skills"])
        self.assertIn("stargate_crew", documents["instructions"])

    def test_operator_rules_can_restrict_the_captain_effort_grant(self):
        config, secrets = example_config(), officer_secrets()
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        config["harness"] = {"captain": {"extraPermissions": [
            {"action": "stargate_model", "resource": "effort", "effect": "deny"},
        ]}}
        cfg = validate(config, secrets)
        captain = cfg["members"][1]
        rules = runtime.captain_permissions(cfg, captain)
        self.assertEqual(fixture.opencode_decision(rules, "stargate_model", ["inspect"]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "stargate_model", ["effort"]), "deny")
        self.assertEqual(fixture.opencode_decision(rules, "stargate_model", ["switch"]), "deny")

    def test_officers_reach_the_api_agent_listener_as_themselves(self):
        config = example_config()
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        config["api"] = {"url": "http://127.0.0.1:3000", "agentUrl": "http://127.0.0.1:3001"}
        cfg = validate(config, api_secrets())
        self.assertEqual(cfg["api"]["agentMcpUrl"], "http://127.0.0.1:3001/mcp")
        home = Path("/var/lib/subspace-fleet/agents/orion")
        saved = {}

        def save(name, settings, _user):
            saved[name] = settings
            return Path(f"/etc/subspace-fleet/{name}.json")

        for person in cfg["members"]:
            documents = roles.role_documents(cfg, person, Path("/references/atlas"), home)
            # The same generic skill a person's local OpenCode installs from agent-skills/.
            self.assertEqual(documents["skills"]["subspace-tracker"], (
                Path(__file__).resolve().parents[2] / "agent-skills/opencode/subspace-tracker/SKILL.md").read_text())
            with patch.object(runtime, "directory"), patch.object(runtime, "save_settings", side_effect=save), \
                    patch.object(runtime, "serve_unit", return_value="subspace-fleet-engine.service"):
                _, backend, _, _ = runtime.render_engine(
                    cfg, person, home, home / "workspace/project", 4097, b"seed", documents, "ses_fixture")
            engine = saved[f"opencode-{person['id']}"]
            # The officer, and crew sessions on its server, act as the officer's agent record, with no token.
            self.assertEqual(engine["mcp"]["servers"]["subspace"], {
                "type": "remote", "url": "http://127.0.0.1:3001/mcp",
                "headers": {"X-Subspace-Agent": person["account"]},
            })
            self.assertNotIn("Authorization", json.dumps(engine["mcp"]))
            self.assertIn(runtime.SUBSPACE_MCP_GRANT, engine["permissions"])
            self.assertIn(runtime.SUBSPACE_MCP_GRANT, backend["permissions"])
            self.assertEqual(backend["permissions"].count(runtime.SUBSPACE_MCP_GRANT), 1)

    def test_claude_officers_get_the_api_server_and_skill_in_their_prompt(self):
        config = example_config()
        config["api"] = {"url": "http://127.0.0.1:3000", "agentUrl": "http://127.0.0.1:3001"}
        cfg = validate(config, api_secrets())
        person = cfg["members"][1]
        documents = roles.role_documents(cfg, person, None, Path("/home"))
        skill = documents["skills"]["subspace-tracker"].partition("\n---\n")[2]
        self.assertTrue(skill.strip() and documents["claudePrompt"].endswith(skill))
        self.assertNotIn("name: subspace-tracker", documents["claudePrompt"])
        without = dict(config, api={"url": "http://127.0.0.1:3000"})
        plain = roles.role_documents(validate(without, api_secrets()), person, None, Path("/home"))
        self.assertNotIn("subspace_whoami", plain["claudePrompt"])

    def test_claude_officer_backend_names_the_server_and_only_built_in_tools(self):
        # Stargate permits a configured server's tools itself (mcp__<server>) and its allowedTools accepts
        # only built-in tool names, so the installer adds the server and leaves allowedTools alone.
        config = example_config()
        config["api"] = {"url": "http://127.0.0.1:3000", "agentUrl": "http://127.0.0.1:3001"}
        cfg = validate(config, api_secrets())
        person = dict(cfg["members"][1], harness={
            "type": "claude-sdk", "family": "claude", "model": "example-model", "effort": None, "permissions": None,
            "allowedTools": ["Read"]})
        documents = {"claudePrompt": "Example prompt"}
        backend = runtime.claude_backend(cfg, person, Path("/workspace"), documents)
        self.assertNotIn("effort", backend)
        self.assertEqual(runtime.claude_backend(cfg, {**person, "harness": {**person["harness"], "effort": "xhigh"}},
                                                Path("/workspace"), documents)["effort"], "xhigh")
        self.assertIn("subspace_files", backend["mcpServers"])
        self.assertEqual({"subspace": backend["mcpServers"]["subspace"]}, {"subspace": {
            "type": "http", "url": "http://127.0.0.1:3001/mcp",
            "headersHelper": f"/opt/subspace-fleet/lib/runtime.py agent-header {person['account']}",
        }})
        self.assertEqual(backend["allowedTools"], ["Read"])
        self.assertEqual(backend["systemPrompt"], "Example prompt")
        self.assertNotIn("mcpServers", runtime.claude_backend(
            validate(dict(config, api={"url": "http://127.0.0.1:3000"}), api_secrets()), person,
            Path("/workspace"), documents))

    def test_an_operator_rule_for_the_api_tools_stands(self):
        config = example_config()
        config["api"] = {"url": "http://127.0.0.1:3000", "agentUrl": "http://127.0.0.1:3001"}
        cfg = validate(config, api_secrets())
        person = cfg["members"][0]
        base = person["harness"]["permissions"]
        self.assertEqual(runtime.harness_permissions(cfg, person)[-1], runtime.SUBSPACE_MCP_GRANT)
        for effect in ("deny", "ask"):
            for action in ("subspace_*", "subspace_create_issue"):
                rule = {"action": action, "resource": "*", "effect": effect}
                operator = dict(person, harness=dict(person["harness"], permissions=[*base, rule]))
                granted = runtime.harness_permissions(cfg, operator)
                self.assertEqual(granted, [*base, rule])
                self.assertNotIn(runtime.SUBSPACE_MCP_GRANT, granted)
        # The agent listener has no REST command of its own; a hosted API's is granted to every officer.
        self.assertNotIn(runtime.SUBSPACE_REQUEST_GRANT, runtime.harness_permissions(cfg, person))

    def test_a_hosted_api_gives_every_officer_its_rest_command_and_the_operator_the_last_word(self):
        def hosted(harness=None):
            config = example_config()
            config["api"] = {"url": "https://subspace.example.com", "enrolment": True}
            config["stargate"] = {"ref": runtime.SUBSPACE_AGENTS_COMMIT}
            if harness:
                config["harness"] = {**config["harness"], **harness}
            cfg = validate(config, officer_secrets())
            return [(person, runtime.harness_permissions(cfg, person)) for person in cfg["members"]]

        def request(granted):
            return fixture.opencode_decision(granted, "shell", ["subspace-request /api/v1/me"])

        for person, granted in hosted():
            self.assertEqual(granted, [*person["harness"]["permissions"], runtime.SUBSPACE_REQUEST_GRANT,
                                       runtime.SUBSPACE_MCP_GRANT])
            self.assertEqual(request(granted), "allow")
        # After the starter policy and before the operator's own rules, which win under last-match-wins, a
        # wildcard included: a role's deny of subspace-* or of every command holds.
        for resource in ("subspace-*", "*"):
            rule = {"action": "shell", "resource": resource, "effect": "deny"}
            rear, captain = hosted({"captain": {"extraPermissions": [rule]}})
            self.assertEqual(captain[1][-3:], [runtime.SUBSPACE_REQUEST_GRANT, rule, runtime.SUBSPACE_MCP_GRANT])
            self.assertEqual((request(rear[1]), request(captain[1])), ("allow", "deny"))
        # An explicit harness.permissions is the operator's too, so the grant comes first and the policy decides.
        own = [{"action": "*", "resource": "*", "effect": "allow"}]
        for _, granted in hosted({"permissions": own}):
            self.assertEqual(granted, [runtime.SUBSPACE_REQUEST_GRANT, *own, runtime.SUBSPACE_MCP_GRANT])
            self.assertEqual(request(granted), "allow")
        closed = [{"action": "*", "resource": "*", "effect": "deny"}, {"action": "read", "resource": "*", "effect": "allow"}]
        for _, granted in hosted({"permissions": closed}):
            self.assertEqual(request(granted), "deny")

    def test_without_the_agent_listener_officers_get_no_api_server_or_skill(self):
        config = example_config()
        config["api"] = {"url": "http://127.0.0.1:3000"}
        cfg = validate(config, api_secrets())
        person = cfg["members"][1]
        self.assertNotIn("subspace-tracker", roles.role_documents(cfg, person, None, Path("/home"))["skills"])
        self.assertEqual(runtime.harness_permissions(cfg, person), person["harness"]["permissions"])
        self.assertIsNone(runtime.subspace_mcp_url(cfg))

    def test_a_missing_api_skill_is_named_before_anything_is_rendered(self):
        config = example_config()
        config["api"] = {"url": "http://127.0.0.1:3000", "agentUrl": "http://127.0.0.1:3001"}
        with patch.object(roles, "API_SKILL_SOURCES", (Path("/nonexistent/SKILL.md"),)):
            with self.assertRaises(Invalid) as raised:
                validate(config, api_secrets())
        self.assertEqual(str(raised.exception), roles.API_SKILL_MISSING)
        self.assertIn("agent-skills/opencode/subspace-tracker/SKILL.md", roles.API_SKILL_MISSING)
        # The installer script stops with the same words before it changes anything.
        with tempfile.TemporaryDirectory() as directory:
            copy = Path(directory) / "subspace-installer"
            copy.mkdir()
            script = copy / "install.sh"
            script.write_text((Path(__file__).resolve().parents[1] / "install.sh").read_text())
            fleet = Path(directory) / "fleet.json"
            fleet.write_text(json.dumps(config))
            result = subprocess.run(["bash", str(script), "--config", str(fleet), "--dry-run"],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stderr.strip(), f"subspace-fleet: {roles.API_SKILL_MISSING}")
            # Without the agent listener, the missing skill is not needed.
            fleet.write_text(json.dumps({**config, "api": {"url": "http://127.0.0.1:3000"}}))
            result = subprocess.run(["bash", str(script), "--config", str(fleet), "--dry-run"],
                                    capture_output=True, text=True)
            self.assertNotIn("subspace-tracker", result.stderr)

    def test_the_agent_listener_must_be_on_loopback(self):
        for agent_url in ("https://api.example.com", "http://192.0.2.10:3001", "http://127.0.0.1:3001/api"):
            config = example_config()
            config["api"] = {"url": "http://127.0.0.1:3000", "agentUrl": agent_url}
            with self.assertRaises(Invalid):
                validate(config, api_secrets())

    def test_claude_officers_take_the_agent_header_from_a_helper(self):
        output = subprocess.run(
            [sys.executable, str(Path(runtime.__file__)), "agent-header", "example-orion"],
            capture_output=True, text=True, check=True).stdout
        self.assertEqual(json.loads(output), {"X-Subspace-Agent": "example-orion"})
        refused = subprocess.run(
            [sys.executable, str(Path(runtime.__file__)), "agent-header", "Example Orion"], capture_output=True)
        self.assertNotEqual(refused.returncode, 0)

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

    def test_private_crew_worktrees_fetch_the_remote_head_once(self):
        captain = {"id": "orion", "role": "captain", "crew": [
            {"id": "claude-1", "branch": "crew/example-fleet-orion-claude-1"},
            {"id": "codex-1", "branch": "crew/example-fleet-orion-codex-1"},
        ]}
        home = Path("/agents/orion")
        checkout = home / "workspace/project"
        calls = []

        def git(user, *args, output=False, env=None, shared=False):
            calls.append(args)
            if args[-2:] == ("rev-parse", "FETCH_HEAD"):
                return "b" * 40
            if "--git-common-dir" in args:
                return str(checkout / ".git")
            return "" if output else None

        source = "https://github.com/example/private.git"
        with patch.object(workspaces, "clone"), patch.object(workspaces, "check_origin"), \
                patch.object(workspaces, "as_user"), patch.object(workspaces, "private_credentials", return_value={}), \
                patch.object(workspaces, "git", side_effect=git):
            workspaces.prepare_member(captain, home, "project", source, None, "repos", Path("/opt/fixture/github_app.py"))
        self.assertEqual(sum("fetch" in args for args in calls), 1)
        self.assertEqual(sum("add" in args for args in calls if "worktree" in args), 2)

    def test_a_deleted_crew_worktree_is_added_again_on_its_branch(self):
        home = Path("/agents/orion")
        checkout = home / "workspace/project"
        dest = home / "worktrees/claude-1/project"
        branch = "crew/example-fleet-orion-claude-1"
        listing = (f"worktree {checkout}\nHEAD {'a' * 40}\nbranch refs/heads/main\n\n"
                   f"worktree {dest}\nHEAD {'a' * 40}\nbranch refs/heads/{branch}\n"
                   "prunable gitdir file points to non-existent location\n")
        calls = []

        def git(user, *args, output=False, env=None, shared=False):
            calls.append(args)
            if args[-2:] == ("list", "--porcelain"):
                return listing.strip()
            if args[-3:-1] == ("branch", "--list"):
                return f"  {branch}"
            if "--git-common-dir" in args:
                return str(checkout / ".git")
            return "" if output else None

        def start_point():
            raise AssertionError("An existing branch needs no start point.")

        with patch.object(workspaces, "as_user"), patch.object(workspaces, "git", side_effect=git):
            workspaces.prepare_crew("ss-orion", checkout, dest, branch, start_point)
        worktree = [args[2:] for args in calls if args[2:3] == ("worktree",)]
        self.assertEqual(worktree, [("worktree", "list", "--porcelain"), ("worktree", "remove", dest),
                                    ("worktree", "add", "--quiet", dest, branch)])

    def test_stargate_fleet_api_is_an_operator_listener(self):
        example = Path(__file__).resolve().parents[1] / "fleet.example.json"
        config = fixture.legacy_names(json.loads(example.read_text()))
        config["irc"]["auth"] = "password"
        config["crew"] = {"claude": {"count": 0}, "codex": {"count": 0}}
        secrets = {"proxyKey": "fixture-model-proxy-key", "ircPasswords": {
            account: "fixture-password" for account in ("example-atlas", "example-orion")
        }}
        self.assertIsNone(validate(config, secrets)["stargate"]["api"])
        config["stargate"] = {"api": {}}
        cfg = validate(config, secrets)
        # Stargate allows only subspace://* by default; the Windows Fleet Console needs its own origin.
        self.assertEqual(cfg["stargate"]["api"],
                         {"port": 8484, "corsOrigins": ["subspace://*", "https://subspace.fleet-console"]})
        self.assertIn(8484, cfg["listeners"])
        config["stargate"] = {"api": {"corsOrigins": ["subspace://*", "https://subspace.console-two"]}}
        self.assertEqual(validate(config, secrets)["stargate"]["api"]["corsOrigins"][1], "https://subspace.console-two")
        config["stargate"] = {"api": {"corsOrigins": ["https://user@subspace.example"]}}
        with self.assertRaisesRegex(Invalid, "CORS origin"):
            validate(config, secrets)
        config["stargate"] = {"api": {"port": 8070}}
        with self.assertRaisesRegex(Invalid, "distinct ports"):
            validate(config, secrets)
        config["stargate"] = {"api": {"port": 8484, "listen": "0.0.0.0:8484"}}
        with self.assertRaises(Invalid):
            validate(config, secrets)
        result = rules({"dashboard": 8070, "agentsview": 8080, "proxy": 8317, "stargate-api": 8484}, [1700], [701], 703)
        self.assertIn('oifname "lo" tcp dport 8484 meta skuid != { 0, 1700 } reject', result)

    def test_stargate_is_pinned_to_a_release_with_agent_dispatch(self):
        # A commit, labelled with its release. scripts/stargate-pin.py checks the label names that commit
        # and moves both to Stargate's latest release; the installer fetches the tag so the Fleet API
        # reports the version.
        self.assertRegex(runtime.STARGATE_REF, r"\A[0-9a-f]{40}\Z")
        release = re.fullmatch(r"v(\d+)\.(\d+)\.(\d+)", runtime.STARGATE_RELEASE)
        self.assertIsNotNone(release, runtime.STARGATE_RELEASE)
        self.assertGreaterEqual(tuple(int(part) for part in release.groups()), (0, 43, 0))
        cfg = validate(example_config(), officer_secrets())
        self.assertEqual(cfg["stargate"]["ref"], runtime.STARGATE_REF)

    def test_toolchain_versions_are_data(self):
        # More in toolchains_test.py.
        self.assertEqual(toolchain_config({"node": False, "go": "1.26.1"}),
                         {"go": {"versions": ["1.26.1"], "install": "shared", "options": {}}})
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


def workspace_fleet(**api):
    """The example fleet with crew, two repositories and, optionally, the Subspace API."""
    config = example_config()
    config["crew"] = {"claude": {"count": 1}, "codex": {"count": 1}}
    config["repositories"] = [
        {"id": "project", "url": "https://github.com/example/project.git", "description": "The example service.",
         "tracker": "Example issues at https://tracker.example.com/project"},
        {"id": "library", "url": "https://github.com/example/library.git"},
    ]
    config["captains"].append({"id": "vega", "name": "Vega", "account": "example-vega"})
    secrets = officer_secrets("example-vega")
    if api:
        config["api"] = api
        secrets = {**secrets, "apiToken": api_secrets()["apiToken"]}
    return validate(config, secrets)


def captain_fleet(tools=None, **changes):
    """The example fleet with a repository, a captain with crew (orion) and one without (vega)."""
    config = example_config()
    config.pop("captainTools", None)
    if tools:
        config["captainTools"] = tools
    config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
    config["captains"].append({"id": "vega", "name": "Vega", "account": "example-vega",
                               "crew": {"claude": {"count": 0}, "codex": {"count": 0}}})
    config.update(changes)
    return validate(config, officer_secrets("example-vega"))


# Command lines and the resources OpenCode 2.3.2's shell scanner (packages/core/src/shell/scan.ts and parse.ts)
# gives their commands, from reading its source: a command line splits at &&, ||, ;, | and newlines; a subshell's
# or group's commands are listed; a command substitution's commands follow the command that holds them; `cd` to a
# literal directory is no resource (only an external_directory check); and each resource is the command's text
# from its first word, variable assignments included, to its last, quoted as typed.
DELEGATE_ALLOWED = (
    ("pwd", ["pwd"]),
    ("head -n 40 README.md", ["head -n 40 README.md"]),
    ("git fetch origin && git log --oneline origin/main..HEAD | head -20",
     ["git fetch origin", "git log --oneline origin/main..HEAD", "head -20"]),
    ("cd project && git diff origin/main...HEAD --stat", ["git diff origin/main...HEAD --stat"]),
    ("git -C project status --short; git -C project branch --show-current",
     ["git -C project status --short", "git -C project branch --show-current"]),
    ("GIT_PAGER=cat git log -3 --format=%s", ["GIT_PAGER=cat git log -3 --format=%s"]),
    ("git branch -a && git stash list && git worktree list && git tag --list",
     ["git branch -a", "git stash list", "git worktree list", "git tag --list"]),
    ("git config --get user.email && git remote -v", ["git config --get user.email", "git remote -v"]),
    ("rg -n 'git push' scripts | wc -l", ["rg -n 'git push' scripts", "wc -l"]),
    ("git -C project log --grep 'stash'", ["git -C project log --grep 'stash'"]),
    ("make test", ["make test"]),
    ("subspace-github gh pr create --title 'Fix the parser' --body-file /tmp/body.md",
     ["subspace-github gh pr create --title 'Fix the parser' --body-file /tmp/body.md"]),
    ("subspace-api /api/v1/me", ["subspace-api /api/v1/me"]),
)
DELEGATE_REFUSED = (
    ("git commit -m 'Fix the parser'", ["git commit -m 'Fix the parser'"]),
    ("git add -A && git commit -m wip", ["git add -A", "git commit -m wip"]),
    ("git status; git push origin HEAD", ["git status", "git push origin HEAD"]),
    ("(cd project && git push)", ["git push"]),
    ("echo $(git push)", ["echo $(git push)", "git push"]),
    ("GIT_TERMINAL_PROMPT=0 git push origin crew/example", ["GIT_TERMINAL_PROMPT=0 git push origin crew/example"]),
    ("env git push", ["env git push"]),
    ("git -C project push", ["git -C project push"]),
    ("git -c user.name=Orion commit --amend --no-edit", ["git -c user.name=Orion commit --amend --no-edit"]),
    ("git ls-files -z | xargs -0 git rm", ["git ls-files -z", "xargs -0 git rm"]),
    ("git stash", ["git stash"]),
    ("git stash pop", ["git stash pop"]),
    ("git branch -D crew/example", ["git branch -D crew/example"]),
    ("git checkout -b fix", ["git checkout -b fix"]),
    ("git switch main", ["git switch main"]),
    ("git reset --hard origin/main", ["git reset --hard origin/main"]),
    ("git merge --ff-only origin/main", ["git merge --ff-only origin/main"]),
    ("git config user.email orion@example.com", ["git config user.email orion@example.com"]),
    # A read form in a write's arguments allows nothing: Git's writes are refused after the read forms.
    ("git -C project commit -m 'stash list'", ["git -C project commit -m 'stash list'"]),
    ("sed -i 's/a/b/' src/main.ts", ["sed -i 's/a/b/' src/main.ts"]),
    ("find . -name '*.ts' | xargs sed -i 's/a/b/'", ["find . -name '*.ts'", "xargs sed -i 's/a/b/'"]),
    ("perl -pi -e 's/a/b/' src/main.ts", ["perl -pi -e 's/a/b/' src/main.ts"]),
    ("/usr/bin/git push", ["/usr/bin/git push"]),
    ("git  push origin HEAD", ["git  push origin HEAD"]),
    ("git   commit -m wip", ["git   commit -m wip"]),
    # The README's known false refusal: a bare Git write word after a global option.
    ("git -C project log --grep stash", ["git -C project log --grep stash"]),
)
# The listing forms of Git's mixed subcommands, and their writes, each checked as typed, after a global option and
# after an assignment (DELEGATE_PREFIXES).
DELEGATE_GIT_LISTINGS = (
    "config --get user.email", "config --get-all remote.origin.fetch", "config --get-regexp '^remote\\.'",
    "config --list", "config -l", "config --list --show-origin", "config get user.email", "config list",
    "config get --local user.email", "config --show-origin --list", "config --name-only --list",
    *(f"config --{scope} {read}" for scope in ("local", "global", "system", "worktree")
      for read in ("--list", "-l", "--get user.email", "--get-regexp alias", "--list --show-origin")),
    "config -f .gitmodules --get-regexp path", "config --file .gitmodules --list",
    "branch", "branch -a", "branch -r", "branch -v", "branch -vv", "branch --verbose", "branch --all",
    "branch --remotes", "branch -av", "branch -avv", "branch -rv", "branch -rvv", "branch -a -vv", "branch -vv -a",
    "branch -r --sort=-committerdate", "branch -a --format='%(refname:short) %(committerdate)'",
    "branch -a --color=always", "branch --list", "branch --list 'crew/*'", "branch -l", "branch -a --list 'crew/*'",
    "branch -vv --list", "branch --sort=-committerdate --list", "branch --show-current", "branch --contains HEAD",
    "branch -r --contains HEAD", "branch --merged origin/main", "branch --no-merged origin/main",
    "branch -a --merged", "branch --points-at HEAD",
    "tag", "tag -l", "tag -ln", "tag --list", "tag -l 'v*'", "tag --list --sort=-v:refname",
    "tag --sort=-v:refname --list", "tag --contains HEAD", "tag --points-at HEAD",
    "remote", "remote -v", "remote --verbose", "remote show origin", "remote get-url origin",
    "stash list", "stash show", "stash show -p 'stash@{0}'", "worktree list", "worktree list --porcelain",
    "notes list", "notes show HEAD", "submodule", "submodule status", "submodule status --recursive",
    "sparse-checkout list",
)
DELEGATE_GIT_LISTING_WRITES = (
    # Deleting, renaming or copying through a listing option, alone or bundled, and a name after -v.
    "branch -r -d origin/stale", "branch -rd origin/stale", "branch -dr origin/stale", "branch -r -D origin/stale",
    "branch -r --delete origin/stale", "branch -r -vd origin/stale", "branch -a -d crew/example",
    "branch -ad crew/example", "branch -a -D crew/example", "branch -a -m old new", "branch -a -M old new",
    "branch -a -c old new", "branch --all --move old new", "branch -r -u origin/main",
    "branch -a --set-upstream-to=origin/main", "branch -r --unset-upstream", "branch -a --edit-description",
    "branch -m old -a", "branch -v crew/example", "branch -vv crew/example", "branch --verbose crew/example",
    "branch crew/example", "branch -f crew/example origin/main", "branch -d crew/example", "branch -D crew/example",
    "tag v1.0", "tag -a v1.0 -m release", "tag -d v1.0", "tag -f v1.0",
    "remote -v add upstream https://example.com/x.git", "remote add upstream https://example.com/x.git",
    "remote set-url origin https://example.com/x.git", "remote remove upstream",
    "config user.email orion@example.com", "config --local user.email orion@example.com",
    "config --global --unset user.name", "config --unset user.email", "config --local --add a.b c",
    "stash", "stash push -m wip", "stash pop", "stash drop", "worktree add ../x", "worktree remove ../x",
    "notes add -m x", "submodule update --init", "sparse-checkout set src",
)
DELEGATE_PREFIXES = ("git", "git -C project", "GIT_OPTIONAL_LOCKS=0 git")
# What the rules cannot see, as the README says: they match command text, so these still run.
DELEGATE_NOT_REFUSED = (
    ("bash -c 'git push'", ["bash -c 'git push'"]),
    ("echo fix > src/main.ts", ["echo fix > src/main.ts"]),
    ("cp /tmp/main.ts src/main.ts", ["cp /tmp/main.ts src/main.ts"]),
    ("python3 -c 'import pathlib; pathlib.Path(\"x\").write_text(\"y\")'",
     ["python3 -c 'import pathlib; pathlib.Path(\"x\").write_text(\"y\")'"]),
)


class CaptainPolicyTests(unittest.TestCase):
    """captainTools: standard captains code; delegate captains have an open shell with writes refused."""

    worktrees = "/var/lib/subspace-fleet/agents/orion/worktrees"

    def test_a_delegate_captain_runs_any_command_but_writes(self):
        cfg = captain_fleet("delegate")
        rules = cfg["members"][1]["harness"]["permissions"]
        for line, resources in DELEGATE_ALLOWED + DELEGATE_NOT_REFUSED:
            with self.subTest(line):
                self.assertEqual(fixture.opencode_decision(rules, "shell", resources), "allow")
        for line, resources in DELEGATE_REFUSED:
            with self.subTest(line):
                self.assertEqual(fixture.opencode_decision(rules, "shell", resources), "deny")
        for prefix in DELEGATE_PREFIXES:
            for command, effect in ([(listing, "allow") for listing in DELEGATE_GIT_LISTINGS]
                                    + [(write, "deny") for write in DELEGATE_GIT_LISTING_WRITES]):
                with self.subTest(f"{prefix} {command}"):
                    self.assertEqual(fixture.opencode_decision(rules, "shell", [f"{prefix} {command}"]), effect)
        # No edit, write or patch tool (OpenCode's edit action); reads, but not of secrets, in any directory.
        self.assertEqual(fixture.opencode_decision(rules, "edit", ["src/main.ts"]), "deny")
        self.assertEqual(fixture.opencode_decision(rules, "read", ["src/main.ts", "/tmp/notes.md"]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "read", ["deploy/.env"]), "deny")
        self.assertEqual(fixture.opencode_decision(rules, "external_directory", ["/tmp/*"]), "allow")

    def test_the_rear_admiral_can_render_a_diagram_from_standard_input(self):
        config = captain_fleet()
        rear = config["members"][0]
        rules = rear["harness"]["permissions"]
        command = 'mermaid-render - "${TMPDIR:-/tmp}/diagram.png"'
        instructions = roles.role_documents(config, rear, Path("/references/example"), Path("/example"))["instructions"]
        self.assertIn(command, instructions)
        self.assertEqual(fixture.opencode_decision(rules, "shell", [command]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "edit", ["diagram.mmd"]), "deny")

    def test_a_standard_captain_keeps_its_policy_and_reads_its_crews_worktrees(self):
        cfg = captain_fleet()
        self.assertEqual(cfg["captainTools"], "standard")
        rear, orion, vega = cfg["members"]
        rules = orion["harness"]["permissions"]
        self.assertFalse(orion.get("delegatesCoding"))
        self.assertEqual(fixture.opencode_decision(rules, "edit", ["src/main.ts"]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "shell", ["git add -A", "git commit -m wip"]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "shell", ["git push origin HEAD"]), "deny")
        for command in ("mermaid-render diagram.mmd diagram.png", "playwright screenshot URL page.png"):
            self.assertEqual(fixture.opencode_decision(rules, "shell", [command]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "shell", ["playwright test"]), "deny")
        self.assertEqual(fixture.opencode_decision(rules, "shell", ["git status", "head -20"]), "deny")
        # Its crew's worktrees, and nothing else outside its workspace (the read tool and a shell's directory).
        for directory in (f"{self.worktrees}/claude-1/project/*", f"{self.worktrees}/codex-2/project/src/*"):
            self.assertEqual(fixture.opencode_decision(rules, "external_directory", [directory]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "external_directory", ["/tmp/*"]), "deny")
        self.assertIn({"action": "external_directory", "resource": f"{self.worktrees}/*", "effect": "allow"}, rules)
        # It reads them, but its edit, write and patch tools, which name an outside file by its absolute path,
        # are refused there; its own checkout's files, named relative to its directory, it edits as before.
        crew_file = f"{self.worktrees}/claude-1/project/src/main.ts"
        self.assertEqual(fixture.opencode_decision(rules, "read", [crew_file]), "allow")
        self.assertEqual(fixture.opencode_decision(rules, "edit", [crew_file]), "deny")
        self.assertEqual(fixture.opencode_decision(rules, "edit", ["project/src/main.ts"]), "allow")
        # A captain without crew has no worktrees; the rear admiral has its reference checkouts only.
        self.assertFalse(any(rule["action"] == "external_directory" for rule in vega["harness"]["permissions"]))
        self.assertEqual([rule["resource"] for rule in rear["harness"]["permissions"]
                          if rule["action"] == "external_directory"], ["/var/lib/subspace-fleet/references/atlas/*"])
        # The starter policy is otherwise what it was.
        self.assertEqual([rule for rule in rules if rule["action"] != "external_directory" and
                          rule["resource"] != f"{self.worktrees}/*"], default_permissions("captain", None))

    def test_a_captain_without_crew_keeps_the_standard_policy_in_a_delegate_fleet(self):
        cfg = captain_fleet("delegate")
        _, orion, vega = cfg["members"]
        self.assertTrue(orion["delegatesCoding"])
        self.assertFalse(vega.get("delegatesCoding"))
        self.assertEqual(vega["harness"]["permissions"], default_permissions("captain", None))
        self.assertIn("orion leave edits, commits and pushes to their crew", runtime.captain_tools_line(cfg))
        self.assertIn("vega has no crew, so keeps the standard policy", runtime.captain_tools_line(cfg))
        self.assertIn("Captain tools: standard", runtime.captain_tools_line(captain_fleet()))

    def test_the_delegate_guidance_follows_captain_tools_not_the_permissions(self):
        own = [{"action": "*", "resource": "*", "effect": "allow"}]
        for harness in ({}, {"permissions": own}):
            with self.subTest(harness=harness):
                cfg = captain_fleet("delegate", harness={**harness, "type": "opencode"})
                orion = cfg["members"][1]
                self.assertTrue(orion["delegatesCoding"])
                documents = roles.role_documents(cfg, orion, None, Path("/var/lib/subspace-fleet/agents/orion"))
                self.assertIn("## Your tools", documents["instructions"])
                self.assertIn("open the pull request with `subspace-github gh pr create`",
                              documents["skills"]["fleet-captain"])
                if harness:
                    self.assertEqual(orion["harness"]["permissions"], own)
        standard = roles.role_documents(captain_fleet(), captain_fleet()["members"][1], None, Path("/h"))
        self.assertNotIn("## Your tools", standard["instructions"])
        self.assertIn("Crew commit as you, so check", standard["skills"]["fleet-captain"])

    def test_extra_permissions_extend_the_starter_policy_and_an_explicit_policy_still_replaces_it(self):
        def rule(resource, effect="allow"):
            return {"action": "shell", "resource": resource, "effect": effect}

        harness = {"type": "opencode", "extraPermissions": [rule("kubectl get *")],
                   "captain": {"extraPermissions": [rule("git stash *")]},
                   "rearAdmiral": {"extraPermissions": [rule("make docs *")]}}
        cfg = captain_fleet("delegate", harness=harness)
        rear, orion, vega = cfg["members"]
        # Appended in order, fleet, role, officer, after the starter policy, so they win.
        self.assertEqual(orion["harness"]["permissions"],
                         [*default_permissions("captain", None, delegate=True), rule("kubectl get *"), rule("git stash *")])
        self.assertEqual(rear["harness"]["permissions"][-2:], [rule("kubectl get *"), rule("make docs *")])
        self.assertEqual(rear["harness"]["permissions"][:-2],
                         default_permissions("rear_admiral", Path("/var/lib/subspace-fleet/references/atlas")))
        self.assertEqual(fixture.opencode_decision(orion["harness"]["permissions"], "shell", ["git stash pop"]), "allow")
        self.assertTrue(orion["delegatesCoding"])
        # An officer's own rules come last; an explicit harness.permissions replaces the starter policy and is
        # extended the same way.
        own = [{"action": "*", "resource": "*", "effect": "allow"}]
        config = example_config()
        config["captains"][0]["harness"] = {"permissions": own, "extraPermissions": [rule("git push *", "deny")]}
        captain = validate(config, officer_secrets())["members"][1]
        self.assertEqual(captain["harness"]["permissions"], [*own, rule("git push *", "deny")])
        # Validated like harness.permissions, at every level, and refused for a Claude Agent SDK officer's own.
        for change in ({"harness": {"extraPermissions": {"action": "shell"}}},
                       {"harness": {"captain": {"extraPermissions": [{"action": "shell", "resource": "*"}]}}},
                       {"harness": {"extraPermissions": [{**rule("*"), "effect": "maybe"}]}}):
            with self.subTest(change), self.assertRaises(Invalid):
                validate({**example_config(), **change}, officer_secrets())
        config = example_config()
        config["captains"][0]["crew"] = {"claude": {"count": 0}, "codex": {"count": 0}}
        config["captains"][0]["harness"] = {"type": "claude-sdk", "family": "claude", "extraPermissions": [rule("*")]}
        with self.assertRaisesRegex(Invalid, "uses allowedTools"):
            validate(config, officer_secrets())
        config["captains"][0]["harness"].pop("extraPermissions")
        config["harness"] = {**config["harness"], "extraPermissions": [rule("*")]}
        self.assertEqual(validate(config, officer_secrets())["members"][1]["harness"]["type"], "claude-sdk")


class WorkspaceRootTests(unittest.TestCase):
    """A captain's session starts in its workspace above the checkouts, with generated instructions there."""

    home = Path("/var/lib/subspace-fleet/agents/orion")

    def test_the_crew_plugin_roots_the_captain_in_its_workspace(self):
        cfg = workspace_fleet()
        captain = cfg["members"][1]
        options = runtime.crew_options(cfg, captain, self.home, 4097, "ses_fixture", self.home / "workspace")
        # The captain's directory need not be a Git repository; the first repository stays the default project.
        self.assertEqual(options["captainDirectory"], f"{self.home}/workspace")
        self.assertEqual(options["projectDirectory"], f"{self.home}/workspace/project")
        self.assertEqual(options["projects"]["library"]["projectDirectory"], f"{self.home}/workspace/library")
        # Stargate requires the captain's directory and every worker worktree to be distinct and non-nested.
        workers = [*options["worktrees"].values(),
                   *(path for project in options["projects"].values() for path in project["worktrees"].values())]
        captain_directory = Path(options["captainDirectory"])
        for path in map(Path, workers):
            self.assertFalse(path.is_relative_to(captain_directory) or captain_directory.is_relative_to(path))
        # Without a directory it keeps the previous root, the first repository's checkout.
        legacy = runtime.crew_options(cfg, captain, self.home, 4097, "ses_fixture")
        self.assertEqual(legacy["captainDirectory"], legacy["projectDirectory"])

    def test_the_session_root_is_a_captain_setting(self):
        config = example_config()
        config["captains"][0]["sessionRoot"] = "repository"
        self.assertEqual(validate(config, officer_secrets())["members"][1]["sessionRoot"], "repository")
        config["captains"][0]["sessionRoot"] = "home"
        with self.assertRaisesRegex(Invalid, "workspace or repository"):
            validate(config, officer_secrets())
        config = example_config()
        config["rearAdmiral"]["sessionRoot"] = "workspace"
        with self.assertRaisesRegex(Invalid, "Only captains take sessionRoot"):
            validate(config, officer_secrets())

    def test_repository_notes_are_one_line_and_kept_apart_from_the_sources(self):
        cfg = workspace_fleet()
        self.assertEqual(cfg["repositoryNotes"], {
            "project": {"description": "The example service.",
                        "tracker": "Example issues at https://tracker.example.com/project"},
            "library": {},
        })
        # Notes stay out of the repository sources, whose URL and visibility are fixed once installed.
        self.assertEqual(cfg["repositories"]["project"], {"url": "https://github.com/example/project.git",
                                                          "private": False})
        for row, message in (({"description": "two\nlines"}, "one line"), ({"tracker": "x" * 301}, "one line"),
                             ({"tracker": 7}, "one line")):
            config = example_config()
            config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git", **row}]
            with self.subTest(row=row), self.assertRaisesRegex(Invalid, message):
                validate(config, officer_secrets())

    def test_the_workspace_instructions_describe_repositories_trackers_and_channels(self):
        cfg = workspace_fleet(url="http://127.0.0.1:3000")
        captain = cfg["members"][1]
        documents = roles.workspace_documents(cfg, captain, self.home / "workspace", self.home)
        agents = documents["AGENTS.md"]
        self.assertTrue(agents.startswith(roles.GENERATED))
        self.assertIn("You are Orion, a captain in Example Fleet.", agents)
        self.assertIn(f"`{self.home}/workspace`", agents)
        self.assertIn("Your sessions start here", agents)
        self.assertIn("The rear admiral, Atlas,", agents)
        # Each repository: its ID, description, remote, tracker and where its own instructions are.
        project, library = agents.split("### `project`")[1].split("### `library`")
        self.assertIn("The example service.", project)
        self.assertIn("- Remote: https://github.com/example/project.git", project)
        self.assertIn("- Tracker: Example issues at https://tracker.example.com/project", project)
        self.assertIn("- Instructions: `project/AGENTS.md`", project)
        self.assertIn(f"- Crew worktrees: `{self.home}/worktrees/<crew ID>/project`", project)
        # A repository without a tracker note uses the fleet's Subspace tracker.
        self.assertIn("- Tracker: the fleet's Subspace tracker", library)
        self.assertIn("- Instructions: `library/AGENTS.md`", library)
        self.assertIn("passing the repository ID as `project` for any repository but `project`", agents)
        for line in ("- `#example-fleet-orion`: your ship channel", "- `#example-fleet-command`: fleet command",
                     "- `#example-fleet-vega`: Vega's ship channel.", "- `#example-fleet-starbase`: Starbase"):
            self.assertIn(line, agents)
        self.assertIn("its AGENTS.md, branch and review rules, checks and tracker take precedence", agents)
        self.assertNotIn("$", agents)
        self.assertEqual(documents["CLAUDE.md"].splitlines()[-1], "@AGENTS.md")
        self.assertTrue(documents["CLAUDE.md"].startswith(roles.GENERATED))
        # Without the API a repository with no note says the tracker is not recorded.
        cfg = workspace_fleet()
        agents = roles.workspace_documents(cfg, cfg["members"][2], self.home / "workspace/project", self.home)["AGENTS.md"]
        self.assertIn("- Tracker: not recorded in the fleet configuration", agents)
        self.assertIn("Your sessions start in `project/`, your checkout of the first repository", agents)

    def provision(self, home, documents):
        with patch.dict("os.environ", {"HOME": str(home)}):
            roles.provision_workspace(documents)

    def test_regeneration_keeps_operator_notes_and_never_writes_into_a_checkout(self):
        cfg = workspace_fleet()
        captain = cfg["members"][1]
        with tempfile.TemporaryDirectory() as temp:
            home = Path(temp)
            workspace = home / "workspace"
            for repo in ("project", "library"):
                (workspace / repo / ".git").mkdir(parents=True)
                (workspace / repo / "AGENTS.md").write_text(f"{repo}'s own instructions\n")
            checkouts = {path: path.read_bytes() for path in workspace.glob("*/**/*") if path.is_file()}
            documents = roles.workspace_documents(cfg, captain, workspace, home)
            self.provision(home, documents)
            agents, claude = workspace / "AGENTS.md", workspace / "CLAUDE.md"
            self.assertEqual(roles.operator_notes(agents.read_text()), "")
            self.assertEqual((agents.stat().st_mode & 0o777, claude.stat().st_mode & 0o777), (0o600, 0o600))
            self.assertEqual(claude.read_text(), documents["CLAUDE.md"])
            # Idempotent: an unchanged run replaces neither file.
            inodes = (agents.stat().st_ino, claude.stat().st_ino)
            self.provision(home, documents)
            self.assertEqual((agents.stat().st_ino, claude.stat().st_ino), inodes)
            # An operator's notes survive a regeneration with changed fleet details.
            text = agents.read_text().replace(f"{roles.NOTES_START}\n\n",
                                              f"{roles.NOTES_START}\nDeploy windows are Tuesdays.\n\nAsk first.\n")
            agents.write_text(text)
            cfg["repositoryNotes"]["library"]["tracker"] = "Library board"
            changed = roles.workspace_documents(cfg, captain, workspace, home)
            self.provision(home, changed)
            regenerated = agents.read_text()
            self.assertIn("- Tracker: Library board", regenerated)
            self.assertEqual(roles.operator_notes(regenerated), "Deploy windows are Tuesdays.\n\nAsk first.")
            self.assertTrue(regenerated.rstrip().endswith(roles.NOTES_END))
            self.provision(home, changed)
            self.assertEqual(agents.read_text(), regenerated)
            # Only the workspace's own two files were written: every checkout is as it was.
            self.assertEqual({path: path.read_bytes() for path in workspace.glob("*/**/*") if path.is_file()}, checkouts)
            self.assertEqual(sorted(path.name for path in workspace.iterdir()),
                             ["AGENTS.md", "CLAUDE.md", "library", "project"])

    def test_files_the_installer_did_not_write_are_kept_or_left_alone(self):
        cfg = workspace_fleet()
        captain = cfg["members"][1]
        with tempfile.TemporaryDirectory() as temp:
            home = Path(temp)
            workspace = home / "workspace"
            workspace.mkdir()
            (workspace / "AGENTS.md").write_text("Hand-written workspace notes.\n")
            (workspace / "CLAUDE.md").write_text("Prefer small pull requests.\n")
            documents = roles.workspace_documents(cfg, captain, workspace, home)
            with patch("sys.stderr", new_callable=io.StringIO) as warnings:
                self.provision(home, documents)
            self.assertIn("kept the text of", warnings.getvalue())
            notes = roles.operator_notes((workspace / "AGENTS.md").read_text())
            self.assertEqual(notes, "Hand-written workspace notes.\n\nPrefer small pull requests.")
            self.assertEqual((workspace / "CLAUDE.md").read_text(), documents["CLAUDE.md"])
            self.provision(home, documents)
            self.assertEqual(roles.operator_notes((workspace / "AGENTS.md").read_text()), notes)
            # A file the captain cannot read safely, such as a symbolic link, is left as it is.
            (workspace / "AGENTS.md").unlink()
            (workspace / "AGENTS.md").symlink_to(workspace / "CLAUDE.md")
            before = (workspace / "CLAUDE.md").read_text()
            with patch("sys.stderr", new_callable=io.StringIO) as warnings:
                self.provision(home, documents)
            self.assertIn("unchanged", warnings.getvalue())
            self.assertTrue((workspace / "AGENTS.md").is_symlink())
            self.assertEqual((workspace / "CLAUDE.md").read_text(), before)


class StandardChannelTests(unittest.TestCase):
    """Every fleet has Starbase, a separate command channel and one ship channel per captain."""

    def setUp(self):
        self.config = example_config()
        self.config["captains"].append({"id": "lyra", "name": "Lyra", "account": "example-lyra"})
        self.secrets = officer_secrets("example-lyra")

    def officers(self, cfg):
        def registration(person):
            return {"id": person["id"], "account": person["account"], "backend": {"directory": "/workspace"}}
        return [runtime.irc_officer(cfg, person, registration(person)) for person in cfg["members"]]

    def test_the_standard_set_is_always_present_and_joined(self):
        cfg = validate(self.config, self.secrets)
        command = f"#example-fleet-{standard_channels.COMMAND_SUFFIX}"
        self.assertEqual(cfg["irc"]["channels"],
                         ["#example-fleet-starbase", command, "#example-fleet-orion", "#example-fleet-lyra"])
        self.assertEqual((cfg["irc"]["starbaseChannel"], cfg["irc"]["commandChannel"]), ("#example-fleet-starbase", command))
        self.assertEqual([person["channel"] for person in cfg["members"]],
                         ["#example-fleet-starbase", "#example-fleet-orion", "#example-fleet-lyra"])
        rear, orion, lyra = self.officers(cfg)
        # The rear admiral joins every standard channel; captains join command and every ship channel.
        self.assertEqual(rear["channels"], ["#example-fleet-starbase", command, "#example-fleet-orion", "#example-fleet-lyra"])
        self.assertEqual(orion["channels"], ["#example-fleet-orion", command, "#example-fleet-lyra"])
        self.assertEqual(lyra["channels"], ["#example-fleet-lyra", command, "#example-fleet-orion"])
        # Humans wake the rear admiral in Starbase and a captain in its own ship; command needs an @nick.
        self.assertEqual(rear["access"]["humanWakeChannels"], [{"channel": "#example-fleet-starbase", "replies": "all"}])
        self.assertEqual(orion["access"]["humanWakeChannels"], [{"channel": "#example-fleet-orion", "replies": "all"}])
        self.assertTrue(all(entry["access"]["channelRequests"] == "nick-mention" for entry in (rear, orion, lyra)))
        self.assertIn("example-atlas", orion["access"]["peerCaptains"])

    def test_direct_messages_point_people_to_the_agents_own_channel(self):
        cfg = validate(self.config, self.secrets)
        rear, orion, lyra = self.officers(cfg)
        # Stargate's redirect answers a direct message with the registration's first channel and starts no turn.
        self.assertTrue(all(entry["dmPolicy"] == "redirect" for entry in (rear, orion, lyra)))
        self.assertEqual([entry["channels"][0] for entry in (rear, orion, lyra)],
                         ["#example-fleet-starbase", "#example-fleet-orion", "#example-fleet-lyra"])

    def test_the_standard_names_may_be_stated(self):
        self.config["irc"].update(starbaseChannel="#example-fleet-starbase",
                                  commandChannel=standard_channels.command("example-fleet"))
        self.config["rearAdmiral"]["channel"] = "#example-fleet-starbase"
        self.config["captains"][0]["channel"] = "#example-fleet-orion"
        self.assertEqual(validate(self.config, self.secrets)["irc"]["channels"][:2],
                         ["#example-fleet-starbase", standard_channels.command("example-fleet")])

    def test_renamed_standard_channels_are_refused_clearly(self):
        cases = [
            (("irc", "commandChannel"), "#example-fleet-starbase", "irc.commandChannel"),
            (("irc", "starbaseChannel"), "#example-fleet-bridge", "irc.starbaseChannel"),
            (("rearAdmiral", "channel"), "#example-fleet-bridge", "rearAdmiral.channel"),
            (("captains", 0, "channel"), "#orion-work", "Captain orion's channel"),
            (("captains", 0, "channel"), "#Not A Channel", "Captain orion's channel"),
        ]
        for path, value, label in cases:
            config = json.loads(json.dumps(self.config))
            target = config
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(Invalid) as raised:
                validate(config, self.secrets)
            message = str(raised.exception)
            self.assertTrue(message.startswith(label), message)
            for expected in ("#example-fleet-starbase", standard_channels.command("example-fleet"),
                             "#example-fleet-<captain-id>", "Extra channels are welcome", "irc.compose.channels"):
                self.assertIn(expected, message)

    def test_extra_channels_are_kept_beside_the_standard_set(self):
        self.config["irc"] = {"mode": "local", "allowedAccounts": ["operator"],
                              "compose": {"operator": "ircop", "ref": "0" * 40, "channels": ["#example-fleet-nav"]}}
        self.secrets["ircOperatorPassword"] = "fixture-operator-password"
        cfg = validate(self.config, self.secrets)
        self.assertEqual(cfg["irc"]["compose"]["channels"], ["#example-fleet-nav"])
        self.assertEqual(len(cfg["irc"]["channels"]), 4)

    def test_names_that_cannot_be_standard_are_refused(self):
        long_slug = "a" * 35
        self.config["slug"] = long_slug
        self.config["captains"][1]["id"] = "b" * 20
        with self.assertRaisesRegex(Invalid, rf"#{long_slug}-b+ is 57 characters.*limited to 50.*shorter"):
            validate(self.config, self.secrets)
        self.config["slug"] = "example-fleet"
        self.config["captains"][1]["id"] = standard_channels.COMMAND_SUFFIX
        with self.assertRaisesRegex(Invalid, "named like a standard fleet channel"):
            validate(self.config, self.secrets)
        self.config["captains"][1]["id"] = "lyra"
        self.config["captains"][1]["account"] = "example-" + "l" * 30
        with self.assertRaisesRegex(Invalid, "at most 32"):
            validate(self.config, self.secrets)

    def test_the_rear_admiral_joins_at_most_twenty_channels(self):
        self.config["captains"] = [{"id": f"ship{index}", "name": f"Ship {index}", "account": f"example-ship{index}"}
                                   for index in range(19)]
        secrets = {"proxyKey": "fixture-model-proxy-key", "ircPasswords": {
            person["account"]: "fixture-password" for person in [self.config["rearAdmiral"], *self.config["captains"]]}}
        with self.assertRaisesRegex(Invalid, "at most 18 captains"):
            validate(self.config, secrets)
        self.config["captains"].pop()
        secrets["ircPasswords"].pop("example-ship18")
        self.assertEqual(len(validate(self.config, secrets)["irc"]["channels"]), 20)


FIXTURE_JWT = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJmaXh0dXJlIn0.fixture"
ADMIN_JWT = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJhZG1pbiJ9.admin"


class AgentDispatchTests(unittest.TestCase):
    """Stargate's agent dispatch: off by default, and on only where it can work."""

    def setUp(self):
        self.config = example_config()
        self.config["api"] = {"url": "https://subspace.example.com"}
        # Crew need a repository; only a captain with crew has the pinned root that answers offers.
        self.config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        self.secrets = {**officer_secrets(), "apiToken": ADMIN_JWT}

    def enable(self, **settings):
        self.config["agentDispatch"] = {"enabled": True, **settings}
        self.secrets["rearAdmiralApiToken"] = FIXTURE_JWT

    def test_dispatch_is_off_by_default(self):
        cfg = validate(self.config, self.secrets)
        self.assertIsNone(cfg["agentDispatch"])
        self.assertEqual(runtime.agent_dispatch_settings(cfg), (None, {}))
        self.config["agentDispatch"] = {"enabled": False, "pollIntervalSeconds": 60}
        self.assertIsNone(validate(self.config, self.secrets)["agentDispatch"])

    def test_dispatch_block_uses_the_command_channel_and_stargate_defaults(self):
        self.enable()
        cfg = validate(self.config, self.secrets)
        block, environment = runtime.agent_dispatch_settings(cfg)
        self.assertEqual(block, {
            "enabled": True, "fleet": "example-fleet",
            "api": {"baseUrl": "https://subspace.example.com/api", "tokenEnv": "SUBSPACE_DISPATCH_TOKEN"},
            "messaging": {"type": "irc", "registration": "atlas", "channel": "#example-fleet-command"},
            "captainAccounts": {"orion": "example-orion", "Orion": "example-orion"},
            "pollIntervalSeconds": 120, "answerTimeoutSeconds": 300,
            "retryLadderSeconds": [3600, 10800, 21600, 43200],
        })
        self.assertEqual(block["messaging"]["channel"], standard_channels.command("example-fleet"))
        # The block names the variable only; the token goes in the gateway's private environment.
        self.assertNotIn(FIXTURE_JWT, json.dumps(block))
        self.assertEqual(environment, {"SUBSPACE_DISPATCH_TOKEN": FIXTURE_JWT})

    def test_timings_are_overridable_within_stargates_ranges(self):
        self.enable(pollIntervalSeconds=30, answerTimeoutSeconds=600, retryLadderSeconds=[])
        block, _ = runtime.agent_dispatch_settings(validate(self.config, self.secrets))
        self.assertEqual((block["pollIntervalSeconds"], block["answerTimeoutSeconds"], block["retryLadderSeconds"]),
                         (30, 600, []))
        for key, value in (("pollIntervalSeconds", 29), ("pollIntervalSeconds", 3601), ("answerTimeoutSeconds", 59),
                           ("retryLadderSeconds", [59]), ("retryLadderSeconds", [3600] * 11),
                           ("pollIntervalSeconds", "120"), ("retryLadderSeconds", 3600)):
            self.enable(**{key: value})
            with self.subTest(key=key, value=value), self.assertRaisesRegex(Invalid, f"agentDispatch.{key}|{key}"):
                validate(self.config, self.secrets)
        self.config["agentDispatch"] = {"enabled": True, "channel": "#elsewhere"}
        with self.assertRaisesRegex(Invalid, "unknown field"):
            validate(self.config, self.secrets)

    def test_what_cannot_work_is_refused(self):
        self.enable()
        no_api = json.loads(json.dumps(self.config))
        del no_api["api"]
        secrets = {key: value for key, value in self.secrets.items() if key != "apiToken"}
        with self.assertRaisesRegex(Invalid, "no Subspace API registration"):
            validate(no_api, secrets)
        no_captains = json.loads(json.dumps(self.config))
        no_captains["captains"] = []
        with self.assertRaisesRegex(Invalid, "no captains"):
            validate(no_captains, {**self.secrets, "ircPasswords": {"example-atlas": "fixture-password"}})
        claude = json.loads(json.dumps(self.config))
        claude["rearAdmiral"]["harness"] = {"type": "claude-sdk", "family": "claude"}
        with self.assertRaisesRegex(Invalid, "rear admiral uses claude-sdk"):
            validate(claude, self.secrets)
        without_token = {key: value for key, value in self.secrets.items() if key != "rearAdmiralApiToken"}
        with self.assertRaisesRegex(Invalid, "supply rearAdmiralApiToken"):
            validate(self.config, without_token)
        with self.assertRaisesRegex(Invalid, "not apiToken"):
            validate(self.config, {**self.secrets, "rearAdmiralApiToken": ADMIN_JWT})
        with self.assertRaisesRegex(Invalid, "Invalid rearAdmiralApiToken"):
            validate(self.config, {**self.secrets, "rearAdmiralApiToken": "not a token"})
        crewless = json.loads(json.dumps(self.config))
        crewless["repositories"] = []
        with self.assertRaisesRegex(Invalid, "no captain can answer an offer"):
            validate(crewless, self.secrets)
        asking = json.loads(json.dumps(self.config))
        asking["captains"][0]["harness"] = {"permissions": [{"action": "*", "resource": "*", "effect": "ask"}]}
        with self.assertRaisesRegex(Invalid, "Captain orion's harness.permissions.*explicit allow or deny"):
            validate(asking, self.secrets)
        # Rules that cannot reach a fleet tool may still ask.
        asking["captains"][0]["harness"]["permissions"] = [
            {"action": "*", "resource": "*", "effect": "deny"}, {"action": "shell", "resource": "git *", "effect": "ask"}]
        validate(asking, self.secrets)
        del self.config["agentDispatch"]
        with self.assertRaisesRegex(Invalid, "only for agent dispatch"):
            validate(self.config, self.secrets)

    def test_captains_answer_offers_through_the_fleet_plugin(self):
        self.enable()
        cfg = validate(self.config, self.secrets)
        rear, captain = cfg["members"]
        gateway = Path("/var/lib/subspace-fleet/gateway")
        self.assertIn(runtime.DISPATCH_GRANT, runtime.captain_permissions(cfg, captain))
        # Nothing until both the captain's and the rear admiral's roots are pinned.
        self.assertIsNone(runtime.fleet_control(cfg, captain, "ses_captain", None, gateway))
        self.assertIsNone(runtime.fleet_control(cfg, captain, None, "ses_rear", gateway))
        self.assertIsNone(runtime.fleet_control(cfg, rear, "ses_rear", "ses_rear", gateway))
        control = runtime.fleet_control(cfg, captain, "ses_captain", "ses_rear", gateway)
        self.assertEqual(control, {
            "socketPath": "/var/lib/subspace-fleet/fleet-control/orion/fleet.sock",
            "statePath": "/var/lib/subspace-fleet/gateway/fleet-control/evidence.sqlite",
            # The rear_admiral target is how the captain recognises dispatch requests and records offers.
            "targets": [{"id": "atlas", "account": "example-atlas", "role": "rear_admiral",
                         "channel": "#example-fleet-orion", "sessionID": "ses_rear"}],
        })
        home = Path("/var/lib/subspace-fleet/agents/orion")
        documents = roles.role_documents(cfg, captain, Path("/references/atlas"), home)
        saved = {}

        def save(name, settings, _user):
            saved[name] = settings
            return Path(f"/etc/subspace-fleet/{name}.json")

        with patch.object(runtime, "directory"), patch.object(runtime, "save_settings", side_effect=save), \
                patch.object(runtime, "serve_unit", return_value="subspace-fleet-engine-orion.service"):
            _, backend, _, _ = runtime.render_engine(cfg, captain, home, home / "workspace/project", 4097, b"seed",
                                                     documents, "ses_captain", fleet_tools=True)
            crew_plugin, fleet_plugin = saved["opencode-orion"]["plugins"]
            self.assertEqual(fleet_plugin, {
                "package": f"/opt/subspace-fleet/src/stargate-{runtime.STARGATE_REF}/src/fleet",
                "options": {"socketPath": control["socketPath"]},
            })
            self.assertIn(runtime.DISPATCH_GRANT, backend["permissions"])
            # The rear admiral's IRC registration is pinned to the root that captains name.
            _, backend, _, _ = runtime.render_engine(cfg, rear, Path("/var/lib/subspace-fleet/agents/atlas"),
                                                     Path("/references/atlas/project"), 4096, b"seed",
                                                     roles.role_documents(cfg, rear, Path("/references/atlas"), home),
                                                     "ses_rear")
            self.assertEqual(backend["sessionID"], "ses_rear")
            self.assertNotIn("plugins", saved["opencode-atlas"])
            self.assertNotIn("repo_refresh", saved["opencode-orion"].get("mcp", {}).get("servers", {}))
            tool = saved["opencode-atlas"]["mcp"]["servers"]["repo_refresh"]
            self.assertEqual(tool["command"], runtime.repository_refresh_command())
            self.assertIn({"action": "repo_refresh_refresh_origin", "resource": "*", "effect": "allow"},
                          backend["permissions"])
        runtime.fleet_ipc_policy(runtime.captain_permissions(cfg, captain), "fixture")

    def test_the_delegate_policy_and_extra_permissions_meet_stargates_fleet_tool_check(self):
        self.enable()
        self.config["captainTools"] = "delegate"
        cfg = validate(self.config, self.secrets)
        runtime.fleet_ipc_policy(runtime.captain_permissions(cfg, cfg["members"][1]), "fixture")
        self.config["harness"] = {**self.config["harness"],
                                  "extraPermissions": [{"action": "fleet_*", "resource": "*", "effect": "ask"}]}
        with self.assertRaisesRegex(Invalid, "fleet tools"):
            validate(self.config, self.secrets)

    def test_role_guidance_follows_the_dispatch_switch(self):
        home = Path("/var/lib/subspace-fleet/agents/orion")
        references = Path("/references/atlas")
        cfg = validate(self.config, self.secrets)
        for person in cfg["members"]:
            text = roles.role_documents(cfg, person, references, home)["instructions"]
            self.assertIn("Agent dispatch is off", text)
            self.assertNotIn("fleet_answer_offer", text)
        self.enable(answerTimeoutSeconds=600)
        cfg = validate(self.config, self.secrets)
        rear, captain = (roles.role_documents(cfg, person, references, home)["instructions"] for person in cfg["members"])
        self.assertIn("+stargate/dispatch", rear)
        self.assertIn("#example-fleet-command", rear)
        self.assertNotIn("fleet_answer_offer", rear)
        self.assertIn("fleet_answer_offer", captain)
        self.assertIn("10 minutes", captain)
        self.assertIn("untrusted data", captain)
        crewless = json.loads(json.dumps(self.config))
        crewless["captains"].append({"id": "lyra", "name": "Lyra", "account": "example-lyra",
                                     "crew": {"claude": {"count": 0}, "codex": {"count": 0}}})
        cfg = validate(crewless, {**self.secrets, **officer_secrets("example-lyra"),
                                  "rearAdmiralApiToken": FIXTURE_JWT})
        lyra = roles.role_documents(cfg, cfg["members"][2], references, home)["instructions"]
        self.assertIn("cannot accept offers", lyra)
        self.assertNotIn("$", lyra)

    def test_dispatch_wakes_only_the_addressed_captain(self):
        """Stargate addresses each request to one captain's @nick; answers and status lines are NOTICEs."""
        self.enable()
        cfg = validate(self.config, self.secrets)
        rear, captain = cfg["members"]

        def registration(person):
            return {"id": person["id"], "account": person["account"], "backend": {"directory": "/workspace"}}

        entry = runtime.irc_officer(cfg, captain, registration(captain))
        command = cfg["irc"]["commandChannel"]
        self.assertIn(command, entry["channels"])
        self.assertEqual(entry["access"]["channelRequests"], "nick-mention")
        self.assertEqual(entry["access"]["humanWakeChannels"], [{"channel": captain["channel"], "replies": "all"}])
        self.assertEqual(entry["access"]["allCallWords"], [])
        # The rear admiral is an enrolled peer, so its addressed request is admitted; an enrolled peer that is
        # also the rear_admiral target is exempt from the peer turn limit.
        self.assertIn(rear["account"], entry["access"]["peerCaptains"])
        self.assertIn(rear["account"], entry["fleetPolicy"]["agentAccounts"])
        rear_entry = runtime.irc_officer(cfg, rear, registration(rear))
        self.assertIn(command, rear_entry["channels"])
        self.assertIn(captain["account"], rear_entry["fleetPolicy"]["agentAccounts"])


def slack_settings(app="AFIXTURE1", **extra):
    return {"appId": app, "users": ["UFIXTURE1"], "channels": ["CFIXTURE1"], **extra}


class OfficerSlackTests(unittest.TestCase):
    """An officer's own Slack bot, on the root session its IRC registration uses."""

    def setUp(self):
        self.config = example_config()
        self.config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        self.config["captains"][0]["slack"] = slack_settings()
        self.secrets = officer_secrets()
        self.root = tempfile.TemporaryDirectory()
        self.addCleanup(self.root.cleanup)
        self.directory = Path(self.root.name) / "fleet.secrets"
        self.directory.mkdir(mode=0o700)

    def token(self, agent_id, name, value, mode=0o600):
        folder = self.directory / agent_id
        folder.mkdir(mode=0o700, exist_ok=True)
        (folder / name).write_text(value + "\n")
        (folder / name).chmod(mode)

    def tokens(self, agent_id="orion", suffix="orion"):
        self.token(agent_id, "slack-app-token", f"xapp-1-fixture-{suffix}")
        self.token(agent_id, "slack-bot-token", f"xoxb-1-fixture-{suffix}")

    def test_settings_name_explicit_slack_users_and_channels(self):
        captain = validate(self.config, self.secrets)["members"][1]
        self.assertEqual(captain["slack"], slack_settings())
        refusals = (
            ({"appId": "AFIXTURE1", "users": ["UFIXTURE1"]}, "missing a required field"),
            (slack_settings(users=["*"]), r"users entry: use explicit Slack member IDs .*not \*"),
            (slack_settings(channels=["*"]), r"channels entry: use explicit Slack conversation IDs"),
            (slack_settings(users=[]), "nobody may address the bot by default"),
            (slack_settings(channels="CFIXTURE1"), "nobody may address the bot by default"),
            (slack_settings(users=["UFIXTURE1", "UFIXTURE1"]), "lists an ID twice"),
            (slack_settings(app="fixture"), "expected a Slack app ID"),
            (slack_settings(mentionOnly="yes"), "mentionOnly must be true or false"),
            # Tokens belong in the secrets directory, never in fleet.json.
            (slack_settings(botToken="xoxb-1-fixture"), "unknown field"),
        )
        for settings, message in refusals:
            self.config["captains"][0]["slack"] = settings
            with self.subTest(settings=settings), self.assertRaisesRegex(Invalid, message):
                validate(self.config, self.secrets)

    def test_what_cannot_share_a_session_or_an_app_is_refused(self):
        claude = json.loads(json.dumps(self.config))
        claude["captains"][0]["harness"] = {"type": "claude-sdk", "family": "claude"}
        claude["captains"][0]["crew"] = {"claude": {"count": 0}, "codex": {"count": 0}}
        with self.assertRaisesRegex(Invalid, "Captain orion uses claude-sdk and cannot have its own Slack bot"):
            validate(claude, self.secrets)
        twice = json.loads(json.dumps(self.config))
        twice["rearAdmiral"]["slack"] = slack_settings()
        with self.assertRaisesRegex(Invalid, "own Slack app: an appId appears twice"):
            validate(twice, self.secrets)
        stakeholder = json.loads(json.dumps(self.config))
        stakeholder["rearAdmiral"]["slack"] = slack_settings("AFIXTURE2")
        stakeholder["stakeholders"] = {"slack": slack_settings("AFIXTURE3")}
        with self.assertRaisesRegex(Invalid, "Give the rear admiral one Slack bot"):
            validate(stakeholder, self.secrets)
        stakeholder["stakeholders"]["slack"]["appId"] = "AFIXTURE1"
        del stakeholder["rearAdmiral"]["slack"]
        with self.assertRaisesRegex(Invalid, "an appId appears twice"):
            validate(stakeholder, self.secrets)

    def test_tokens_come_from_each_officers_secrets_directory(self):
        cfg = validate(self.config, self.secrets)
        with self.assertRaisesRegex(Invalid, "Supply slack-app-token in captain orion's secrets directory"):
            runtime.extra_snapshot(cfg, self.directory, {}, False)
        self.token("orion", "slack-bot-token", "xoxb-1-fixture-orion")
        # Half of the pair is refused as well.
        with self.assertRaisesRegex(Invalid, "Supply slack-app-token in captain orion's secrets directory"):
            runtime.extra_snapshot(cfg, self.directory, {}, False)
        self.token("orion", "slack-app-token", "xapp-1-fixture-orion", 0o644)
        with self.assertRaisesRegex(ValueError, "owner-only"):
            runtime.extra_snapshot(cfg, self.directory, {}, False)
        self.token("orion", "slack-app-token", "xoxb-1-fixture-orion")
        with self.assertRaisesRegex(Invalid, "Invalid slack-app-token"):
            runtime.extra_snapshot(cfg, self.directory, {}, False)
        self.tokens()
        extra = runtime.extra_snapshot(cfg, self.directory, {}, False)
        self.assertEqual(extra["slack"], {"orion": {"slack-app-token": "xapp-1-fixture-orion",
                                                    "slack-bot-token": "xoxb-1-fixture-orion", "appId": "AFIXTURE1"}})
        # An upgrade without a secrets directory keeps the installed tokens.
        self.assertEqual(runtime.extra_snapshot(cfg, None, extra, False)["slack"], extra["slack"])
        # Two officers cannot share a bot.
        self.config["rearAdmiral"]["slack"] = slack_settings("AFIXTURE2")
        self.token("atlas", "slack-app-token", "xapp-1-fixture-atlas")
        self.token("atlas", "slack-bot-token", "xoxb-1-fixture-orion")
        with self.assertRaisesRegex(Invalid, "own slack-bot-token"):
            runtime.extra_snapshot(validate(self.config, self.secrets), self.directory, {}, False)
        # Removing an officer's Slack keeps its tokens until they are pruned, and renders no bot.
        del self.config["rearAdmiral"]["slack"], self.config["captains"][0]["slack"]
        cfg = validate(self.config, self.secrets)
        self.assertEqual(runtime.extra_snapshot(cfg, None, extra, False)["slack"], extra["slack"])
        self.assertEqual(runtime.extra_snapshot(cfg, self.directory, extra, True)["slack"], {})
        self.assertEqual(runtime.officer_slack_bot(cfg["members"][1], {"backend": {}}, extra["slack"]["orion"]),
                         (None, {}))

    def test_a_new_slack_app_needs_both_of_its_tokens(self):
        self.tokens()
        installed = runtime.extra_snapshot(validate(self.config, self.secrets), self.directory, {}, False)
        # The officer moves to another Slack app and supplies only the new app-level token.
        self.config["captains"][0]["slack"] = slack_settings("AFIXTURE9")
        cfg = validate(self.config, self.secrets)
        fresh = Path(self.root.name) / "new.secrets"
        (fresh / "orion").mkdir(parents=True, mode=0o700)
        fresh.chmod(0o700)
        (fresh / "orion/slack-app-token").write_text("xapp-1-fixture-new\n")
        (fresh / "orion/slack-app-token").chmod(0o600)
        with self.assertRaisesRegex(Invalid, "slack-bot-token .*Slack app changed to AFIXTURE9, so both"):
            runtime.extra_snapshot(cfg, fresh, installed, False)
        # Without any secrets directory the old app's tokens are not reused either.
        with self.assertRaisesRegex(Invalid, "Supply slack-app-token .*Slack app changed to AFIXTURE9"):
            runtime.extra_snapshot(cfg, None, installed, False)
        (fresh / "orion/slack-bot-token").write_text("xoxb-1-fixture-new\n")
        (fresh / "orion/slack-bot-token").chmod(0o600)
        replaced = runtime.extra_snapshot(cfg, fresh, installed, False)["slack"]["orion"]
        self.assertEqual(replaced, {"slack-app-token": "xapp-1-fixture-new", "slack-bot-token": "xoxb-1-fixture-new",
                                    "appId": "AFIXTURE9"})
        # The render never pairs a bot with tokens recorded for another app.
        registration = {"backend": {"directory": "/w"}}
        with self.assertRaisesRegex(Invalid, "for Slack app AFIXTURE9"):
            runtime.officer_slack_bot(cfg["members"][1], registration, installed["slack"]["orion"])
        self.assertEqual(runtime.officer_slack_bot(cfg["members"][1], registration, replaced)[1]["SLACK_BOT_TOKEN_ORION"],
                         "xoxb-1-fixture-new")
        # Tokens installed before the app was recorded are kept for the configured app on an upgrade.
        legacy = {"slack": {"orion": {name: value for name, value in installed["slack"]["orion"].items()
                                      if name != "appId"}}}
        self.assertEqual(runtime.extra_snapshot(cfg, None, legacy, False)["slack"]["orion"]["appId"], "AFIXTURE9")

    def test_the_bot_uses_the_irc_registrations_backend_and_pinned_session(self):
        self.config["captains"][0]["slack"] = slack_settings(mentionOnly=False)
        captain = validate(self.config, self.secrets)["members"][1]
        backend = {"type": "opencode", "url": "http://127.0.0.1:4097", "directory": "/workspace/project",
                   "agent": "build", "sessionID": "ses_fixture", "permissions": [], "enforcePermissions": True,
                   "headersEnv": {"authorization": "ENGINE_ORION"}}
        registration, _ = runtime.irc_agent(validate(self.config, self.secrets), captain, backend)
        tokens = {"slack-app-token": "xapp-1-fixture-orion", "slack-bot-token": "xoxb-1-fixture-orion"}
        bot, environment = runtime.officer_slack_bot(captain, registration, tokens)
        self.assertEqual(bot, {
            "id": "fleet-slack-orion", "displayName": "Orion", "backend": backend,
            "slack": {"appId": "AFIXTURE1", "appTokenEnv": "SLACK_APP_TOKEN_ORION",
                      "botTokenEnv": "SLACK_BOT_TOKEN_ORION"},
            "access": {"users": ["UFIXTURE1"], "channels": ["CFIXTURE1"], "mentionOnly": False},
        })
        # The configuration names the variables only; the tokens go in the gateway's private environment.
        self.assertNotIn("fixture-orion", json.dumps(bot))
        self.assertEqual(environment, {"SLACK_APP_TOKEN_ORION": "xapp-1-fixture-orion",
                                       "SLACK_BOT_TOKEN_ORION": "xoxb-1-fixture-orion"})
        self.config["captains"][0]["slack"] = slack_settings()
        captain = validate(self.config, self.secrets)["members"][1]
        self.assertTrue(runtime.officer_slack_bot(captain, registration, tokens)[0]["access"]["mentionOnly"])
        with self.assertRaisesRegex(Invalid, "Supply slack-app-token and slack-bot-token"):
            runtime.officer_slack_bot(captain, registration, None)

    def test_the_rear_admirals_own_bot_pins_its_root_and_reports_quota(self):
        cfg = validate(self.config, self.secrets)
        self.assertFalse(runtime.rear_root(cfg))
        self.config["rearAdmiral"]["slack"] = slack_settings("AFIXTURE2")
        with self.assertRaisesRegex(Invalid, "Usage notices require rear-admiral Slack"):
            validate({**self.config, "rearAdmiral": example_config()["rearAdmiral"],
                      "usage": {"reportChannel": "CFIXTURE1"}}, self.secrets)
        self.config["usage"] = {"reportChannel": "CFIXTURE1"}
        cfg = validate(self.config, self.secrets)
        self.assertTrue(runtime.rear_root(cfg))
        tokens = {"slack-app-token": "xapp-1-fixture-atlas", "slack-bot-token": "xoxb-1-fixture-atlas",
                  "proxy-management-key": "fixture-proxy-management-key"}
        with patch.object(runtime.subprocess, "run"):
            settings, _ = runtime.stakeholder_settings(cfg, tokens, {}, "ss-fleet-gateway",
                                                       Path("/var/lib/subspace-fleet/gateway/data"), {})
        self.assertNotIn("bots", settings)
        self.assertEqual(settings["quotaWatch"]["report"], {"bot": "fleet-slack-atlas", "channel": "CFIXTURE1"})

    def test_officers_with_a_bot_are_told_how_slack_requests_arrive(self):
        cfg = validate(self.config, self.secrets)
        home = Path("/var/lib/subspace-fleet/agents/orion")
        rear, captain = (roles.role_documents(cfg, person, Path("/references/atlas"), home)["instructions"]
                         for person in cfg["members"])
        self.assertIn("s=slack;c=<channel>;t=<thread>;m=<message>;u=<user>", captain)
        self.assertIn("goal of that Slack thread", captain)
        self.assertNotIn("s=slack", rear)
        self.assertNotIn("$", captain)

    def test_shared_chat_upload_guidance_reaches_officers_and_crew(self):
        cfg = validate(self.config, self.secrets)
        home = Path("/var/lib/subspace-fleet/agents/example-captain")
        rear, captain = (roles.role_documents(cfg, person, Path("/references/example"), home)
                         for person in cfg["members"])
        for documents, role in ((rear, "rear-admiral"), (captain, "captain")):
            for content in (documents["instructions"], documents["skills"][f"fleet-{role}"]):
                self.assertIn("UPL-…", content)
                self.assertIn("subspace_read_chat_upload", content)
        self.assertTrue(captain["crew"])
        for prompt in (*captain["crew"].values(), captain["skills"]["fleet-crew"]):
            self.assertIn("subspace_read_chat_upload", prompt)

    def test_a_workspace_rooted_captain_keeps_its_slack_guidance_beside_the_workspace_instructions(self):
        # The workspace AGENTS.md is the project file OpenCode loads in the workspace; the role instructions with
        # the Slack section are its global file. Both are written as the captain, and neither replaces the other.
        cfg = validate(self.config, self.secrets)
        captain = cfg["members"][1]
        with tempfile.TemporaryDirectory() as temp, patch.dict("os.environ", {"HOME": temp}):
            home = Path(temp)
            (home / "workspace" / "project").mkdir(parents=True)
            roles.provision_role(roles.role_documents(cfg, captain, Path("/references/atlas"), home))
            roles.provision_workspace(roles.workspace_documents(cfg, captain, home / "workspace", home))
            role = (home / ".config/opencode/AGENTS.md").read_text()
            workspace = (home / "workspace/AGENTS.md").read_text()
        self.assertIn("s=slack;c=<channel>;t=<thread>;m=<message>;u=<user>", role)
        self.assertNotIn("s=slack", workspace)
        self.assertIn("- Slack: people can also reach you through your own Slack bot", workspace)
        self.assertIn("Your sessions start here", workspace)
        # Without a bot, neither file mentions Slack.
        del self.config["captains"][0]["slack"]
        captain = validate(self.config, self.secrets)["members"][1]
        documents = roles.workspace_documents(cfg, captain, Path("/w"), Path("/"))["AGENTS.md"]
        self.assertNotIn("Slack", documents)

    def test_dry_run_lists_the_bots_and_refuses_a_half_configured_officer(self):
        self.config["rearAdmiral"]["slack"] = slack_settings("AFIXTURE2")
        self.config["captains"].append({"id": "lyra", "name": "Lyra", "account": "example-lyra",
                                        "crew": {"claude": {"count": 0}, "codex": {"count": 0}},
                                        "slack": slack_settings("AFIXTURE3", mentionOnly=False)})
        config_path = Path(self.root.name) / "fleet.json"
        config_path.write_text(json.dumps(self.config))
        secrets_path = Path(self.root.name) / "fleet.secrets.json"
        secrets_path.write_text(json.dumps(officer_secrets("example-lyra")))
        secrets_path.chmod(0o600)

        def plan():
            output = io.StringIO()
            with patch.object(runtime, "ETC", Path(self.root.name) / "etc"), \
                    patch.object(runtime, "STATE", Path(self.root.name) / "state"), \
                    patch.object(runtime, "SESSIONS", Path(self.root.name) / "etc/sessions"), \
                    patch.object(sys, "stdout", output), patch.object(sys, "stderr", io.StringIO()):
                runtime.main(["plan", str(config_path), str(secrets_path), str(self.directory), "false"])
            return output.getvalue()

        self.tokens()
        self.tokens("atlas", "atlas")
        self.token("lyra", "slack-bot-token", "xoxb-1-fixture-lyra")
        with self.assertRaisesRegex(Invalid, "Supply slack-app-token in captain lyra's secrets directory"):
            plan()
        self.token("lyra", "slack-app-token", "xapp-1-fixture-lyra")
        output = plan()
        # The rear admiral's bot pins its root, so the dry run says where, as it does for each captain's root.
        state = Path(self.root.name) / "state"
        self.assertIn(f"The rear admiral's root session will be pinned in {state}/references/atlas/project, its "
                      "reference checkout of the first repository, because it has its own Slack bot.\nIts first "
                      "repository is then fixed", output)
        self.assertIn(f"Captain orion's sessions will start in {state}/agents/orion/workspace", output)
        lines = [line for line in output.splitlines() if line.startswith("Slack:")]
        self.assertEqual(len(lines), 3)
        self.assertIn("Rear admiral atlas answers as Slack app AFIXTURE2 (fleet-slack-atlas), to users UFIXTURE1 in "
                      "CFIXTURE1, when mentioned; it shares its pinned root session with its IRC registration.", lines[0])
        self.assertIn("Captain orion answers as Slack app AFIXTURE1", lines[1])
        self.assertIn("shares its pinned root session", lines[1])
        self.assertIn("in CFIXTURE1; it has no pinned root without crew", lines[2])
        self.assertIn("as SLACK_APP_TOKEN_LYRA and SLACK_BOT_TOKEN_LYRA", lines[2])
        self.assertNotIn("xoxb-", "\n".join(lines))


class FleetRecordsTests(unittest.TestCase):
    """The fleet's standard channels, ships and officers in the Subspace API (fleetctl register)."""

    def setUp(self):
        self.config = example_config()
        self.config["captains"].append({"id": "lyra", "name": "Lyra", "account": "example-lyra",
                                        "harness": {"family": "codex"}})
        self.config["api"] = {"url": "http://127.0.0.1:3000"}
        self.secrets = {**officer_secrets("example-lyra"), "apiToken": "header.payload.admin"}
        self.api = fixture.Subspace()
        self.said = []

    def register(self, config=None):
        cfg = validate(config or self.config, self.secrets)
        before = len(self.api.writes)
        fleet_records.register(self.api.call, fleet_records.fleet_fields(cfg), fleet_records.roster(cfg), self.said.append)
        return self.api.writes[before:]

    def test_the_fleet_its_ships_and_officers_are_recorded_once(self):
        writes = self.register()
        self.assertEqual(writes, [("POST", "/fleets"), ("POST", "/agents"), ("POST", "/fleets/<id>/ships"),
                                  ("POST", "/agents"), ("POST", "/fleets/<id>/ships"), ("POST", "/agents")])
        self.assertEqual(self.api.summary(), {
            "fleets": {"example-fleet": {"name": "Example Fleet", "starbaseChannel": "#example-fleet-starbase",
                                         "commandChannel": standard_channels.command("example-fleet")}},
            "ships": [("Lyra", "#example-fleet-lyra"), ("Orion", "#example-fleet-orion")],
            # The rear admiral keeps its own name; a captain is named by its account, its ship by the ship's name.
            "agents": [("Atlas", "rearAdmiral", "example-atlas", None, True),
                       ("example-lyra", "captain", "example-lyra", "Lyra", True),
                       ("example-orion", "captain", "example-orion", "Orion", True)],
        })
        agents = {row["name"]: row for row in self.api.agents.values()}
        self.assertEqual(agents["Atlas"]["serviceTokenClientId"], "example-atlas@agents.example.com")
        self.assertEqual((agents["Atlas"]["defaultModel"], agents["example-lyra"]["defaultModel"]),
                         ("claude-fable-5-2", runtime.FAMILIES["codex"]))
        self.assertEqual(self.register(), [], "a re-run changes nothing")

    def test_registration_assigns_an_initiative_of_no_fleet_and_never_moves_another_fleets(self):
        import fleetctl
        self.api.call("POST", "/fleets", {"slug": "other", "name": "Other"})
        other, theirs = self.api.fleet_by("other")["id"], "00000000-0000-4000-8000-0000000000a1"
        self.api.initiatives[theirs] = {"id": theirs, "name": "Their work", "fleetId": other, "channels": ""}
        self.config["api"]["initiatives"] = [fixture.INITIATIVE, theirs]
        api = self.api

        class Api(fleetctl.Api):
            def __init__(self, _cfg):
                pass

            def request(self, method, path, body=None):
                return api.call(method, path, body)

        out = io.StringIO()
        with patch.object(fleetctl, "Api", Api), contextlib.redirect_stdout(out):
            fleetctl.register(validate(self.config, self.secrets))
        ours = self.api.fleet_by("example-fleet")["id"]
        self.assertEqual((self.api.initiatives[fixture.INITIATIVE]["fleetId"], self.api.initiatives[theirs]["fleetId"]),
                         (ours, other))
        self.assertIn(f"Assigned initiative {fixture.INITIATIVE}.", out.getvalue())
        self.assertIn(f"Initiative {theirs} belongs to another fleet ({other}), so it was left there. To move it to "
                      f"example-fleet, run 'subspace-fleetctl assign {theirs} example-fleet'.", out.getvalue())

    def test_a_changed_roster_is_reconciled(self):
        self.register()
        lyra = dict(self.config["captains"].pop())
        self.secrets["ircPasswords"].pop("example-lyra")
        self.config["rearAdmiral"]["name"] = "Nova"
        self.config["captains"][0]["name"] = "Orion Two"
        self.register()
        summary = self.api.summary()
        # A retired captain is deactivated and its ship no longer names a channel for the desktop to join.
        self.assertIn(("example-lyra", "captain", "example-lyra", "Lyra", False), summary["agents"])
        self.assertIn(("Lyra", None), summary["ships"])
        # A renamed ship keeps its record; the rear admiral's record takes its new name.
        self.assertIn(("Orion Two", "#example-fleet-orion"), summary["ships"])
        self.assertEqual(len(summary["ships"]), 2)
        self.assertIn(("Nova", "rearAdmiral", "example-atlas", None, True), summary["agents"])
        # The captain's return reactivates its record and gives its ship its channel again.
        self.config["captains"].append(lyra)
        self.secrets["ircPasswords"]["example-lyra"] = "fixture-password"
        self.register()
        self.assertIn(("example-lyra", "captain", "example-lyra", "Lyra", True), self.api.summary()["agents"])
        self.assertIn(("Lyra", "#example-fleet-lyra"), self.api.summary()["ships"])
        self.assertEqual(len(self.api.agents), 3)
        self.assertEqual(self.register(), [])

    def test_existing_agents_are_found_across_pages_and_by_account_subject(self):
        self.api.page = 1
        self.register()
        # An agent an administrator registered by hand, without a nick, is taken over rather than duplicated.
        [orion] = [row for row in self.api.agents.values() if row["name"] == "example-orion"]
        orion["ircNick"] = None
        self.register()
        self.assertEqual(orion["ircNick"], "example-orion")
        self.assertEqual(len(self.api.agents), 3)

    def test_records_that_belong_to_someone_else_are_refused(self):
        self.register()
        cases = {
            "The name Atlas is taken in the Subspace API: agent atlas": lambda api: api.agents.update(x={
                "userId": "x", "name": "atlas", "rank": "crew", "fleetId": None, "shipId": None, "ircNick": None,
                "serviceTokenClientId": "someone@example.com", "defaultModel": "m", "isActive": True, "version": 1}),
            "under another account subject": lambda api: next(
                row for row in api.agents.values() if row["name"] == "Atlas").update(serviceTokenClientId="other@example.com"),
            "belongs to another fleet": lambda api: next(
                row for row in api.agents.values() if row["name"] == "Atlas").update(fleetId="another-fleet"),
        }
        for message, change in cases.items():
            api = self.api = fixture.Subspace()
            self.assertTrue(self.register())
            change(api)
            with self.subTest(message=message), self.assertRaisesRegex(fleet_records.Refused, message):
                self.register()

    def test_the_fleet_records_where_a_desktop_reaches_its_fleet_api(self):
        # Without stargate.api there is no Fleet API, so the fleet records no address.
        self.register()
        self.assertIsNone(self.api.fleet_by("example-fleet")["fleetApiUrl"])
        # With it, the SSH tunnel's end on the desktop: the same port on the desktop's loopback.
        self.config["stargate"] = {"api": {"port": 8585}}
        self.assertEqual(self.register(), [("PATCH", "/fleets/example-fleet")])
        self.assertEqual(self.api.fleet_by("example-fleet")["fleetApiUrl"], "http://127.0.0.1:8585")
        self.assertEqual(self.register(), [], "a re-run changes nothing")
        # An address the operator gives, such as an HTTPS name on the tailnet, is sent as the API stores it.
        self.config["api"]["fleetApiUrl"] = "HTTPS://Fleet.Example.ts.net:443/"
        self.assertEqual(self.register(), [("PATCH", "/fleets/example-fleet")])
        self.assertEqual(self.api.fleet_by("example-fleet")["fleetApiUrl"], "https://fleet.example.ts.net")
        self.assertEqual(self.register(), [], "a re-run changes nothing")
        # Switching the Fleet API off clears it, and needs the address gone from the config too.
        self.config["stargate"] = {}
        with self.assertRaisesRegex(Invalid, "api.fleetApiUrl.*off"):
            self.register()
        del self.config["api"]["fleetApiUrl"]
        self.assertEqual(self.register(), [("PATCH", "/fleets/example-fleet")])
        self.assertIsNone(self.api.fleet_by("example-fleet")["fleetApiUrl"])
        self.config["stargate"] = {"api": {}}
        for address in ("http://fleet.example.ts.net:8484", "https://operator@fleet.example.ts.net",
                        "https://fleet.example.ts.net/?token=x", "fleet.example.ts.net"):
            self.config["api"]["fleetApiUrl"] = address
            with self.subTest(address=address), self.assertRaisesRegex(Invalid, "Invalid api.fleetApiUrl"):
                validate(self.config, self.secrets)

    def test_fleet_api_addresses_take_the_api_s_stored_form(self):
        accepted = {
            "https://fleet.example.com": "https://fleet.example.com",
            "https://fleet.example.com/": "https://fleet.example.com",
            "HTTPS://Fleet.Example.COM:443/": "https://fleet.example.com",
            "https://fleet.example.com:8443/api/": "https://fleet.example.com:8443/api",
            "https://fleet.example.com/api//": "https://fleet.example.com/api",
            "https://100.64.0.7:7400": "https://100.64.0.7:7400",
            "http://127.0.0.1:7400/": "http://127.0.0.1:7400",
            "http://LOCALHOST:7400": "http://localhost:7400",
            "http://[::1]:7400": "http://[::1]:7400",
        }
        for value, stored in accepted.items():
            with self.subTest(value=value):
                self.assertEqual(fleet_records.fleet_api_url(value), stored)
                self.assertEqual(fixture.Subspace.fleet_api_url(value), stored, "the API stores the same form")
        self.assertEqual(fleet_records.fleet_api_url("http://[0:0:0:0:0:0:0:1]:80/"), "http://[::1]")
        refused = ["http://fleet.example.com:7400", "http://100.64.0.7:7400", "http://localhost.example.com",
                   "http://0.0.0.0:7400", "ftp://fleet.example.com", "wss://fleet.example.com", "/api",
                   "fleet.example.com", "https://", "https://operator@fleet.example.com",
                   "https://operator:secret@fleet.example.com", "https://fleet.example.com/?token=x",
                   "https://fleet.example.com/?", "https://fleet.example.com/#token", "https://fleet.example.com/a b",
                   " https://fleet.example.com", "https://fleet.example.com\\api", "https://fleet.example.com:99999",
                   "https://127.1", "https://fleet.example.com/../api", "https://bücher.example.com",
                   f"https://fleet.example.com/{'a' * 2048}", None, 8484]
        for value in refused:
            with self.subTest(value=value), self.assertRaises(fleet_records.Refused):
                fleet_records.fleet_api_url(value)

    def test_the_api_s_refusals_are_explained(self):
        self.api.admin = False
        with self.assertRaisesRegex(fleet_records.Refused, "human organisation administrator"):
            self.register()
        self.api = fixture.Subspace()
        self.config["rearAdmiral"]["serviceTokenClientId"] = "00000000-0000-4000-8000-00000000000a"
        with self.assertRaisesRegex(fleet_records.Refused, "example-atlas's serviceTokenClientId.*Entra object ID"):
            self.register()

    def test_names_the_api_cannot_hold_are_refused_before_installing(self):
        cases = [
            (("rearAdmiral", "name"), "Rear Admiral Atlas", "letters, digits and hyphens"),
            (("captains", 0, "account"), "example_orion", "without underscores"),
            (("captains", 0, "name"), "O" * 65, "at most 64"),
            (("captains", 1, "name"), "orion", "ship names must differ"),
        ]
        for path, value, message in cases:
            config = json.loads(json.dumps(self.config))
            secrets = json.loads(json.dumps(self.secrets))
            target = config
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            if path[-1] == "account":
                secrets["ircPasswords"][value] = secrets["ircPasswords"].pop("example-orion")
            with self.subTest(path=path), self.assertRaisesRegex(Invalid, message):
                validate(config, secrets)
            # A fleet without the API registers nothing, so its names are not the API's business.
            config.pop("api")
            secrets.pop("apiToken")
            validate(config, secrets)


class PlainNamesTests(unittest.TestCase):
    """An officer has one name, its ID: IRC account and nick, API agent name and Stargate ID; ship channels are
    #<captain-id>. A fleet installed with fleet-prefixed names keeps them until it is renamed."""

    def setUp(self):
        self.config = json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text())
        self.config["irc"]["auth"] = "password"
        self.config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        self.config["captains"].append({"id": "vega", "name": "Vega", "crewNames": {
            "claude-1": "Altair", "claude-2": "Castor", "codex-1": "Pollux", "codex-2": "Spica"}})
        self.secrets = {"proxyKey": "fixture-model-proxy-key",
                        "ircPasswords": {name: "fixture-password" for name in ("atlas", "orion", "vega")}}

    def test_each_officer_has_one_name_everywhere(self):
        cfg = validate(self.config, self.secrets)
        self.assertEqual([(person["id"], person["account"], person["channel"]) for person in cfg["members"]],
                         [("atlas", "atlas", "#example-fleet-starbase"), ("orion", "orion", "#orion"),
                          ("vega", "vega", "#vega")])
        self.assertEqual(cfg["irc"]["channels"], ["#example-fleet-starbase", "#example-fleet-command", "#orion", "#vega"])
        records = fleet_records.check(cfg["slug"], fleet_records.roster(cfg))
        self.assertEqual((records["rearAdmiral"]["name"], records["rearAdmiral"]["ircNick"]), ("atlas", "atlas"))
        self.assertEqual([(row["name"], row["ircNick"], row["ship"], row["channel"]) for row in records["captains"]],
                         [("orion", "orion", "Orion", "#orion"), ("vega", "vega", "Vega", "#vega")])
        cfg["api"] = {"origin": "https://subspace.example.com"}
        agents = runtime.subspace_agents_settings(cfg, Path("/var/lib/subspace-fleet/gateway"))["officers"]
        self.assertEqual([(row["name"], row["ircNick"]) for row in agents],
                         [("atlas", "atlas"), ("orion", "orion"), ("vega", "vega")])
        registration, _ = runtime.irc_agent(cfg, cfg["members"][1], {})
        self.assertEqual((registration["id"], registration["account"], registration["displayName"]),
                         ("orion", "orion", "Orion"))

    def test_an_account_other_than_the_id_is_refused_with_plain_names(self):
        self.config["captains"][0]["account"] = "example-orion"
        with self.assertRaisesRegex(Invalid, "an officer's IRC account is its ID"):
            validate(self.config, self.secrets)
        self.config["captains"][0]["channel"] = "#example-fleet-orion"
        self.config["captains"][0]["account"] = "orion"
        with self.assertRaisesRegex(Invalid, "must be #orion"):
            validate(self.config, self.secrets)
        # A ship channel may not be a standard fleet channel.
        self.config["captains"][0].pop("channel")
        self.config["slug"] = "ex"
        self.config["captains"][1] = {"id": "ex-command", "name": "Command"}
        with self.assertRaisesRegex(Invalid, "named like a standard fleet channel"):
            validate(self.config, {**self.secrets, "ircPasswords": {"atlas": "x" * 20, "orion": "x" * 20,
                                                                     "ex-command": "x" * 20}})

    def test_a_fleet_without_officer_names_keeps_its_prefixed_names(self):
        cfg = validate(*fixture_legacy())
        self.assertEqual(cfg["officerNames"], "prefixed")
        self.assertEqual([(person["account"], person["channel"]) for person in cfg["members"]],
                         [("example-atlas", "#example-fleet-starbase"), ("example-orion", "#example-fleet-orion")])
        records = fleet_records.roster(cfg)
        self.assertEqual((records["rearAdmiral"]["name"], records["captains"][0]["name"]), ("Atlas", "example-orion"))
        with self.assertRaisesRegex(Invalid, "officerNames"):
            validate({**fixture_legacy()[0], "officerNames": "fleet"}, fixture_legacy()[1])

    def test_crew_names_are_unique_in_the_fleet_and_never_an_officers(self):
        cfg = validate(self.config, self.secrets)
        crew = [(worker["id"], worker["name"], worker["slot"], worker["named"]) for worker in cfg["members"][1]["crew"]]
        self.assertEqual(crew, [("rigel", "Rigel", "claude-1", True), ("deneb", "Deneb", "claude-2", True),
                                ("mira", "Mira", "codex-1", True), ("sirius", "Sirius", "codex-2", True)])
        plan = "\n".join(runtime.names_plan(cfg))
        self.assertIn("Captain Orion: orion, channel #orion", plan)
        self.assertIn("crew: Rigel (Claude 1, claude-sonnet-5-5); Deneb (Claude 2, claude-sonnet-5-5)", plan)
        for names, message in (({"claude-1": "rigel"}, "same name as captain orion's Rigel"),
                               ({"claude-1": "Orion"}, "has an officer's name"),
                               ({"claude-1": "ATLAS"}, "has an officer's name")):
            config = json.loads(json.dumps(self.config))
            config["captains"][1]["crewNames"].update(names)
            with self.subTest(names), self.assertRaisesRegex(Invalid, message):
                validate(config, self.secrets)
        # Slots without names of their own keep their slot names on every ship, as installed fleets do.
        for captain in self.config["captains"]:
            captain.pop("crewNames")
        cfg = validate(self.config, self.secrets)
        self.assertEqual([worker["name"] for worker in cfg["members"][2]["crew"]],
                         ["Claude 1", "Claude 2", "Codex 1", "Codex 2"])
        self.assertIn("Claude 1 (Claude 1, claude-sonnet-5-5) [slot name]", "\n".join(runtime.names_plan(cfg)))

    def test_a_new_captains_crew_are_named_in_a_plain_fleet(self):
        self.config["captains"][1].pop("crewNames")
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            paths = [work / "fleet.json", work / "fleet.secrets.json"]
            for path, value in zip(paths, (self.config, self.secrets), strict=True):
                path.write_text(json.dumps(value))
                path.chmod(0o600)
            # On a new VM the installer stops before anything else, dry run or not.
            for verb, tail in (("plan", []), ("prepare", [str(work / "stage")])):
                with self.subTest(verb), patch.object(runtime, "ETC", work / "etc"), \
                        patch.object(runtime, "STATE", work / "state"), patch.object(runtime, "SESSIONS", work / "none"), \
                        self.assertRaisesRegex(Invalid, "Crew vega claude-1, vega claude-2, vega codex-1, vega codex-2 "
                                               "have no name"):
                    runtime.main([verb, *map(str, paths), *tail, "", "false"])
        cfg = validate(self.config, self.secrets)
        # A captain installed with repositories keeps its crew as they are; a fleet installed with fleet-prefixed
        # names keeps its slot names.
        runtime.check_new_crew_named(cfg, {"captains": [{"id": "orion"}, {"id": "vega"}],
                                           "repositories": [{"id": "project"}]})
        with self.assertRaisesRegex(Invalid, "vega claude-1"):
            runtime.check_new_crew_named(cfg, {"captains": [{"id": "orion"}, {"id": "vega"}]})
        runtime.check_new_crew_named(validate(*fixture_legacy()), {})

    def test_an_installed_crew_member_keeps_its_id_under_a_new_name_only_where_stargate_allows(self):
        self.config["usageGuard"] = {"enabled": False}
        self.config["captains"][0]["crewNames"] = {"claude-1": "Rigel"}
        self.config["captains"][0]["crewIds"] = {"claude-1": "claude-1"}
        cfg = validate(self.config, self.secrets)
        worker = cfg["members"][1]["crew"][0]
        self.assertEqual((worker["id"], worker["name"], worker["agent"]), ("claude-1", "Rigel", "orion-crew-claude-1"))
        cfg["stargate"]["ref"] = "c" * 40
        with self.assertRaisesRegex(Invalid, "requires a crew member's name to match its ID"):
            runtime.check_crew_ids(cfg)
        with patch.object(runtime, "stargate_crew_names_free", return_value=True):
            runtime.check_crew_ids(cfg)
        # The dry run cannot read a Stargate that is not checked out yet (a release newer than the installed one):
        # it says the install checks it, and refuses only a checkout that is there and has the rule.
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            paths = [work / "fleet.json", work / "fleet.secrets.json"]
            for path, value in zip(paths, (self.config, self.secrets), strict=True):
                path.write_text(json.dumps(value))
                path.chmod(0o600)
            ref = validate(self.config, self.secrets)["stargate"]["ref"]

            def plan():
                output = io.StringIO()
                with patch.object(runtime, "ETC", work / "etc"), patch.object(runtime, "STATE", work / "state"), \
                        patch.object(runtime, "SESSIONS", work / "none"), patch.object(runtime, "APP", work / "app"), \
                        patch.object(runtime, "check_new_crew_named"), contextlib.redirect_stdout(output):
                    runtime.main(["plan", *map(str, paths), "", "false"])
                return output.getvalue()

            self.assertIn("is not checked out here yet: the install checks", plan())
            team = work / "app/src" / f"stargate-{ref}" / "src/coordination/crew/team.ts"
            team.parent.mkdir(parents=True)
            team.write_text("throw new Error('Agent IDs must match assigned names')\n")
            with self.assertRaisesRegex(Invalid, "requires a crew member's name to match its ID"):
                plan()
            team.write_text("export {}\n")
            self.assertNotIn("is not checked out here yet", plan())
        self.config["captains"][0]["crewIds"] = {"claude-9": "x"}
        with self.assertRaisesRegex(Invalid, "crewIds may give only enabled crew slots"):
            validate(self.config, self.secrets)

    def test_installed_crew_keep_their_ids_under_names_unique_in_the_fleet(self):
        # Three ships whose crew were installed under their slot names, so that every ship has a claude-1, then
        # named by rename-officers --crew-names: each member keeps its installed ID (crewIds) under its new name.
        # It is the names that are unique in the fleet; an ID is unique only in its captain's crew.
        slots = ["claude-1", "claude-2", "codex-1", "codex-2"]
        crews = {"draco": ["Kestrel", "Heron", "Wren", "Finch"], "cygnus": ["Osprey", "Plover", "Linnet", "Swift"],
                 "pavo": ["Merlin", "Egret", "Robin", "Siskin"]}
        self.config["captains"] = [{"id": captain, "name": captain.title(),
                                    "crewNames": dict(zip(slots, names, strict=True)),
                                    "crewIds": {slot: slot for slot in slots}} for captain, names in crews.items()]
        secrets = {**self.secrets, "ircPasswords": {name: "fixture-password" for name in ("atlas", *crews)}}
        cfg = validate(self.config, secrets)
        for person, names in zip(cfg["members"][1:], crews.values(), strict=True):
            self.assertEqual([(worker["id"], worker["name"]) for worker in person["crew"]],
                             list(zip(slots, names, strict=True)))
            # What the fleet renders keys every crew member by its ID and shows its name: Stargate's team, the
            # worktrees, the native agents, the captain's roster and the crew member's own instructions.
            home = Path("/var/lib/subspace-fleet/agents") / person["id"]
            options = runtime.crew_options(cfg, person, home, 4097, "ses_fixture")
            team = options["team"]
            self.assertEqual([(member["id"], member["name"], member["agent"]) for member in team["crew"]],
                             [(slot, name, f"{person['id']}-crew-{slot}") for slot, name in zip(slots, names)])
            self.assertEqual(stargate_team_problems(team), [])
            clash = [dict(team["crew"][0], name="codex-1"), *team["crew"][1:]]
            self.assertEqual(stargate_team_problems({**team, "crew": clash}), ["codex-1"])
            self.assertEqual(options["worktrees"], {slot: f"{home}/worktrees/{slot}/project" for slot in slots})
            documents = roles.role_documents(cfg, person, None, home)
            self.assertIn(f"- {names[0]} (`claude-1`, Claude family, slot claude-1", documents["instructions"])
            self.assertEqual(sorted(documents["crew"]), slots)
            self.assertIn(names[0], documents["crew"]["claude-1"])
        self.assertIn("Captain Pavo: pavo, channel #pavo\n    crew: Merlin (Claude 1, claude-sonnet-5-5, ID claude-1); ",
                      "\n".join(runtime.names_plan(cfg)))
        # A name on two ships is refused, naming both, as is an officer's name.
        for captain, slot, name, message in (
                (2, "codex-2", "heron", "Crew member heron of captain pavo has the same name as captain draco's "
                                        "Heron, ignoring case"),
                (1, "claude-1", "Pavo", "Crew member Pavo of captain cygnus has an officer's name \\(pavo\\)")):
            config = json.loads(json.dumps(self.config))
            config["captains"][captain]["crewNames"][slot] = name
            with self.subTest(name), self.assertRaisesRegex(Invalid, message):
                validate(config, secrets)
        # In one captain's crew a name is no other member's name or ID, as Stargate's team requires.
        for name, message in (("Codex 1", "Codex 1 \\(claude-1\\) has the same name as the ID of Wren \\(codex-1\\)"),
                              ("heron", "Heron \\(claude-2\\) has the same name as heron \\(claude-1\\)"),
                              ("Draco", "Draco \\(claude-1\\) has the same name as captain draco's ID"),
                              ("All", "All \\(claude-1\\) has the same name as the alias all")):
            config = json.loads(json.dumps(self.config))
            config["captains"][0]["crewNames"]["claude-1"] = name
            with self.subTest(name), self.assertRaisesRegex(Invalid, f"Captain draco's crew member {message}"):
                validate(config, secrets)

    def test_records_accept_either_standard_ship_channel(self):
        cfg = validate(self.config, self.secrets)
        records = fleet_records.roster(cfg)
        records["captains"][0]["channel"] = "#vega"
        with self.assertRaisesRegex(fleet_records.Refused, "standard ship channel: #orion"):
            fleet_records.check(cfg["slug"], records)

    def test_a_prefixed_fleet_is_renamed_in_place_in_the_api(self):
        legacy, secrets = fixture_legacy()
        legacy["api"] = {"url": "http://127.0.0.1:3000"}
        api = fixture.Subspace()
        cfg = validate(legacy, {**secrets, "apiToken": "header.payload.admin"})
        fleet_records.register(api.call, fleet_records.fleet_fields(cfg), fleet_records.roster(cfg), lambda _: None)
        ids = sorted(row["userId"] for row in api.agents.values())
        renames = [{"old": "example-atlas", "name": "atlas", "nick": "atlas", "channel": None},
                   {"old": "example-orion", "name": "orion", "nick": "orion",
                    "channel": ("#example-fleet-orion", "#orion")}]
        said = []
        self.assertEqual(fleet_records.rename(api.call, "example-fleet", renames, said.append), 3)
        self.assertEqual(api.summary()["agents"], [("atlas", "rearAdmiral", "atlas", None, True),
                                                   ("orion", "captain", "orion", "Orion", True)])
        self.assertEqual(api.summary()["ships"], [("Orion", "#orion")])
        self.assertEqual(sorted(row["userId"] for row in api.agents.values()), ids)
        # A re-run after a partial failure finds the renamed records and writes nothing.
        self.assertEqual(fleet_records.rename(api.call, "example-fleet", renames, said.append), 0)
        # A name another fleet's agent holds is refused, naming it.
        other = fleet_records.taken_names(
            [{"name": "vega", "ircNick": "vega", "fleet": "other-fleet", "isActive": True}], "example-fleet",
            [("vega", "vega", {"vega"}), ("orion", "orion", {"orion"})])
        self.assertEqual(other, ["vega is taken in the Subspace API: agent vega of fleet other-fleet has it"])


def fixture_legacy():
    """The example as subspace-setup installed it before plain names, with its officers' passwords."""
    config = fixture.legacy_names(json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text()))
    config["irc"]["auth"] = "password"
    return config, officer_secrets()


class RepositoryAccessTests(unittest.TestCase):
    """A repository the installer would clone as public must be readable without credentials."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)
        config = example_config()
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"},
                                  {"id": "library", "url": "https://github.com/example/library.git"}]
        (self.work / "fleet.json").write_text(json.dumps(config))
        (self.work / "fleet.secrets.json").write_text(json.dumps(officer_secrets()))
        (self.work / "fleet.secrets.json").chmod(0o600)

    def main(self, verb, readable):
        output = io.StringIO()
        (self.work / "stage").mkdir(exist_ok=True)
        tail = [str(self.work / "stage")] if verb == "prepare" else []
        with patch.object(runtime, "ETC", self.work / "etc"), patch.object(runtime, "STATE", self.work / "state"), \
                patch.object(runtime, "readable_without_credentials", side_effect=readable) as check, \
                contextlib.redirect_stdout(output):
            runtime.main([verb, str(self.work / "fleet.json"), str(self.work / "fleet.secrets.json"), *tail, "", "false"])
        return output.getvalue(), [call.args[0] for call in check.call_args_list]

    def test_the_dry_run_and_the_real_run_both_stop_before_cloning_and_say_how_to_go_on(self):
        private = lambda url: "library" not in url  # noqa: E731
        for verb in ("plan", "prepare"):
            with self.subTest(verb), self.assertRaisesRegex(Invalid, "Git cannot read library without credentials") \
                    as refused:
                self.main(verb, private)
            message = str(refused.exception)
            self.assertIn("install without them now", message)
            self.assertIn("subspace-setup --repositories with the others, or '' for none", message)
            self.assertIn("'Add private repositories later'", message)
        self.assertFalse((self.work / "stage/fleet.json").exists())
        text, checked = self.main("plan", lambda url: True)
        self.assertEqual(checked, ["https://github.com/example/project.git", "https://github.com/example/library.git"])
        self.assertNotIn("Git cannot read", text)
        # On a remote IRC server the officers create the standard channels by joining: nothing to register first.
        self.assertIn("the officers create them by joining, so they need no registration", text)
        self.assertNotIn("Register them on the IRC server", text)
        # An installed repository is not checked again.
        (self.work / "etc").mkdir()
        (self.work / "etc/repository-sources.json").write_text(json.dumps(
            {"library": {"url": "https://github.com/example/library.git", "private": False}}))
        (self.work / "etc/repository-sources.json").chmod(0o600)
        _, checked = self.main("plan", private)
        self.assertEqual(checked, ["https://github.com/example/project.git"])

    def test_the_check_asks_git_with_no_credentials_or_prompt(self):
        with patch.object(runtime.subprocess, "run", return_value=subprocess.CompletedProcess([], 128)) as run:
            self.assertFalse(readable_without_credentials("https://github.com/example/private.git"))
        command, environment = run.call_args.args[0], run.call_args.kwargs["env"]
        self.assertEqual(command, ["git", "-c", "credential.helper=", "ls-remote", "--heads", "--",
                                   "https://github.com/example/private.git"])
        self.assertEqual(environment["GIT_TERMINAL_PROMPT"], "0")
        with patch.object(runtime.subprocess, "run", side_effect=subprocess.TimeoutExpired("git", 30)):
            self.assertFalse(readable_without_credentials("https://github.com/example/slow.git"))


class OfficerModelTests(unittest.TestCase):
    """Captains default to Claude Opus 5.5 at xhigh effort and the rear admiral to Claude Fable 5.2 at high."""

    def config(self, harness=None):
        config = example_config()
        config["captains"].append({"id": "vega", "name": "Vega", "account": "example-vega"})
        if harness is None:
            del config["harness"]
        else:
            config["harness"] = harness
        return config

    def harnesses(self, config):
        cfg = validate(config, officer_secrets("example-vega"))
        return {person["id"]: (person["harness"]["family"], person["harness"]["model"], person["harness"]["effort"])
                for person in cfg["members"]}

    def test_each_role_has_its_default_and_the_example_names_it(self):
        defaults = {"atlas": ("claude", "claude-fable-5-2", "high"), "orion": ("claude", "claude-opus-5-5", "xhigh"),
                    "vega": ("claude", "claude-opus-5-5", "xhigh")}
        self.assertEqual(self.harnesses(self.config()), defaults)
        self.assertEqual(self.harnesses(example_config() | {"captains": self.config()["captains"]}), defaults)
        cfg = validate(self.config(), officer_secrets("example-vega"))
        self.assertEqual([runtime.harness_model(person["harness"]) for person in cfg["members"]],
                         ["claude-proxy/claude-fable-5-2#high", "claude-proxy/claude-opus-5-5#xhigh",
                          "claude-proxy/claude-opus-5-5#xhigh"])
        self.assertEqual(runtime.session_model(cfg["members"][1]["harness"]),
                         {"providerID": "claude-proxy", "id": "claude-opus-5-5", "variant": "xhigh"})
        self.assertEqual(runtime.officer_models_line(cfg),
                         "Officer models: atlas on claude-fable-5-2 at high effort; orion, vega on claude-opus-5-5 "
                         "at xhigh effort. The model proxy needs a claude account for them.")

    def test_the_fleet_then_the_role_then_the_officer_override_the_default(self):
        # A fleet-wide model, as fleets wrote before the role defaults, applies to both roles at the provider's
        # default effort: an effort belongs to the model it was set for.
        old = {"type": "opencode", "family": "codex", "model": "gpt-5.4"}
        self.assertEqual(set(self.harnesses(self.config(old)).values()), {("codex", "gpt-5.4", None)})
        roles = {"type": "opencode", "captain": {"family": "codex", "model": "gpt-6.2-sol", "effort": "high"},
                 "rearAdmiral": {"effort": "max"}}
        config = self.config(roles)
        config["captains"][1]["harness"] = {"model": "claude-sonnet-5-5", "family": "claude"}
        self.assertEqual(self.harnesses(config), {
            "atlas": ("claude", "claude-fable-5-2", "max"), "orion": ("codex", "gpt-6.2-sol", "high"),
            "vega": ("claude", "claude-sonnet-5-5", None)})
        # The same model keeps its role's effort; a Claude Agent SDK captain takes it too.
        config["captains"][1].update(harness={"type": "claude-sdk", "model": "claude-opus-5-5"},
                                     crew={"claude": {"count": 0}, "codex": {"count": 0}})
        config["harness"] = {"type": "opencode"}
        self.assertEqual(self.harnesses(config)["vega"], ("claude", "claude-opus-5-5", "xhigh"))

    def test_a_hand_written_model_goes_to_its_own_family_and_renders_as_before(self):
        # Before the role defaults every officer was Codex, so a harness that named only a model, as the old
        # default {"type":"opencode","model":"gpt-5.4"} did, ran it through openai-proxy. It still does, at the
        # provider's default effort, fleet-wide or on one officer.
        for harness in ({"type": "opencode", "model": "gpt-5.4"}, {"model": "gpt-6.2-sol"}):
            with self.subTest(harness):
                cfg = validate(self.config(harness), officer_secrets("example-vega"))
                self.assertEqual({runtime.harness_model(person["harness"]) for person in cfg["members"]},
                                 {f"openai-proxy/{harness['model']}"})
                self.assertEqual({person["harness"]["effort"] for person in cfg["members"]}, {None})
        config = self.config()
        config["captains"][1]["harness"] = {"model": "gpt-6.2-sol"}
        config["rearAdmiral"]["harness"] = {"model": "claude-sonnet-5-5"}
        self.assertEqual(self.harnesses(config), {
            "atlas": ("claude", "claude-sonnet-5-5", None), "orion": ("claude", "claude-opus-5-5", "xhigh"),
            "vega": ("codex", "gpt-6.2-sol", None)})
        # A harness that names only a family: codex runs its default as before; claude runs the role's Claude
        # default, which is what the role defaults to anyway, rather than claude-sonnet-5-5.
        self.assertEqual(set(self.harnesses(self.config({"family": "codex"})).values()), {("codex", "gpt-5.4", None)})
        self.assertEqual(self.harnesses(self.config({"family": "claude"}))["orion"], ("claude", "claude-opus-5-5", "xhigh"))
        # A model and a family that disagree are refused, whichever way round.
        for harness, family in (({"family": "claude", "model": "gpt-5.4"}, "claude"),
                                ({"family": "codex", "model": "claude-opus-5-5"}, "codex")):
            with self.subTest(harness), self.assertRaisesRegex(Invalid, f"does not belong to the {family} family"):
                self.harnesses(self.config(harness))

    def test_an_effort_the_harness_cannot_give_is_refused(self):
        cases = [
            ({"captain": {"family": "codex", "model": "gpt-5.4", "effort": "max"}},
             "'max' is not available for gpt-5.4 with the opencode harness, which offers low, medium, high, xhigh"),
            ({"captain": {"model": "claude-sonnet-4-6", "effort": "xhigh"}},
             "offers low, medium, high, max"),
            ({"captain": {"model": "claude-sonnet-4-5", "effort": "low"}}, "offers high, max"),
            # Dated IDs: the date is not a version, and the version-first spelling is read as such.
            ({"captain": {"model": "claude-3-7-sonnet-20250219", "effort": "xhigh"}}, "offers high, max"),
            ({"captain": {"model": "claude-opus-4-20250514", "effort": "xhigh"}}, "offers high, max"),
            ({"captain": {"model": "claude-sonnet-4-5-20250929", "effort": "medium"}}, "offers high, max"),
            ({"captain": {"model": "claude-opus-5-5#xhigh"}}, "give the model ID alone"),
            ({"captain": {"model": "claude-opus-5-5", "speed": "fast"}}, "harness.captain contains an unknown field"),
        ]
        for harness, message in cases:
            with self.subTest(harness), self.assertRaisesRegex(Invalid, re.escape(message)):
                self.harnesses(self.config({"type": "opencode", **harness}))
        self.assertEqual(self.harnesses(self.config({"captain": {"model": "claude-opus-4-5", "effort": "xhigh"}}))
                         ["orion"], ("claude", "claude-opus-4-5", "xhigh"))
        for model, effort in (("claude-opus-4-5-20251101", "xhigh"), ("claude-sonnet-4-5-20250929", "high"),
                              ("claude-opus-4-6", "max"), ("claude-opus-5-5", "max")):
            with self.subTest(model):
                self.assertEqual(self.harnesses(self.config({"captain": {"model": model, "effort": effort}}))["orion"],
                                 ("claude", model, effort))

    def test_codex_extra_high_is_explicit_for_supported_models(self):
        for model in ("gpt-5.4", "gpt-6.2-sol"):
            with self.subTest(model):
                cfg = validate(self.config({"captain": {"model": model, "effort": "xhigh"}}),
                               officer_secrets("example-vega"))
                self.assertEqual(cfg["members"][1]["harness"]["effort"], "xhigh")
                catalogue = runtime.model_provider(cfg, "codex", [model, "custom-review-model"])["models"]
                self.assertEqual(catalogue[model]["variants"],
                                 [{"id": effort, "settings": {"reasoningEffort": effort}}
                                  for effort in ("low", "medium", "high", "xhigh")])
                self.assertNotIn("variants", catalogue["custom-review-model"])

    def test_officers_whose_model_no_signed_in_account_serves_stop_both_runs(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        work = Path(temporary.name)
        (work / "fleet.json").write_text(json.dumps(self.config()))
        (work / "fleet.secrets.json").write_text(json.dumps(officer_secrets("example-vega")))
        (work / "fleet.secrets.json").chmod(0o600)
        auth = work / "state/proxy/auth"

        def main(verb):
            output = io.StringIO()
            (work / "stage").mkdir(exist_ok=True)
            tail = [str(work / "stage")] if verb == "prepare" else []
            with patch.object(runtime, "ETC", work / "etc"), patch.object(runtime, "STATE", work / "state"), \
                    contextlib.redirect_stdout(output):
                runtime.main([verb, str(work / "fleet.json"), str(work / "fleet.secrets.json"), *tail, "", "false"])
            return output.getvalue()

        # Before the first sign-in nothing is checked: the install comes first, then the sign-in.
        self.assertIn("The model proxy needs a claude account for them.", main("plan"))
        auth.mkdir(parents=True)
        (auth / "codex-someone@example.com-plus.json").write_text(json.dumps({"type": "codex", "access_token": "x"}))
        for verb in ("plan", "prepare"):
            with self.subTest(verb), self.assertRaises(Invalid) as refused:
                main(verb)
            message = str(refused.exception)
            self.assertIn("signed in to codex accounts only, which cannot serve claude-fable-5-2 (claude account), "
                          "claude-opus-5-5 (claude account), the model of atlas, orion, vega", message)
            self.assertIn("providers login claude", message)
            self.assertIn("--captain-model and --rear-admiral-model", message)
            self.assertNotIn("access_token", message)
        (auth / "claude-someone@example.com.json").write_text(json.dumps({"type": "claude"}))
        main("plan")
        # A model of another provider is the operator's to check.
        (auth / "claude-someone@example.com.json").unlink()
        config = self.config({"type": "opencode", "captain": {"family": "codex", "model": "kimi-k3"},
                              "rearAdmiral": {"family": "codex", "model": "gpt-5.4"}})
        (work / "fleet.json").write_text(json.dumps(config))
        self.assertIn("The model proxy needs a codex account for them.", main("plan"))


class IrcCertificateTests(unittest.TestCase):
    """Officers sign in to a remote IRC server with a certificate each (SASL EXTERNAL) unless told otherwise."""

    SECRETS = {"proxyKey": "fixture-model-proxy-key"}

    def setUp(self):
        self.config = fixture.legacy_names(json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text()))
        # The example names the default explicitly; these tests check the default itself.
        del self.config["irc"]["auth"]
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)

    def bundle(self, account="example-atlas"):
        return runtime.new_irc_bundle(account)

    def supply(self, agent_id, contents, mode=0o600):
        directory = self.work / "fleet.secrets" / agent_id
        directory.mkdir(parents=True, mode=0o700, exist_ok=True)
        (self.work / "fleet.secrets").chmod(0o700)
        path = directory / runtime.IRC_CLIENT_PEM
        path.write_text(contents)
        path.chmod(mode)
        return path

    @staticmethod
    def split(bundle):
        """A generated bundle's certificate and its private key."""
        start = runtime.IRC_PEM_KEY.search(bundle).start()
        return bundle[:start], bundle[start:]

    def fingerprint(self, bundle):
        """Independently of the installer: SHA-256 over the first certificate's DER, as Ergo computes certfp."""
        first = bundle[:bundle.index("-----END CERTIFICATE-----") + len("-----END CERTIFICATE-----")]
        return hashlib.sha256(ssl.PEM_cert_to_DER_cert(first)).hexdigest()

    def test_a_remote_server_defaults_to_certificates_and_keeps_no_officer_password(self):
        cfg = validate(self.config, self.SECRETS)
        self.assertEqual(cfg["irc"]["auth"], "certificate")
        self.assertEqual(cfg["passwords"], {})
        officer_passwords = {"ircPasswords": {"example-orion": "fixture-password"}}
        with self.assertRaisesRegex(Invalid, "no IRC password for the rear admiral or a captain.*irc.auth to password"):
            validate(self.config, {**self.SECRETS, **officer_passwords})
        # Account names fold: a password under another spelling of an officer's account is still refused.
        with self.assertRaisesRegex(Invalid, "Remove their entries from ircPasswords"):
            validate(self.config, {**self.SECRETS, "ircPasswords": {"Example-Orion": "fixture-password"}})

    def test_the_example_files_are_an_installable_pair(self):
        # Quick start copies both files: remote IRC with certificates, so the secrets hold no officer password.
        root = Path(__file__).resolve().parents[1]
        cfg = validate(json.loads((root / "fleet.example.json").read_text()),
                       json.loads((root / "secrets.example.json").read_text()))
        self.assertEqual((cfg["irc"]["auth"], cfg["passwords"]), ("certificate", {}))
        # Every optional feature is off: no repositories (so no crew or pinned root), no Slack bot, no API.
        self.assertEqual((cfg["repositories"], cfg["api"], cfg["agentDispatch"]), ({}, None, None))
        self.assertFalse(any("slack" in person or "sessionRoot" in person or person["crew"]
                             for person in cfg["members"]))
        self.assertFalse(runtime.rear_root(cfg))

    def test_the_readmes_optional_settings_complete_the_example(self):
        root = Path(__file__).resolve().parents[1]
        readme = (root / "README.md").read_text()
        fragment = json.loads(re.search(r"### Optional settings\n.*?```json\n(.*?)```", readme, re.S).group(1))
        config = {**json.loads((root / "fleet.example.json").read_text()), **fragment}
        cfg = validate(config, json.loads((root / "secrets.example.json").read_text()))
        rear, orion = cfg["members"]
        self.assertEqual((rear["slack"]["appId"], orion["slack"]["appId"], orion["sessionRoot"]),
                         ("A0EXAMPLE01", "A0EXAMPLE02", "workspace"))
        self.assertEqual(cfg["repositoryNotes"]["project"]["tracker"],
                         "Example issues at https://tracker.example.com/project")
        self.assertEqual(cfg["irc"]["auth"], "certificate")
        self.assertTrue(runtime.rear_root(cfg))

    def test_passwords_remain_an_explicit_choice(self):
        self.config["irc"]["auth"] = "password"
        with self.assertRaisesRegex(Invalid, "Supply an IRC password"):
            validate(self.config, self.SECRETS)
        cfg = validate(self.config, officer_secrets())
        registration, environment = runtime.irc_agent(cfg, cfg["members"][1], {"directory": "/w"})
        self.assertEqual(registration["passwordEnv"], "IRC_ORION")
        self.assertNotIn("clientPemFile", registration)
        self.assertEqual(environment, {"IRC_ORION": "fixture-password"})
        self.assertEqual(runtime.irc_sign_in(cfg), ["Officers sign in to IRC with passwords (SASL PLAIN, irc.auth password)."])
        with self.assertRaisesRegex(Invalid, r"irc.auth \(certificate or password\)"):
            validate(dict(self.config, irc={**self.config["irc"], "auth": "external"}), officer_secrets())

    def test_the_managed_ergo_keeps_passwords(self):
        self.config["irc"] = {"mode": "local", "allowedAccounts": ["operator"],
                              "compose": {"operator": "ircop", "ref": "0" * 40}}
        secrets = {**officer_secrets(), "ircOperatorPassword": "fixture-operator-password"}
        self.assertEqual(validate(self.config, secrets)["irc"]["auth"], "password")
        self.config["irc"]["auth"] = "certificate"
        with self.assertRaisesRegex(Invalid, "needs a remote IRC server.*only over TLS"):
            validate(self.config, secrets)

    def test_the_gateway_registration_names_the_bundle_and_no_password(self):
        cfg = validate(self.config, self.SECRETS)
        for person in cfg["members"]:
            registration, environment = runtime.irc_agent(cfg, person, {"directory": "/w"})
            self.assertEqual(registration["clientPemFile"], f"/etc/subspace-fleet/irc-certs/{person['id']}.pem")
            self.assertNotIn("passwordEnv", registration)
            self.assertEqual(environment, {})
            entry = runtime.irc_officer(cfg, person, registration)
            self.assertTrue(entry["server"]["tls"])
            runtime.check_sign_in(entry, entry["server"])
        # Stargate's rule: exactly one of the two, and a certificate only over TLS.
        both = {**registration, "passwordEnv": "IRC_ORION"}
        neither = {key: value for key, value in registration.items() if key != "clientPemFile"}
        for broken in (both, neither):
            with self.assertRaisesRegex(Invalid, "exactly one of a password and a client certificate"):
                runtime.check_sign_in(broken, {"tls": True})
        with self.assertRaisesRegex(Invalid, "needs TLS"):
            runtime.check_sign_in(registration, {"tls": False})

    def test_bundles_are_kept_and_owned_only_by_the_gateway_that_serves_the_officer(self):
        # Orion runs on the Claude Agent SDK, so its own Linux user runs its gateway.
        self.config["captains"][0].update(harness={"type": "claude-sdk", "family": "claude"},
                                          crew={"claude": {"count": 0}, "codex": {"count": 0}})
        cfg = validate(self.config, self.SECRETS)
        owners = []
        me = pwd.getpwuid(os.getuid())

        def lookup(name):
            owners.append(name)
            return me

        certs = self.work / "irc-certs"
        with patch.object(runtime, "IRC_CERTS", certs), patch.object(runtime, "STATE", self.work), \
                patch.object(runtime, "directory", lambda path, mode, *_: path.mkdir(mode=mode, exist_ok=True)), \
                patch.object(runtime.pwd, "getpwnam", lookup):
            runtime.irc_certificates(cfg)
            self.assertEqual(owners, [runtime.GATEWAY, "ss-orion"])
            first = {path.name: path.read_text() for path in certs.iterdir()}
            self.assertEqual(set(first), {"atlas.pem", "orion.pem"})
            for name, bundle in first.items():
                info = (certs / name).stat()
                self.assertEqual((stat.S_IMODE(info.st_mode), info.st_uid), (0o600, os.getuid()))
                self.assertEqual(len(runtime.IRC_PEM_CERTIFICATE.findall(bundle)), 1)
                self.assertIsNotNone(runtime.IRC_PEM_KEY.search(bundle))
                self.assertNotIn("ENCRYPTED", bundle)
            # An upgrade keeps each officer's certificate, so its bound fingerprint stays valid.
            (self.work / "pending").unlink()
            runtime.irc_certificates(cfg)
            self.assertEqual({path.name: path.read_text() for path in certs.iterdir()}, first)
            self.assertFalse((self.work / "pending").exists())
            # A bundle that is not a certificate and its key stops the render rather than reaching Stargate.
            (certs / "atlas.pem").write_text(self.split(first["orion.pem"])[0])
            with self.assertRaisesRegex(Invalid, "certificate and its unencrypted private key"):
                runtime.irc_certificates(cfg)

    def test_an_operator_can_bring_existing_bundles(self):
        cfg = validate(self.config, self.SECRETS)
        supplied = self.bundle("example-orion")
        self.supply("orion", supplied)
        self.assertEqual(runtime.irc_bundles(cfg, self.work / "fleet.secrets"), {"orion": supplied})
        # Passwords need no bundle, so none is read.
        self.assertEqual(runtime.irc_bundles(validate(example_config(), officer_secrets()), self.work / "fleet.secrets"), {})
        # Only owner-only files, and only a certificate with its own key, as Stargate requires.
        self.supply("orion", supplied, 0o640)
        with self.assertRaisesRegex(Invalid, "owner-only"):
            runtime.irc_bundles(cfg, self.work / "fleet.secrets")
        other = self.bundle("example-atlas")
        certificate, _ = self.split(supplied)
        _, key = self.split(other)
        self.supply("orion", certificate + key)
        with self.assertRaisesRegex(Invalid, "does not match an unencrypted private key"):
            runtime.irc_bundles(cfg, self.work / "fleet.secrets")
        self.supply("orion", certificate)
        with self.assertRaisesRegex(Invalid, "certificate and its unencrypted private key"):
            runtime.irc_bundles(cfg, self.work / "fleet.secrets")

    def test_the_readmes_move_from_passwords_to_certificates_works_in_its_order(self):
        config, secrets = self.work / "fleet.json", self.work / "fleet.secrets.json"
        config.write_text(json.dumps(self.config))

        def dry_run():
            secrets.chmod(0o600)
            output = io.StringIO()
            with patch.object(runtime, "IRC_CERTS", self.work / "installed"), contextlib.redirect_stdout(output):
                runtime.main(["plan", str(config), str(secrets), str(self.work / "fleet.secrets"), "false"])
            return output.getvalue()
        # An installed remote fleet's secrets still hold the officers' passwords: even the dry run is refused.
        secrets.write_text(json.dumps({**self.SECRETS, "ircPasswords": {
            "example-atlas": "fixture-password", "example-orion": "fixture-password"}}))
        with self.assertRaisesRegex(Invalid, "Remove their entries from ircPasswords"):
            dry_run()
        # 1. Remove them; 2. supply bundles; 3. the dry run prints each fingerprint to bind (4) before upgrading (5).
        secrets.write_text(json.dumps(self.SECRETS))
        bundles = {agent: self.bundle(f"example-{agent}") for agent in ("atlas", "orion")}
        for agent, bundle in bundles.items():
            self.supply(agent, bundle)
        text = dry_run()
        for agent, bundle in bundles.items():
            self.assertIn(f"/msg NickServ CERT ADD example-{agent} {self.fingerprint(bundle)} (supplied)", text)

    def test_dry_run_prints_fingerprints_for_binding_and_never_key_material(self):
        supplied = self.bundle("example-orion")
        self.supply("orion", supplied)
        config, secrets = self.work / "fleet.json", self.work / "fleet.secrets.json"
        config.write_text(json.dumps(self.config))
        secrets.write_text(json.dumps(self.SECRETS))
        secrets.chmod(0o600)
        output = io.StringIO()
        with patch.object(runtime, "IRC_CERTS", self.work / "installed"), contextlib.redirect_stdout(output):
            runtime.main(["plan", str(config), str(secrets), str(self.work / "fleet.secrets"), "false"])
        text = output.getvalue()
        fingerprint = self.fingerprint(supplied)
        self.assertRegex(fingerprint, r"\A[0-9a-f]{64}\Z")
        self.assertIn(f"/msg NickServ CERT ADD example-orion {fingerprint} (supplied)", text)
        self.assertIn("example-atlas: a new certificate is generated on install", text)
        self.assertIn("bound to its account on irc.example.com", text)
        self.assertNotIn("PRIVATE KEY", text)
        key_body = self.split(supplied)[1].splitlines()[1]
        self.assertNotIn(key_body, text)
        # The install summary reads the installed bundles and prints the same form.
        cfg = validate(self.config, self.SECRETS)
        installed = self.work / "installed"
        installed.mkdir()
        for agent_id, bundle in (("atlas", self.bundle()), ("orion", supplied)):
            (installed / f"{agent_id}.pem").write_text(bundle)
        with patch.object(runtime, "IRC_CERTS", installed):
            lines = runtime.irc_sign_in(cfg)
        self.assertEqual(lines[1:], [
            f"  /msg NickServ CERT ADD example-atlas {self.fingerprint((installed / 'atlas.pem').read_text())}",
            f"  /msg NickServ CERT ADD example-orion {fingerprint}",
        ])

    def test_dispatch_carries_supplied_bundles_to_the_vm(self):
        import dispatch
        supplied = self.bundle("example-orion")
        self.supply("orion", supplied)
        (self.work / "fleet.json").write_text(json.dumps(self.config))
        (self.work / "fleet.secrets.json").write_text(json.dumps(self.SECRETS))
        (self.work / "fleet.secrets.json").chmod(0o600)
        payload = dispatch.bundle({"config": "fleet.json", "secrets": "fleet.secrets.json"}, self.work)
        with tarfile.open(fileobj=io.BytesIO(payload)) as archive:
            member = archive.getmember(f"fleet.secrets/orion/{runtime.IRC_CLIENT_PEM}")
            self.assertEqual(member.mode, 0o600)
            self.assertEqual(archive.extractfile(member).read().decode(), supplied)


if __name__ == "__main__":
    unittest.main()
