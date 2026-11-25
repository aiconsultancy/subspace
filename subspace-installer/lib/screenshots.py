"""The screenshot tools every fleet VM gets in its base install, whatever its toolchains: agents always need to
take screenshots, of web pages and of desktop apps.

- A virtual display: Xvfb (with xvfb-run and xauth) and working keyboard maps (x11-xkb-utils, whose xkbcomp
  Xvfb needs to start, and xkb-data), and basic fonts.
- A headless browser: Playwright's Chromium (and its headless shell), at the revision the pinned Playwright
  names, installed by root into /opt/subspace-fleet/browsers, with the browser's shared libraries from the
  distribution's packages (never Playwright's --with-deps).
- The Playwright command line, /opt/subspace-fleet/bin/playwright on every officer's and crew member's PATH: the
  pinned Playwright (/opt/subspace-fleet/playwright, installed with npm ci from a lockfile, so every package is
  checked against its integrity hash) run by a pinned Node in the shared toolchain store, pointed at those
  browsers. Officers and crew have no outbound network to download a browser, so it never does.
- The mermaid-render command on the same PATH, using a pinned Mermaid and the same Chromium to export local
  diagram definitions as SVG or PNG.

Every version is pinned in lib/screenshots/package.json (with package-lock.json beside it), and nowhere else.
"screenshots": false in fleet.json turns it off.
"""

import json
import os
from pathlib import Path
import stat
import struct
import subprocess
import tempfile

import toolchains


APP = Path("/opt/subspace-fleet")
# The pinned versions, beside this file: the installer's copy (APP/lib/screenshots) or the payload's.
PINS = Path(__file__).resolve().parent / "screenshots"
PLAYWRIGHT = APP / "playwright"
BROWSERS = APP / "browsers"
WRAPPER = APP / "bin/playwright"
MERMAID_WRAPPER = APP / "bin/mermaid-render"
PNG = b"\x89PNG\r\n\x1a\n"
# A local page, so the check needs no network.
PAGE = ("data:text/html,<title>Subspace</title><body style='font-family:sans-serif'><h1>Subspace screenshot check"
        "</h1><p>Fonts: Liberation, DejaVu, emoji &#x1F6F0;</p></body>")
DIAGRAM = "flowchart LR\n  A[Draft] --> B[Review]\n"


def pins(directory=PINS):
    """The pinned versions and packages, from lib/screenshots/package.json."""
    data = json.loads((Path(directory) / "package.json").read_text())
    own = data["subspace"]
    return {**data["dependencies"], "node": own["node"],
            "chromiumRevision": own["chromiumRevision"], "chromiumVersion": own["chromiumVersion"],
            "packages": own["packages"]}


def release(path="/etc/os-release"):
    """The distribution and version, as ubuntu:24.04, or None."""
    try:
        fields = dict(line.split("=", 1) for line in Path(path).read_text().splitlines() if "=" in line)
    except OSError:
        return None
    name, version = (fields.get(key, "").strip().strip('"') for key in ("ID", "VERSION_ID"))
    return f"{name}:{version}" if name and version else None


def packages(pinned, distribution):
    """The distribution packages the tools need: the common ones, then those named differently by distribution
    (Ubuntu 24.04's t64 libraries)."""
    return [*pinned["packages"]["common"], *pinned["packages"].get(distribution or "", [])]


def node(pinned):
    return toolchains.SHARED / "installs/node" / pinned["node"] / "bin/node"


def cli():
    return PLAYWRIGHT / "node_modules/playwright/cli.js"


def browser_directories(pinned):
    revision = pinned["chromiumRevision"]
    return [BROWSERS / f"chromium-{revision}", BROWSERS / f"chromium_headless_shell-{revision}"]


def installed_version(package):
    try:
        return json.loads((PLAYWRIGHT / "node_modules" / package / "package.json").read_text())["version"]
    except (OSError, ValueError, KeyError):
        return None


def state(pinned):
    """What is installed already."""
    return {"node": node(pinned).is_file(), "playwright": installed_version("playwright") == pinned["playwright"],
            "mermaid": installed_version("mermaid") == pinned["mermaid"] and MERMAID_WRAPPER.is_file(),
            "chromium": all(path.is_dir() for path in browser_directories(pinned))}


