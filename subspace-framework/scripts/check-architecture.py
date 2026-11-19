"""Check the Framework's dependency direction and source includes."""

import json
import subprocess
import sys
from pathlib import Path

import architecture_source

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {
    "host": {"host", "board-kit", "services", "runtimes", "clients"},
    "spaces": {"board-kit", "services", "runtimes", "clients"},
    "board-kit": {"board-kit", "services"},
    "runtimes": {"board-kit", "services", "clients", "runtimes"},
    "clients": {"services", "clients"},
    "services": {"services"},
}
# The Framework's layers. Its catalogue Space lives in spaces/.
LAYERS = {"host", "board-kit", "services", "runtimes"}


def group(directory: Path) -> tuple[str, str | None]:
    parts = directory.relative_to(ROOT).parts
    # The facade crate at the root owns only its src/.
    if parts in {(), ("src",)}:
        return "facade", None
    if len(parts) == 2 and parts[0] in LAYERS | {"clients"}:
        return parts[0], None
    if len(parts) == 2 and parts[0] == "spaces":
        return "spaces", parts[1]
    if len(parts) == 4 and parts[0] == "spaces" and parts[2] == "crates":
        return "spaces", parts[1]
    raise ValueError(f"Unclassified internal package directory: {directory}")


def packages() -> dict[Path, dict]:
    # --no-deps includes every declared dependency, including optional,
    # target-specific, build and dev edges.
    output = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps",
         "--locked", "--manifest-path", str(ROOT / "Cargo.toml")],
        cwd=ROOT, text=True,
    )
    result = {}
    for package in json.loads(output)["packages"]:
        directory = Path(package["manifest_path"]).resolve().parent
        # The facade's sources are its src/; the rest of the root holds the
        # other packages, docs and scripts.
        result[ROOT / "src" if directory == ROOT else directory] = package
    return result


def allowed(source: tuple[str, str | None], target: tuple[str, str | None]) -> bool:
    layer, owner = source
    target_layer, target_owner = target
    return (layer == "facade"
            or target_layer in ALLOWED[layer]
            or layer == target_layer == "spaces" and owner == target_owner)


def allowed_package(source: Path, target: Path) -> bool:
    """Apply layer rules to Cargo edges and source includes alike."""
    return allowed(group(source), group(target))


def headless_dependency_violation(directory: Path, package: dict, dependency: dict) -> str | None:
    """Reject direct GPUI and agent SDK packages in neutral headless services."""
    if directory not in {ROOT / "services/agent-tools", ROOT / "services/agent-model"}:
        return None
    # Cargo metadata's name is the actual package, even for renamed edges.
    name = dependency["name"]
    if name == "gpui" or name.startswith("gpui-") or name == "github-copilot-sdk":
        kind = dependency["kind"] or "normal"
        return (f"{package['name']} -> {name} ({kind}): "
                 f"{directory.name} permits no GPUI or agent SDK dependencies")
    return None


def test_support_violation(package: dict, dependency: dict,
                           source: tuple[str, str | None]) -> str | None:
    """Offline client fixtures are dev-only, except as the facade's optional feature."""
    if dependency["name"] != "subspace-client-test-support" or dependency["kind"] == "dev":
        return None
    if source[0] == "facade" and dependency.get("optional"):
        return None
    return f"{package['name']}: offline client fixtures are permitted only as dev-dependencies"


def dependency_violation(directory: Path, package: dict, dependency: dict,
                         source: tuple[str, str | None]) -> str | None:
    """Describe forbidden package directions and agent-tools' headless boundary."""
    failure = (test_support_violation(package, dependency, source)
               or headless_dependency_violation(directory, package, dependency))
    if failure is not None or "path" not in dependency:
        return failure
    target_dir = Path(dependency["path"]).resolve()
    kind = dependency["kind"] or "normal"
    if not target_dir.is_relative_to(ROOT):
        return (f"{package['name']} -> {dependency['name']} ({kind}): "
                "a path dependency outside the Framework is forbidden")
    target = group(target_dir)
    if allowed_package(directory, target_dir):
        return None
    rule = ", ".join(sorted(ALLOWED.get(source[0], set())))
    if source[0] == "spaces":
        rule += ", own board-owned crates only"
    restriction = ("package ownership boundary" if allowed(source, target)
                   else f"{source[0]} may depend on {rule}")
    return (f"{package['name']} -> {dependency['name']} ({kind}): "
            f"{source[0]} -> {target[0]} forbidden; {restriction}")


def package_location_violations(directory: Path, package: dict) -> list[str]:
    """Keep feature-owned package identities at their existing boundaries."""
    failures = []
    if package["name"] == "subspace-agent" and directory != ROOT / "runtimes/agent":
        failures.append(f"{package['name']}: runtime implementation must live in runtimes/agent")
    if package["name"] == "subspace-search-ui" and directory != ROOT / "host/search-ui":
        failures.append(f"{package['name']}: search UI implementation must live in host/search-ui")
    return failures


def violations(all_packages: dict[Path, dict]) -> list[str]:
    """Collect package-location, dependency and source-reference policy failures."""
    failures = []
    for directory, package in all_packages.items():
        failures.extend(package_location_violations(directory, package))
        source = group(directory)
        for dependency in package["dependencies"]:
            failure = dependency_violation(directory, package, dependency, source)
            if failure is not None:
                failures.append(failure)
    return failures + architecture_source.violations(ROOT, all_packages, group, allowed_package)


def main() -> int:
    all_packages = packages()
    failures = violations(all_packages)
    if failures:
        print("Architecture check failed:\n" + "\n".join(sorted(set(failures))),
              file=sys.stderr)
        return 1
    print(f"Framework architecture check passed for {len(all_packages)} packages "
          "(layer directions for normal/build/dev edges, headless services, "
          "source includes and source rules).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
