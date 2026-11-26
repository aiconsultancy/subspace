"""Check Subspace Desktop's dependency direction and source includes.

Products compose Spaces; Spaces, clients and the terminal crates reach the
Framework only through the `subspace-framework` crate, without its host
features or modules. Nothing reaches outside this folder except the
documented Framework dependency and vendored patches.
"""

import json
import subprocess
import sys
import tomllib
from pathlib import Path

import architecture_source

ROOT = Path(__file__).resolve().parents[1]
# Manifests read by `cargo metadata`; the terminal crates are excluded Zig
# workspaces, but --no-deps needs no Zig.
MANIFESTS = ("Cargo.toml", "terminal/runtime/Cargo.toml", "terminal/panel/Cargo.toml")
ALLOWED = {
    "spaces": {"clients"},
    "clients": {"clients"},
    "terminal": {"terminal"},
}
FRAMEWORK = "subspace-framework"
FRAMEWORK_ROOT = ROOT.parent / FRAMEWORK
# Facade features for app builders only. `copilot` chooses whether the app
# builds and bundles the Copilot SDK, which is the product's build option.
HOST_FEATURES = {"host", "capture", "copilot", "gpui-kit-text-patch"}
# The Framework's patches, or this folder's own copies once
# the two folders are separate repositories (docs/publishing.md in the
# Framework).
PATCHES = {
    name: (FRAMEWORK_ROOT / "vendor" / name, ROOT / "vendor" / name)
    for name in ("gpui-base", "markdown")
}


def group(directory: Path) -> tuple[str, str | None]:
    parts = directory.relative_to(ROOT).parts
    if len(parts) == 2 and parts[0] in {"products", "clients", "spaces", "terminal"}:
        return parts[0], parts[1] if parts[0] == "spaces" else None
    if len(parts) == 4 and parts[0] == "spaces" and parts[2] == "crates":
        return "spaces", parts[1]
    raise ValueError(f"Unclassified internal package directory: {directory}")


def packages() -> dict[Path, dict]:
    result = {}
    # --no-deps includes every declared dependency, including optional,
    # target-specific, build and dev edges, without fetching or building Zig.
    for manifest in MANIFESTS:
        output = subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps",
             "--locked", "--manifest-path", str(ROOT / manifest)],
            cwd=ROOT, text=True,
        )
        for package in json.loads(output)["packages"]:
            result[Path(package["manifest_path"]).resolve().parent] = package
    return result


def allowed(source: tuple[str, str | None], target: tuple[str, str | None]) -> bool:
    layer, owner = source
    target_layer, target_owner = target
    return (layer == "products"
            or target_layer in ALLOWED[layer]
            or layer == target_layer == "spaces" and owner == target_owner)


def allowed_package(source: Path, target: Path) -> bool:
    """Apply layer rules to Cargo edges and source includes alike."""
    return allowed(group(source), group(target))


def framework_violation(package: dict, dependency: dict,
                        source: tuple[str, str | None]) -> str | None:
    """The one Framework crate: at its documented path, without host features
    outside products, and with test support only for tests."""
    kind = dependency["kind"] or "normal"
    edge = f"{package['name']} -> {FRAMEWORK} ({kind})"
    path = dependency.get("path")
    if path is not None and Path(path).resolve() != FRAMEWORK_ROOT:
        return f"{edge}: the Framework must be ../{FRAMEWORK} or crates.io"
    features = set(dependency["features"])
    if source[0] != "products" and features & HOST_FEATURES:
        return f"{edge}: only products may enable {', '.join(sorted(features & HOST_FEATURES))}"
    if kind != "dev" and "test-support" in features:
        return f"{edge}: test-support is permitted only on dev-dependencies"
    return None


def internal_violation(directory: Path, package: dict, dependency: dict,
                       source: tuple[str, str | None]) -> str | None:
    """Path edges stay inside this folder and follow the layer directions."""
    target_dir = Path(dependency["path"]).resolve()
    kind = dependency["kind"] or "normal"
    if not target_dir.is_relative_to(ROOT):
        return (f"{package['name']} -> {dependency['name']} ({kind}): "
                "a path dependency outside Subspace Desktop is forbidden")
    target = group(target_dir)
    if allowed_package(directory, target_dir):
        return None
    rule = ", ".join(sorted(ALLOWED.get(source[0], set())) + [FRAMEWORK])
    if source[0] == "spaces":
        rule += ", own Space-owned crates only"
    restriction = ("package ownership boundary" if allowed(source, target)
                   else f"{source[0]} may depend on {rule}")
    return (f"{package['name']} -> {dependency['name']} ({kind}): "
            f"{source[0]} -> {target[0]} forbidden; {restriction}")


def dependency_violation(directory: Path, package: dict, dependency: dict,
                         source: tuple[str, str | None], local: set[str]) -> str | None:
    """Describe forbidden edges: Framework crates by name, paths by layer."""
    name = dependency["name"]
    if name == FRAMEWORK:
        return framework_violation(package, dependency, source)
    if name.startswith("subspace-") and name not in local:
        kind = dependency["kind"] or "normal"
        return (f"{package['name']} -> {name} ({kind}): name the Framework only "
                f"through {FRAMEWORK}")
    if "path" not in dependency:
        return None
    return internal_violation(directory, package, dependency, source)


def patch_violations(manifest: str) -> list[str]:
    """Only the documented Framework patches may be applied."""
    path = ROOT / manifest
    table = tomllib.loads(path.read_text()).get("patch", {}).get("crates-io", {})
    failures = []
    for name, entry in table.items():
        target = (path.parent / entry.get("path", "")).resolve()
        if target not in PATCHES.get(name, ()):
            failures.append(f"{manifest}: [patch.crates-io] {name} must be a documented "
                            "vendored patch at the Framework or desktop vendor path")
    return failures


def violations(all_packages: dict[Path, dict]) -> list[str]:
    """Collect dependency, patch and source-reference policy failures."""
    local = {package["name"] for package in all_packages.values()}
    failures = [failure for manifest in MANIFESTS for failure in patch_violations(manifest)]
    for directory, package in all_packages.items():
        source = group(directory)
        for dependency in package["dependencies"]:
            failure = dependency_violation(directory, package, dependency, source, local)
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
    print(f"Desktop architecture check passed for {len(all_packages)} packages "
          "including excluded terminals (normal/build/dev edges, the one Framework "
          "crate and its features, patches, source includes and source rules).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
