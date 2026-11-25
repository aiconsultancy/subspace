#!/usr/bin/python3
"""The fleet's toolchains: the toolchains and systemPackages settings, where each tool is installed, the
environment officers and crew run their tools in, and detection of what the fleet's repositories need.

Where things are installed:

- systemPackages: distribution packages, by apt as root, system-wide.
- toolchains, shared (the default): installed by root with mise, as explicit tool@version arguments, into the
  shared store /opt/subspace-fleet/toolchains, from /opt/subspace-fleet and never from an agent's checkout. Every
  officer and crew member sees them: the shared store is their mise system data directory.
- toolchains, per user (by default rust, and the tools of backends that run third-party code while they install,
  such as cargo:, npm: or asdf:; or "install": "user"): installed as each officer's Linux user, as explicit
  tool@version arguments from its home, into its own mise data directory. Crew run as their captain's user, so
  they share their captain's.

Detection reads the repositories' files as data. It never runs mise, a package manager, Git or anything else
from a repository, and never follows a symbolic link.
"""

import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess
import sys


APP = Path("/opt/subspace-fleet")
ETC = Path("/etc/subspace-fleet")
STATE = Path("/var/lib/subspace-fleet")
# The shared store: root's mise data directory, and every officer's mise system data directory.
SHARED = APP / "toolchains"
# The fleet's mise configuration, rendered from fleet.json's toolchains: every officer's system configuration.
SHARED_CONFIG = ETC / "toolchains.toml"
# An empty directory, so no /etc/mise configuration reaches root's installs or the officers.
SYSTEM_CONFIG_DIR = SHARED / "system-config"
# Each officer's own mise data directory, under its home.
USER_DATA = ".local/share/mise"
MISE = APP / "bin/mise"
BASE_PATH = "/usr/local/bin:/usr/bin:/bin"
DEFAULT_TOOLS = {"node": "24"}

# mise backends a toolchain may name. Each installs from its own registry or releases; none runs a command
# from fleet.json. http: (arbitrary URLs) and spm: are left out.
BACKENDS = ("aqua", "asdf", "cargo", "conda", "dotnet", "gem", "github", "gitlab", "go", "npm", "pipx", "ubi", "vfox")
# Installed per user by default: rustup keeps its toolchains in the user's home and installs a repository's
# pinned toolchain and components on first use, which a read-only shared store cannot. .NET likewise: the dotnet
# host finds SDKs in one root only (DOTNET_ROOT), so an SDK a repository's global.json pins that the fleet has not
# got installs there on first use, which needs a root the officer can write.
USER_TOOLS = {"rust", "dotnet"}
# Backends that run third-party code while they install: cargo: (build scripts), npm: (lifecycle scripts), gem:
# (native extensions), pipx: (package builds), asdf: and vfox: (plugins are scripts), conda: (packages may carry
# link scripts). Their tools install as each officer's user unless the operator marks one "install": "shared".
USER_BACKENDS = {"cargo", "npm", "gem", "pipx", "asdf", "vfox", "conda"}
# Root's installs never use these backends, so a plain tool name that mise's registry resolves to one of them
# (pypi: is pipx's) fails as root instead of running its code; the operator installs it per user instead.
CODE_BACKENDS = USER_BACKENDS | {"pypi"}
PLAIN_TOOL = r"[a-z0-9][a-z0-9._-]{0,63}"
BACKEND_TOOL = rf"(?:{'|'.join(BACKENDS)}):@?[A-Za-z0-9][A-Za-z0-9._+-]*(?:/[A-Za-z0-9._+-]+){{0,6}}"
VERSION = r"[A-Za-z0-9][A-Za-z0-9._+-]{0,63}"
# The mise tool options a toolchain may carry, each checked for what it is for: they reach the configuration root's
# mise reads, and repositories can propose them. Options that run commands or set an install's environment, such as
# postinstall or install_env, are refused: root installs the shared tools.
NAME = r"[A-Za-z0-9][A-Za-z0-9._+-]{0,99}"
NAMES = rf"{NAME}(?:,{NAME}){{0,49}}"
FEATURE = r"[A-Za-z0-9_][A-Za-z0-9_.+-]{0,63}(?:/[A-Za-z0-9_][A-Za-z0-9_.+-]{0,63})?"
# Relative, inside the tool's install directory: no leading /, and no . or .. component.
SUBDIRECTORY = r"[A-Za-z0-9_][A-Za-z0-9_.+-]{0,99}(?:/[A-Za-z0-9_][A-Za-z0-9_.+-]{0,99}){0,7}"
OPTIONS = {
    # Lists of names, comma-separated: Rust components and targets, executables to link, cargo features.
    "components": (NAMES, "names separated by commas"),
    "targets": (NAMES, "names separated by commas"),
    "filter_bins": (NAMES, "executable names separated by commas"),
    "features": (rf"{FEATURE}(?:,{FEATURE}){{0,49}}", "cargo features separated by commas"),
    # An executable inside the tool's install directory: a file name, never a path.
    "bin": (NAME, "an executable's file name, not a path"),
    "exe": (NAME, "an executable's file name, not a path"),
    "rename_exe": (NAME, "an executable's file name, not a path"),
    # A directory inside the tool's install directory.
    "bin_path": (SUBDIRECTORY, "a directory inside the tool's install directory: relative, without . or .."),
    # How a release asset or a version is chosen: matched against names, never a path or an address.
    "asset_pattern": (r"[A-Za-z0-9._*?+-]{1,100}", "a release asset's name, with * or ?"),
    "matching": (NAME, "a word in a release asset's name"),
    "version_prefix": (r"[A-Za-z0-9._+-]{1,40}", "the text before the version in a release's tag"),
    "profile": ("minimal|default|complete", "minimal, default or complete"),
    "channel": (NAME, "a channel's name, not an address"),
    "crate": (r"[A-Za-z0-9_-]{1,64}", "a crate's name"),
    "runtime": (NAME, "a name"),
    "locked": (bool, "true or false"),
    "default-features": (bool, "true or false"),
}
OPTION_KEYS = set(OPTIONS)
LIST_OPTIONS = {"components", "targets", "filter_bins", "features"}


def option_valid(key, value):
    """Whether a value is what the option is for (OPTIONS)."""
    rule = OPTIONS[key][0]
    if rule is bool:
        return type(value) is bool
    return isinstance(value, str) and re.fullmatch(rule, value) is not None
# A Debian package name (Debian Policy 5.6.1: lower case letters, digits, plus, minus and full stops) as apt
# installs it, and only installs it: never ending in a minus, which apt-get takes for "remove this package", and
# with no version (=), release (/), architecture (:) or pattern. A trailing plus is allowed, as in g++: apt-get
# takes a package of that exact name first, and otherwise reads the plus as "install".
PACKAGE = r"[a-z0-9][a-z0-9+.-]{0,98}[a-z0-9+.]"
MAX_VERSIONS = 8
MAX_PACKAGES = 200
MAX_TOOLS = 100


