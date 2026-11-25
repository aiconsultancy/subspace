"""Offline checks for the fleet's toolchains: the toolchains and systemPackages settings, the officers' tool
environment and the dry run's plan.

Run with Python 3.10 or later: python3 -B subspace-installer/tests/toolchains_test.py
"""

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "vm"))
import runtime  # noqa: E402
import toolchains  # noqa: E402
from toolchains import Invalid  # noqa: E402


class ConfigurationTests(unittest.TestCase):
    def test_existing_values_keep_their_meaning(self):
        self.assertEqual(toolchains.toolchain_config({"go": "1.26.1", "kubectl": "1.35", "rust": "stable"}), {
            "node": {"versions": ["24"], "install": "shared", "options": {}},
            "go": {"versions": ["1.26.1"], "install": "shared", "options": {}},
            "kubectl": {"versions": ["1.35"], "install": "shared", "options": {}},
            "rust": {"versions": ["stable"], "install": "user", "options": {}}})
        self.assertEqual(toolchains.toolchain_config({"node": False}), {})

    def test_any_mise_tool_and_several_versions(self):
        tools = toolchains.toolchain_config({
            "node": ["22.11.0", "24"], "dotnet": "8.0.404", "pnpm": "9.12.3", "cargo:cargo-nextest": "0.9.100",
            "github:example/release-tool": {"version": "2.4.0", "options": {"bin": "release-tool"}},
            "conda:openssl": "3.4.0", "npm:@example/cli": "1.2.0",
            "rust": {"version": "1.99.0", "options": {"components": "clippy,rustfmt"}},
            "terraform": {"version": "1.9.8", "install": "user"}})
        self.assertEqual(tools["node"]["versions"], ["22.11.0", "24"])
        self.assertEqual({spec: entry["install"] for spec, entry in tools.items()}, {
            "node": "shared", "dotnet": "shared", "pnpm": "shared", "cargo:cargo-nextest": "user",
            "github:example/release-tool": "shared", "conda:openssl": "shared", "npm:@example/cli": "shared",
            "rust": "user", "terraform": "user"})
        self.assertEqual(toolchains.requests(tools, "shared"), [
            "node@22.11.0", "node@24", "dotnet@8.0.404", "pnpm@9.12.3", "github:example/release-tool@2.4.0",
            "conda:openssl@3.4.0", "npm:@example/cli@1.2.0"])
        self.assertEqual(toolchains.requests(tools, "user"), ["rust@1.99.0", "terraform@1.9.8",
                                                              "cargo:cargo-nextest@0.9.100"])
        self.assertEqual(toolchains.mise_config(tools).splitlines()[1:], [
            "[tools]",
            'node = ["22.11.0", "24"]',
            'dotnet = "8.0.404"',
            'pnpm = "9.12.3"',
            '"cargo:cargo-nextest" = "0.9.100"',
            '"github:example/release-tool" = { version = "2.4.0", bin = "release-tool" }',
            '"conda:openssl" = "3.4.0"',
            '"npm:@example/cli" = "1.2.0"',
            'rust = { version = "1.99.0", components = "clippy,rustfmt" }',
            'terraform = "1.9.8"'])

    def test_names_versions_options_and_packages_are_strict(self):
        # These end up on command lines run as root, and in a TOML file that root's mise reads.
        for tools in ({"node; rm -rf /": "1"}, {"-rf": "1"}, {"node": "24 --force"}, {"node": "$(id)"},
                      {"node": "24\n[env]"}, {"github:../../etc": "1"}, {"http:https://example.com/x": "1"},
                      {"Node": "24"}, {"node": ""}, {"node": []}, {"node": ["1"] * 2}, {"node": [str(n) for n in range(9)]},
                      {"rust": {"version": "1", "options": {"postinstall": "curl example.com | sh"}}},
                      {"rust": {"version": "1", "options": {"install_env": {"A": "b"}}}},
                      {"rust": {"version": "1", "options": {"components": "a b"}}},
                      {"rust": {"version": "1", "install": "root"}}, {"rust": {"versions": ["1"]}}, {"rust": True}):
            with self.subTest(tools=tools), self.assertRaises(Invalid):
                toolchains.toolchain_config(tools)
        for packages in (["zsh; reboot"], ["-y"], ["Clang"], ["libssl-dev", "libssl-dev"], "zsh", [7], ["a"]):
            with self.subTest(packages=packages), self.assertRaises(Invalid):
                toolchains.package_config(packages)
        self.assertEqual(toolchains.package_config(["zsh", "g++", "libwebkit2gtk-4.1-dev"]),
                         ["zsh", "g++", "libwebkit2gtk-4.1-dev"])

    def test_the_fleet_installer_takes_both_settings(self):
        config = json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text())
        config.update(toolchains={"dotnet": "8.0.404", "rust": "1.99.0"}, systemPackages=["zsh", "clang"])
        cfg = runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"}, check_files=False)
        self.assertEqual(list(cfg["toolchains"]), ["node", "dotnet", "rust"])
        self.assertEqual(cfg["systemPackages"], ["zsh", "clang"])
        config["systemPackages"] = ["zsh && reboot"]
        with self.assertRaises(Invalid):
            runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"}, check_files=False)


