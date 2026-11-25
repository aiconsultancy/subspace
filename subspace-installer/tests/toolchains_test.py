"""Offline checks for the fleet's toolchains: the toolchains and systemPackages settings, the officers' tool
environment, the dry run's plan, detection from small fixture repositories, and subspace-setup's recorded
toolchains across runs.

Run with Python 3.11 or later (detection reads TOML): python3 -B subspace-installer/tests/toolchains_test.py
"""

import argparse
import contextlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "vm"))
import fleet_setup  # noqa: E402
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
        self.assertIn("(the matrix lists 20, 22)", matrix[0])

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

    def test_common_workflow_syntax_is_read_and_the_rest_is_reported(self):
        workflow = """jobs:
  test:
    runs-on: ubuntu-24.04
    steps:
      - 'uses': pnpm/action-setup@v4
        with: {version: '9.12.3', run_install: false}
      - "uses": "actions/setup-node@v4"
        with:
          node-version: 22.11.0
      - uses: oven-sh/setup-bun@v2
        with: {
          bun-version: "1.3.2" }
      - run: >
          sudo apt-get install -y
          libssl-dev
          pkg-config
      - uses: hashicorp/setup-terraform@v3
        with:
          terraform_version: 1.9.8
"""
        repo = write_repositories(self.directory.name, {"flow": {".github/workflows/ci.yml": workflow}})["flow"]
        proposal = toolchains.detect({"flow": repo})
        self.assertEqual({(item["name"], item["version"]) for item in proposal["items"]}, {
            ("pnpm", "9.12.3"), ("node", "22.11.0"), ("bun", "1.3.2"), ("libssl-dev", None), ("pkg-config", None),
            ("terraform", "1.9.8")})
        self.assertEqual(items_of(proposal, "pnpm")["9.12.3"]["sources"][0]["line"], 6)
        self.assertEqual(reasons(proposal, "flow"), [])
        # What the reader does not handle is reported against the workflow, never dropped quietly.
        unsupported = """jobs:
  test:
    runs-on: ubuntu-24.04
    steps:
      - &setup
        uses: actions/setup-node@v4
        with:
          node-version: 22.11.0
      - *setup
      - uses: pnpm/action-setup@v4
        with: {version: [unclosed}
"""
        repo = write_repositories(self.directory.name, {"anchors": {".github/workflows/ci.yml": unsupported}})["anchors"]
        proposal = toolchains.detect({"anchors": repo})
        problem = [reason for reason in reasons(proposal, "anchors") if "detection does not read" in reason]
        self.assertEqual(len(problem), 1, reasons(proposal, "anchors"))
        self.assertIn("an anchor at line 5 and", problem[0])
        self.assertIn("may be missing from the proposal", problem[0])

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

    def test_unusual_but_parseable_files_never_stop_detection(self):
        repo = write_repositories(self.directory.name, {"odd": {
            "pyproject.toml": 'tool = "a string"\n[project]\nrequires-python = ">=3.11"\n',
            "global.json": '{"sdk": {"version": "8.0.404", "rollForward": ["latestPatch"]}}\n',
            "rust-toolchain.toml": '[toolchain]\nchannel = "1.99.0"\ncomponents = ["rustfmt", 1]\n',
            ".nvmrc": "22.11.0\n"}})["odd"]
        proposal = toolchains.detect({"odd": repo})
        unreadable = {entry["file"] for entry in proposal["unknown"] if "could not read this file" in entry["reason"]}
        self.assertEqual(unreadable, {"pyproject.toml", "global.json"})
        # The rest is read: Rust without the component it cannot carry over, and Node.
        self.assertEqual(items_of(proposal, "rust")["1.99.0"]["options"], {})
        self.assertTrue(any("components is not carried over" in reason for reason in reasons(proposal, "odd")))
        self.assertEqual(list(items_of(proposal, "node")), ["22.11.0"])
        # Any scanner that fails is contained the same way, and the next file is still read.
        with patch.object(toolchains, "scan_version_file", side_effect=RecursionError):
            proposal = toolchains.detect({"odd": repo})
        self.assertIn((".nvmrc", "detection could not read this file (RecursionError); check what it asks for yourself"),
                      [(entry["file"], entry["reason"]) for entry in proposal["unknown"]])
        self.assertIn("rust", {item["name"] for item in proposal["items"]})
        # Even a failure outside one file's scanner only reports the repository.
        with patch.object(toolchains, "repository_files", side_effect=ValueError("odd")):
            proposal = toolchains.detect({"odd": repo})
        self.assertEqual(proposal["items"], [])
        self.assertIn("detection stopped reading the repository (ValueError)", reasons(proposal, "odd")[0])

    def test_no_symbolic_link_is_followed_even_when_a_directory_is_swapped_for_one(self):
        # The mirror service refreshes checkouts while detection reads them: a directory may become a symbolic link
        # between listing and reading. Detection opens everything relative to directories it already holds open.
        root = Path(self.directory.name)
        outside = root / "outside"
        (outside / "b").mkdir(parents=True)
        (outside / ".nvmrc").write_text("99.0.0\n")
        (outside / "b" / ".nvmrc").write_text("99.0.0\n")
        repo = root / "swapped"
        for path in ("a/b", "c", "d/e"):
            (repo / path).mkdir(parents=True)
        (repo / "a" / "b" / ".nvmrc").write_text("22.11.0\n")
        (repo / "c" / ".nvmrc").write_text("20.18.1\n")
        (repo / "d" / "e" / ".nvmrc").write_text("24.11.0\n")
        swapped = []

        def wanted(rel):
            if rel == "a/b/.nvmrc":
                # A parent of the file being read, and a directory not entered yet, become links outside.
                for name in ("a", "c"):
                    (repo / name).rename(root / f"old-{name}")
                    (repo / name).symlink_to(outside, target_is_directory=True)
                    swapped.append(name)
            return rel.endswith(".nvmrc")
        read = dict(toolchains.repository_files(repo, wanted))
        self.assertEqual(swapped, ["a", "c"])
        # The file in the swapped parent is the original one; the swapped directory not yet entered is skipped.
        self.assertEqual(read, {"a/b/.nvmrc": "22.11.0\n", "d/e/.nvmrc": "24.11.0\n"})
        # A directory or file that is a symbolic link to begin with is never followed either.
        proposal = toolchains.detect({"swapped": repo})
        self.assertNotIn("99.0.0", {item["version"] for item in proposal["items"]})

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
            "libssl-dev:arm64 g++ && make"), (["clang", "libssl-dev", "g++"], []))
        self.assertEqual(toolchains.apt_packages("apt-get install -y zsh ${{ matrix.extra }} git"),
                         (["zsh"], [("${{", "a variable or expression")]))
        self.assertEqual(toolchains.apt_packages("apt-get update; apt-get remove nano"), ([], []))
        # apt-get install removes a package whose name ends in a minus: never proposed, always reported.
        packages, others = toolchains.apt_packages("apt-get install -y zsh nano- foo/bookworm-backports 'lib*'")
        self.assertEqual(packages, ["zsh"])
        self.assertEqual([word for word, _ in others], ["nano-", "foo/bookworm-backports", "lib*"])
        self.assertIn("apt-get removes nano", others[0][1])

    def test_options_that_reach_outside_the_install_directory_are_reported_not_proposed(self):
        repo = write_repositories(self.directory.name, {"paths": {"mise.toml": (
            '[tools]\n"github:example/tool" = { version = "1.0.0", bin_path = "/usr/local/bin", '
            'rename_exe = "../../../usr/bin/sudo", asset_pattern = "tool-*.tar.gz" }\n')}})["paths"]
        proposal = toolchains.detect({"paths": repo})
        self.assertEqual(items_of(proposal, "github:example/tool")["1.0.0"]["options"],
                         {"asset_pattern": "tool-*.tar.gz"})
        unknown = reasons(proposal, "paths")
        self.assertTrue(any("bin_path is not carried over" in reason and "relative" in reason for reason in unknown))
        self.assertTrue(any("rename_exe is not carried over" in reason and "file name" in reason for reason in unknown))

    def test_a_removal_in_a_workflow_is_reported_not_proposed(self):
        repo = write_repositories(self.directory.name, {"removes": {".github/workflows/ci.yml": (
            "jobs:\n  test:\n    runs-on: ubuntu-24.04\n    steps:\n"
            "      - run: sudo apt-get install -y zsh vim-tiny- nano-\n")}})["removes"]
        proposal = toolchains.detect({"removes": repo})
        self.assertEqual([item["name"] for item in proposal["items"]], ["zsh"])
        self.assertEqual(len([reason for reason in reasons(proposal, "removes") if "apt-get removes" in reason]), 2)


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
            "node": "shared", "dotnet": "user", "pnpm": "shared", "cargo:cargo-nextest": "user",
            "github:example/release-tool": "shared", "conda:openssl": "user", "npm:@example/cli": "user",
            "rust": "user", "terraform": "user"})
        self.assertEqual(toolchains.requests(tools, "shared"), [
            "node@22.11.0", "node@24", "pnpm@9.12.3", "github:example/release-tool@2.4.0"])
        self.assertEqual(toolchains.requests(tools, "user"), ["dotnet@8.0.404", "rust@1.99.0", "terraform@1.9.8",
                                                              "cargo:cargo-nextest@0.9.100", "conda:openssl@3.4.0",
                                                              "npm:@example/cli@1.2.0"])
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

    def test_tools_that_run_code_while_installing_are_installed_per_user_unless_marked_shared(self):
        tools = toolchains.toolchain_config({
            "npm:@example/cli": "1.2.0", "gem:example": "2.0.0", "pipx:example-tool": "3.1.0",
            "asdf:example/asdf-tool": "1.0.0", "vfox:example/vfox-tool": "1.0.0", "conda:openssl": "3.4.0",
            "cargo:cargo-nextest": "0.9.100", "aqua:example/tool": "1.0.0", "go:example.com/tool": "1.0.0"})
        self.assertEqual(sorted(spec for spec, entry in tools.items() if entry["install"] == "user"), [
            "asdf:example/asdf-tool", "cargo:cargo-nextest", "conda:openssl", "gem:example", "npm:@example/cli",
            "pipx:example-tool", "vfox:example/vfox-tool"])
        self.assertEqual(toolchains.requests(tools, "shared"), ["node@24", "aqua:example/tool@1.0.0",
                                                                "go:example.com/tool@1.0.0"])
        # Root's mise never uses those backends, so a plain name that mise's registry resolves to one fails as
        # root rather than running its code there.
        self.assertEqual(toolchains.root_environment(tools)["MISE_DISABLE_BACKENDS"],
                         "asdf,cargo,conda,gem,npm,pipx,pypi,vfox")
        # The operator may mark one shared: then root installs it, and that backend only is enabled for root.
        marked = toolchains.toolchain_config({"npm:@example/cli": {"version": "1.2.0", "install": "shared"}})
        self.assertEqual(toolchains.requests(marked, "shared"), ["node@24", "npm:@example/cli@1.2.0"])
        self.assertEqual(toolchains.root_environment(marked)["MISE_DISABLE_BACKENDS"],
                         "asdf,cargo,conda,gem,pipx,pypi,vfox")
        # Detection proposes them per user too: a repository cannot mark anything shared.
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        repo = Path(directory.name)
        (repo / "mise.toml").write_text('[tools]\n"npm:@example/cli" = { version = "1.2.0", install = "shared" }\n')
        proposal = toolchains.annotate(toolchains.detect({"cli": repo}), toolchains.toolchain_config({}), [], [],
                                       states={})
        self.assertEqual(items_of(proposal, "npm:@example/cli")["1.2.0"]["where"], "user")

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
        # Options by what they are for: an executable is a file name, a directory stays inside the install
        # directory, and the rest are plain names or choices; none is a path or an address elsewhere.
        for options in ({"bin_path": "/usr/local/bin"}, {"bin_path": "../../etc"}, {"bin_path": "bin/../../x"},
                        {"bin_path": "./bin"}, {"bin_path": "bin//x"}, {"rename_exe": "../../usr/bin/tool"},
                        {"rename_exe": "/usr/bin/tool"}, {"bin": "bin/tool"}, {"exe": ".."},
                        {"filter_bins": "tool,../x"}, {"channel": "https://example.com/conda"},
                        {"asset_pattern": "../tool.tar.gz"}, {"version_prefix": "{{env.HOME}}"},
                        {"profile": "everything"}, {"locked": "true"}, {"locked": 1}, {"components": "a b"},
                        {"crate": "a/b"}):
            with self.subTest(options=options), self.assertRaises(Invalid):
                toolchains.toolchain_config({"github:example/tool": {"version": "1.0.0", "options": options}})
        allowed = {"bin": "tool", "rename_exe": "tool", "bin_path": "tool-1.0.0/bin", "filter_bins": "tool,toolctl",
                   "asset_pattern": "tool-*-linux-x64.tar.gz", "version_prefix": "release-", "locked": True}
        self.assertEqual(toolchains.toolchain_config({"github:example/tool": {"version": "1.0.0", "options": allowed}})[
            "github:example/tool"]["options"], allowed)
        # Only names apt installs: never a removal (a trailing minus), a version, release, architecture or pattern.
        for packages in (["zsh; reboot"], ["-y"], ["Clang"], ["libssl-dev", "libssl-dev"], "zsh", [7], ["a"],
                         ["nano-"], ["zsh=5.9-6"], ["zsh/noble-updates"], ["zsh:arm64"], ["lib*"], ["zsh?"],
                         ["./local.deb"]):
            with self.subTest(packages=packages), self.assertRaises(Invalid):
                toolchains.package_config(packages)
        self.assertEqual(toolchains.package_config(["zsh", "g++", "libwebkit2gtk-4.1-dev"]),
                         ["zsh", "g++", "libwebkit2gtk-4.1-dev"])
        with self.assertRaises(fleet_setup.SetupError):
            fleet_setup.parse_package_items("zsh,nano-")

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


INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


def shell_functions(*names):
    """The named functions from the fleet installer's install.sh, as text bash can run."""
    lines, text = INSTALLER.read_text().splitlines(), []
    for name in names:
        start = next(index for index, line in enumerate(lines) if line.startswith(f"{name}() {{"))
        end = start if lines[start].rstrip().endswith("}") else lines.index("}", start)
        text += lines[start:end + 1]
    return "\n".join(text) + "\n"


class SystemPackageInstallTests(unittest.TestCase):
    """install.sh installs systemPackages by exact name, never removing anything, with apt run from /."""

    def run_installer_functions(self, call, known=("zsh", "g++", "libnss3")):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stubs, caller = root / "bin", root / "caller"
            stubs.mkdir()
            caller.mkdir()
            log = root / "log"
            # apt-get records where it ran and its arguments; apt-cache knows only these names; nothing is installed.
            (stubs / "apt-get").write_text(f'#!/bin/sh\necho "$PWD $*" >> {log}\n')
            (stubs / "apt-cache").write_text("#!/bin/sh\n[ \"$1\" = pkgnames ] && printf '%s\\n' "
                                             + " ".join(f"'{name}' '{name}-doc'" for name in known) + "\n")
            (stubs / "dpkg-query").write_text("#!/bin/sh\necho not-installed\n")
            for stub in stubs.iterdir():
                stub.chmod(0o755)
            script = shell_functions("die", "apt_get", "package_installed", "ensure_listed_packages") + call
            result = subprocess.run(["bash", "-c", script], cwd=caller, capture_output=True, text=True,
                                    env={"PATH": f"{stubs}:/usr/bin:/bin"})
            return result, (log.read_text().splitlines() if log.exists() else [])

    def test_packages_install_by_exact_name_from_the_root_directory(self):
        result, log = self.run_installer_functions("ensure_listed_packages required zsh g++\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(log, ["/ update -qq", "/ install -y --no-install-recommends --no-remove zsh g++"])

    def test_a_removal_or_an_unknown_name_stops_before_apt_installs(self):
        for package, message in (("nano-", "Invalid system package name nano-"),
                                 ("zsh=5.9", "Invalid system package name zsh=5.9"),
                                 ("zs.", "No apt source on this VM offers the package zs.")):
            with self.subTest(package=package):
                result, log = self.run_installer_functions(f"ensure_listed_packages required zsh '{package}'\n")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertFalse(any(" install " in line for line in log), log)

    def test_optional_packages_leave_out_what_apt_does_not_offer(self):
        result, log = self.run_installer_functions("ensure_listed_packages optional libnss3 libmissing1\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no apt source on this VM offers libmissing1; left out", result.stderr)
        self.assertEqual(log[-1], "/ install -y --no-install-recommends --no-remove libnss3")

    def test_dry_run_package_states_run_from_the_root_directory(self):
        def run(command, **kwargs):
            calls.append((command, kwargs["cwd"]))
            if command[0] == "dpkg-query":
                return subprocess_result(1, "")
            return subprocess_result(0, "zsh\nzsh-common\n")
        calls = []
        with patch.object(toolchains.subprocess, "run", side_effect=run):
            self.assertEqual(toolchains.package_state("zsh"), "available")
            self.assertEqual(toolchains.package_state("zs"), "missing")
        self.assertEqual({cwd for _, cwd in calls}, {"/"})


@unittest.skipUnless(shutil.which("pwsh"), "PowerShell (pwsh) is not installed")
class PowerShellArgumentTests(unittest.TestCase):
    def test_an_unquoted_comma_separated_list_keeps_its_commas(self):
        # PowerShell passes node@24,pnpm@9.12.3 to a script as an array; install.ps1 joins it with commas again,
        # as setup on the VM expects, where [string] would have joined it with spaces.
        script = Path(__file__).resolve().parents[2] / "install.ps1"
        line = next(line for line in script.read_text().splitlines() if line.startswith("$list = @("))
        with tempfile.TemporaryDirectory() as directory:
            probe = Path(directory) / "arguments.ps1"
            probe.write_text(line + "\n$list | ForEach-Object { [string]$_ }\n")
            result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-Command",
                                     f"& '{probe}' --toolchains node@24,pnpm@9.12.3 --captains 'Orion,Vega' "
                                     "--name 'Example Fleet'"], capture_output=True, text=True, check=True)
        self.assertEqual(result.stdout.splitlines(), ["--toolchains", "node@24,pnpm@9.12.3", "--captains",
                                                      "Orion,Vega", "--name", "Example Fleet"])


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
            # .NET is per user by default: an SDK a repository's global.json pins installs into the officer's
            # own root on first use, which a read-only shared root would refuse.
            "DOTNET_ROOT": "/var/lib/subspace-fleet/agents/orion/.local/share/mise/dotnet-root"})
        self.assertEqual(tools["dotnet"]["install"], "user")
        # Without .NET, no DOTNET_ROOT; a .NET the operator marks shared uses the shared store's root.
        self.assertNotIn("DOTNET_ROOT", toolchains.officer_environment(toolchains.toolchain_config({}), "ss-orion",
                                                                       home, []))
        shared = toolchains.toolchain_config({"dotnet": {"version": "8.0.404", "install": "shared"}})
        self.assertEqual(toolchains.officer_environment(shared, "ss-orion", home, [])["DOTNET_ROOT"],
                         "/opt/subspace-fleet/toolchains/dotnet-root")
        # Root's installs use the shared store and the fleet's configuration only, and set DOTNET_ROOT only for a
        # shared .NET, which root installs.
        root = toolchains.root_environment(tools)
        self.assertEqual((root["MISE_DATA_DIR"], root["MISE_GLOBAL_CONFIG_FILE"], root["HOME"]),
                         ("/opt/subspace-fleet/toolchains", "/etc/subspace-fleet/toolchains.toml", "/root"))
        self.assertNotIn("DOTNET_ROOT", root)
        self.assertEqual(toolchains.root_environment(shared)["DOTNET_ROOT"], "/opt/subspace-fleet/toolchains/dotnet-root")

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
        checks, unchecked = toolchains.verification_checks(found, tools, "make check")
        # Each tool the repository names that has a version probe, asked where the file naming it is (pnpm in web/,
        # whose package.json names it), then its own check from the worktree.
        self.assertEqual([(command, directory, item and toolchains.item_label(item)) for command, directory, item in checks], [
            (["dotnet", "--version"], "", "dotnet@8.0.404"), (["node", "--version"], "", "node@22.11.0"),
            (["pnpm", "--version"], "web", "pnpm@9.12.3"), (["/bin/bash", "-c", "make check"], "", None)])
        self.assertEqual(unchecked, [])

    def run_checks(self, outputs, checks):
        def run(command, **kwargs):
            results.append((command, kwargs))
            if command[-3:-1] == ["mktemp", "-d"]:
                return subprocess_result(0, "/tmp/ss-orion-verify.abc123\n")
            return next((outputs[word] for word in command if word in outputs), subprocess_result(0, ""))
        results = []
        environment = {"PATH": "/p", "TMPDIR": "/tmp/ss-orion"}
        home = Path("/var/lib/subspace-fleet/agents/orion")
        with tempfile.TemporaryDirectory() as worktree, patch.object(toolchains.subprocess, "run", side_effect=run):
            (Path(worktree) / "web").mkdir()
            passed, report = toolchains.verify_repository("ss-orion", home, worktree, environment, checks, 60)
            return passed, report.replace(worktree, "<worktree>"), results, worktree

    def test_it_runs_as_the_captain_and_compares_what_each_tool_resolves_to(self):
        node = {"kind": "tool", "name": "node", "version": "22.11.0", "options": {}, "confidence": "high",
                "sources": [{"repository": "web", "file": "web/.nvmrc", "line": 1, "detail": ""}]}
        outputs = {"node": subprocess_result(0, "v22.11.0\n"), "check": subprocess_result(2, "", "a\nb\nc\nd\n")}
        passed, report, results, worktree = self.run_checks(outputs, [
            (["node", "--version"], "web", node), (["make", "check"], "", None)])
        self.assertFalse(passed)
        self.assertEqual(report, "make check failed (2): b / c / d")
        command, options = results[1]
        self.assertEqual(command[:6], ["runuser", "-u", "ss-orion", "--", "/usr/bin/env", "-i"])
        self.assertIn("TMPDIR=/tmp/ss-orion-verify.abc123", command)
        self.assertEqual(options["cwd"], Path(worktree) / "web")
        # The private temporary directory is removed afterwards.
        self.assertEqual(results[-1][0][-4:], ["rm", "-rf", "--", "/tmp/ss-orion-verify.abc123"])
        # A tool that runs but resolves to another version than the repository asks for fails.
        outputs = {"node": subprocess_result(0, "v24.11.1\n")}
        passed, report, _, _ = self.run_checks(outputs, [(["node", "--version"], "web", node)])
        self.assertFalse(passed)
        self.assertEqual(report, "node is 24.11.1 in web, but web: web/.nvmrc:1 asks for node 22.11.0")
        outputs = {"node": subprocess_result(0, "v22.11.0\n")}
        self.assertEqual(self.run_checks(outputs, [(["node", "--version"], "web", node)])[:2],
                         (True, "1 checks passed: node 22.11.0"))

    def test_a_range_the_system_already_meets_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = write_repositories(directory, {"py": {
                "pyproject.toml": '[project]\nname = "py"\nrequires-python = ">=3.11"\n',
                "package.json": json.dumps({"engines": {"node": ">=22.11.0 <22.12.0"}})}})["py"]
            found = toolchains.detect({"py": repo})
        checks, unchecked = toolchains.verification_checks(found, toolchains.toolchain_config({}))
        self.assertEqual(unchecked, [])
        by_name = {item["name"]: (command, directory, item) for command, directory, item in checks}
        self.assertEqual(by_name["python"][0], ["python3", "--version"])
        # Python 3.12 from the system meets >=3.11; Node 22.20 does not meet >=22.11.0 <22.12.0, though 22 runs.
        outputs = {"python3": subprocess_result(0, "Python 3.12.3\n"), "node": subprocess_result(0, "v22.20.0\n")}
        passed, report, _, _ = self.run_checks(outputs, [by_name["python"]])
        self.assertEqual((passed, report), (True, "1 checks passed: python 3.12.3"))
        passed, report, _, _ = self.run_checks(outputs, [by_name["node"]])
        self.assertFalse(passed)
        self.assertIn("node is 22.20.0, but py: package.json:1 asks for engines node >=22.11.0 <22.12.0", report)

    def test_ranges_propose_a_request_that_installs_within_them(self):
        # mise installs the newest version a prefix names, so a range proposes a prefix all of whose versions it
        # allows, or the exact lowest version.
        cases = {">=22.11.0 <22.12.0": "22.11", ">=22.11.0 <22.11.5": "22.11.0", "^20.11.0": "20",
                 ">=20.11 <23": "22", "20.x || 22.x": "22", "~22.3": "22.3", ">22": "24", "*": "lts"}
        for expression, expected in cases.items():
            self.assertEqual(toolchains.range_proposal("node", toolchains.semver_alternatives(expression)), expected,
                             expression)
        for expression, expected in {">=3.11,<3.14": "3.13", "==3.11.4": "3.11.4", ">=3.11": "3.11"}.items():
            self.assertEqual(toolchains.range_proposal("python", toolchains.pep440_alternatives(expression), depth=2),
                             expected, expression)
        # A configured prefix meets a range only if everything it installs lies within it.
        narrow = {"kind": "tool", "name": "node", "version": "22.11", "constraint": {
            "kind": "range", "intervals": [[[22, 11, 0], [22, 12, 0]]]}}
        self.assertFalse(toolchains.accepts(narrow, "22"))
        self.assertTrue(toolchains.accepts(narrow, "22.11.0"))
        self.assertTrue(toolchains.accepts(narrow, "22.11"))


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
        self.assertIn(f"shared, installed by root into {shared}: node 24 [installed]; pnpm 9.12.3 [not installed].",
                      text)
        self.assertIn("per officer, installed as each officer's user into ~/.local/share/mise (ss-orion); crew use "
                      "their captain's: rust 1.99.0 [installed]; dotnet 8.0.404 [not installed].", text)
        self.assertIn("system packages, installed by apt as root: zsh [installed]; clang [available]; "
                      "libmissing-dev [missing].", text)
        self.assertIn("The install stops: no apt source on this VM offers libmissing-dev.", text)
        self.assertIn("No longer configured, and left installed: go 1.26.", text)
        self.assertIn("System packages no longer listed, and left installed: nano.", text)
        self.assertIn("DOTNET_ROOT ~/.local/share/mise/dotnet-root, each officer's own.", text)

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
        self.assertIn("  [high] dotnet@8.0.404: per user (~/.local/share/mise), not installed; global.json "
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


def setup_args(*argv):
    return fleet_setup.parse_args(["--non-interactive", *argv])


class SetupRecordTests(unittest.TestCase):
    """subspace-setup's recorded toolchains: confirmed choices keep their sources across runs, later runs propose
    only what repositories added since need, nothing confirmed is dropped and nothing is added unasked."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.inputs = root / "inputs"
        self.inputs.mkdir()
        self.references = root / "references"
        for name, value in (("INPUTS", self.inputs), ("REFERENCES", self.references), ("AGENT_HOMES", root / "agents")):
            patcher = patch.object(fleet_setup, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        patcher = patch.object(toolchains, "package_state", return_value="available")
        patcher.start()
        self.addCleanup(patcher.stop)
        self.config = {"slug": "example-fleet", "rearAdmiral": {"id": "atlas"}, "captains": [{"id": "orion"}],
                       "repositories": [{"id": "harbour-api", "url": "https://github.com/example/harbour-api.git"}],
                       "toolchains": {"node": "24", "go": "1.26"}}
        (self.inputs / "fleet.json").write_text(json.dumps(self.config))

    def clone(self, *names):
        write_repositories(self.references / "atlas", {name: REPOSITORIES[name] for name in names})

    def run_setup(self, *argv):
        """What one setup run does with the toolchains: update the record, write fleet.json, propose."""
        args = setup_args(*argv)
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            record, changes = fleet_setup.update_toolchains(args, {})
            for line in changes:
                fleet_setup.say(line)
            config = dict(self.config)
            config["toolchains"], packages = fleet_setup.toolchain_settings(record, config)
            if packages:
                config["systemPackages"] = packages
            fleet_setup.write_fleet(self.inputs / "fleet.json", config)
            record = json.loads((self.inputs / "toolchains.json").read_text())
            if proposal := fleet_setup.propose_toolchains(config, record):
                fleet_setup.show_proposal(proposal, True, record)
        return config, json.loads((self.inputs / "toolchains.json").read_text()), output.getvalue()

    def test_an_existing_fleet_keeps_its_toolchains_and_records_them(self):
        config, record, output = self.run_setup()
        self.assertEqual(config["toolchains"], {"node": "24", "go": "1.26"})
        self.assertEqual([(row["tool"], row["version"], row["sources"]) for row in record["tools"]],
                         [("node", "24", [{"by": "fleet.json"}]), ("go", "1.26", [{"by": "fleet.json"}])])
        self.assertIn("recorded the fleet's toolchains from fleet.json: node@24, go@1.26", output)
        # No detection unless the fleet derives its toolchains.
        self.assertNotIn("proposal", output)

    def test_derive_proposes_after_cloning_and_confirming_records_the_sources(self):
        _, _, output = self.run_setup("--toolchains", "derive")
        self.assertIn("not cloned on this VM yet", output)
        self.clone("harbour-api")
        config, record, output = self.run_setup("--toolchains", "derive")
        # Proposed, recorded as waiting, and nothing installed: fleet.json is unchanged.
        self.assertIn("Toolchain proposal from harbour-api.", output)
        self.assertEqual(config["toolchains"], {"node": "24", "go": "1.26"})
        self.assertEqual(record["proposal"]["repositories"], ["harbour-api"])
        self.assertEqual(record["examined"], [])
        # The person confirms some of it; the rest is recorded as declined.
        config, record, output = self.run_setup("--toolchains", "dotnet@8.0.404,node@22.11.0",
                                                "--system-packages", "zsh")
        self.assertEqual(config["toolchains"], {"node": ["24", "22.11.0"], "go": "1.26", "dotnet": "8.0.404"})
        self.assertEqual(config["systemPackages"], ["zsh"])
        dotnet = next(row for row in record["tools"] if row["tool"] == "dotnet")
        self.assertEqual(dotnet["sources"][0]["file"], "global.json")
        self.assertEqual(dotnet["install"], "user")
        self.assertEqual(record["packages"][0]["sources"][0]["file"], ".github/workflows/ci.yml")
        self.assertEqual(record["examined"], ["harbour-api"])
        self.assertEqual([row["name"] for row in record["declined"]["tools"]], ["pnpm"])
        self.assertIsNone(record["proposal"])
        self.assertIn("not confirmed, so recorded as declined: pnpm@9.12.3", output)
        # A later run keeps every choice and proposes nothing from repositories already examined.
        again, record_again, output = self.run_setup()
        self.assertEqual(again["toolchains"], config["toolchains"])
        self.assertEqual(record_again["tools"], record["tools"])
        self.assertNotIn("proposal", output)
        # A repository added since is examined on its own.
        self.config["repositories"].append({"id": "orbit-platform", "url": "https://github.com/example/orbit.git"})
        self.clone("orbit-platform")
        _, record, output = self.run_setup()
        self.assertIn("Toolchain proposal from orbit-platform.", output)
        self.assertNotIn("dotnet@8.0.404", output.split("Toolchain proposal", 1)[1])
        self.assertEqual(record["proposal"]["repositories"], ["orbit-platform"])

    def test_nothing_confirmed_is_dropped_silently(self):
        self.run_setup("--toolchains", "pnpm@9.12.3,rust@1.99.0")
        # Taken out of fleet.json by hand, a confirmed tool stays, and setup says how to remove it.
        edited = dict(self.config, toolchains={"node": "24", "go": "1.26", "rust": "1.99.0"})
        (self.inputs / "fleet.json").write_text(json.dumps(edited))
        config, _, output = self.run_setup()
        self.assertEqual(config["toolchains"]["pnpm"], "9.12.3")
        self.assertIn("kept pnpm@9.12.3 (confirmed", output)
        self.assertIn("'subspace-setup --toolchains=-pnpm@9.12.3' removes it", output)
        # Removing it by name does remove it, and says it stays installed.
        config, _, output = self.run_setup("--toolchains=-pnpm,-node@24")
        self.assertNotIn("pnpm", config["toolchains"])
        self.assertIs(config["toolchains"]["node"], False)
        self.assertIn("removed pnpm@9.12.3 from the toolchains (it stays installed)", output)
        # Added to fleet.json by hand, a tool is recorded, with fleet.json as its source.
        edited = dict(self.config, toolchains={**config["toolchains"], "deno": "2"})
        (self.inputs / "fleet.json").write_text(json.dumps(edited))
        _, record, output = self.run_setup()
        self.assertIn("recorded deno@2, which fleet.json lists", output)
        self.assertEqual(next(row for row in record["tools"] if row["tool"] == "deno")["sources"], [{"by": "fleet.json"}])

    def test_a_run_that_stops_between_its_writes_is_not_undone_by_the_next(self):
        self.run_setup("--toolchains", "pnpm@9.12.3,rust@1.99.0", "--system-packages", "zsh,nano")
        # Setup removes pnpm and nano, records that, and stops before it writes fleet.json.
        fleet_setup.update_toolchains(setup_args("--toolchains=-pnpm", "--system-packages=-nano"), {})
        self.assertIn("pnpm", json.loads((self.inputs / "fleet.json").read_text())["toolchains"])
        # The retry does not take fleet.json's stale pnpm and nano for hand additions.
        config, record, output = self.run_setup()
        self.assertNotIn("pnpm", config["toolchains"])
        self.assertEqual(config["systemPackages"], ["zsh"])
        self.assertNotIn("recorded", output)
        # Stopping after fleet.json is written, before the record notes it, is harmless too.
        fleet_setup.update_toolchains(setup_args("--toolchains=-rust"), {})
        config = json.loads((self.inputs / "fleet.json").read_text())
        config["toolchains"].pop("rust")
        (self.inputs / "fleet.json").write_text(json.dumps(config))
        config, record, output = self.run_setup()
        self.assertNotIn("rust", config["toolchains"])
        self.assertNotIn("recorded", output)
        self.assertNotIn("kept", output)
        # A real hand edit is still told apart from setup's writes.
        (self.inputs / "fleet.json").write_text(json.dumps({**config, "toolchains": {**config["toolchains"],
                                                                                      "deno": "2"}}))
        config, _, output = self.run_setup()
        self.assertIn("recorded deno@2, which fleet.json lists", output)
        self.assertEqual(config["toolchains"]["deno"], "2")

    def test_items_are_validated_before_anything_is_recorded(self):
        for argv in (["--toolchains", "node"], ["--toolchains", "node@24;reboot"], ["--system-packages", "zsh;id"],
                     ["--toolchains", "postinstall:x@1"]):
            with self.subTest(argv=argv), self.assertRaises(fleet_setup.SetupError):
                fleet_setup.update_toolchains(setup_args(*argv), {})
        self.assertFalse((self.inputs / "toolchains.json").exists())
        self.assertEqual(fleet_setup.parse_toolchain_items("npm:@example/cli@1.2.0,-node,cargo:cargo-nextest@0.9.100"),
                         [(False, "npm:@example/cli", "1.2.0"), (True, "node", None),
                          (False, "cargo:cargo-nextest", "0.9.100")])

    def test_a_proposal_with_nothing_new_is_settled_at_once(self):
        self.config["toolchains"] = {"bun": "1.3.2", "prek": "0.2.10", "gitleaks": "8.28.0", "shellcheck": "0.11.0",
                                     "github:example/relay-cli": "0.7.1"}
        self.config["repositories"] = [{"id": "relay-gateway", "url": "https://github.com/example/relay.git"}]
        (self.inputs / "fleet.json").write_text(json.dumps(self.config))
        self.clone("relay-gateway")
        _, record, output = self.run_setup("--toolchains", "derive")
        self.assertIn("Nothing new to confirm.", output)
        self.assertEqual(record["examined"], ["relay-gateway"])
        self.assertIsNone(record["proposal"])

    def test_a_proposal_named_only_by_documentation_is_answered_at_once(self):
        # Documentation-only items are left out of the confirming command, so such a proposal has nothing to
        # confirm: it is answered at once rather than proposed again on every run.
        self.config["repositories"] = [{"id": "docs-only", "url": "https://github.com/example/docs-only.git"}]
        (self.inputs / "fleet.json").write_text(json.dumps(self.config))
        write_repositories(self.references / "atlas", {"docs-only": {
            "README.md": "# Docs only\n\n```sh\nsudo apt-get install -y libfoo-dev\n```\n"}})
        _, record, output = self.run_setup("--toolchains", "derive")
        self.assertIn("[low] libfoo-dev", output)
        self.assertIn("Nothing to confirm: only documentation names the items above", output)
        self.assertNotIn("To confirm, edit and run", output)
        self.assertIsNone(record["proposal"])
        self.assertEqual(record["examined"], ["docs-only"])
        self.assertEqual([row["name"] for row in record["declined"]["packages"]], ["libfoo-dev"])
        _, _, output = self.run_setup("--toolchains", "derive")
        self.assertNotIn("proposal", output)

    def test_options_for_another_version_of_a_tool_are_joined_not_overwritten(self):
        # mise applies a tool's options to every version of it: two repositories pinning different Rust versions
        # with different components get both components on both versions, and the proposal says so beforehand.
        self.config["repositories"] = [{"id": name, "url": f"https://github.com/example/{name}.git"}
                                       for name in ("lint", "same")]
        (self.inputs / "fleet.json").write_text(json.dumps(self.config))
        write_repositories(self.references / "atlas", {
            "lint": {"rust-toolchain.toml": '[toolchain]\nchannel = "1.99.0"\ncomponents = ["clippy"]\n'},
            "same": {"mise.toml": '[tools]\nrust = { version = "1.99.0", components = "rust-src" }\n'},
            "fmt": {"rust-toolchain.toml": '[toolchain]\nchannel = "1.98.0"\ncomponents = ["rustfmt"]\n'}})
        _, record, output = self.run_setup("--toolchains", "derive")
        self.assertIn("mise applies rust's options to every version of it, so confirming this gives every rust "
                      "version components=clippy,rust-src", output)
        config, record, output = self.run_setup("--toolchains", "rust@1.99.0")
        # The same version asked for with different options by two repositories: both sets, joined.
        self.assertEqual(config["toolchains"]["rust"], {"version": "1.99.0", "options": {"components": "clippy,rust-src"}})
        # A repository added later pins another version with another component: the proposal says what confirming
        # it does to the version already confirmed, and confirming it keeps that version's components.
        self.config["repositories"].append({"id": "fmt", "url": "https://github.com/example/fmt.git"})
        _, record, output = self.run_setup()
        self.assertIn("confirming this gives every rust version components=clippy,rust-src,rustfmt", output)
        config, record, output = self.run_setup("--toolchains", "rust@1.98.0")
        self.assertEqual(config["toolchains"]["rust"], {"version": ["1.99.0", "1.98.0"],
                                                        "options": {"components": "clippy,rust-src,rustfmt"}})
        self.assertIn("confirmed rust@1.98.0, from fmt: rust-toolchain.toml:2; mise applies rust's options to every "
                      "version, so every rust version now has components=clippy,rust-src,rustfmt", output)
        self.assertEqual({row["version"]: row["options"]["components"] for row in record["tools"]
                          if row["tool"] == "rust"}, {"1.99.0": "clippy,rust-src,rustfmt",
                                                      "1.98.0": "clippy,rust-src,rustfmt"})
        # Options that cannot be joined stop setup with what to choose, and change nothing.
        record["tools"].append({"tool": "github:example/tool", "version": "1.0.0", "install": "shared",
                                "options": {"bin_path": "bin"}, "sources": [{"by": "listed"}], "confirmed": "2026-11-01"})
        record["proposal"] = {"repositories": ["tool"], "unknown": [], "items": [
            {"kind": "tool", "name": "github:example/tool", "version": "2.0.0", "options": {"bin_path": "tool/bin"},
             "confidence": "high", "sources": [], "where": "shared"}]}
        before = json.dumps(record)
        (self.inputs / "toolchains.json").write_text(before)
        with self.assertRaisesRegex(fleet_setup.SetupError, "choose one bin_path for github:example/tool"):
            fleet_setup.update_toolchains(setup_args("--toolchains", "github:example/tool@2.0.0"), {})
        self.assertEqual((self.inputs / "toolchains.json").read_text(), before)

    def test_the_show_and_detect_commands(self):
        self.clone("harbour-api")
        self.run_setup()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            fleet_setup.toolchains_command([])
        self.assertIn("  go@1.26: shared (/opt/subspace-fleet/toolchains); confirmed", output.getvalue())
        self.assertIn("Repositories examined: none", output.getvalue())
        output = io.StringIO()
        with contextlib.redirect_stdout(output), patch.object(fleet_setup.os, "geteuid", return_value=0):
            fleet_setup.toolchains_command(["detect", "--json"])
        proposal = json.loads(output.getvalue())
        self.assertEqual(proposal["repositories"], ["harbour-api"])
        record = json.loads((self.inputs / "toolchains.json").read_text())
        self.assertEqual(record["proposal"]["repositories"], ["harbour-api"])
        with patch.object(fleet_setup.os, "geteuid", return_value=1000), self.assertRaises(fleet_setup.SetupError):
            fleet_setup.toolchains_command(["detect"])

    def test_setups_dry_run_shows_the_proposal_and_records_nothing(self):
        self.clone("harbour-api")
        (self.inputs / "fleet.json").unlink()
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--crew-claude", "0", "--crew-codex", "0", "--inputs", str(self.inputs),
                "--stargate-ref", "c" * 40, "--no-demo", "--no-api", "--repositories",
                "harbour-api=https://github.com/example/harbour-api.git", "--toolchains", "derive",
                "--system-packages", "zsh", "--dry-run"]
        planned, said = [], []

        def installer(config_path, dry_run=False, secrets_dir=None):
            self.assertTrue(dry_run)
            planned.append(json.loads(Path(config_path).read_text()))

        output = io.StringIO()
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup, "say", side_effect=said.append), \
                patch.object(fleet_setup, "installer", side_effect=installer), \
                patch.object(fleet_setup, "check_outbound"), patch.object(fleet_setup, "ownership_check"), \
                contextlib.redirect_stdout(output):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        # The fleet installer's dry run gets the toolchains the real run would install, and setup proposes.
        self.assertEqual(planned[-1]["toolchains"], {"node": "24"})
        self.assertEqual(planned[-1]["systemPackages"], ["zsh"])
        self.assertIn("toolchain proposal (dry run: not recorded):", said)
        self.assertIn("  [high] dotnet@8.0.404: per user", output.getvalue())
        self.assertIn("Already configured: zsh.", output.getvalue())
        # Nothing the dry run did is kept.
        self.assertEqual(sorted(path.name for path in self.inputs.iterdir()), [])

    def test_a_detection_failure_never_stops_setup_before_its_lockdown(self):
        self.clone("harbour-api")
        (self.inputs / "fleet.json").unlink()
        argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral", "Atlas",
                "--captains", "Orion", "--crew-claude", "0", "--crew-codex", "0", "--inputs", str(self.inputs),
                "--stargate-ref", "c" * 40, "--no-demo", "--no-api", "--repositories",
                "harbour-api=https://github.com/example/harbour-api.git", "--toolchains", "derive"]
        order, said = [], []

        def run(*command, **_options):
            order.append(" ".join(str(part).rsplit("/", 1)[-1] for part in command[:2]))
            return ""
        with patch.object(fleet_setup.pwd, "getpwnam"), patch.object(fleet_setup.os, "geteuid", return_value=0), \
                patch.object(fleet_setup, "say", side_effect=said.append), \
                patch.object(fleet_setup, "installer", side_effect=lambda *a, dry_run=False, **k: order.append(
                    "check" if dry_run else "install")), \
                patch.object(fleet_setup, "run", side_effect=run), patch.object(fleet_setup, "api_down"), \
                patch.object(fleet_setup, "check_outbound"), patch.object(fleet_setup, "ownership_check"), \
                patch.object(fleet_setup, "irc_source", return_value="a" * 40), \
                patch.object(fleet_setup, "confirm_fleet_api", return_value=False), \
                patch.object(fleet_setup, "installed_stargate_source"), \
                patch.object(fleet_setup, "summary", side_effect=fleet_setup.SetupError("stop here")), \
                patch.object(toolchains, "detect", side_effect=RuntimeError("a repository's file")), \
                self.assertRaisesRegex(fleet_setup.SetupError, "stop here"):
            fleet_setup.setup(fleet_setup.parse_args(argv))
        self.assertIn("WARNING: the toolchain proposal failed (RuntimeError: a repository's file); setup carries on "
                      "without it. Run 'sudo subspace-setup toolchains detect' to try again.", said)
        # The lockdown is applied after the failed proposal, as on any run.
        self.assertLess(order.index("install"), order.index("lockdown.sh apply"))

    def test_the_question_and_its_answers(self):
        args = setup_args("--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                          "--operator", "operator")
        with patch.object(fleet_setup.pwd, "getpwnam"):
            self.assertEqual(fleet_setup.gather(args, {})["toolchains"], "none")
            for value, mode in (("derive", "derive"), ("none", "none"), ("node@24,pnpm@9.12.3", "list")):
                args = setup_args("--name", "Example Fleet", "--rear-admiral", "Atlas", "--captains", "Orion",
                                  "--operator", "operator", "--toolchains", value)
                self.assertEqual(fleet_setup.gather(args, {})["toolchains"], mode)
            # A fleet that derives keeps deriving when tools are confirmed by name.
            args = setup_args("--toolchains", "pnpm@9.12.3")
            saved = {"name": "Example Fleet", "slug": "example-fleet", "rearAdmiral": "Atlas", "captains": ["Orion"],
                     "toolchains": "derive", "crew": {"claude": 0, "codex": 0}, "operator": "operator",
                     "officerModels": {}}
            self.assertEqual(fleet_setup.gather(args, saved)["toolchains"], "derive")


if __name__ == "__main__":
    unittest.main()
