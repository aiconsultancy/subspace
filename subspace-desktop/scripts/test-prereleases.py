"""Offline pre-release boundaries: real Git history, fake GitHub, no Cargo."""

import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1]
# The fixture repository nests the project as the real repository does.
PROJECT = "subspace-desktop"
SCRIPTS = (
    "prerelease-common.sh", "prerelease-notes.sh", "prerelease-plan.sh",
    "prerelease-publish.sh", "prerelease-retention.sh", "prerelease-signing-allowed.sh",
    "install-release.sh", "install-release.ps1",
    "integration-prerelease-plan.sh",
    "release-macos.sh", "product.sh", "stamp-prerelease-version.sh", "github-oauth-secret.sh",
)
PREFIX = "subspace-desktop-v"


def release(tag: str, *, draft: bool = False, prerelease: bool = True) -> dict:
    return {"tag_name": tag, "draft": draft, "prerelease": prerelease, "body": ""}


class PrereleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="subspace-prerelease-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "bin").mkdir()
        # A clean Git environment: no GIT_DIR or index inherited from a hook,
        # no global or system config (signing programs, default branches), and
        # no SSH agent: `ssh-keygen -Y sign` asks an agent before it reads the
        # fixture key, and a developer's agent can prompt or never answer.
        (self.root / "gitconfig").write_text("")
        inherited = {name: value for name, value in os.environ.items()
                     if not name.startswith(("GIT_", "SSH_", "MISE_", "__MISE_"))}
        self.env = inherited | {
            "GIT_CONFIG_GLOBAL": str(self.root / "gitconfig"),
            "GIT_CONFIG_NOSYSTEM": "1",
            "PATH": f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}",
            "FIXTURE": str(self.root),
            # Fixture repositories resolve their own mise config, which the
            # outer project's session variables must not override.
            "MISE_TRUSTED_CONFIG_PATHS": str(self.root),
            "GITHUB_EVENT_NAME": "schedule", "GITHUB_REF": "refs/heads/main",
            "RELEASE_TYPE": "nightly", "RELEASE_FORCE": "false",
            "GITHUB_REPOSITORY": "fixture/repository", "GITHUB_RUN_ID": "1234",
            "GITHUB_OUTPUT": str(self.root / "output"),
            "GITHUB_STEP_SUMMARY": str(self.root / "summary"),
        }
        # Only this fake gh may mutate state. Every invocation is retained for
        # assertions that invalid requests never reach publication or cleanup.
        (self.root / "bin/gh").write_text("""#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"$FIXTURE/gh.log"
if [[ "$1" = api ]]; then
  case "$*" in
    */actions/runs/*) echo 2026-11-01T02:00:37Z ;;
    *matching-refs*) printf '%s\\n' "${FAKE_REFS:-}" ;;
    *) jq -s '[.[0]]' "$FIXTURE/releases.json" ;;
  esac
elif [[ "$1 $2" = 'release view' ]]; then
  [[ "${RELEASE_EXISTS:-false}" = true ]] && exit 0
  exit 1
elif [[ "$1 $2" = 'release create' && "${FAIL_CREATE:-false}" = true ]]; then
  exit 19
elif [[ "$1 $2" = 'release upload' && "${FAIL_UPLOAD:-false}" = true ]]; then
  exit 19
elif [[ "$1 $2" = 'release delete' ]]; then
  if [[ "${FAIL_DELETE:-false}" = true ]]; then exit 20; fi
  if [[ " $* " = *' --cleanup-tag '* ]] &&
    jq -e --arg tag "$3" 'any(.[]; .tag_name == $tag and .draft == true)' "$FIXTURE/releases.json" >/dev/null; then
    echo '422: draft release has no tag ref' >&2
    exit 22
  fi
fi
""")
        (self.root / "bin/gh").chmod(0o755)
        self.repository = self.root / "repository"
        self.scripts = self.repository / PROJECT / "scripts"
        self.scripts.mkdir(parents=True)
        for name in SCRIPTS:
            shutil.copyfile(TOOL / "scripts" / name, self.scripts / name)
        self.set_releases([])
        self.git("init", "-q")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.test")
        self.git("add", PROJECT)
        self.commit("README.md", "base")
        self.base = self.git("rev-parse", "HEAD")
        self.git("tag", f"{PREFIX}1.10.1")

    def git(self, *args: str) -> str:
        return subprocess.check_output(
            ["git", *args], cwd=self.repository, env=self.env, text=True, stderr=subprocess.PIPE
        ).strip()

    def commit(self, path: str, message: str) -> None:
        file = self.repository / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(message)
        self.git("add", path)
        self.git("commit", "-qm", message)
        self.env["GITHUB_SHA"] = self.git("rev-parse", "HEAD")

    def set_releases(self, releases: list[dict]) -> None:
        stable = [release(f"{PREFIX}1.10.1", prerelease=False)]
        (self.root / "releases.json").write_text(json.dumps(stable + releases))

    def run_script(self, name: str, *args: str, success: bool = True) -> subprocess.CompletedProcess:
        result = subprocess.run(
            ["bash", str(self.scripts / f"{name}.sh"), *args],
            cwd=self.repository, env=self.env, text=True, capture_output=True,
        )
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def plan(self) -> dict[str, str]:
        (self.root / "output").write_text("")
        self.run_script("prerelease-plan")
        return dict(line.split("=", 1) for line in (self.root / "output").read_text().splitlines())

    def previous(self, commit: str | None = None) -> str:
        tag = f"{PREFIX}1.10.2-nightly.202610310200.g0123456"
        self.git("tag", tag, commit or self.base)
        self.set_releases([release(tag)])
        return tag

    def test_decision_table_and_notes(self) -> None:
        self.assertEqual(self.plan()["build"], "true")
        previous = self.previous()
        self.commit("docs/note.md", "unrelated")
        self.assertEqual(self.plan()["build"], "false")
        summary = (self.root / "summary").read_text()
        self.assertIn(previous, summary)
        self.assertIn(self.base, summary)
        self.assertIn("no changes to the project", summary)
        self.env.update(GITHUB_EVENT_NAME="workflow_dispatch", RELEASE_TYPE="nightly", RELEASE_FORCE="true")
        self.run_script("prerelease-plan", success=False)
        # Force is allowed only for a manual alpha/beta, with its own baseline.
        beta = previous.replace("nightly", "beta")
        self.git("tag", beta, self.base)
        self.set_releases([release(beta)])
        self.env["RELEASE_TYPE"] = "beta"
        self.assertEqual(self.plan()["build"], "true")
        self.env["RELEASE_FORCE"] = "false"
        self.assertEqual(self.plan()["build"], "false")
        self.commit(f"{PROJECT}/framework/host/app/source.rs", "relevant")
        output = self.plan()
        self.assertEqual(output["build"], "true")
        self.assertRegex(output["version"], r"^1\.10\.2-beta\.202611010200\.g[0-9a-f]{7}$")
        notes = self.run_script("prerelease-notes", beta, self.env["GITHUB_SHA"]).stdout
        self.assertIn("beta pre-release of Subspace", notes)
        self.assertIn("relevant", notes)
        self.assertNotIn("unrelated", notes)

    def test_notes_and_plan_ignore_sibling_projects(self) -> None:
        previous = self.previous()
        self.commit("issue-tracker/source.rs", "sibling")
        self.assertEqual(self.plan()["build"], "false")
        self.commit(f"{PROJECT}/source.rs", "project-source")
        self.assertEqual(self.plan()["build"], "true")
        notes = self.run_script("prerelease-notes", previous, self.env["GITHUB_SHA"]).stdout
        self.assertIn("project-source", notes)
        self.assertNotIn("sibling", notes)

    def test_a_path_built_framework_counts_as_the_project(self) -> None:
        manifest = '[workspace.dependencies]\nsubspace-framework = { path = "../subspace-framework", version = "0.1.0" }\n'
        self.commit(f"{PROJECT}/Cargo.toml", manifest)
        previous = self.previous(self.git("rev-parse", "HEAD"))
        self.commit("subspace-framework/src/lib.rs", "framework-change")
        self.assertEqual(self.plan()["build"], "true")
        notes = self.run_script("prerelease-notes", previous, self.env["GITHUB_SHA"]).stdout
        self.assertIn("framework-change", notes)
        published = '[workspace.dependencies]\nsubspace-framework = "0.1.0"\n'
        self.commit(f"{PROJECT}/Cargo.toml", published)
        self.git("tag", "-f", previous, self.git("rev-parse", "HEAD"))
        self.commit("subspace-framework/src/lib.rs", "framework-after-publishing")
        self.assertEqual(self.plan()["build"], "false")

    def test_nonancestor_builds_and_ignores_other_types_and_drafts(self) -> None:
        self.git("checkout", "-qb", "other")
        self.commit("docs/other", "other history")
        previous = self.previous(self.env["GITHUB_SHA"])
        self.git("checkout", "-q", "-")
        self.env["GITHUB_SHA"] = self.git("rev-parse", "HEAD")
        self.set_releases([
            release(previous),
            release(previous.replace("nightly", "beta")),
            release(previous.replace("20261031", "20261102"), draft=True),
        ])
        self.assertEqual(self.plan()["build"], "true")
        self.assertIn("not an ancestor", (self.root / "summary").read_text())

    def test_published_stable_base_and_old_run_rejection(self) -> None:
        self.set_releases([
            release(f"{PREFIX}1.11.0", prerelease=False),
            release(f"{PREFIX}1.12.0", draft=True, prerelease=False),
            release("other-component-v9.0.0", prerelease=False),
        ])
        output = self.plan()
        self.assertTrue(output["version"].startswith("1.11.1-nightly."))
        self.assertEqual(output["previous"], f"{PREFIX}1.11.0")
        tag = self.previous().replace("202610310200", "202611020200")
        self.git("tag", tag, self.base)
        self.set_releases([release(tag)])
        self.commit(f"{PROJECT}/source.rs", "changed")
        result = self.run_script("prerelease-plan", success=False)
        self.assertIn("not newer", result.stderr)

    def test_integration_plan_is_deterministic_from_commit_time_and_published_stable(self) -> None:
        self.git("checkout", "-qb", "Feat/Subspace_Desktop")
        self.commit(f"{PROJECT}/source.rs", "feat(subspace): add a capability")
        subprocess.run(
            ["git", "commit", "--allow-empty", "-qm", "dated integration"],
            cwd=self.repository, check=True,
            env=self.env | {"GIT_COMMITTER_DATE": "2026-11-15T15:45:37+02:00"},
        )
        sha = self.git("rev-parse", "HEAD")
        self.set_releases([
            release(f"{PREFIX}1.10.9", prerelease=False),
            release(f"{PREFIX}1.11.0", prerelease=False),
            release(f"{PREFIX}1.12.0", prerelease=False, draft=True),
            release("other-v9.9.9", prerelease=False),
        ])
        self.git("tag", f"{PREFIX}1.11.0", self.base)
        expected = f"1.12.0-feat-subspace-desktop.202611151345.g{sha[:7]}"
        for _ in range(2):
            self.assertEqual(self.run_script("integration-prerelease-plan").stdout.strip(), expected)
        log = (self.root / "gh.log").read_text()
        self.assertNotIn("release create", log)
        self.assertNotIn("actions/runs", log)

    def integration_stable(self, version: str = "0.2.1") -> None:
        self.git("tag", f"{PREFIX}{version}", self.base)
        (self.root / "releases.json").write_text(json.dumps([
            release(f"{PREFIX}{version}", prerelease=False),
        ]))

    def test_integration_feat_bumps_minor_and_channels_keep_the_patch_base(self) -> None:
        self.integration_stable()
        self.commit(f"{PROJECT}/source.rs", "fix(subspace): correct a defect")
        self.commit(f"{PROJECT}/source.rs", "feat(subspace): add a capability")
        version = self.run_script("integration-prerelease-plan").stdout.strip()
        self.assertTrue(version.startswith("0.3.0-"), version)
        for kind in ("nightly", "alpha", "beta"):
            with self.subTest(channel=kind):
                self.env.update(RELEASE_TYPE=kind, GITHUB_EVENT_NAME=(
                    "schedule" if kind == "nightly" else "workflow_dispatch"
                ))
                self.assertTrue(self.plan()["version"].startswith(f"0.2.2-{kind}."))

    def test_integration_fix_and_perf_bump_patch(self) -> None:
        self.integration_stable()
        for kind in ("fix", "perf"):
            with self.subTest(kind=kind):
                self.git("checkout", "-qb", kind, self.base)
                self.commit(f"{PROJECT}/source.rs", f"{kind}(subspace): improve existing behaviour")
                version = self.run_script("integration-prerelease-plan").stdout.strip()
                self.assertTrue(version.startswith("0.2.2-"), version)

    def test_integration_breaking_changes_bump_minor_below_one(self) -> None:
        self.integration_stable()
        messages = (
            "refactor(subspace)!: change the configuration",
            "refactor!: change the configuration",
            "refactor(subspace): change the configuration\n\nBREAKING CHANGE: rename the saved key",
            "refactor(subspace): change the configuration\n\nBREAKING-CHANGE: rename the saved key",
        )
        for index, message in enumerate(messages):
            with self.subTest(message=message):
                self.git("checkout", "-qb", f"breaking-{index}", self.base)
                self.commit(f"{PROJECT}/source.rs", message)
                version = self.run_script("integration-prerelease-plan").stdout.strip()
                self.assertTrue(version.startswith("0.3.0-"), version)

    def test_empty_breaking_footer_cannot_consume_the_next_paragraph(self) -> None:
        self.integration_stable()
        for index, footer in enumerate(("BREAKING CHANGE:", "BREAKING-CHANGE: \t")):
            with self.subTest(footer=footer):
                self.git("checkout", "-qb", f"empty-footer-{index}", self.base)
                self.commit(
                    f"{PROJECT}/source.rs",
                    f"fix(subspace): correct existing behaviour\n\n{footer}\n\nAn ordinary paragraph follows.",
                )
                version = self.run_script("integration-prerelease-plan").stdout.strip()
                self.assertTrue(version.startswith("0.2.2-"), version)

    def test_breaking_change_examples_in_the_body_do_not_bump_minor(self) -> None:
        self.integration_stable()
        for index, marker in enumerate(("BREAKING CHANGE:", "BREAKING-CHANGE:")):
            with self.subTest(marker=marker):
                self.git("checkout", "-qb", f"quoted-footer-{index}", self.base)
                self.commit(
                    f"{PROJECT}/source.rs",
                    f"fix(subspace): clarify a parser error\n\n"
                    f"The old message contained this example:\n{marker} rename a key\n\n"
                    "This fix preserves the existing configuration.",
                )
                version = self.run_script("integration-prerelease-plan").stdout.strip()
                self.assertTrue(version.startswith("0.2.2-"), version)

    def test_signature_display_does_not_change_the_integration_plan(self) -> None:
        self.integration_stable()
        key = self.root / "signing-key"
        subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)], check=True)
        allowed = self.root / "allowed-signers"
        allowed.write_text("fixture@example.test " + key.with_suffix(".pub").read_text())
        self.git("config", "gpg.format", "ssh")
        self.git("config", "user.signingkey", str(key))
        self.git("config", "gpg.ssh.allowedSignersFile", str(allowed))
        self.git("config", "commit.gpgsign", "true")
        self.commit(f"{PROJECT}/source.rs", "feat(subspace): add a signed capability")
        expected = self.run_script("integration-prerelease-plan").stdout.strip()
        self.assertTrue(expected.startswith("0.3.0-"), expected)
        self.git("config", "log.showSignature", "true")
        self.assertIn("Good", self.git("log", "-1", "--format=%B"))
        with self.subTest(output="commit messages"):
            result = subprocess.run(
                ["bash", "-c", 'source "$1"; integration_base 0.2.1 HEAD', "bash",
                 str(self.scripts / "prerelease-common.sh")],
                cwd=self.repository, env=self.env, text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "0.3.0")
        with self.subTest(output="committer time"):
            self.assertEqual(self.run_script("integration-prerelease-plan").stdout.strip(), expected)

    def test_integration_without_published_stable_reports_the_problem(self) -> None:
        self.commit(f"{PROJECT}/source.rs", "feat(subspace): add a capability")
        for entries in (
            [],
            [release(f"{PREFIX}0.2.1", prerelease=False, draft=True)],
            [release(f"{PREFIX}0.3.0-nightly.202611151345.gabc1234")],
            [release("other-component-v0.2.1", prerelease=False)],
        ):
            with self.subTest(releases=entries):
                (self.root / "releases.json").write_text(json.dumps(entries))
                result = self.run_script("integration-prerelease-plan", success=False)
                self.assertIn("no published stable Subspace release", result.stderr)
                self.assertNotIn("release create", (self.root / "gh.log").read_text())

    def test_diverged_stable_tag_counts_only_commits_unique_to_the_integration_branch(self) -> None:
        self.git("checkout", "-qb", "published-stable")
        self.commit(f"{PROJECT}/stable.rs", "feat(subspace): add a stable-only feature")
        self.git("tag", f"{PREFIX}0.2.1")
        (self.root / "releases.json").write_text(json.dumps([
            release(f"{PREFIX}0.2.1", prerelease=False),
        ]))
        self.git("checkout", "-qb", "integration", self.base)
        self.commit(f"{PROJECT}/source.rs", "fix(subspace): correct existing behaviour")
        ancestry = subprocess.run(
            ["git", "merge-base", "--is-ancestor", f"{PREFIX}0.2.1", "HEAD"],
            cwd=self.repository, env=self.env,
        )
        self.assertEqual(ancestry.returncode, 1)
        version = self.run_script("integration-prerelease-plan").stdout.strip()
        self.assertTrue(version.startswith("0.2.2-integration."), version)

    def test_integration_make_target_rejects_empty_signing_arguments(self) -> None:
        project = self.repository / PROJECT
        shutil.copyfile(TOOL / "Makefile", project / "Makefile")
        mise = self.root / "bin/mise"
        mise.write_text('#!/bin/sh\nprintf "mise invoked\\n" >>"$FIXTURE/mise.log"\n')
        mise.chmod(0o755)
        for setting in ("SIGNING_IDENTITY", "KEYCHAIN_PROFILE"):
            with self.subTest(setting=setting):
                result = subprocess.run(
                    ["make", "--no-print-directory", "release-macos-prerelease", f"{setting}="],
                    cwd=project, env=self.env, text=True, capture_output=True,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(f"{setting} is required", result.stdout + result.stderr)
                self.assertFalse((self.root / "mise.log").exists())

    def test_integration_without_app_changing_commits_refuses_a_build(self) -> None:
        self.integration_stable()
        for path, message in (
            (None, None),
            (f"{PROJECT}/docs/note.md", "docs(subspace): explain the app"),
            ("sibling/source.rs", "feat(sibling): add an unrelated capability"),
            (f"{PROJECT}/source.rs", "refactor(subspace): tidy existing code"),
        ):
            with self.subTest(message=message):
                if path:
                    self.commit(path, message)
                result = self.run_script("integration-prerelease-plan", success=False)
                self.assertIn("no app-changing commits", result.stderr)
                self.assertNotIn("release create", (self.root / "gh.log").read_text())

    def test_integration_breaking_change_takes_precedence_over_features_after_one(self) -> None:
        self.commit(f"{PROJECT}/source.rs", "feat(subspace): add a capability")
        self.commit(f"{PROJECT}/source.rs", "fix(subspace)!: replace the old behaviour")
        version = self.run_script("integration-prerelease-plan").stdout.strip()
        self.assertTrue(version.startswith("2.0.0-"), version)

    def test_integration_missing_stable_tag_refuses_a_build(self) -> None:
        self.set_releases([release(f"{PREFIX}1.11.0", prerelease=False)])
        self.commit(f"{PROJECT}/source.rs", "feat(subspace): add a capability")
        result = self.run_script("integration-prerelease-plan", success=False)
        self.assertIn("fetch tags first", result.stderr)
        self.assertNotIn("release create", (self.root / "gh.log").read_text())

    def test_integration_plan_refuses_dirty_detached_and_reserved_branches(self) -> None:
        original = self.git("rev-parse", "--abbrev-ref", "HEAD")
        for branch in ("nightly", "alpha", "beta", "stable", "Stable", "01"):
            self.git("checkout", "-qb", branch)
            self.run_script("integration-prerelease-plan", success=False)
            # Case-insensitive file systems cannot hold `stable` and `Stable`.
            self.git("checkout", "-q", original)
            self.git("branch", "-qD", branch)
        self.git("checkout", "-qb", "feat/subspace-desktop")
        (self.repository / "outside-project.txt").write_text("untracked")
        self.run_script("integration-prerelease-plan", success=False)
        self.git("add", "outside-project.txt")
        self.run_script("integration-prerelease-plan", success=False)
        self.git("commit", "-qm", "tracked")
        (self.repository / "outside-project.txt").write_text("dirty")
        self.run_script("integration-prerelease-plan", success=False)
        self.git("restore", "outside-project.txt")
        self.git("checkout", "--detach", "HEAD")
        self.run_script("integration-prerelease-plan", success=False)
        self.assertFalse((self.root / "gh.log").exists())

    def prepare_local_release(self) -> Path:
        project = self.repository / PROJECT
        (project / "Cargo.toml").write_text('[workspace.package]\nversion = "0.2.1"\n')
        (project / "Cargo.lock").write_text('[[package]]\nname = "subspace"\nversion = "0.2.1"\n')
        (project / "mise.toml").write_text('[tools]\nrust = { version = "1.77.0" }\n')
        # The two build configs every release packages.
        (project / "products/subspace").mkdir(parents=True)
        for name in ("build-config.json", "build-config.no-api.json"):
            shutil.copyfile(TOOL / "products/subspace" / name, project / "products/subspace" / name)
        rust_docs = self.root / "sysroot/share/doc/rust"
        (rust_docs / "licenses").mkdir(parents=True)
        for notice in ("COPYRIGHT-library.html", "licenses/Apache-2.0.txt", "licenses/MIT.txt"):
            (rust_docs / notice).write_text("Fixture standard library notice\n")
        rustc = self.root / "bin/rustc"
        rustc.write_text("""#!/bin/sh
case "$*" in
  --version) echo 'rustc 1.77.0 (fixture)' ;;
  '--print sysroot') echo "$FIXTURE/sysroot" ;;
  *) exit 1 ;;
esac
""")
        rustc.chmod(0o755)
        (self.repository / ".gitignore").write_text(f"/{PROJECT}/dist/\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fix(subspace): update release fixture")
        self.git("checkout", "-qb", "feat/subspace-desktop")
        (self.root / "bin/uname").write_text('#!/bin/sh\necho "${FAKE_OS:-Darwin}"\n')
        (self.root / "bin/cargo").write_text("""#!/usr/bin/env bash
set -euo pipefail
version="$(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)"
version="${FAKE_METADATA_VERSION:-$version}"
if [[ "$1" = metadata ]]; then
  jq -n --arg v "$version" --arg target "$PWD/target" '{packages:[{name:"subspace",version:$v,source:null}],target_directory:$target}'
else
  test "$1" = run
  test "${SUBSPACE_RELEASE_CHANNEL:-}" = stable
  jq -n --arg v "$version" '{binary_name:"subspace",display_name:"Subspace",bundle_id:"com.aiconsultancy.subspace",asset_stem:"Subspace",channel:"stable",version:$v,subspace_version:$v,icon_paths:["assets/subspace.icns"]}'
fi
""")
        package = self.root / "package.sh"
        package.write_text("""#!/usr/bin/env bash
set -euo pipefail
echo "package $*" >>"$FIXTURE/gh.log"
[[ "${FAIL_PACKAGE:-false}" != true ]] || exit 18
version="$(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)"
mkdir -p dist
if [[ "$(jq -r '.api_backend // ""' "$SUBSPACE_BUILD_CONFIG")" = none ]]; then
  printf 'fixture app with no API\\n' >"dist/Subspace-NoAPI_${version}_darwin_arm64.zip"
else
  printf 'fixture app\\n' >"dist/Subspace_${version}_darwin_arm64.zip"
fi
""")
        # Never the real keychain: the variable supplies the OAuth secret.
        (self.root / "bin/security").write_text("#!/bin/sh\nexit 44\n")
        # The desktop login session, so release-macos.sh never reaches launchd.
        (self.root / "bin/launchctl").write_text("#!/bin/sh\necho Aqua\n")
        for command in (self.root / "bin/uname", self.root / "bin/cargo", self.root / "bin/security",
                        self.root / "bin/launchctl"):
            command.chmod(0o755)
        self.env.update(SUBSPACE_PACKAGE_ROOT=str(project), SUBSPACE_RELEASE_PACKAGE=str(package),
                        SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET="fixture-oauth-secret", RUSTC=str(rustc))
        return project

    def test_local_integration_release_stamps_builds_and_restores_the_clean_checkout(self) -> None:
        project = self.prepare_local_release()
        sha = self.git("rev-parse", "HEAD")
        version = self.run_script("integration-prerelease-plan").stdout.strip()
        self.run_script("release-macos", "--prerelease", "Fixture Developer ID", "fixture-profile")
        log = (self.root / "gh.log").read_text()
        self.assertIn("package Fixture Developer ID fixture-profile aarch64-apple-darwin", log)
        self.assertIn(
            f"release create {PREFIX}{version} dist/Subspace-NoAPI_{version}_darwin_arm64.zip "
            f"dist/Subspace-NoAPI_{version}_darwin_arm64.zip.sha256 dist/Subspace_{version}_darwin_arm64.zip",
            log,
        )
        self.assertIn(f".zip.sha256 --repo aiconsultancy/subspace --target {sha} --title Subspace {version}", log)
        self.assertIn("--prerelease --latest=false", log)
        self.assertIn("repos/aiconsultancy/subspace/releases?per_page=100", log)
        checksum = project / "dist" / f"Subspace_{version}_darwin_arm64.zip.sha256"
        self.assertIn(f"Subspace_{version}_darwin_arm64.zip", checksum.read_text())
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.git("rev-parse", "HEAD"), sha)

    def test_already_stamped_integration_releases_cannot_be_published_as_stable(self) -> None:
        self.prepare_local_release()
        version = "0.3.0-feat-subspace-desktop.202611151345.gabc1234"
        self.run_script("stamp-prerelease-version", version)
        self.git("add", ".")
        self.git("commit", "-qm", "stamped fixture")
        self.env["SUBSPACE_RELEASE_CHANNEL"] = "stable"
        self.run_script("release-macos", "Fixture", "profile")
        log = (self.root / "gh.log").read_text()
        self.assertIn("--prerelease --latest=false", log)

    def test_integration_release_rejects_metadata_that_differs_from_the_plan(self) -> None:
        self.prepare_local_release()
        for version in ("0.2.1", "0.3.0-other-branch.202611151345.gabc1234"):
            with self.subTest(version=version):
                (self.root / "gh.log").write_text("")
                self.env["FAKE_METADATA_VERSION"] = version
                result = self.run_script("release-macos", "--prerelease", "Fixture", "profile", success=False)
                self.assertIn("integration version does not match the planned version", result.stderr)
                log = (self.root / "gh.log").read_text()
                self.assertNotIn("package ", log)
                self.assertNotIn("release create", log)
                self.assertEqual(self.git("status", "--porcelain"), "")

    def test_local_integration_release_failures_restore_versions_and_never_publish(self) -> None:
        self.prepare_local_release()
        for flag in ("RELEASE_EXISTS", "FAIL_PACKAGE", "FAIL_CREATE"):
            with self.subTest(flag=flag):
                (self.root / "gh.log").write_text("")
                self.env[flag] = "true"
                self.run_script("release-macos", "--prerelease", "Fixture", "profile", success=False)
                del self.env[flag]
                self.assertEqual(self.git("status", "--porcelain"), "")
                log = (self.root / "gh.log").read_text()
                if flag != "FAIL_CREATE":
                    self.assertNotIn("release create", log)
        self.env["FAKE_OS"] = "Linux"
        (self.root / "gh.log").write_text("")
        self.run_script("release-macos", "--prerelease", "Fixture", "profile", success=False)
        self.assertEqual((self.root / "gh.log").read_text(), "")

    def test_boundary_rejections_and_signing_gate(self) -> None:
        for field, value in (
            ("GITHUB_REF", "refs/heads/feature"), ("RELEASE_TYPE", "rc"),
            ("RELEASE_FORCE", "yes"), ("RELEASE_FORCE", ""), ("RELEASE_FORCE", "true"),
            ("GITHUB_EVENT_NAME", "push"),
        ):
            with self.subTest(field=field, value=value):
                original = self.env[field]
                self.env[field] = value
                self.run_script("prerelease-plan", success=False)
                self.run_script("prerelease-signing-allowed", success=False)
                self.env[field] = original
        self.assertFalse((self.root / "gh.log").exists())
        self.assertEqual(self.run_script("prerelease-signing-allowed").stdout.strip(), "true")
        for kind in ("alpha", "beta"):
            self.env.update(GITHUB_EVENT_NAME="workflow_dispatch", RELEASE_TYPE=kind, RELEASE_FORCE="true")
            self.assertEqual(self.run_script("prerelease-signing-allowed").stdout.strip(), "true")
        self.env["RELEASE_TYPE"] = "nightly"
        self.run_script("prerelease-signing-allowed", success=False)

    def test_retention_is_prefix_type_and_flag_scoped(self) -> None:
        tags = [f"{PREFIX}1.10.2-nightly.2026103{index}0200.g0123456" for index in range(1, 6)]
        stale = tags[0].replace("20261031", "20261030")
        self.set_releases([
            *(release(tag) for tag in tags), release(stale, draft=True),
            release(f"{PREFIX}1.10.1", prerelease=False),
            release(tags[0].replace("nightly", "beta")),
            release(tags[0].replace(PREFIX, "other-component-v")),
            release(tags[0].replace("20261031", "20261029"), prerelease=False),
        ])
        self.run_script("prerelease-retention", tags[-1])
        deleted = [line.split()[2] for line in (self.root / "gh.log").read_text().splitlines() if line.startswith("release delete ")]
        self.assertEqual(set(deleted), {tags[0], tags[1], stale})
        commands = [line for line in (self.root / "gh.log").read_text().splitlines() if line.startswith("release delete ")]
        for command in commands:
            self.assertEqual("--cleanup-tag" in command, command.split()[2] != stale)
        self.env["FAIL_DELETE"] = "true"
        self.run_script("prerelease-retention", tags[-1], success=False)

    def test_publish_draft_retry_collision_and_upload_failure(self) -> None:
        version = f"1.10.2-nightly.202611010200.g{self.env['GITHUB_SHA'][:7]}"
        tag = f"{PREFIX}{version}"
        args = (version, self.env["GITHUB_SHA"], "notes.md", "dist")
        self.run_script("prerelease-publish", *args)
        log = (self.root / "gh.log").read_text()
        self.assertIn("release create", log)
        self.assertIn("--draft --prerelease --latest=false", log)
        self.assertLess(log.index("release upload"), log.index("release edit"))
        self.assertIn(str(self.scripts / "install-release.sh"), log)
        self.assertIn(str(self.scripts / "install-release.ps1"), log)
        self.assertIn("--draft=false --prerelease --latest=false", log)
        for existing in (release(tag), release(tag, draft=True)):
            self.set_releases([existing])
            (self.root / "gh.log").write_text("")
            self.run_script("prerelease-publish", *args, success=False)
            self.assertNotIn("release upload", (self.root / "gh.log").read_text())
        owned = release(tag, draft=True) | {
            "body": "<!-- subspace-prerelease-run:1234 -->", "target_commitish": self.env["GITHUB_SHA"]
        }
        self.set_releases([owned])
        self.env["FAIL_UPLOAD"] = "true"
        (self.root / "gh.log").write_text("")
        self.run_script("prerelease-publish", *args, success=False)
        self.assertNotIn("release edit", (self.root / "gh.log").read_text())
        del self.env["FAIL_UPLOAD"]
        self.run_script("prerelease-publish", *args)
        self.assertNotIn("release create", (self.root / "gh.log").read_text())
        self.set_releases([])
        self.env["FAKE_REFS"] = f"refs/tags/{tag}"
        (self.root / "gh.log").write_text("")
        self.run_script("prerelease-publish", *args, success=False)
        self.assertNotIn("release create", (self.root / "gh.log").read_text())


