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
from roles import default_permissions
from runtime import Invalid, repository_config, toolchain_config, validate
import workspaces


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

    def test_private_repository_requires_distinct_apps_for_crew(self):
        example = Path(__file__).resolve().parents[1] / "fleet.example.json"
        config = json.loads(example.read_text())
        config["crew"] = {"claude": {"count": 0}, "codex": {"count": 1}}
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git", "private": True}]
        secrets = {"proxyKey": "fixture-model-proxy-key", "ircPasswords": {
            account: "fixture-password" for account in ("example-atlas", "example-orion", "example-orion-codex-1")
        }}
        apps = [
            {"appId": index, "installationId": index, "clientId": f"fixture-{index}",
             "botId": index, "botLogin": f"fixture-{index}[bot]"}
            for index in range(1, 4)
        ]
        config["rearAdmiral"]["githubApp"], config["captains"][0]["githubApp"] = apps[:2]
        with self.assertRaisesRegex(Invalid, "distinct GitHub App for every active agent"):
            validate(config, secrets)
        config["captains"][0]["crewGitHubApps"] = {"codex-1": apps[1]}
        with self.assertRaisesRegex(Invalid, "distinct GitHub App identity"):
            validate(config, secrets)
        config["captains"][0]["crewGitHubApps"]["codex-1"] = apps[2]
        config["captains"][0]["crewNames"] = {"codex-1": "Rigel"}
        crew = validate(config, secrets)["members"][-1]
        self.assertEqual(crew["githubApp"], apps[2])
        self.assertEqual(crew["name"], "Rigel")

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
