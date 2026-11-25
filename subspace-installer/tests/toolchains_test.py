"""Offline checks for the fleet's toolchains: the toolchains and systemPackages settings, the officers' tool
environment, the dry run's plan, detection from small fixture repositories and checking a repository's tools.

Run with Python 3.11 or later (detection reads TOML): python3 -B subspace-installer/tests/toolchains_test.py
"""

import argparse
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


# Five invented repositories of the kinds the first real fleet had (fleet-install-lessons.md, section 2). Names and
# versions are made up.
REPOSITORIES = {
    # A .NET service with a web front end.
    "harbour-api": {
        ".tool-versions": "nodejs 22.11.0\n",
        "web/mise.toml": '[tools]\nnode = "22.11.0"\n',
        "global.json": '{\n  // The SDK the service builds with.\n  "sdk": {\n    "version": "8.0.404",\n'
                       '    "rollForward": "disable",\n  },\n}\n',
        "web/package.json": json.dumps({"name": "harbour-web", "packageManager": "pnpm@9.12.3+sha512.0123abcd",
                                        "engines": {"node": ">=20"}}, indent=2) + "\n",
        ".github/workflows/ci.yml": """name: CI
on: [push]
jobs:
  test:
    runs-on: ubuntu-24.04
    services:
      postgres:
        image: postgres:16
    steps:
      - uses: actions/checkout@v4
      - name: Shell for the scripts
        run: sudo apt-get update && sudo apt-get install -y --no-install-recommends zsh
      - uses: actions/setup-dotnet@v4
        with:
          global-json-file: global.json
""",
    },
    # A Rust desktop app with a TypeScript API.
    "comet-desktop": {
        "app/mise.toml": '[tools]\nrust = { version = "1.99.0", components = "rustfmt,clippy" }\n'
                         '"cargo:cargo-nextest" = "0.9.100"\n'
                         '"github:example/release-tool" = { version = "2.4.0", postinstall = "echo installed" }\n'
                         '"conda:openssl" = "3.4.0"\n',
        "api/mise.toml": '[tools]\nrust = { version = "1.99.0", components = ["clippy", "rustfmt"] }\n',
        "api/package.json": json.dumps({"name": "comet-api", "packageManager": "bun@1.3.2"}) + "\n",
        "docs/INSTALL.md": "# Building\n\nInstall the build tools first:\n\n```sh\n"
                           "sudo apt-get install -y gcc g++ clang pkg-config \\\n    libgtk-3-dev libssl-dev\n```\n\n"
                           "Then run `cargo build` from the app directory.\n",
        ".github/workflows/build.yml": """name: Build
on: [push]
jobs:
  linux:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: |
          sudo apt-get update
          sudo apt-get install -y \\
            libwebkit2gtk-4.1-dev \\
            libssl-dev pkg-config
      - name: Build with the vendor SDK
        run: ./scripts/build.sh
        env:
          VENDOR_LICENSE: ${{ secrets.VENDOR_SDK_LICENSE }}
  macos:
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v4
""",
    },
    # A TypeScript gateway.
    "relay-gateway": {
        "mise.toml": '[tools]\nbun = "1.3.2"\nprek = "0.2.10"\ngitleaks = "8.28.0"\nshellcheck = "0.11.0"\n'
                     '"github:example/relay-cli" = "0.7.1"\n\n[env]\nRELAY_MODE = "test"\n',
    },
    # A .NET service whose SDK may roll forward to a later feature band.
    "ledger-service": {
        "global.json": '{"sdk": {"version": "8.0.100", "rollForward": "latestFeature"}}\n',
        "client/package.json": json.dumps({"name": "ledger-client", "engines": {"node": ">=20.11 <23"}}) + "\n",
        "docker-compose.yml": "services:\n  db:\n    image: postgres:16\n  app:\n    build: .\n",
    },
    # A TypeScript monorepo with infrastructure. Its workflow holds the trap: each step's with: belongs to that
    # step's uses:, wherever the two keys sit, and mise-action's version is mise's own.
    "orbit-platform": {
        "package.json": json.dumps({"name": "orbit", "private": True, "packageManager": "pnpm@9.15.0"}) + "\n",
        "infra/.terraform-version": "1.9.8\n",
        ".github/workflows/ci.yml": """name: CI
on:
  push:
    branches: [main]
jobs:
  test:
    runs-on: ubuntu-24.04
    strategy:
      matrix:
        node: [20, 22]
    steps:
      - uses: actions/checkout@v4
      - uses: jdx/mise-action@v2
        with:
          version: 2026.1.0
      - name: Set up Deno
        with:
          deno-version: v2.x
        uses: denoland/setup-deno@v2
      - uses: pnpm/action-setup@v4
        with:
          version: 9.15.0
      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node }}
          cache: pnpm
      - uses: actions/setup-node@v4  # the release job's Node
        with:
          node-version: 22.x
      - uses: hashicorp/setup-terraform@v3
        with:
          terraform_version: 1.9.8
      - run: pnpm install --frozen-lockfile && pnpm test
""",
    },
}