class Invalid(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise Invalid(message)


# Configuration -------------------------------------------------------------------------------------------------

def tool_name(value):
    """A mise tool specification: a plain name such as node, or backend:name such as cargo:cargo-nextest."""
    require(isinstance(value, str) and len(value) <= 200
            and (re.fullmatch(PLAIN_TOOL, value) or re.fullmatch(BACKEND_TOOL, value))
            and not any(part.strip(".") == "" for part in value.partition(":")[2].split("/") if ":" in value),
            f"Invalid toolchain {value!r}: use a mise tool name such as node, or backend:name with one of the "
            f"backends {', '.join(BACKENDS)}, such as cargo:cargo-nextest or github:example/tool.")
    return value


def tool_version(value, spec):
    require(isinstance(value, str) and re.fullmatch(VERSION, value),
            f"Invalid version {value!r} for toolchain {spec}: use a version or channel such as 24, 1.99.0, stable "
            "or latest.")
    return value


def default_install(spec):
    return "user" if spec in USER_TOOLS or spec.partition(":")[0] in USER_BACKENDS and ":" in spec else "shared"


def tool_options(value, spec):
    require(isinstance(value, dict) and len(value) <= 16, f"Toolchain {spec}'s options must be an object.")
    options = {}
    for key, option in value.items():
        require(key in OPTION_KEYS, f"Toolchain {spec} has the option {key!r}, which the installer does not pass to "
                f"mise. It takes {', '.join(sorted(OPTION_KEYS))}; options that run commands, such as postinstall, "
                "are refused because root installs the shared tools.")
        require(option_valid(key, option), f"Invalid value for toolchain {spec}'s option {key}: use "
                f"{OPTIONS[key][1]}.")
        options[key] = option
    return options


def tool_entry(spec, value):
    """One toolchains entry as {"versions", "install", "options"}, or None for false (not installed)."""
    if value is False:
        return None
    if isinstance(value, (str, list)):
        value = {"version": value}
    require(isinstance(value, dict) and value.keys() <= {"version", "install", "options"} and "version" in value,
            f"Toolchain {spec} takes a version, a list of versions, false, or an object with version and optional "
            "install and options.")
    versions = value["version"]
    if isinstance(versions, str):
        versions = [versions]
    require(isinstance(versions, list) and 1 <= len(versions) <= MAX_VERSIONS,
            f"Toolchain {spec} takes 1 to {MAX_VERSIONS} versions.")
    versions = [tool_version(version, spec) for version in versions]
    require(len(set(versions)) == len(versions), f"Toolchain {spec} lists a version twice.")
    install = value.get("install", default_install(spec))
    require(install in ("shared", "user"), f"Toolchain {spec}'s install is shared or user.")
    return {"versions": versions, "install": install, "options": tool_options(value.get("options", {}), spec)}


def toolchain_config(value):
    """fleet.json's toolchains: a mise tool specification for each key, with a version, a list of versions or an
    object; Node 24 unless node is false. Existing fleets' {"node": "24", "go": "1.26"} keep their meaning."""
    require(isinstance(value, dict) and len(value) <= MAX_TOOLS,
            f"toolchains must be an object of at most {MAX_TOOLS} tools.")
    result = {}
    for spec, entry in {**DEFAULT_TOOLS, **value}.items():
        entry = tool_entry(tool_name(spec), entry)
        if entry:
            result[spec] = entry
    return result


def package_config(value):
    """fleet.json's systemPackages: Debian package names, installed with apt."""
    require(isinstance(value, list) and len(value) <= MAX_PACKAGES,
            f"systemPackages must be an array of at most {MAX_PACKAGES} package names.")
    for name in value:
        require(isinstance(name, str) and re.fullmatch(PACKAGE, name),
                f"Invalid system package {name!r}: use a Debian package name such as libssl-dev, with no version, "
                "release, pattern or trailing minus (which apt reads as a removal).")
    require(len(set(value)) == len(value), "systemPackages lists a package twice.")
    return list(value)


def toml_key(spec):
    return spec if re.fullmatch(r"[A-Za-z0-9_-]+", spec) else json.dumps(spec)


def toml_scalar(value):
    if type(value) is bool:
        return "true" if value else "false"
    return str(value) if type(value) is int else json.dumps(value)


def toml_value(entry):
    def one(version):
        if not entry["options"]:
            return json.dumps(version)
        pairs = [("version", version), *sorted(entry["options"].items())]
        return "{ " + ", ".join(f"{toml_key(key)} = {toml_scalar(value)}" for key, value in pairs) + " }"
    values = [one(version) for version in entry["versions"]]
    return values[0] if len(values) == 1 else "[" + ", ".join(values) + "]"


def mise_config(tools):
    """The fleet's mise configuration, ETC/toolchains.toml: every toolchain, shared and per user, so each one is
    the default for every officer and crew member where a repository pins nothing else."""
    lines = ["# Written by the Subspace fleet installer from fleet.json's toolchains. Edit fleet.json, not this file.",
             "[tools]"]
    lines += [f"{toml_key(spec)} = {toml_value(entry)}" for spec, entry in tools.items()]
    return "\n".join(lines) + "\n"


def requests(tools, install):
    """Explicit tool@version arguments for one place, plain tool names first, so a backend's tool (cargo:, npm:)
    comes after the language it builds with."""
    rows = [(":" in spec, spec, version) for spec, entry in tools.items() if entry["install"] == install
            for version in entry["versions"]]
    return [f"{spec}@{version}" for _, spec, version in sorted(rows, key=lambda row: row[0])]


def user_data(home):
    return Path(home) / USER_DATA


def dotnet_root(tools, home=None):
    """Where the .NET SDKs are, for an officer (home) or for root (no home): the officer's own root for a per-user
    .NET (the default), the shared store's for a shared one; None without .NET, or for root with a per-user one."""
    entry = tools.get("dotnet")
    if not entry:
        return None
    if entry["install"] == "user":
        return str(user_data(home) / "dotnet-root") if home else None
    return str(SHARED / "dotnet-root")


def officer_environment(tools, user, home, trusted):
    """The environment an officer's engine, and the crew on it, run their tools in.

    A per-user mise data directory with its shims first on PATH, then the shared store's shims; the shared store
    as mise's system data directory and the fleet's configuration as its system configuration; mise configuration
    trusted in the officer's workspace and worktrees; a private TMPDIR under /tmp (the engine creates it, 0700),
    since tests that drive Git fail with a TMPDIR inside the home; DOTNET_ROOT when a .NET SDK is configured.
    """
    data = user_data(home)
    environment = {
        "PATH": f"{data}/shims:{SHARED}/shims:{APP}/bin:{BASE_PATH}",
        "MISE_DATA_DIR": str(data),
        "MISE_SYSTEM_DATA_DIR": str(SHARED),
        "MISE_SYSTEM_CONFIG_DIR": str(SYSTEM_CONFIG_DIR),
        "MISE_SYSTEM_CONFIG_FILE": str(SHARED_CONFIG),
        "MISE_TRUSTED_CONFIG_PATHS": ":".join(str(path) for path in trusted),
        "TMPDIR": f"/tmp/{user}",
    }
    if root := dotnet_root(tools, home):
        environment["DOTNET_ROOT"] = root
    return environment


def root_environment(tools):
    """Root's environment for the shared installs: the shared store as its data directory, the fleet's
    configuration as its only configuration, no project configuration (it runs from /opt/subspace-fleet), and the
    backends that run third-party code at install disabled, except those of tools the operator marked shared."""
    shared = {spec.partition(":")[0] for spec, entry in tools.items() if ":" in spec and entry["install"] == "shared"}
    if "pipx" in shared:
        shared.add("pypi")
    environment = {
        "PATH": f"{APP}/bin:{SHARED}/shims:{BASE_PATH}", "HOME": "/root",
        "MISE_DATA_DIR": str(SHARED), "MISE_GLOBAL_CONFIG_FILE": str(SHARED_CONFIG),
        "MISE_SYSTEM_CONFIG_DIR": str(SYSTEM_CONFIG_DIR), "MISE_YES": "1", "MISE_LOCKFILE": "0",
        "MISE_SELF_UPDATE_AVAILABLE": "0", "MISE_DISABLE_BACKENDS": ",".join(sorted(CODE_BACKENDS - shared)),
    }
    if root := dotnet_root(tools):
        environment["DOTNET_ROOT"] = root
    return environment


def clean_environment(user, home, environment):
    """/usr/bin/env arguments that run a command as an officer with only its tool environment, as its engine
    would, and nothing of the caller's (root's) environment."""
    return ["/usr/bin/env", "-i", f"HOME={home}", f"USER={user}", f"LOGNAME={user}", "LANG=C.UTF-8",
            *(f"{key}={value}" for key, value in environment.items())]


def private_tmp(path):
    """Make the officer's TMPDIR, owned by the running user with mode 0700, and refuse one that is not."""
    try:
        os.mkdir(path, 0o700)
    except FileExistsError:
        pass
    info = os.lstat(path)
    require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid(),
            f"{path} exists but is not a directory of this user; remove it.")
    if stat.S_IMODE(info.st_mode) != 0o700:
        os.chmod(path, 0o700)


# What is installed ---------------------------------------------------------------------------------------------

def tool_dir(spec):
    """mise's installs directory for a tool: its name in kebab case, as mise's BackendArg names it."""
    words = []
    for part in re.split(r"[^A-Za-z0-9]+", spec):
        words += re.findall(r"[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z0-9]+|[A-Z0-9]+", part)
    return "-".join(word.lower() for word in words)


def installed(data, spec, version):
    try:
        return (Path(data) / "installs" / tool_dir(spec) / version).is_dir()
    except OSError:
        return False


def package_state(name):
    """installed, available, missing (in no apt source) or unknown."""
    try:
        # From /, never the caller's working directory.
        status = subprocess.run(["dpkg-query", "-W", "-f", "${db:Status-Status}", name], capture_output=True,
                                text=True, check=False, timeout=30, cwd="/")
        if status.returncode == 0 and status.stdout.strip() == "installed":
            return "installed"
        # Package names apt knows that start with this one; the name must be among them exactly, as the
        # installer requires (apt-get would read an unknown name as a pattern).
        known = subprocess.run(["apt-cache", "pkgnames", name], capture_output=True, text=True, check=False,
                               timeout=30, cwd="/")
    except (OSError, subprocess.TimeoutExpired):
        return "unknown"
    if known.returncode:
        return "unknown"
    return "available" if name in known.stdout.split() else "missing"


def tool_state(spec, version, install, homes):
    """Where a tool version is installed: "installed", "not installed", "installed for some" or "unknown" (per-user
    stores in homes this user cannot read)."""
    if install == "shared":
        return "installed" if installed(SHARED, spec, version) else "not installed"
    states = []
    for home in homes:
        if os.path.lexists(home) and not os.access(home, os.R_OK | os.X_OK):
            return "unknown"
        states.append(installed(user_data(home), spec, version))
    if states and all(states):
        return "installed"
    return "installed for some" if any(states) else "not installed"


def describe(spec, entry, version):
    options = ", ".join(f"{key}={value}" for key, value in sorted(entry["options"].items()))
    return f"{spec} {version}" + (f" ({options})" if options else "")