def workspace_version(manifest: str) -> str:
    return tomllib.loads(manifest)["workspace"]["package"]["version"]


class StampTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory(prefix="subspace-stamp-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "scripts").mkdir()
        shutil.copyfile(TOOL / "scripts/stamp-prerelease-version.sh", self.root / "scripts/stamp-prerelease-version.sh")
        shutil.copyfile(TOOL / "scripts/prerelease-common.sh", self.root / "scripts/prerelease-common.sh")
        for name in ("Cargo.toml", "Cargo.lock"):
            shutil.copyfile(TOOL / name, self.root / name)
        lock = tomllib.loads((TOOL / "Cargo.lock").read_text())
        version = workspace_version((TOOL / "Cargo.toml").read_text())
        self.members = {
            package["name"] for package in lock["package"]
            if "source" not in package and package["version"] == version
        }
        self.assertIn("subspace", self.members)
        commands = self.root / "bin"
        commands.mkdir()
        # Metadata reflects the manifest's current workspace version, as Cargo would.
        (commands / "cargo").write_text(
            '#!/usr/bin/env bash\nset -euo pipefail\n'
            '[[ "$*" = "metadata --locked --format-version=1 --no-deps" ]]\n'
            'printf "%s\\n" "$*" >>"$STAMP_LOG"\n'
            'version="$(awk \'/^\\[workspace.package\\]$/ { s = 1; next } /^\\[/ { s = 0 } '
            's && /^version = / { gsub(/"/, "", $3); print $3; exit }\' Cargo.toml)"\n'
            'jq -n --arg version "$version" --arg names "$STAMP_MEMBERS" '
            '\'{packages: [($names | split(" ")[] | {name: ., version: $version, source: null}),'
            ' {name: "excluded-terminal", version: "0.1.0", source: null}]}\'\n'
        )
        (commands / "cargo").chmod(0o755)
        # A build runner must not need either spelling of Python.
        for executable in ("python", "python3"):
            (commands / executable).write_text('#!/usr/bin/env bash\necho "unexpected Python dependency" >&2\nexit 97\n')
            (commands / executable).chmod(0o755)
        self.env = os.environ | {
            "PATH": f"{commands}{os.pathsep}{os.environ['PATH']}",
            "STAMP_LOG": str(self.root / "cargo.log"),
            "STAMP_MEMBERS": " ".join(sorted(self.members)),
        }

    def stamp(self, version: str, *, success: bool = True) -> None:
        result = subprocess.run(
            ["bash", str(self.root / "scripts/stamp-prerelease-version.sh"), version],
            cwd=self.root / "bin", env=self.env, text=True, capture_output=True,
        )
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)

    def snapshot(self) -> dict[str, bytes]:
        return {name: (self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")}

    def test_workspace_version_and_members_only_and_idempotence(self) -> None:
        original = self.snapshot()
        version = "1.10.2-nightly.202611010200.g0123456"
        self.stamp(version)
        first = self.snapshot()
        self.stamp(version)
        self.assertEqual(first, self.snapshot())
        self.assertEqual(len((self.root / "cargo.log").read_text().splitlines()), 4)
        self.assertTrue(all(b"\r" not in contents for contents in first.values()))
        before = tomllib.loads(original["Cargo.toml"].decode())
        after = tomllib.loads(first["Cargo.toml"].decode())
        before["workspace"]["package"]["version"] = version
        self.assertEqual(after, before)
        before_lock = tomllib.loads(original["Cargo.lock"].decode())["package"]
        after_lock = tomllib.loads(first["Cargo.lock"].decode())["package"]
        for old, new in zip(before_lock, after_lock, strict=True):
            expected = old | {"version": version} if old["name"] in self.members and "source" not in old else old
            self.assertEqual(new, expected)
        self.stamp("1.10.2-nightly.202611010200.0123456", success=False)
        self.assertEqual(first, self.snapshot())
        self.assertEqual(list(self.root.glob(".prerelease-version.*")), [])

    def test_missing_workspace_version_changes_nothing(self) -> None:
        manifest = self.root / "Cargo.toml"
        manifest.write_text(re.sub(r"(?m)^\[workspace\.package\]$", "[workspace.other]", manifest.read_text()))
        self.env["STAMP_MEMBERS"] = "subspace"
        original = self.snapshot()
        self.stamp("1.10.2-beta.202611010200.g0123456", success=False)
        self.assertEqual(original, self.snapshot())
        self.assertEqual(list(self.root.glob(".prerelease-version.*")), [])

    def test_branch_slug_versions_and_reserved_channels(self) -> None:
        for slug in ("feat-subspace-desktop", "fix-123", "123", "--topic"):
            with self.subTest(slug=slug):
                version = f"0.3.0-{slug}.202611151345.gabc1234"
                self.stamp(version)
                self.assertEqual(workspace_version((self.root / "Cargo.toml").read_text()), version)
        original = self.snapshot()
        for slug in ("stable", "Feat-subspace", "feat/subspace", "feat_subspace", "01"):
            with self.subTest(slug=slug):
                self.stamp(f"0.3.0-{slug}.202611151345.gabc1234", success=False)
                self.assertEqual(self.snapshot(), original)


if __name__ == "__main__":
    unittest.main()