class EnvironmentTests(unittest.TestCase):
    def test_an_officers_tool_environment(self):
        tools = toolchains.toolchain_config({"dotnet": "8.0.404"})
        home = Path("/var/lib/subspace-fleet/agents/orion")
        environment = toolchains.officer_environment(tools, "ss-orion", home, [home / "workspace", home / "worktrees"])
        self.assertEqual(environment, {
            "PATH": "/var/lib/subspace-fleet/agents/orion/.local/share/mise/shims:/opt/subspace-fleet/toolchains/shims:"
                    "/opt/subspace-fleet/bin:/usr/local/bin:/usr/bin:/bin",
            "MISE_DATA_DIR": "/var/lib/subspace-fleet/agents/orion/.local/share/mise",
            "MISE_SYSTEM_DATA_DIR": "/opt/subspace-fleet/toolchains",
            "MISE_SYSTEM_CONFIG_DIR": "/opt/subspace-fleet/toolchains/system-config",
            "MISE_SYSTEM_CONFIG_FILE": "/etc/subspace-fleet/toolchains.toml",
            "MISE_TRUSTED_CONFIG_PATHS": "/var/lib/subspace-fleet/agents/orion/workspace:"
                                         "/var/lib/subspace-fleet/agents/orion/worktrees",
            "TMPDIR": "/tmp/ss-orion",
            "DOTNET_ROOT": "/opt/subspace-fleet/toolchains/dotnet-root"})
        # Without .NET, no DOTNET_ROOT; a per-user .NET has its own.
        self.assertNotIn("DOTNET_ROOT", toolchains.officer_environment(toolchains.toolchain_config({}), "ss-orion",
                                                                       home, []))
        personal = toolchains.toolchain_config({"dotnet": {"version": "8.0.404", "install": "user"}})
        self.assertEqual(toolchains.officer_environment(personal, "ss-orion", home, [])["DOTNET_ROOT"],
                         "/var/lib/subspace-fleet/agents/orion/.local/share/mise/dotnet-root")
        # Root's installs use the shared store and the fleet's configuration only.
        root = toolchains.root_environment(tools)
        self.assertEqual((root["MISE_DATA_DIR"], root["MISE_GLOBAL_CONFIG_FILE"], root["HOME"], root["DOTNET_ROOT"]),
                         ("/opt/subspace-fleet/toolchains", "/etc/subspace-fleet/toolchains.toml", "/root",
                          "/opt/subspace-fleet/toolchains/dotnet-root"))

    def test_the_private_temporary_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ss-orion"
            toolchains.private_tmp(path)
            self.assertEqual(path.stat().st_mode & 0o777, 0o700)
            path.chmod(0o755)
            toolchains.private_tmp(path)
            self.assertEqual(path.stat().st_mode & 0o777, 0o700)
            planted = Path(directory) / "ss-vega"
            planted.symlink_to(directory)
            with self.assertRaisesRegex(Invalid, "not a directory of this user"):
                toolchains.private_tmp(planted)

    def test_installs_run_explicit_requests_as_root_and_as_each_officer(self):
        config = json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text())
        config.update(toolchains={"pnpm": "9.12.3", "rust": {"version": "1.99.0",
                                                             "options": {"components": "clippy,rustfmt"}}})
        cfg = runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"}, check_files=False)
        homes = {person["id"]: Path(f"/var/lib/subspace-fleet/agents/{person['id']}") for person in cfg["members"]}
        with patch.object(runtime, "directory"), patch.object(runtime.subprocess, "run") as run:
            runtime.install_toolchains(cfg, homes)
            runtime.install_toolchains(cfg, homes, update=True)
        calls = run.call_args_list
        self.assertEqual(calls[0].args[0], ["/opt/subspace-fleet/bin/mise", "install", "node@24", "pnpm@9.12.3"])
        self.assertEqual(calls[0].kwargs["cwd"], runtime.APP)
        self.assertEqual(calls[0].kwargs["env"]["MISE_DATA_DIR"], "/opt/subspace-fleet/toolchains")
        users = [call.args[0][2] for call in calls[1:3]]
        self.assertEqual(users, ["ss-atlas", "ss-orion"])
        orion = calls[2].args[0]
        # Only the officer's tool environment: nothing of root's.
        self.assertEqual(orion[:6], ["runuser", "-u", "ss-orion", "--", "/usr/bin/env", "-i"])
        self.assertEqual(orion[-3:], ["/opt/subspace-fleet/bin/mise", "install", "rust@1.99.0"])
        self.assertIn("MISE_DATA_DIR=/var/lib/subspace-fleet/agents/orion/.local/share/mise", orion)
        self.assertFalse(any(part.startswith("TMPDIR=") for part in orion))
        self.assertEqual(calls[2].kwargs["cwd"], homes["orion"])
        # The weekly update keeps each tool within its configured versions.
        self.assertEqual(calls[3].args[0], ["/opt/subspace-fleet/bin/mise", "upgrade", "--no-prune", "node", "pnpm"])
        self.assertEqual(calls[5].args[0][-4:], ["/opt/subspace-fleet/bin/mise", "upgrade", "--no-prune", "rust"])