def plan_lines(tools, packages, officers, previous=None, states=None):
    """--dry-run: what this run installs and where, what is installed already, and the officers' environment.

    officers maps each officer's Linux user to its home; previous is the installed fleet.json's toolchains and
    systemPackages, when readable; states maps package names to package_state (looked up when absent)."""
    homes = list(officers.values())
    lines = ["Toolchains (fleet.json toolchains and systemPackages):"]
    for install, label in (("shared", f"shared, installed by root into {SHARED}"),
                           ("user", f"per officer, installed as each officer's user into ~/{USER_DATA} "
                                    f"({', '.join(officers) or 'no officers'}); crew use their captain's")):
        rows = [f"{describe(spec, entry, version)} [{tool_state(spec, version, install, homes)}]"
                for spec, entry in tools.items() if entry["install"] == install for version in entry["versions"]]
        if rows:
            lines.append(f"  {label}: {'; '.join(rows)}.")
    if packages:
        states = states if states is not None else {name: package_state(name) for name in packages}
        lines.append("  system packages, installed by apt as root: "
                     + "; ".join(f"{name} [{states.get(name, 'unknown')}]" for name in packages) + ".")
        if missing := [name for name in packages if states.get(name) == "missing"]:
            lines.append(f"  The install stops: no apt source on this VM offers {', '.join(missing)}.")
    if previous is not None:
        before = {(spec, version) for spec, entry in previous[0].items() for version in entry["versions"]}
        after = {(spec, version) for spec, entry in tools.items() for version in entry["versions"]}
        if gone := sorted(before - after):
            lines.append("  No longer configured, and left installed: "
                         + ", ".join(f"{spec} {version}" for spec, version in gone) + ".")
        if dropped := [name for name in previous[1] if name not in packages]:
            lines.append(f"  System packages no longer listed, and left installed: {', '.join(dropped)}.")
    environment = ("  Officers and crew run their tools with their own mise data directory (~/" + USER_DATA
                   + ") and its shims first on PATH, then the shared store's; the shared store as mise's system "
                   "data directory; mise configuration trusted in their workspace and crew worktrees; and a "
                   "private TMPDIR, /tmp/<user> (0700)")
    if "dotnet" not in tools:
        lines.append(environment + ".")
    elif root := dotnet_root(tools):
        lines.append(environment + f"; DOTNET_ROOT {root} (shared: an SDK a repository pins that the fleet has not "
                     "got cannot install on first use).")
    else:
        lines.append(environment + f"; DOTNET_ROOT ~/{USER_DATA}/dotnet-root, each officer's own.")
    return lines


# Detection -----------------------------------------------------------------------------------------------------

CONFIDENCE = ("high", "medium", "low")
MAX_FILE = 1 << 20
MAX_FILES = 20000
MAX_DEPTH = 8
MAX_DOCS = 200
SKIP_DIRS = {".git", "node_modules", "vendor", "target", "dist", "out", "obj", ".venv", "venv", "__pycache__",
             ".next", ".nuxt", ".cache", "coverage", ".terraform", ".gradle", ".idea", ".turbo", "bower_components"}
ASDF_NAMES = {"nodejs": "node", "golang": "go", "dotnet-core": "dotnet"}
VERSION_FILES = {".node-version": "node", ".nvmrc": "node", ".terraform-version": "terraform",
                 ".opentofu-version": "opentofu", ".python-version": "python", ".ruby-version": "ruby",
                 ".go-version": "go", ".bun-version": "bun", ".java-version": "java",
                 ".terragrunt-version": "terragrunt", ".deno-version": "deno"}
MISE_FILE = re.compile(r"(?:^|/)(?:\.?mise(?:\.([a-z0-9_-]+))?\.toml|\.?mise/config\.toml|\.config/mise\.toml"
                       r"|\.config/mise/config\.toml|\.?(?:config/)?mise/conf\.d/[^/]+\.toml)$")
COMPOSE_FILES = {"compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"}
SCANNED_FILES = {".tool-versions", "rust-toolchain", "rust-toolchain.toml", "global.json", "package.json", "go.mod",
                 "pyproject.toml"}
WORKFLOW_FILE = re.compile(r"(?:.*/)?\.github/(?:workflows/[^/]+|actions/.+/action)\.ya?ml")
DOC_FILE = re.compile(r"(?:^|/)(?:README|INSTALL|INSTALLING|CONTRIBUTING|BUILDING|BUILD|DEVELOPMENT|HACKING)"
                      r"[^/]*\.md$|^docs?/.+\.md$", re.IGNORECASE)
# GitHub set-up actions: the tool each installs and the with: keys that name its version. A version belongs to
# the uses: of its own step, never to a neighbouring step's.
SETUP_ACTIONS = {
    "actions/setup-node": ("node", ("node-version",)),
    "actions/setup-python": ("python", ("python-version",)),
    "actions/setup-go": ("go", ("go-version",)),
    "actions/setup-dotnet": ("dotnet", ("dotnet-version",)),
    "actions/setup-java": ("java", ("java-version",)),
    "pnpm/action-setup": ("pnpm", ("version",)),
    "oven-sh/setup-bun": ("bun", ("bun-version",)),
    "denoland/setup-deno": ("deno", ("deno-version",)),
    "hashicorp/setup-terraform": ("terraform", ("terraform_version",)),
    "opentofu/setup-opentofu": ("opentofu", ("tofu_version",)),
    "ruby/setup-ruby": ("ruby", ("ruby-version",)),
    "astral-sh/setup-uv": ("uv", ("version",)),
    "azure/setup-kubectl": ("kubectl", ("version",)),
    "azure/setup-helm": ("helm", ("version",)),
    "dtolnay/rust-toolchain": ("rust", ("toolchain",)),
    "actions-rs/toolchain": ("rust", ("toolchain",)),
    "mlugg/setup-zig": ("zig", ("version",)),
    "goto-bus-stop/setup-zig": ("zig", ("version",)),
}
# Set-up actions whose with: version is not a tool's: mise-action's is mise's own, and the tools it installs come
# from the repository's mise files, which detection reads directly.
NOT_TOOLS = {"jdx/mise-action"}
DATABASES = re.compile(r"(?:^|/)(postgres|postgis|mysql|mariadb|mongo|redis|valkey|mssql|sqlserver|cockroach"
                       r"|clickhouse|elasticsearch|opensearch|cassandra|neo4j)", re.IGNORECASE)


class Finder:
    """Collects proposal items and what could not be determined, for one detection run."""

    def __init__(self):
        self.items = {}
        self.unknown = []

    def add(self, kind, name, version, confidence, source, options=None, constraint=None, note=None):
        key = (kind, name, version, json.dumps(options or {}, sort_keys=True))
        item = self.items.setdefault(key, {"kind": kind, "name": name, "version": version,
                                           "options": options or {}, "confidence": confidence, "sources": []})
        if CONFIDENCE.index(confidence) < CONFIDENCE.index(item["confidence"]):
            item["confidence"] = confidence
        if constraint and "constraint" not in item:
            item["constraint"] = constraint
        if note and note not in item.setdefault("notes", []):
            item["notes"].append(note)
        if source not in item["sources"]:
            item["sources"].append(source)

    def cannot(self, source, reason):
        entry = {**source, "reason": reason}
        if entry not in self.unknown:
            self.unknown.append(entry)


def source_of(repository, rel, line, detail=""):
    detail = " ".join(str(detail).split())
    return {"repository": repository, "file": rel, "line": line, "detail": detail[:160]}


def read_at(directory_fd, name):
    """A regular file's text, opened relative to its directory's descriptor and never through a symbolic link, at
    most MAX_FILE bytes; None otherwise."""
    try:
        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory_fd)
    except OSError:
        return None
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_FILE:
            return None
        return stream.read(MAX_FILE + 1)[:MAX_FILE].decode("utf-8", "replace")


def real_directory(name, directory_fd):
    try:
        return stat.S_ISDIR(os.stat(name, dir_fd=directory_fd, follow_symlinks=False).st_mode)
    except OSError:
        return False


def repository_files(root, wanted):
    """(relative path, text) for each file wanted(path) selects, outside dependency and build directories; text is
    None for one detection does not read (a symbolic link, not a regular file, or over MAX_FILE).

    Nothing is opened by its path, so no symbolic link is followed at any component, even while the mirror service
    refreshes the checkout: os.fwalk opens the checkout once and each directory relative to its parent's
    descriptor, and skips one that is, or was swapped for, a symbolic link (it compares the directory it opened
    with what it saw before opening it); each file is opened relative to its directory's descriptor with
    O_NOFOLLOW."""
    count = 0
    for directory, subdirectories, files, directory_fd in os.fwalk(root, follow_symlinks=False):
        rel_dir = os.path.relpath(directory, root)
        depth = 0 if rel_dir == "." else rel_dir.count(os.sep) + 1
        subdirectories[:] = sorted(name for name in subdirectories
                                   if name not in SKIP_DIRS and depth < MAX_DEPTH
                                   and real_directory(name, directory_fd))
        for name in sorted(files):
            count += 1
            if count > MAX_FILES:
                return
            rel = name if rel_dir == "." else f"{rel_dir}/{name}".replace(os.sep, "/")
            if wanted(rel):
                yield rel, read_at(directory_fd, name)


