"""The fleet's toolchains: the toolchains and systemPackages settings, where each tool is installed, and the
environment officers and crew run their tools in.

Where things are installed:

- systemPackages: distribution packages, by apt as root, system-wide.
- toolchains, shared (the default): installed by root with mise, as explicit tool@version arguments, into the
  shared store /opt/subspace-fleet/toolchains, from /opt/subspace-fleet and never from an agent's checkout. Every
  officer and crew member sees them: the shared store is their mise system data directory.
- toolchains, per user (rust and cargo: tools by default, or "install": "user"): installed as each officer's
  Linux user, as explicit tool@version arguments from its home, into its own mise data directory. Crew run as
  their captain's user, so they share their captain's.
"""

import json
import os
from pathlib import Path
import re
import stat
import subprocess


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
# pinned toolchain and components on first use, which a read-only shared store cannot; cargo: tools build with
# the user's own Rust.
USER_TOOLS = {"rust"}
USER_BACKENDS = {"cargo"}
# The mise tool options a toolchain may carry. Options that run commands or set an install's environment, such
# as postinstall or install_env, are refused: root installs the shared tools.
OPTION_KEYS = {"components", "targets", "profile", "bin", "bin_path", "exe", "rename_exe", "asset_pattern",
               "matching", "version_prefix", "filter_bins", "runtime", "features", "default-features", "crate",
               "locked", "channel"}
PLAIN_TOOL = r"[a-z0-9][a-z0-9._-]{0,63}"
BACKEND_TOOL = rf"(?:{'|'.join(BACKENDS)}):@?[A-Za-z0-9][A-Za-z0-9._+-]*(?:/[A-Za-z0-9._+-]+){{0,6}}"
VERSION = r"[A-Za-z0-9][A-Za-z0-9._+-]{0,63}"
OPTION_VALUE = r"[A-Za-z0-9._,:/@+=*{}-]{0,200}"
# Debian package names (Debian Policy 5.6.1): lower case letters, digits, plus, minus and full stops.
PACKAGE = r"[a-z0-9][a-z0-9+.-]{1,99}"
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
        require(type(option) is bool or type(option) is int or isinstance(option, str)
                and re.fullmatch(OPTION_VALUE, option), f"Invalid value for toolchain {spec}'s option {key}.")
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
                f"Invalid system package {name!r}: use a Debian package name such as libssl-dev.")
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
    entry = tools.get("dotnet")
    if not entry:
        return None
    return str(user_data(home) / "dotnet-root" if entry["install"] == "user" and home else SHARED / "dotnet-root")


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
    configuration as its only configuration, and no project configuration (it runs from /opt/subspace-fleet)."""
    environment = {
        "PATH": f"{APP}/bin:{SHARED}/shims:{BASE_PATH}", "HOME": "/root",
        "MISE_DATA_DIR": str(SHARED), "MISE_GLOBAL_CONFIG_FILE": str(SHARED_CONFIG),
        "MISE_SYSTEM_CONFIG_DIR": str(SYSTEM_CONFIG_DIR), "MISE_YES": "1", "MISE_LOCKFILE": "0",
        "MISE_SELF_UPDATE_AVAILABLE": "0",
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
        status = subprocess.run(["dpkg-query", "-W", "-f", "${db:Status-Status}", name], capture_output=True,
                                text=True, check=False, timeout=30)
        if status.returncode == 0 and status.stdout.strip() == "installed":
            return "installed"
        shown = subprocess.run(["apt-cache", "show", "--no-all-versions", name], capture_output=True, text=True,
                               check=False, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return "unknown"
    return "available" if shown.returncode == 0 and shown.stdout.strip() else "missing"


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
    root = dotnet_root(tools)
    lines.append(environment + (f"; DOTNET_ROOT {root}." if root else "."))
    return lines