def installed_packages(names):
    """The packages among names that dpkg has installed (one query, from /)."""
    if not names:
        return set()
    try:
        result = subprocess.run(["dpkg-query", "-W", "-f", "${Package} ${db:Status-Status}\n", *names],
                                capture_output=True, text=True, check=False, timeout=60, cwd="/")
    except (OSError, subprocess.TimeoutExpired):
        return set()
    return {line.split()[0] for line in result.stdout.splitlines() if line.split()[1:] == ["installed"]}


def plan_lines(enabled, pinned, distribution, present=None, done=None):
    """--dry-run: the screenshot tools, and what this run installs of them. present is the set of installed
    packages and done the state(), each looked up when absent."""
    if not enabled:
        return ["Screenshots: off (\"screenshots\": false in fleet.json); the installer adds nothing for them and "
                "leaves in place what an earlier run installed. " + remove_hint()]
    wanted = packages(pinned, distribution)
    present = installed_packages(wanted) if present is None else present
    done = state(pinned) if done is None else done
    lines = [f"Screenshots (base install; \"screenshots\": false in fleet.json turns them off): Xvfb with keyboard "
             f"maps and fonts, and Playwright {pinned['playwright']} with its Chromium {pinned['chromiumVersion']} "
             f"(revision {pinned['chromiumRevision']}) in {BROWSERS}, run by Node {pinned['node']}, as {WRAPPER} for "
             f"officers and crew. Mermaid {pinned['mermaid']} uses the same Chromium via {MERMAID_WRAPPER}."]
    missing = [name for name in wanted if name not in present]
    parts = ([f"{len(missing)} of {len(wanted)} packages ({', '.join(missing)})"] if missing else []) + [
        label for key, label in (("node", f"Node {pinned['node']}"), ("playwright", f"Playwright {pinned['playwright']}"),
                                 ("mermaid", f"Mermaid {pinned['mermaid']}"),
                                 ("chromium", "Chromium")) if not done[key]]
    lines.append("  This run installs: " + "; ".join(parts) + "." if parts else "  Installed already.")
    if distribution not in pinned["packages"]:
        lines.append(f"  This installer has no package list for {distribution or 'this distribution'}: it installs "
                     "the common packages only.")
    return lines


def wrapper_text(pinned, entrypoint):
    return ("#!/bin/sh\n# Written by the Subspace fleet installer. Use the shared Chromium.\n"
            f"export PLAYWRIGHT_BROWSERS_PATH={BROWSERS}\nexport PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1\n"
            f"exec {node(pinned)} {entrypoint} \"$@\"\n")


def write_file(path, text, mode):
    """Write a root-owned file atomically; unchanged contents are left alone."""
    if path.is_file() and not path.is_symlink() and path.read_text() == text and stat.S_IMODE(path.stat().st_mode) == mode:
        return
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False) as stream:
        os.fchmod(stream.fileno(), mode)
        stream.write(text)
    os.replace(stream.name, path)


def install_packages(pinned, manifest, lock):
    """Download into a staging directory so a failed npm run leaves the installed tools usable."""
    modules = PLAYWRIGHT / "node_modules"
    with tempfile.TemporaryDirectory(prefix=".npm-", dir=PLAYWRIGHT) as temporary:
        stage = Path(temporary)
        write_file(stage / "package.json", manifest, 0o644)
        write_file(stage / "package-lock.json", lock, 0o644)
        subprocess.run([str(node(pinned).parent / "npm"), "ci", "--ignore-scripts", "--omit=dev", "--no-audit",
                        "--no-fund"], cwd=stage, check=True, env={
                            "PATH": f"{node(pinned).parent}:{toolchains.BASE_PATH}", "HOME": "/root",
                            "npm_config_cache": str(stage / "cache"), "npm_config_update_notifier": "false"})
        backup = stage / "previous-node-modules"
        if modules.exists():
            modules.rename(backup)
        try:
            (stage / modules.name).rename(modules)
        except OSError:
            if backup.exists():
                backup.rename(modules)
            raise
    write_file(PLAYWRIGHT / "package.json", manifest, 0o644)
    write_file(PLAYWRIGHT / "package-lock.json", lock, 0o644)