def line_of(text, pattern_text, start=0):
    """The 1-based line of the first match of a regular expression, or 1."""
    match = re.compile(pattern_text, re.MULTILINE).search(text, start)
    return text.count("\n", 0, match.start()) + 1 if match else 1


def clean_version(value):
    value = str(value).strip().strip("'\"")
    value = re.sub(r"^v(?=\d)", "", value)
    return value


def valid_version(value):
    return isinstance(value, str) and re.fullmatch(VERSION, value) is not None


def valid_tool(value):
    try:
        tool_name(value)
        return True
    except Invalid:
        return False


# Versions and ranges

def numbers(text):
    parts = re.findall(r"\d+", text)
    return tuple(int(part) for part in parts[:3])


def full(parts):
    return tuple(list(parts[:3]) + [0] * (3 - len(parts[:3])))


def bump(parts, index):
    """The smallest version above every version that starts with parts[:index + 1]."""
    padded = list(full(parts))
    padded[index] += 1
    return tuple(padded[:index + 1] + [0] * (2 - index))


def semver_alternatives(text):
    """An npm version range as intervals [low, high), each a 3-tuple or None for unbounded. None when unparsable."""
    alternatives = []
    for alternative in str(text).split("||"):
        low, high = (0, 0, 0), None
        alternative = alternative.replace(",", " ").strip()
        hyphen = re.fullmatch(r"v?([\d.xX*]+)\s+-\s+v?([\d.xX*]+)", alternative)
        if hyphen:
            first, last = (numbers(re.split(r"[xX*]", part)[0]) for part in hyphen.groups())
            if not first or not last:
                return None
            comparators = [(">=", first), ("<", bump(last, len(last) - 1))]
        else:
            comparators = []
            for token in re.findall(r"(?:[<>]=?|=|\^|~)?\s*v?[\dxX*][\w.*-]*|[*xX]", alternative):
                match = re.fullmatch(r"([<>]=?|=|\^|~)?\s*v?([\dxX*][\w.*-]*)", token.strip())
                if not match:
                    return None
                operator, version = match.groups()
                parts = numbers(re.split(r"[xX*]", version)[0]) if re.match(r"\d", version) else ()
                if not parts:
                    continue
                if operator in (None, "="):
                    comparators += [(">=", parts), ("<", bump(parts, len(parts) - 1))]
                elif operator == "^":
                    index = next((i for i, part in enumerate(full(parts)) if part), len(parts) - 1)
                    comparators += [(">=", parts), ("<", bump(full(parts), min(index, len(parts) - 1)))]
                elif operator == "~":
                    comparators += [(">=", parts), ("<", bump(parts, min(1, len(parts) - 1)))]
                elif operator == ">=":
                    comparators.append((">=", parts))
                elif operator == ">":
                    comparators.append((">=", bump(parts, len(parts) - 1)))
                elif operator == "<":
                    comparators.append(("<", full(parts)))
                elif operator == "<=":
                    comparators.append(("<", bump(parts, len(parts) - 1)))
        for operator, parts in comparators:
            parts = full(parts)
            if operator == ">=":
                low = max(low, parts)
            else:
                high = parts if high is None else min(high, parts)
        alternatives.append((low, high))
    return alternatives or None


def pep440_alternatives(text):
    """A Python version specifier (PEP 440, or Poetry's ^ and ~) as one interval [low, high), or None."""
    low, high = (0, 0, 0), None

    def cap(value):
        nonlocal high
        high = value if high is None else min(high, value)

    for clause in str(text).split(","):
        match = re.fullmatch(r"\s*(~=|===?|!=|>=|<=|>|<|\^|~)?\s*([\d.*]+)\s*", clause)
        if not match:
            return None
        operator, version = match.groups()
        parts = numbers(version.replace("*", ""))
        if not parts or operator == "!=":
            continue
        if operator == ">=":
            low = max(low, full(parts))
        elif operator == ">":
            low = max(low, bump(parts, len(parts) - 1))
        elif operator in (None, "==", "==="):
            low = max(low, full(parts))
            cap(bump(parts, len(parts) - 1))
        elif operator == "~=":
            low = max(low, full(parts))
            cap(bump(parts, max(len(parts) - 2, 0)))
        elif operator == "^":
            low = max(low, full(parts))
            cap(bump(parts, next((index for index, part in enumerate(parts) if part), len(parts) - 1)))
        elif operator == "~":
            low = max(low, full(parts))
            cap(bump(parts, min(1, len(parts) - 1)))
        elif operator == "<":
            cap(full(parts))
        elif operator == "<=":
            cap(bump(parts, len(parts) - 1))
    return [(low, high)]


def overlaps(intervals, version):
    """Whether a version request (24, 24.11 or 24.11.0) can meet one of the intervals."""
    if not re.fullmatch(r"\d+(?:\.\d+){0,2}", str(version)):
        return False
    parts = numbers(version)
    start, end = full(parts), bump(parts, len(parts) - 1)
    return any(start < (high or (10 ** 9, 0, 0)) and low < end for low, high in intervals)


# How far each global.json rollForward policy lets the SDK move from its version: within the feature band
# (8.0.4xx), the minor version (8.0), the major version (8), or anywhere above it.
ROLL_FORWARD = {"disable": "exact", "patch": "band", "latestPatch": "band", "feature": "minor",
                "latestFeature": "minor", "minor": "major", "latestMinor": "major", "major": "any", "latestMajor": "any"}


def dotnet_accepts(constraint, version):
    """Whether an SDK version request meets a global.json version and its rollForward policy. An exact version
    must lie within the policy's reach; a prefix (8.0, which mise resolves to the newest SDK in it) must lie
    within it entirely."""
    minimum, reach = tuple(constraint["minimum"]), ROLL_FORWARD.get(constraint["policy"], "band")
    if not re.fullmatch(r"\d+(?:\.\d+){0,2}", str(version)):
        return False
    parts = numbers(version)
    if len(parts) == 3:
        if reach == "exact":
            return parts == minimum
        within = {"band": parts[:2] == minimum[:2] and parts[2] // 100 == minimum[2] // 100,
                  "minor": parts[:2] == minimum[:2], "major": parts[0] == minimum[0], "any": True}[reach]
        return within and parts >= minimum
    if reach == "minor":
        return len(parts) == 2 and parts == minimum[:2]
    if reach == "major":
        return parts[0] == minimum[0] and (len(parts) == 1 or parts[1] >= minimum[1])
    return reach == "any" and parts[0] >= minimum[0]


def accepts(item, version):
    """Whether a configured or proposed version request meets an item: the same request, an exact version within a
    prefix (22.11.0 for 22), or one that meets the item's range."""
    if version == item["version"]:
        return True
    constraint = item.get("constraint")
    if constraint:
        if constraint["kind"] == "dotnet":
            return dotnet_accepts(constraint, version)
        intervals = [(tuple(low), tuple(high) if high else None) for low, high in constraint["intervals"]]
        return overlaps(intervals, version)
    requested = item["version"]
    return bool(requested) and re.fullmatch(r"\d+(?:\.\d+)*", requested) is not None \
        and version.startswith(requested + ".")


# The files

def toml_load(text):
    try:
        import tomllib
    except ImportError:
        return None
    try:
        return tomllib.loads(text)
    except (tomllib.TOMLDecodeError, ValueError):
        return None


def mise_versions(value):
    """A mise [tools] value as (version, options) pairs."""
    values = value if isinstance(value, list) else [value]
    for entry in values:
        if isinstance(entry, dict):
            yield entry.get("version"), {key: option for key, option in entry.items() if key != "version"}
        else:
            yield entry, {}


def carried_options(finder, source, spec, options):
    """The options detection proposes: those the installer passes on; the rest are reported."""
    kept = {}
    for key, option in options.items():
        if key in LIST_OPTIONS and (isinstance(option, str) or isinstance(option, list)
                                    and all(isinstance(part, str) for part in option)):
            # A list or a comma-separated string, in any order: one form, so equal requests are one item.
            parts = option if isinstance(option, list) else option.split(",")
            option = ",".join(sorted({part.strip() for part in parts if part.strip()}))
        if key in OPTION_KEYS and option_valid(key, option):
            kept[key] = option
        elif key in ("postinstall", "install_env") or "env" in key or "hook" in key:
            finder.cannot(source, f"{spec}'s option {key} is not carried over: root installs the shared tools, so "
                          "no option that runs a command or sets an install's environment is passed to mise")
        elif key in OPTION_KEYS:
            finder.cannot(source, f"{spec}'s option {key} is not carried over: the installer takes only "
                          f"{OPTIONS[key][1]} for it")
        else:
            finder.cannot(source, f"{spec}'s option {key} is not carried over: it is not one of the mise options the "
                          "installer passes on")
    return kept