class EngineTests(unittest.TestCase):
    """The engine and a Claude Agent SDK officer's gateway start with the officer's tool environment from their
    settings, and make its private TMPDIR; the attached TUI and a configuration check leave it alone."""

    def settings(self, directory, environment):
        path = Path(directory) / "engine-orion.json"
        path.write_text(json.dumps({"environment": environment, "port": 4097, "directory": directory,
                                    "slug": "example-fleet", "config": "/dev/null", "entrypoint": "main.ts"}))
        return path

    def test_the_engine_and_gateway_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            tmp = Path(directory) / "ss-orion"
            environment = toolchains.officer_environment(toolchains.toolchain_config({}), "ss-orion",
                                                         Path(directory), [Path(directory) / "workspace"])
            environment["TMPDIR"] = str(tmp)
            path = self.settings(directory, {**environment, "OPENCODE_PASSWORD": "x"})
            with patch.object(runtime.os, "execve") as execve, patch.object(runtime.os, "execvpe") as execvpe, \
                    patch.object(runtime.os, "umask"), patch.object(runtime.os, "chdir"):
                runtime.engine(path)
                env = execve.call_args.args[2]
                self.assertEqual(env["PATH"], environment["PATH"])
                self.assertEqual(env["MISE_DATA_DIR"], f"{directory}/.local/share/mise")
                self.assertEqual(tmp.stat().st_mode & 0o777, 0o700)
                tmp.rmdir()
                runtime.engine(path, attach=True)
                self.assertNotIn("TMPDIR", execvpe.call_args.args[2])
                self.assertFalse(tmp.exists())
                runtime.gateway(path, ["config", "check"])
                self.assertNotIn("TMPDIR", execve.call_args.args[2])
                self.assertFalse(tmp.exists())
                runtime.gateway(path, ["start"])
                self.assertEqual(execve.call_args.args[2]["TMPDIR"], str(tmp))
                self.assertTrue(tmp.is_dir())
            # Settings rendered by an earlier installer still start with the shared store as before.
            old = self.settings(directory, {"OPENCODE_PASSWORD": "x"})
            with patch.object(runtime.os, "execve") as execve, patch.object(runtime.os, "umask"), \
                    patch.object(runtime.os, "chdir"):
                runtime.engine(old)
            self.assertEqual(execve.call_args.args[2]["MISE_DATA_DIR"], "/opt/subspace-fleet/toolchains")


class PlanTests(unittest.TestCase):
    def test_the_dry_run_says_what_installs_where(self):
        with tempfile.TemporaryDirectory() as directory:
            shared, home = Path(directory) / "shared", Path(directory) / "orion"
            (shared / "installs/node/24").mkdir(parents=True)
            (home / ".local/share/mise/installs/rust/1.99.0").mkdir(parents=True)
            tools = toolchains.toolchain_config({"pnpm": "9.12.3", "rust": "1.99.0", "dotnet": "8.0.404"})
            previous = (toolchains.toolchain_config({"go": "1.26"}), ["zsh", "nano"])
            with patch.object(toolchains, "SHARED", shared):
                lines = toolchains.plan_lines(tools, ["zsh", "clang", "libmissing-dev"], {"ss-orion": home}, previous,
                                              {"zsh": "installed", "clang": "available", "libmissing-dev": "missing"})
        text = "\n".join(lines)
        self.assertIn(f"shared, installed by root into {shared}: node 24 [installed]; pnpm 9.12.3 [not installed]; "
                      "dotnet 8.0.404 [not installed].", text)
        self.assertIn("per officer, installed as each officer's user into ~/.local/share/mise (ss-orion); crew use "
                      "their captain's: rust 1.99.0 [installed].", text)
        self.assertIn("system packages, installed by apt as root: zsh [installed]; clang [available]; "
                      "libmissing-dev [missing].", text)
        self.assertIn("The install stops: no apt source on this VM offers libmissing-dev.", text)
        self.assertIn("No longer configured, and left installed: go 1.26.", text)
        self.assertIn("System packages no longer listed, and left installed: nano.", text)
        self.assertIn(f"DOTNET_ROOT {shared}/dotnet-root.", text)


if __name__ == "__main__":
    unittest.main()