def install(pinned, root_environment, say=print):
    """As root, install what the pins name that is missing: Node into the shared toolchain store (mise, explicit
    node@VERSION), Playwright and Mermaid with npm ci from the lockfile (scripts off), and Playwright's Chromium
    into BROWSERS; then the wrappers. A failure is reported and never stops the install."""
    previous = os.umask(0o022)
    try:
        BROWSERS.mkdir(mode=0o755, exist_ok=True)
        PLAYWRIGHT.mkdir(mode=0o755, exist_ok=True)
        if not node(pinned).is_file():
            subprocess.run([str(toolchains.MISE), "install", f"node@{pinned['node']}"], cwd=APP,
                           env=root_environment, check=True)
        lock = (PINS / "package-lock.json").read_text()
        if any(installed_version(package) != pinned[package] for package in ("playwright", "mermaid")) or \
                not (PLAYWRIGHT / "package-lock.json").is_file() or \
                (PLAYWRIGHT / "package-lock.json").read_text() != lock:
            install_packages(pinned, (PINS / "package.json").read_text(), lock)
        # Playwright skips a browser it has installed already, and removes the browsers of an earlier pin.
        subprocess.run([str(node(pinned)), str(cli()), "install", "chromium"], cwd=PLAYWRIGHT, check=True, env={
            "PATH": f"{node(pinned).parent}:{toolchains.BASE_PATH}", "HOME": "/root",
            "PLAYWRIGHT_BROWSERS_PATH": str(BROWSERS)})
        if missing := [str(path) for path in browser_directories(pinned) if not path.is_dir()]:
            return [f"Playwright {pinned['playwright']} installed no Chromium revision {pinned['chromiumRevision']} "
                    f"({', '.join(missing)} missing): set chromiumRevision in lib/screenshots/package.json to the "
                    "revision that Playwright names"]
        renderer = PLAYWRIGHT / "mermaid-render.cjs"
        write_file(renderer, (PINS / renderer.name).read_text(), 0o644)
        write_file(WRAPPER, wrapper_text(pinned, cli()), 0o755)
        write_file(MERMAID_WRAPPER, wrapper_text(pinned, renderer), 0o755)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        return [f"{type(error).__name__}: {error}"]
    finally:
        os.umask(previous)
    return []


def png_size(path):
    """A PNG's width and height, read without following a symbolic link; None for anything else."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except OSError:
        return None
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            return None
        head = stream.read(24)
    if len(head) < 24 or not head.startswith(PNG) or head[12:16] != b"IHDR":
        return None
    return struct.unpack(">II", head[16:24])


def check(user, home, environment, timeout=180):
    """Take a page screenshot under Xvfb and render a Mermaid PNG as an officer with its tool environment.
    Returns (passed, report)."""
    scratch = subprocess.run(["runuser", "-u", user, "--", "mktemp", "-d", f"/tmp/{user}-screenshot.XXXXXX"],
                             capture_output=True, text=True, check=False)
    if scratch.returncode:
        return False, "could not make a private temporary directory"
    temporary = scratch.stdout.strip()
    shot = f"{temporary}/check.png"
    diagram = f"{temporary}/diagram.png"
    officer = ["runuser", "-u", user, "--", *toolchains.clean_environment(
        user, home, {**environment, "TMPDIR": temporary})]
    checks = [
        ("Playwright", ["xvfb-run", "--auto-servernum", "--server-args=-screen 0 1280x800x24",
                        str(WRAPPER), "screenshot", "--browser", "chromium", "--viewport-size", "800,600",
                        PAGE, shot], None),
        ("Mermaid", [str(MERMAID_WRAPPER), "-", diagram], DIAGRAM),
    ]
    try:
        sizes = []
        for name, command, source in checks:
            try:
                result = subprocess.run([*officer, *command], input=source, cwd=home, capture_output=True,
                                        text=True, timeout=timeout, check=False)
            except subprocess.TimeoutExpired:
                return False, f"the {name} screenshot did not finish within {timeout} seconds"
            if result.returncode:
                tail = (result.stderr or result.stdout).strip().splitlines()[-3:]
                return False, f"{name} screenshot failed ({result.returncode}): " + " / ".join(tail)
            size = png_size(command[-1])
            if not size or min(size) <= 0:
                return False, f"{name} wrote no valid PNG"
            sizes.append(f"{size[0]}x{size[1]}")
        return True, f"a {sizes[0]} PNG of a local page under Xvfb and a {sizes[1]} Mermaid PNG"
    finally:
        subprocess.run(["runuser", "-u", user, "--", "rm", "-rf", "--", temporary], check=False)


def remove_hint():
    return (f"To remove what an earlier run installed: sudo rm -rf {PLAYWRIGHT} {BROWSERS} {WRAPPER} {MERMAID_WRAPPER} (the "
            "distribution packages stay).")