def scan_mise(finder, repository, rel, text, environment):
    data = toml_load(text)
    if data is None:
        finder.cannot(source_of(repository, rel, 1), "not valid TOML, or this Python cannot read TOML")
        return
    tools = data.get("tools", {})
    if not isinstance(tools, dict):
        return
    section = text.find("[tools]")
    note = f"only with MISE_ENV={environment}" if environment and environment != "local" else None
    for key, value in tools.items():
        line = line_of(text, rf"^\s*[\"']?{re.escape(key)}[\"']?\s*=", max(section, 0))
        source = source_of(repository, rel, line, f"{key} = {json.dumps(value)}")
        if not valid_tool(key):
            finder.cannot(source, f"{key} is not a tool name the installer accepts")
            continue
        for version, options in mise_versions(value):
            version = clean_version(version) if isinstance(version, (str, int, float)) else None
            if not valid_version(version):
                finder.cannot(source, f"{key}'s version {version or '(none)'} cannot be installed as given")
                continue
            finder.add("tool", key, version, "medium" if note else "high", source,
                       carried_options(finder, source, key, options), note=note)


def scan_tool_versions(finder, repository, rel, text):
    for number, line in enumerate(text.splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        name, *versions = line.split()
        tool = ASDF_NAMES.get(name, name)
        source = source_of(repository, rel, number, line)
        if not valid_tool(tool):
            finder.cannot(source, f"{name} is not a tool name the installer accepts")
            continue
        for version in versions:
            version = clean_version(version)
            if valid_version(version):
                finder.add("tool", tool, version, "high", source)
            else:
                finder.cannot(source, f"{name}'s version {version} cannot be installed as given")


def scan_rust_toolchain(finder, repository, rel, text):
    data = toml_load(text) if "[toolchain]" in text else None
    if data is not None:
        toolchain = data.get("toolchain", {})
        channel = toolchain.get("channel") if isinstance(toolchain, dict) else None
        line = line_of(text, r"^\s*channel\s*=")
        options = {key: toolchain[key] for key in ("components", "targets", "profile")
                   if isinstance(toolchain, dict) and key in toolchain}
    else:
        channel = next((line.strip() for line in text.splitlines() if line.strip()), None)
        line, options = 1, {}
    source = source_of(repository, rel, line, f"channel {channel}")
    if not isinstance(channel, str) or not valid_version(channel):
        finder.cannot(source, "no Rust channel the installer can install")
        return
    finder.add("tool", "rust", channel, "high", source, carried_options(finder, source, "rust", options))


def jsonc(text):
    """JSON with comments and trailing commas, as global.json may be."""
    result, index, quoted = [], 0, False
    while index < len(text):
        char = text[index]
        if quoted:
            result.append(char)
            if char == "\\":
                result.append(text[index + 1:index + 2])
                index += 1
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
            result.append(char)
        elif text.startswith("//", index):
            index = text.find("\n", index)
            index = len(text) if index < 0 else index
            continue
        elif text.startswith("/*", index):
            index = text.find("*/", index)
            index = len(text) if index < 0 else index + 2
            continue
        else:
            result.append(char)
        index += 1
    return json.loads(re.sub(r",(\s*[}\]])", r"\1", "".join(result)))


def scan_global_json(finder, repository, rel, text):
    try:
        sdk = jsonc(text).get("sdk", {})
    except (ValueError, AttributeError):
        finder.cannot(source_of(repository, rel, 1), "not valid JSON")
        return
    if not isinstance(sdk, dict) or "version" not in sdk:
        return
    version, policy = sdk.get("version"), sdk.get("rollForward", "latestPatch")
    source = source_of(repository, rel, line_of(text, r'"version"\s*:'), f"version {version}, rollForward {policy}")
    parts = numbers(version) if isinstance(version, str) else ()
    if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d{3}(?:-[\w.]+)?", version) or len(parts) < 3:
        finder.cannot(source, f"SDK version {version} is not an SDK version such as 8.0.404")
        return
    scope = {"feature": "{0}.{1}", "latestFeature": "{0}.{1}", "minor": "{0}", "latestMinor": "{0}",
             "major": "{0}", "latestMajor": "{0}"}
    proposed = scope[policy].format(*parts) if policy in scope else version
    notes = {"disable": "exactly this SDK (rollForward disable)",
             "patch": f"this SDK or a later patch in the {parts[0]}.{parts[1]}.{parts[2] // 100}xx band",
             "latestPatch": f"this SDK or a later patch in the {parts[0]}.{parts[1]}.{parts[2] // 100}xx band",
             "feature": f"{version} or a later {parts[0]}.{parts[1]} feature band",
             "latestFeature": f"{version} or a later {parts[0]}.{parts[1]} feature band",
             "minor": f"{version} or a later {parts[0]}.x SDK", "latestMinor": f"{version} or a later {parts[0]}.x SDK",
             "major": f"{version} or any later SDK", "latestMajor": f"{version} or any later SDK"}
    finder.add("tool", "dotnet", proposed, "high", source,
               constraint={"kind": "dotnet", "minimum": list(parts), "policy": policy},
               note=f"global.json accepts {notes.get(policy, version)}")


def range_proposal(tool, intervals):
    """The version request a range proposes: the newest major below an upper bound (an even one for Node, whose
    long-term releases are even), Node's lts for an open range, else the lowest major."""
    lows = [low for low, _ in intervals]
    bounded = [high for _, high in intervals if high]
    if bounded and len(bounded) == len(intervals):
        top = max(high for high in bounded)
        majors = [major for major in range(min(low[0] for low in lows), top[0] + 1)
                  if overlaps(intervals, str(major))]
        if tool == "node" and any(major % 2 == 0 for major in majors):
            majors = [major for major in majors if major % 2 == 0]
        return str(max(majors)) if majors else None
    return "lts" if tool == "node" else str(min(lows)[0])


def scan_package_json(finder, repository, rel, text):
    try:
        data = json.loads(text)
    except ValueError:
        finder.cannot(source_of(repository, rel, 1), "not valid JSON")
        return
    if not isinstance(data, dict):
        return
    managers = set()
    manager = data.get("packageManager")
    if isinstance(manager, str):
        source = source_of(repository, rel, line_of(text, r'"packageManager"\s*:'), f"packageManager {manager}")
        match = re.fullmatch(r"(pnpm|yarn|bun|npm)@(\d[\w.-]*)(?:\+.*)?", manager)
        if not match:
            finder.cannot(source, f"packageManager {manager} names no package manager the installer knows")
        elif match.group(1) == "npm":
            finder.cannot(source, f"npm {match.group(2)} comes with Node: check that the Node version bundles it")
        else:
            managers.add(match.group(1))
            finder.add("tool", match.group(1), match.group(2), "high", source)
    volta = data.get("volta")
    if isinstance(volta, dict):
        for tool, version in volta.items():
            if tool in ("node", "pnpm", "yarn") and isinstance(version, str) and valid_version(clean_version(version)):
                finder.add("tool", tool, clean_version(version), "high",
                           source_of(repository, rel, line_of(text, r'"volta"\s*:'), f"volta {tool} {version}"))
    engines = data.get("engines")
    if not isinstance(engines, dict):
        return
    for tool, expression in engines.items():
        if tool not in ("node", "pnpm", "yarn", "bun") or tool in managers or not isinstance(expression, str):
            continue
        source = source_of(repository, rel, line_of(text, rf'"{tool}"\s*:\s*"{re.escape(expression)}"'),
                           f"engines {tool} {expression}")
        intervals = semver_alternatives(expression)
        proposed = range_proposal(tool, intervals) if intervals else None
        if not proposed:
            finder.cannot(source, f"engines {tool} {expression!r} is not a range the installer reads")
            continue
        finder.add("tool", tool, proposed, "medium", source, note=f"engines {tool} {expression}",
                   constraint={"kind": "range", "intervals": [[list(low), list(high) if high else None]
                                                              for low, high in intervals]})


def scan_version_file(finder, repository, rel, text, tool):
    for number, line in enumerate(text.splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        source = source_of(repository, rel, number, line)
        version = clean_version(line)
        if tool == "node" and version.startswith("lts/"):
            version = "lts" if version == "lts/*" else version
        version = version.removeprefix("ruby-") if tool == "ruby" else version
        if valid_version(version):
            finder.add("tool", tool, version, "high", source)
        else:
            finder.cannot(source, f"{tool} version {line} cannot be installed as given")


def scan_go_mod(finder, repository, rel, text):
    match = re.search(r"^toolchain\s+go(\d+\.\d+(?:\.\d+)?)\s*$", text, re.MULTILINE) or \
        re.search(r"^go\s+(\d+\.\d+(?:\.\d+)?)\s*$", text, re.MULTILINE)
    if match:
        line = text.count("\n", 0, match.start()) + 1
        finder.add("tool", "go", match.group(1), "medium", source_of(repository, rel, line, match.group(0)),
                   note="the go command fetches a newer toolchain a module asks for itself")


def scan_pyproject(finder, repository, rel, text):
    data = toml_load(text)
    if data is None:
        finder.cannot(source_of(repository, rel, 1), "not valid TOML, or this Python cannot read TOML")
        return
    project, poetry = data.get("project", {}), data.get("tool", {}).get("poetry", {}).get("dependencies", {})
    expression = (project.get("requires-python") if isinstance(project, dict) else None) or \
        (poetry.get("python") if isinstance(poetry, dict) else None)
    if not isinstance(expression, str):
        return
    source = source_of(repository, rel, line_of(text, r"^\s*(?:requires-python|python)\s*="),
                       f"Python {expression}")
    intervals = pep440_alternatives(expression)
    if not intervals or intervals[0][0] == (0, 0, 0):
        finder.cannot(source, f"Python {expression!r} names no lowest version")
        return
    low = intervals[0][0]
    finder.add("tool", "python", f"{low[0]}.{low[1]}", "medium", source, note=f"requires Python {expression}",
               constraint={"kind": "range", "intervals": [[list(lo), list(hi) if hi else None] for lo, hi in intervals]})


# A YAML subset for workflows and Compose files: block mappings and sequences, plain and quoted scalars, block
# scalars, with each scalar's line. Flow collections stay text; anchors and tags are not read.

class Scalar(str):
    line = 0


def scalar(text, line):
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "'\"":
        text = text[1:-1].replace("''", "'") if text[0] == "'" else text[1:-1]
    value = Scalar(text)
    value.line = line
    return value


def strip_comment(text):
    quote = None
    for index, char in enumerate(text):
        if quote:
            if char == quote:
                quote = None
        elif char in "'\"" and (index == 0 or text[index - 1] in " :-[{,"):
            quote = char
        elif char == "#" and (index == 0 or text[index - 1] in " \t"):
            return text[:index].rstrip()
    return text.rstrip()


KEY = re.compile(r"""^(?:"([^"]*)"|'([^']*)'|([^\s#'"][^:#]*?|[^\s#'"]))\s*:(?:\s+(.*)|\s*)$""")


def parse_yaml(text):
    raw = text.splitlines()
    lines = []
    for number, line in enumerate(raw, 1):
        lines.append((len(line) - len(line.lstrip(" ")), line.strip(), number, line))

    def skip(index):
        while index < len(lines) and (not lines[index][1] or lines[index][1].startswith("#")
                                      or lines[index][1] in ("---", "...")):
            index += 1
        return index

    def block_scalar(index, indent):
        collected = []
        while index < len(lines) and (not lines[index][1] or lines[index][0] > indent):
            collected.append(lines[index][3])
            index += 1
        width = min((len(line) - len(line.lstrip(" ")) for line in collected if line.strip()), default=0)
        return "\n".join(line[width:] for line in collected), index

    def value_of(rest, index, indent, number):
        rest = strip_comment(rest)
        if re.fullmatch(r"[|>][-+0-9]*", rest):
            body, index = block_scalar(index, indent)
            value = Scalar(body)
            value.line = number + 1
            return value, index
        if rest:
            # A plain scalar continued on deeper lines.
            parts = [rest]
            while index < len(lines) and lines[index][1] and lines[index][0] > indent \
                    and not lines[index][1].startswith("#") and not rest.startswith(("'", '"', "[", "{")):
                parts.append(strip_comment(lines[index][1]))
                index += 1
            return scalar(" ".join(parts), number), index
        following = skip(index)
        if following < len(lines) and (lines[following][0] > indent or lines[following][0] == indent
                                       and lines[following][1].startswith("- ") or lines[following][1] == "-"):
            return node(following, lines[following][0])
        return scalar("", number), index

    def mapping(index, indent, first=None):
        result = {}
        while True:
            if first is None:
                index = skip(index)
                if index >= len(lines) or lines[index][0] != indent or lines[index][1].startswith("-"):
                    return result, index
                content, number = lines[index][1], lines[index][2]
                index += 1
            else:
                content, number = first
                first = None
            match = KEY.match(content)
            if not match:
                return result, index
            key = next(group for group in match.groups()[:3] if group is not None).strip()
            value, index = value_of(match.group(4) or "", index, indent, number)
            result[key] = value

    def sequence(index, indent):
        result = []
        while True:
            index = skip(index)
            if index >= len(lines) or lines[index][0] != indent or not (
                    lines[index][1].startswith("- ") or lines[index][1] == "-"):
                return result, index
            content, number = lines[index][1], lines[index][2]
            rest = content[1:].lstrip()
            index += 1
            if not rest:
                following = skip(index)
                if following < len(lines) and lines[following][0] > indent:
                    value, index = node(following, lines[following][0])
                else:
                    value = scalar("", number)
            elif KEY.match(strip_comment(rest)) and not rest.startswith(("'", '"')):
                value, index = mapping(index, indent + len(content) - len(rest), (strip_comment(rest), number))
            else:
                value, index = value_of(rest, index, indent, number)
            result.append(value)

    def node(index, indent):
        if lines[index][1].startswith("- ") or lines[index][1] == "-":
            return sequence(index, indent)
        return mapping(index, indent)

    start = skip(0)
    if start >= len(lines):
        return {}
    return node(start, lines[start][0])[0]


def steps_of(value, job=None):
    """Every (job, step) under a steps: key, in workflows and composite actions."""
    if isinstance(value, dict):
        for key, child in value.items():
            if key == "steps" and isinstance(child, list):
                for step in child:
                    if isinstance(step, dict):
                        yield job, step
            else:
                yield from steps_of(child, child if isinstance(child, dict) and "runs-on" in child else job)
    elif isinstance(value, list):
        for child in value:
            yield from steps_of(child, job)


def ci_version(text):
    """A set-up action's version in mise's terms: v2.x is 2, 22.x is 22, lts/* is lts."""
    text = clean_version(text)
    if text in ("lts/*", "lts"):
        return "lts"
    text = re.sub(r"(?:\.[xX*])+$", "", text)
    return text


def apt_packages(command):
    """Package names in apt-get or apt install commands, and the words that are not, each as (word, reason).

    Only packages to install are proposed: a word ending in a minus, which apt-get removes, a release (/), a file
    or a pattern is reported instead. A version (=) or architecture (:) is dropped from the name proposed."""
    packages, others = [], []
    command = re.sub(r"\\\n", " ", command)
    for segment in re.split(r"&&|\|\||[;|\n]", command):
        try:
            tokens = shlex.split(segment, comments=True)
        except ValueError:
            tokens = segment.split()
        for index, token in enumerate(tokens):
            if token.rsplit("/", 1)[-1] in ("apt-get", "apt", "aptitude"):
                rest = tokens[index + 1:]
                if "install" not in rest:
                    break
                words = iter(rest[rest.index("install") + 1:])
                for word in words:
                    if word in ("-o", "--option", "-t", "--target-release", "-c", "--config-file"):
                        next(words, None)
                    elif word.startswith("-"):
                        continue
                    elif "$" in word or "{" in word or "}" in word or "`" in word:
                        # A variable or expression: what follows is not a list of names either.
                        others.append((word, "a variable or expression"))
                        break
                    elif re.fullmatch(r"[a-z0-9][a-z0-9+.-]*-", word):
                        others.append((word, f"apt-get removes {word[:-1]} when a name ends in a minus"))
                    elif re.fullmatch(PACKAGE, name := re.split(r"[=:]", word)[0]):
                        packages.append(name)
                    else:
                        others.append((word, "not a plain package name (a file, release or pattern)"))
                break
    return packages, others


def scan_workflow(finder, repository, rel, text):
    try:
        data = parse_yaml(text)
    except (IndexError, ValueError, RecursionError):
        finder.cannot(source_of(repository, rel, 1), "a workflow detection could not read")
        return
    if not isinstance(data, dict):
        return
    jobs = data.get("jobs", {}) if isinstance(data.get("jobs"), dict) else {}
    for name, job in jobs.items():
        if not isinstance(job, dict):
            continue
        runner = job.get("runs-on")
        if isinstance(runner, str) and re.search(r"macos|windows", runner, re.IGNORECASE):
            finder.cannot(source_of(repository, rel, getattr(runner, "line", 1), f"runs-on {runner}"),
                          f"job {name} runs on {runner}: its tools are not for this Linux VM")
        container = job.get("container")
        if container:
            image = container.get("image", "") if isinstance(container, dict) else container
            finder.cannot(source_of(repository, rel, getattr(image, "line", 1), f"container {image}"),
                          f"job {name} runs in the container image {image}: tools inside it are not installed on the VM")
        services = job.get("services")
        if isinstance(services, dict):
            for service, settings in services.items():
                image = settings.get("image", "") if isinstance(settings, dict) else ""
                line = getattr(image, "line", 1)
                kind = "a database" if DATABASES.search(str(image) or service) else "a service container"
                finder.cannot(source_of(repository, rel, line, f"services {service}: {image}"),
                              f"job {name}'s tests need {kind} ({service}), which no toolchain provides")
    for job, step in steps_of(data):
        uses = step.get("uses")
        options = step.get("with") if isinstance(step.get("with"), dict) else {}
        secrets = re.findall(r"secrets\.([A-Za-z0-9_]+)", json.dumps(step))
        if any(re.search(r"LICEN[CS]E|SERIAL", secret, re.IGNORECASE) for secret in secrets):
            finder.cannot(source_of(repository, rel, getattr(uses or step.get("run", ""), "line", 1), uses or "run"),
                          f"a licensed build step (secrets {', '.join(sorted(set(secrets)))}): it needs a licence the "
                          "VM has not got")
        if isinstance(uses, str):
            action, _, ref = uses.partition("@")
            action = action.lower()
            if action.startswith("docker://"):
                finder.cannot(source_of(repository, rel, uses.line, uses), "a Docker-only step: its tools live in "
                              "the image")
            if action in NOT_TOOLS:
                continue
            known = SETUP_ACTIONS.get("/".join(action.split("/")[:2]))
            if known:
                tool, keys = known
                values = []
                for key in keys:
                    value = options.get(key)
                    values += value if isinstance(value, list) else [value] if isinstance(value, str) else []
                if tool == "rust" and not values and re.fullmatch(r"stable|beta|nightly|\d+\.\d+(?:\.\d+)?", ref):
                    values = [scalar(ref, uses.line)]
                for value in values:
                    line = getattr(value, "line", uses.line)
                    for part in [part for part in re.split(r"[\s,]+", str(value)) if part]:
                        source = source_of(repository, rel, line, f"{uses} with {part}")
                        if "${{" in str(value) or part.startswith("$"):
                            source = source_of(repository, rel, line, f"{uses} with {value}")
                            matrix = re.search(r"matrix\.([\w-]+)", str(value))
                            values_text = ""
                            if matrix and isinstance(job, dict):
                                strategy = job.get("strategy", {})
                                listed = strategy.get("matrix", {}).get(matrix.group(1)) if isinstance(strategy, dict) \
                                    and isinstance(strategy.get("matrix"), dict) else None
                                values_text = f" (the matrix lists {listed})" if listed else ""
                            why = "a matrix" if matrix else "a secret, variable or expression"
                            finder.cannot(source, f"{tool}'s version comes from {why}: {value}{values_text}")
                            break
                        version = ci_version(part)
                        if tool == "java" and options.get("distribution"):
                            version = f"{options['distribution']}-{version}"
                        if valid_version(version):
                            extra = {}
                            if tool == "rust" and options.get("components"):
                                extra = carried_options(finder, source, "rust", {"components": ",".join(
                                    sorted(re.split(r"[\s,]+", str(options["components"]).strip())))})
                            finder.add("tool", tool, version, "medium", source, extra or None)
                        else:
                            finder.cannot(source, f"{tool} version {part} cannot be installed as given")
        run = step.get("run")
        if isinstance(run, str):
            if re.search(r"\bdocker(?:\s+compose)?\s+(?:run|build|compose|exec)\b|\bdocker-compose\b", run):
                finder.cannot(source_of(repository, rel, run.line, run.splitlines()[0] if run else ""),
                              "a Docker-only step: what it runs comes from container images")
            packages, others = apt_packages(run)
            for package in packages:
                finder.add("package", package, None, "medium", source_of(repository, rel, run.line,
                                                                         f"apt-get install {package}"))
            for other, why in others:
                finder.cannot(source_of(repository, rel, run.line, other),
                              f"apt-get is given {other}, which detection does not propose: {why}")


def scan_compose(finder, repository, rel, text):
    try:
        data = parse_yaml(text)
    except (IndexError, ValueError, RecursionError):
        return
    services = data.get("services") if isinstance(data, dict) else None
    if not isinstance(services, dict):
        return
    for service, settings in services.items():
        image = settings.get("image") if isinstance(settings, dict) else None
        if isinstance(image, str) and image:
            kind = "a database" if DATABASES.search(image) else "a service"
            finder.cannot(source_of(repository, rel, getattr(image, "line", 1), f"{service}: {image}"),
                          f"Docker Compose runs {kind} ({service}): tests that need it need Docker, not a toolchain")


def doc_commands(text):
    """Shell lines in documentation: inside fenced code blocks (continuations joined) and inline code spans."""
    fenced, start, pending = False, 0, ""
    for number, line in enumerate(text.splitlines(), 1):
        if re.match(r"\s*(```|~~~)", line):
            fenced = not fenced
            continue
        if fenced:
            line = re.sub(r"^\s*(?:\$|#|>)\s+", "", line)
            if not pending:
                start = number
            pending += line.rstrip("\\") + " "
            if not line.rstrip().endswith("\\"):
                yield start, pending.strip()
                pending = ""
        else:
            for span in re.findall(r"`([^`]+)`", line):
                yield number, span.strip().removeprefix("$ ")


def scan_docs(finder, repository, rel, text):
    for number, command in doc_commands(text):
        if re.search(r"\bapt(?:-get)?\b.*\binstall\b", command):
            packages, _ = apt_packages(command)
            for package in packages:
                finder.add("package", package, None, "low", source_of(repository, rel, number, command))


def scan_repository(finder, repository, root):
    try:
        info = os.lstat(root)
        readable = stat.S_ISDIR(info.st_mode) and os.access(root, os.R_OK | os.X_OK)
    except OSError:
        readable = False
    if not readable:
        finder.cannot(source_of(repository, "", 0), f"the repository is not cloned at {root} yet, or cannot be read")
        return
    docs = 0

    def wanted(rel):
        nonlocal docs
        name = rel.rsplit("/", 1)[-1]
        if DOC_FILE.search(rel) and "/.github/" not in f"/{rel}":
            docs += 1
            return docs <= MAX_DOCS
        return bool(MISE_FILE.search(rel) or WORKFLOW_FILE.fullmatch(rel) or name in VERSION_FILES
                    or name in COMPOSE_FILES or name in SCANNED_FILES)

    try:
        for rel, text in repository_files(root, wanted):
            if text is None:
                finder.cannot(source_of(repository, rel, 0), "not a regular file detection can read (a symbolic "
                              "link, or larger than 1 MiB)")
                continue
            try:
                scan_file(finder, repository, rel, text)
            except Exception as error:  # A repository's file never stops detection, or setup.
                finder.cannot(source_of(repository, rel, 0), f"detection could not read this file ({type(error).__name__}"
                              "); check what it asks for yourself")
    except OSError as error:
        finder.cannot(source_of(repository, "", 0), f"detection could not read the repository at {root}: "
                      f"{error.strerror or error}")


def scan_file(finder, repository, rel, text):
    """Read one file detection selected (scan_repository's wanted) with the scanner for its kind."""
    name = rel.rsplit("/", 1)[-1]
    if mise := MISE_FILE.search(rel):
        scan_mise(finder, repository, rel, text, mise.group(1))
    elif WORKFLOW_FILE.fullmatch(rel):
        scan_workflow(finder, repository, rel, text)
    elif DOC_FILE.search(rel) and "/.github/" not in f"/{rel}":
        scan_docs(finder, repository, rel, text)
    elif name == ".tool-versions":
        scan_tool_versions(finder, repository, rel, text)
    elif name in ("rust-toolchain", "rust-toolchain.toml"):
        scan_rust_toolchain(finder, repository, rel, text)
    elif name == "global.json":
        scan_global_json(finder, repository, rel, text)
    elif name == "package.json":
        scan_package_json(finder, repository, rel, text)
    elif name in VERSION_FILES:
        scan_version_file(finder, repository, rel, text, VERSION_FILES[name])
    elif name == "go.mod":
        scan_go_mod(finder, repository, rel, text)
    elif name == "pyproject.toml":
        scan_pyproject(finder, repository, rel, text)
    elif name in COMPOSE_FILES:
        scan_compose(finder, repository, rel, text)


def fold(items):
    """Join an item into another of the same tool and options that meets it: a range or prefix that a proposed
    exact version satisfies, or the same package. The joined item keeps both sources and the higher confidence."""
    result = []
    order = sorted(items, key=lambda item: (item["kind"], item["name"], -len(numbers(item["version"] or "")),
                                            CONFIDENCE.index(item["confidence"])))
    for item in order:
        target = next((kept for kept in result if kept["kind"] == "tool" == item["kind"]
                       and kept["name"] == item["name"] and kept["options"] == item["options"]
                       and kept["version"] != item["version"] and accepts(item, kept["version"])), None)
        if target is None:
            result.append(item)
            continue
        for source in item["sources"]:
            if source not in target["sources"]:
                target["sources"].append(source)
        target["notes"] = [*target.get("notes", []), *(note for note in item.get("notes", [])
                                                        if note not in target.get("notes", []))]
        if CONFIDENCE.index(item["confidence"]) < CONFIDENCE.index(target["confidence"]):
            target["confidence"] = item["confidence"]
    return result


def detect(repositories):
    """What the repositories need: {"items": [...], "unknown": [...], "repositories": [...]}.

    repositories maps each repository ID to its checkout. Items are tools (name is a mise tool specification) and
    system packages (version None), each with a confidence, its sources (repository, file, line) and notes."""
    finder = Finder()
    for repository, root in repositories.items():
        try:
            scan_repository(finder, repository, root)
        except Exception as error:  # Nothing in a repository stops detection; the repository is reported.
            finder.cannot(source_of(repository, "", 0), f"detection stopped reading the repository "
                          f"({type(error).__name__}); it is proposed from again on the next run")
    items = fold(finder.items.values())
    for item in items:
        if item["confidence"] == "low":
            # Only documentation is low confidence.
            item.setdefault("notes", []).append("named by documentation only")
    items.sort(key=lambda item: (item["kind"] != "tool", CONFIDENCE.index(item["confidence"]), item["name"],
                                 item["version"] or ""))
    return {"repositories": list(repositories), "items": items, "unknown": finder.unknown}


def annotate(proposal, tools, packages, homes, system_python=None, states=None):
    """Give each item where it would be installed, whether the fleet's toolchains already have it (or a version
    that meets it), and whether it is installed. homes are the officers' homes, for per-user tools."""
    system_python = system_python or ".".join(str(part) for part in sys.version_info[:3])
    states = states if states is not None else {}
    for item in proposal["items"]:
        if item["kind"] == "package":
            item["where"] = "system"
            item["configured"] = item["name"] in packages
            if item["name"] not in states:
                states[item["name"]] = package_state(item["name"])
            item["installed"] = states[item["name"]]
            continue
        entry = tools.get(item["name"])
        item["where"] = entry["install"] if entry else default_install(item["name"])
        versions = entry["versions"] if entry else []
        if item["version"] in versions:
            item["configured"] = True
        elif meeting := [version for version in versions if accepts(item, version)]:
            item["configured"] = f"met by {item['name']} {meeting[0]}, already configured"
        elif item["name"] == "python" and accepts(item, system_python):
            item["configured"] = f"met by the system's Python {system_python}"
        else:
            item["configured"] = False
        item["installed"] = tool_state(item["name"], item["version"], item["where"], homes)
    return proposal


def item_label(item):
    if item["kind"] == "package":
        return item["name"]
    options = ", ".join(f"{key}={value}" for key, value in sorted(item["options"].items()))
    return f"{item['name']}@{item['version']}" + (f" ({options})" if options else "")


def where_label(where):
    return {"shared": f"shared ({SHARED})", "user": f"per user (~/{USER_DATA})", "system": "system (apt)"}[where]


def source_label(source):
    if not source["file"]:
        return source["repository"]
    return f"{source['repository']}: {source['file']}" + (f":{source['line']}" if source["line"] else "")


def confirmable(items):
    """The items a proposal asks a person to confirm: not configured already, and named by more than documentation."""
    return [item for item in items if not item.get("configured") and item["confidence"] != "low"]


def suggestion(items):
    """The setup options that confirm the proposed items, for the person to edit before running."""
    tools = list(dict.fromkeys(f"{item['name']}@{item['version']}" for item in confirmable(items)
                               if item["kind"] == "tool"))
    packages = list(dict.fromkeys(item["name"] for item in confirmable(items) if item["kind"] == "package"))
    parts = []
    if tools:
        parts.append(f"--toolchains {shlex.quote(','.join(tools))}")
    if packages:
        parts.append(f"--system-packages {shlex.quote(','.join(packages))}")
    return " ".join(parts)


def proposal_lines(proposal):
    """The proposal for a person to read: each item with where it would go, whether it is installed and where it
    came from, then what detection could not determine. Nothing in it is installed until someone confirms it."""
    names = ", ".join(proposal["repositories"]) or "no repositories"
    lines = [f"Toolchain proposal from {names}. Nothing in it is installed until you confirm it."]
    pending = [item for item in proposal["items"] if not item.get("configured")]
    met = [item for item in proposal["items"] if item.get("configured")]
    for title, kind in (("Tools", "tool"), ("System packages", "package")):
        rows = [item for item in pending if item["kind"] == kind]
        if not rows:
            continue
        lines.append(f"{title}:")
        for item in rows:
            notes = "; ".join(item.get("notes", []))
            lines.append(f"  [{item['confidence']}] {item_label(item)}: {where_label(item['where'])}, "
                         f"{item.get('installed', 'unknown')}" + (f"; {notes}" if notes else ""))
            lines += [f"      from {source_label(source)}" + (f" ({source['detail']})" if source["detail"] else "")
                      for source in item["sources"]]
    if met:
        lines.append("Already configured: " + "; ".join(
            f"{item_label(item)}" + (f" ({item['configured']})" if isinstance(item["configured"], str) else "")
            for item in met) + ".")
    if proposal["unknown"]:
        lines.append("Could not determine (check these yourself):")
        lines += [f"  {source_label(entry)}: {entry['reason']}" + (f" ({entry['detail']})" if entry["detail"] else "")
                  for entry in proposal["unknown"]]
    if command := suggestion(proposal["items"]):
        lines.append(f"To confirm, edit and run: sudo subspace-setup {command}")
        if any(item["confidence"] == "low" for item in pending):
            lines.append("Low-confidence items, named only by documentation, are left out of that command: add any "
                         "that the repositories need.")
    elif pending:
        lines.append("Nothing to confirm: only documentation names the items above, so setup does not propose "
                     "installing them, and counts this proposal as answered. If the repositories need any, add them: "
                     "sudo subspace-setup --system-packages NAME,... or --toolchains TOOL@VERSION,...")
    else:
        lines.append("Nothing new to confirm.")
    return lines


# Verification --------------------------------------------------------------------------------------------------

PROBES = {
    "node": ["node", "--version"], "pnpm": ["pnpm", "--version"], "yarn": ["yarn", "--version"],
    "bun": ["bun", "--version"], "deno": ["deno", "--version"], "dotnet": ["dotnet", "--version"],
    "rust": ["cargo", "--version"], "go": ["go", "version"], "python": ["python", "--version"],
    "terraform": ["terraform", "version"], "opentofu": ["tofu", "version"], "java": ["java", "-version"],
    "ruby": ["ruby", "--version"], "zig": ["zig", "version"], "uv": ["uv", "--version"],
    "kubectl": ["kubectl", "version", "--client"], "helm": ["helm", "version"],
}


def verification_commands(found, tools, check=None):
    """The commands a repository's basic check runs: each configured tool its files name, run from the checkout
    (so mise, global.json and rust-toolchain resolve as they will for the crew), then its own check, if any."""
    commands, missing = [], []
    for item in found["items"]:
        if item["kind"] != "tool" or item["confidence"] == "low":
            continue
        if item["name"] not in tools and not item.get("configured"):
            missing.append(item_label(item))
        elif item["name"] in PROBES and PROBES[item["name"]] not in commands:
            commands.append(PROBES[item["name"]])
    if check:
        commands.append(["/bin/bash", "-c", check])
    return commands, missing


def verify_repository(user, home, directory, environment, commands, timeout):
    """Run each command as the officer's user in the directory with the engine's environment, in a fresh private
    temporary directory. Returns (passed, report)."""
    scratch = subprocess.run(["runuser", "-u", user, "--", "mktemp", "-d", f"/tmp/{user}-verify.XXXXXX"],
                             capture_output=True, text=True, check=False)
    if scratch.returncode:
        return False, "could not make a private temporary directory"
    temporary = scratch.stdout.strip()
    env = {**environment, "TMPDIR": temporary}
    try:
        for command in commands:
            try:
                result = subprocess.run(["runuser", "-u", user, "--", *clean_environment(user, home, env), *command],
                                        cwd=directory, capture_output=True, text=True, timeout=timeout, check=False)
            except subprocess.TimeoutExpired:
                return False, f"{shlex.join(command)} did not finish within {timeout} seconds"
            if result.returncode:
                tail = (result.stderr or result.stdout).strip().splitlines()[-3:]
                return False, f"{shlex.join(command)} failed ({result.returncode}): " + " / ".join(tail)
        return True, f"{len(commands)} checks passed"
    finally:
        subprocess.run(["runuser", "-u", user, "--", "rm", "-rf", "--", temporary], check=False)


def main(argv):
    """sudo /opt/subspace-fleet/lib/toolchains.py detect [--json]: a proposal from the installed fleet's
    repositories, as the rear admiral's reference checkouts hold them."""
    if argv[:1] != ["detect"] or not set(argv[1:]) <= {"--json"}:
        raise Invalid("Usage: toolchains.py detect [--json]")
    config = json.loads((ETC / "fleet.json").read_text())
    rear = config["rearAdmiral"]["id"]
    repositories = {row["id"]: STATE / "references" / rear / row["id"] for row in config.get("repositories", [])}
    tools = toolchain_config(config.get("toolchains", {}))
    homes = [STATE / "agents" / person["id"] for person in [config["rearAdmiral"], *config["captains"]]]
    proposal = annotate(detect(repositories), tools, package_config(config.get("systemPackages", [])), homes)
    print(json.dumps(proposal, indent=2) if "--json" in argv else "\n".join(proposal_lines(proposal)))


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (Invalid, OSError, ValueError, KeyError) as error:
        sys.exit(f"subspace-fleet: {error}")