def write_repositories(root, repositories=REPOSITORIES):
    paths = {}
    for name, files in repositories.items():
        for rel, text in files.items():
            path = Path(root) / name / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        paths[name] = Path(root) / name
    return paths


def items_of(proposal, name, kind="tool"):
    return {item["version"]: item for item in proposal["items"] if item["kind"] == kind and item["name"] == name}


def files_of(item):
    return {(source["repository"], source["file"]) for source in item["sources"]}


def reasons(proposal, repository):
    return [entry["reason"] for entry in proposal["unknown"] if entry["repository"] == repository]


class DetectionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.paths = write_repositories(self.directory.name)

    def detect(self, *names):
        return toolchains.detect({name: self.paths[name] for name in names})

    def test_a_dotnet_service_with_a_web_front_end(self):
        proposal = self.detect("harbour-api")
        node = items_of(proposal, "node")
        # .tool-versions and the nested mise.toml give one exact Node; engines >=20 is met by it, not proposed apart.
        self.assertEqual(list(node), ["22.11.0"])
        self.assertEqual(node["22.11.0"]["confidence"], "high")
        self.assertEqual(files_of(node["22.11.0"]), {("harbour-api", ".tool-versions"), ("harbour-api", "web/mise.toml"),
                                                     ("harbour-api", "web/package.json")})
        dotnet = items_of(proposal, "dotnet")
        self.assertEqual(list(dotnet), ["8.0.404"])
        self.assertIn("exactly this SDK", dotnet["8.0.404"]["notes"][0])
        self.assertEqual(dotnet["8.0.404"]["sources"][0]["line"], 4)
        self.assertEqual(list(items_of(proposal, "pnpm")), ["9.12.3"])
        zsh = items_of(proposal, "zsh", "package")[None]
        self.assertEqual((zsh["confidence"], zsh["sources"][0]["file"], zsh["sources"][0]["line"]),
                         ("medium", ".github/workflows/ci.yml", 12))
        # Nothing else is proposed: setup-dotnet's version comes from global.json, which is read directly.
        self.assertEqual({item["name"] for item in proposal["items"]}, {"node", "dotnet", "pnpm", "zsh"})
        self.assertEqual(reasons(proposal, "harbour-api"),
                         ["job test's tests need a database (postgres), which no toolchain provides"])

    def test_a_rust_desktop_app_with_a_typescript_api(self):
        proposal = self.detect("comet-desktop")
        rust = items_of(proposal, "rust")
        # Both projects' mise.toml name the same Rust and components, written two ways.
        self.assertEqual(list(rust), ["1.99.0"])
        self.assertEqual(rust["1.99.0"]["options"], {"components": "clippy,rustfmt"})
        self.assertEqual(files_of(rust["1.99.0"]), {("comet-desktop", "app/mise.toml"), ("comet-desktop", "api/mise.toml")})
        self.assertEqual(list(items_of(proposal, "cargo:cargo-nextest")), ["0.9.100"])
        self.assertEqual(list(items_of(proposal, "conda:openssl")), ["3.4.0"])
        release = items_of(proposal, "github:example/release-tool")["2.4.0"]
        # An option that would run a command as root is never carried over, and says so.
        self.assertEqual(release["options"], {})
        self.assertTrue(any("postinstall is not carried over: root installs the shared tools" in reason
                            for reason in reasons(proposal, "comet-desktop")))
        self.assertEqual(list(items_of(proposal, "bun")), ["1.3.2"])
        packages = {item["name"]: item for item in proposal["items"] if item["kind"] == "package"}
        self.assertEqual(set(packages), {"gcc", "g++", "clang", "pkg-config", "libgtk-3-dev", "libssl-dev",
                                         "libwebkit2gtk-4.1-dev"})
        # The workflow's packages are medium; those only the installation doc names are low.
        self.assertEqual({name: item["confidence"] for name, item in packages.items()}, {
            "gcc": "low", "g++": "low", "clang": "low", "libgtk-3-dev": "low", "pkg-config": "medium",
            "libssl-dev": "medium", "libwebkit2gtk-4.1-dev": "medium"})
        self.assertEqual(files_of(packages["libssl-dev"]), {("comet-desktop", "docs/INSTALL.md"),
                                                            ("comet-desktop", ".github/workflows/build.yml")})
        # Prose is not a command: "cargo build" in a sentence proposes nothing.
        self.assertNotIn("cargo", {item["name"] for item in proposal["items"]})
        unknown = reasons(proposal, "comet-desktop")
        self.assertTrue(any("licensed build" in reason for reason in unknown))
        self.assertTrue(any("macos-15" in reason for reason in unknown))
        # Rust and cargo: tools go per user; the rest are shared.
        toolchains.annotate(proposal, {}, [], [], states={})
        self.assertEqual(rust["1.99.0"]["where"], "user")
        self.assertEqual(items_of(proposal, "cargo:cargo-nextest")["0.9.100"]["where"], "user")
        self.assertEqual(items_of(proposal, "bun")["1.3.2"]["where"], "shared")

    def test_a_typescript_gateway(self):
        proposal = self.detect("relay-gateway")
        self.assertEqual({(item["name"], item["version"]) for item in proposal["items"]}, {
            ("bun", "1.3.2"), ("prek", "0.2.10"), ("gitleaks", "8.28.0"), ("shellcheck", "0.11.0"),
            ("github:example/relay-cli", "0.7.1")})
        self.assertEqual(items_of(proposal, "gitleaks")["8.28.0"]["sources"][0]["line"], 4)
        self.assertTrue(all(item["confidence"] == "high" for item in proposal["items"]))

    def test_a_dotnet_band_honours_roll_forward(self):
        alone = self.detect("ledger-service")
        dotnet = items_of(alone, "dotnet")
        # latestFeature: any 8.0 SDK from 8.0.100 on, which mise's 8.0 resolves to.
        self.assertEqual(list(dotnet), ["8.0"])
        self.assertIn("8.0.100 or a later 8.0 feature band", dotnet["8.0"]["notes"][0])
        # engines >=20.11 <23: the newest even major in range.
        self.assertEqual(list(items_of(alone, "node")), ["22"])
        self.assertEqual(items_of(alone, "node")["22"]["confidence"], "medium")
        self.assertEqual(reasons(alone, "ledger-service"),
                         ["Docker Compose runs a database (db): tests that need it need Docker, not a toolchain"])
        # Beside a service that needs exactly 8.0.404, that SDK meets both: one item with both sources.
        both = self.detect("harbour-api", "ledger-service")
        dotnet = items_of(both, "dotnet")
        self.assertEqual(list(dotnet), ["8.0.404"])
        self.assertEqual({source["repository"] for source in dotnet["8.0.404"]["sources"]},
                         {"harbour-api", "ledger-service"})
        # And 22.11.0 meets the client's Node range too.
        self.assertEqual(list(items_of(both, "node")), ["22.11.0"])

    def test_a_monorepo_with_infrastructure_and_the_ci_version_trap(self):
        proposal = self.detect("orbit-platform")
        versions = {(item["name"], item["version"]) for item in proposal["items"]}
        self.assertEqual(versions, {("pnpm", "9.15.0"), ("deno", "2"), ("node", "22"), ("terraform", "1.9.8")})
        # mise-action's version is mise's, and belongs to no tool.
        self.assertFalse(any("2026.1.0" in str(item["version"]) for item in proposal["items"]))
        # Deno's with: comes before its uses: in the same step, and still belongs to it.
        deno = items_of(proposal, "deno")["2"]
        self.assertEqual(deno["sources"][0]["line"], 18)
        self.assertEqual(deno["sources"][0]["detail"], "denoland/setup-deno@v2 with v2.x")
        # pnpm's version comes from its own step, and agrees with packageManager.
        pnpm = items_of(proposal, "pnpm")["9.15.0"]
        self.assertEqual(pnpm["confidence"], "high")
        self.assertIn((".github/workflows/ci.yml", 22), {(source["file"], source["line"]) for source in pnpm["sources"]})
        terraform = items_of(proposal, "terraform")["1.9.8"]
        self.assertEqual(files_of(terraform), {("orbit-platform", "infra/.terraform-version"),
                                               ("orbit-platform", ".github/workflows/ci.yml")})
        # The matrix-driven Node is reported, with the matrix's values, not guessed.
        matrix = [reason for reason in reasons(proposal, "orbit-platform") if "matrix" in reason]
        self.assertEqual(len(matrix), 1)
        self.assertIn("[20, 22]", matrix[0])

    def test_a_naive_reading_of_the_trap_is_what_detection_avoids(self):
        # The same steps, read as the workflow's lines in order: a version: line after jdx/mise-action and before
        # denoland/setup-deno would be taken for Deno's. Detection reads each step's own mapping instead.
        text = REPOSITORIES["orbit-platform"][".github/workflows/ci.yml"]
        steps = toolchains.parse_yaml(text)["jobs"]["test"]["steps"]
        self.assertEqual([step.get("uses") for step in steps][:3],
                         ["actions/checkout@v4", "jdx/mise-action@v2", "denoland/setup-deno@v2"])
        self.assertEqual(steps[1]["with"], {"version": "2026.1.0"})
        self.assertEqual(steps[2]["with"], {"deno-version": "v2.x"})
        self.assertEqual(steps[7]["run"], "pnpm install --frozen-lockfile && pnpm test")

    def test_detection_never_follows_a_symbolic_link_or_reads_a_huge_file(self):
        outside = Path(self.directory.name) / "outside"
        outside.write_text("99.0.0\n")
        repo = Path(self.directory.name) / "linked"
        repo.mkdir()
        (repo / ".node-version").symlink_to(outside)
        (repo / "nested").symlink_to(self.paths["relay-gateway"], target_is_directory=True)
        (repo / "go.mod").write_text("module example.com/linked\n\ngo 1.24.2\n")
        (repo / "pyproject.toml").write_text("x" * (toolchains.MAX_FILE + 1))
        proposal = toolchains.detect({"linked": repo, "missing": Path(self.directory.name) / "missing"})
        self.assertEqual({(item["name"], item["version"]) for item in proposal["items"]}, {("go", "1.24.2")})
        self.assertTrue(any(entry["file"] == ".node-version" and "symbolic link" in entry["reason"]
                            for entry in proposal["unknown"]))
        self.assertTrue(any(entry["file"] == "pyproject.toml" for entry in proposal["unknown"]))
        self.assertEqual(reasons(proposal, "missing")[0].split(" at ")[0], "the repository is not cloned")

    def test_version_files_go_mod_and_pyproject(self):
        repo = write_repositories(self.directory.name, {"misc": {
            ".nvmrc": "v20.18.1\n", ".python-version": "3.12.7\n", ".ruby-version": "ruby-3.3.6\n",
            "tools/go.mod": "module example.com/misc\n\ngo 1.23\n\ntoolchain go1.23.4\n",
            "service/pyproject.toml": '[project]\nname = "misc"\nrequires-python = ">=3.11,<3.14"\n',
            "rust-toolchain": "1.98.0\n",
        }})["misc"]
        proposal = toolchains.detect({"misc": repo})
        found = {(item["name"], item["version"], item["confidence"]) for item in proposal["items"]}
        self.assertEqual(found, {("node", "20.18.1", "high"), ("python", "3.12.7", "high"), ("ruby", "3.3.6", "high"),
                                 ("go", "1.23.4", "medium"), ("rust", "1.98.0", "high")})
        # requires-python >=3.11 is met by the exact 3.12.7 and folds into it.
        self.assertIn(("misc", "service/pyproject.toml"), files_of(items_of(proposal, "python")["3.12.7"]))

    def test_python_minimum_met_by_the_system_python_is_not_proposed(self):
        repo = write_repositories(self.directory.name, {"py": {
            "pyproject.toml": '[project]\nname = "py"\nrequires-python = ">=3.11"\n'}})["py"]
        proposal = toolchains.annotate(toolchains.detect({"py": repo}), {}, [], [], system_python="3.12.3", states={})
        item = items_of(proposal, "python")["3.11"]
        self.assertEqual(item["configured"], "met by the system's Python 3.12.3")
        self.assertIn("Nothing new to confirm.", toolchains.proposal_lines(proposal))

    def test_ranges(self):
        cases = {">=20": (True, True, True), "^20.11.0": (True, False, False), ">=20.11 <23": (True, True, False),
                 "20.x || 22.x": (True, True, False), "~22.3": (False, True, False), "18 - 22": (True, True, False),
                 "<=22": (True, True, False), ">22": (False, False, True)}
        for expression, expected in cases.items():
            intervals = toolchains.semver_alternatives(expression)
            self.assertEqual(tuple(toolchains.overlaps(intervals, major) for major in ("20", "22", "24")), expected,
                             expression)
        dotnet = {"minimum": [8, 0, 404], "policy": "latestPatch"}
        for version, expected in {"8.0.404": True, "8.0.410": True, "8.0.500": False, "8.0.400": False, "8.0": False,
                                  "9.0.100": False}.items():
            self.assertEqual(toolchains.dotnet_accepts({"kind": "dotnet", **dotnet}, version), expected, version)
        feature = {"kind": "dotnet", "minimum": [8, 0, 100], "policy": "latestFeature"}
        self.assertTrue(toolchains.dotnet_accepts(feature, "8.0"))
        self.assertTrue(toolchains.dotnet_accepts(feature, "8.0.300"))
        self.assertFalse(toolchains.dotnet_accepts(feature, "9.0"))
        self.assertTrue(toolchains.dotnet_accepts({"kind": "dotnet", "minimum": [8, 0, 100], "policy": "latestMajor"},
                                                  "9.0.100"))
        self.assertFalse(toolchains.dotnet_accepts({"kind": "dotnet", "minimum": [8, 0, 404], "policy": "disable"},
                                                   "8.0.405"))

    def test_apt_commands(self):
        self.assertEqual(toolchains.apt_packages(
            "sudo DEBIAN_FRONTEND=noninteractive apt-get -o Dpkg::Options::=--force-confold install -y clang=1:18 "
            "libssl-dev:arm64 && make"), (["clang", "libssl-dev"], []))
        self.assertEqual(toolchains.apt_packages("apt-get install -y zsh ${{ matrix.extra }} git"), (["zsh"], ["${{"]))
        self.assertEqual(toolchains.apt_packages("apt-get update; apt-get remove nano"), ([], []))


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
        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git",
                                   "check": "make check"}]
        self.assertEqual(runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"},
                                          check_files=False)["repositoryChecks"], {"project": "make check"})
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


class VerificationTests(unittest.TestCase):
    def test_a_repositorys_basic_check(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = write_repositories(directory)
            found = toolchains.detect({"harbour-api": paths["harbour-api"]})
        tools = toolchains.toolchain_config({"dotnet": "8.0.404", "node": "22.11.0"})
        commands, missing = toolchains.verification_commands(found, tools, "make check")
        # Each configured tool the repository names, run from its worktree, then its own check.
        self.assertEqual(commands, [["dotnet", "--version"], ["node", "--version"], ["/bin/bash", "-c", "make check"]])
        self.assertEqual(missing, ["pnpm@9.12.3"])

    def test_it_runs_as_the_captain_and_reports_without_failing(self):
        def run(command, **kwargs):
            results.append((command, kwargs))
            if command[-3:-1] == ["mktemp", "-d"]:
                return subprocess_result(0, "/tmp/ss-orion-verify.abc123\n")
            if command[-1] == "check":
                return subprocess_result(2, "", "first\nsecond\nthird\nfourth\n")
            return subprocess_result(0, "v22.11.0\n")

        results = []
        environment = {"PATH": "/p", "TMPDIR": "/tmp/ss-orion"}
        home = Path("/var/lib/subspace-fleet/agents/orion")
        with patch.object(toolchains.subprocess, "run", side_effect=run):
            passed, report = toolchains.verify_repository("ss-orion", home, "/w", environment,
                                                          [["node", "--version"], ["make", "check"]], 60)
        self.assertFalse(passed)
        self.assertEqual(report, "make check failed (2): second / third / fourth")
        node = results[1][0]
        self.assertEqual(node[:6], ["runuser", "-u", "ss-orion", "--", "/usr/bin/env", "-i"])
        self.assertIn("TMPDIR=/tmp/ss-orion-verify.abc123", node)
        self.assertEqual(results[1][1]["cwd"], "/w")
        # The private temporary directory is removed afterwards.
        self.assertEqual(results[-1][0][-4:], ["rm", "-rf", "--", "/tmp/ss-orion-verify.abc123"])


def subprocess_result(code, stdout, stderr=""):
    return argparse.Namespace(returncode=code, stdout=stdout, stderr=stderr)


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

    def test_the_proposal_for_a_person_to_read(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = write_repositories(directory)
            proposal = toolchains.detect({name: paths[name] for name in ("harbour-api", "comet-desktop")})
            toolchains.annotate(proposal, toolchains.toolchain_config({}), ["zsh"], [Path(directory) / "home"],
                                states={name: "available" for name in ("gcc", "g++", "clang", "pkg-config",
                                                                        "libgtk-3-dev", "libssl-dev",
                                                                        "libwebkit2gtk-4.1-dev", "zsh")})
        lines = toolchains.proposal_lines(proposal)
        self.assertEqual(lines[0], "Toolchain proposal from harbour-api, comet-desktop. Nothing in it is installed "
                                   "until you confirm it.")
        text = "\n".join(lines)
        self.assertIn("  [high] dotnet@8.0.404: shared (/opt/subspace-fleet/toolchains), not installed; global.json "
                      "accepts exactly this SDK (rollForward disable)\n      from harbour-api: global.json:4", text)
        self.assertIn("  [high] rust@1.99.0 (components=clippy,rustfmt): per user (~/.local/share/mise), not installed",
                      text)
        self.assertIn("Already configured: zsh.", text)
        self.assertIn("Could not determine (check these yourself):", text)
        command = lines[-2]
        self.assertTrue(command.startswith("To confirm, edit and run: sudo subspace-setup --toolchains "), command)
        self.assertIn("rust@1.99.0", command)
        # Documentation-only packages are proposed, but left out of the suggested command.
        self.assertIn("--system-packages libssl-dev,libwebkit2gtk-4.1-dev,pkg-config", command)
        self.assertNotIn("clang", command)


if __name__ == "__main__":
    unittest.main()
