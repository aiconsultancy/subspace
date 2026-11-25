#!/usr/bin/python3
"""Memory and task limits for the fleet's engines, and swap for the fleet VM.

The installer sizes each officer's engine from the VM's memory and the fleet's shape, writes the limits into the
engine's systemd unit, makes sure the VM has swap, and says what it chose and why: in the dry run, at the end of an
install and in `subspace-fleetctl memory`. See "Memory limits and swap" in subspace-installer/README.md.

Sizes are whole MiB throughout. A limit that is off is OFF; a swap allowance of 0 means no swap for that engine.
"""

import os
from pathlib import Path
import re
import subprocess

MIB = 1024 * 1024
GIB = 1024  # MiB
OFF = "none"
# The installer's own value for a limit: in a role's block or an officer's own memory, over a less specific one.
AUTO = "auto"

# What the OS and the fleet's shared services need beside the engines: the gateway, the model proxy, the dashboard
# and AgentsView; the Subspace API with its PostgreSQL, and Ergo with Docker, when they run on this VM.
RESERVE = 2 * GIB
API_RESERVE = GIB
IRC_RESERVE = GIB // 2
# An engine at rest: a captain's OpenCode server with its crew's sessions, and the rear admiral's.
RESTING = {"captain": GIB, "rear_admiral": GIB // 2}
# A captain's hard cap is its share of the VM if up to this many ships build at full stretch at once. It is unlikely
# that more do, and the cap exists to stop one ship taking the whole machine, so the caps may add up to more.
BUILDING = 3
CAPTAIN_FLOOR = 2 * GIB
# No build these fleets run needs more; a process that wants more is almost certainly a runaway.
CAPTAIN_CEILING = 32 * GIB
REAR_FLOOR = GIB
REAR_CEILING = 4 * GIB
# A typical build or test run. A computed captain's cap below it means the VM is small for its fleet.
BUILD = 4 * GIB
TASKS = 4096
# The smallest hard cap or soft limit fleet.json may set (OpenCode itself needs a few hundred MiB), and task limit.
MINIMUM = 512
MIN_TASKS = 256
MAX_TASKS = 4194304
OOM_POLICY = "continue"
# The gateway and the model proxy have no cap: the kernel kills an agent's process before them when the whole VM
# runs out of memory.
SHARED_OPTIONS = ["OOMScoreAdjust=-500"]
KEYS = ("max", "high", "swapMax", "tasksMax")
ROLE_KEYS = {"captain": "captain", "rear_admiral": "rearAdmiral"}
DIRECTIVES = {"MemoryHigh": "high", "MemoryMax": "max", "MemorySwapMax": "swapMax", "TasksMax": "tasksMax",
              "OOMPolicy": "oomPolicy"}

# Swap: one file on the root filesystem, an eighth of memory, between 2 and 16 GiB and at most a quarter of the
# free space; the kernel then swaps only under pressure.
SWAP_FILE = Path("/subspace-fleet.swap")
SWAP_MIN = 2 * GIB
SWAP_MAX = 16 * GIB
SWAPPINESS = 10
SWAP_FILESYSTEMS = ("ext2", "ext3", "ext4", "xfs")
SYSCTL_FILE = Path("/etc/sysctl.d/60-subspace-fleet-swap.conf")
FSTAB_MARK = "# The Subspace fleet's swap file (subspace-installer/README.md, 'Memory limits and swap')."
MEMINFO = Path("/proc/meminfo")
PROC_SWAPS = Path("/proc/swaps")
MOUNTS = Path("/proc/self/mounts")
FSTAB = Path("/etc/fstab")
WAAGENT = Path("/etc/waagent.conf")
SWAP_UNIT_DIRS = (Path("/etc/systemd/system"),)
SYSCTL_PATHS = (Path("/etc/sysctl.conf"), Path("/etc/sysctl.d"), Path("/run/sysctl.d"), Path("/usr/local/lib/sysctl.d"),
                Path("/usr/lib/sysctl.d"))
SWAPPINESS_PROC = Path("/proc/sys/vm/swappiness")
# Where systemd looks for a unit's drop-ins, highest priority first: a file wins over one of the same name further
# down, and all of them are then applied in the order of their names, after the unit file itself.
DROPIN_DIRS = (Path("/etc/systemd/system.control"), Path("/run/systemd/system.control"), Path("/run/systemd/transient"),
               Path("/etc/systemd/system"), Path("/run/systemd/system"))
CGROUP = Path("/sys/fs/cgroup")
FLEET_UNIT = re.compile(r"subspace-fleet-[a-z0-9@-]+\.service")


class Refused(ValueError):
    """A memory setting the installer cannot use; runtime.py reports it as invalid configuration."""


class FstabUnreadable(OSError):
    """/etc/fstab exists but cannot be read, so the installer must not add to it."""


# Sizes -----------------------------------------------------------------------

SIZE = re.compile(r"([0-9]{1,7}(?:\.[0-9]{1,3})?)\s*([MGT])(?:i?B)?", re.IGNORECASE)
FACTORS = {"M": 1, "G": GIB, "T": GIB * GIB}


def is_zero(value):
    return (type(value) is int and value == 0) or value == "0"


def parse_size(value, label, minimum=MINIMUM, zero=OFF):
    """A size from fleet.json or an option, in MiB: 20G, 1.5G, 512M (G is GiB and M MiB, as in systemd). none turns
    the limit off, and so does 0, except where zero says otherwise (a swap allowance of 0 is no swap)."""
    if isinstance(value, str) and value.strip().lower() == OFF:
        return OFF
    if is_zero(value):
        return zero
    match = SIZE.fullmatch(value.strip()) if isinstance(value, str) else None
    if not match:
        raise Refused(f"{label} must be a size such as 20G or 512M (G is GiB and M is MiB), or none.")
    mib = int(float(match.group(1)) * FACTORS[match.group(2).upper()])
    if mib < minimum:
        raise Refused(f"{label} must be at least {unit_size(minimum)}, or none.")
    return mib


def parse_tasks(value, label):
    if value == OFF or is_zero(value):
        return OFF
    if type(value) is int and MIN_TASKS <= value <= MAX_TASKS:
        return value
    raise Refused(f"{label} must be a number of tasks from {MIN_TASKS} to {MAX_TASKS}, or none.")


def limits(value, label):
    """One level of limits: fleet.json's memory, its captain or rearAdmiral block, or an officer's own memory."""
    if not isinstance(value, dict) or not value.keys() <= set(KEYS):
        raise Refused(f"{label} must be an object with any of max, high, swapMax and tasksMax.")
    result = {}
    for key, raw in value.items():
        if isinstance(raw, str) and raw.strip().lower() == AUTO:
            result[key] = AUTO
        elif key == "tasksMax":
            result[key] = parse_tasks(raw, f"{label}.tasksMax")
        elif key == "swapMax":
            result[key] = parse_size(raw, f"{label}.swapMax", minimum=0, zero=0)
        else:
            result[key] = parse_size(raw, f"{label}.{key}")
    return result


def swap_setting(value, label="memory.swap"):
    """auto (the default), a size, or none (also 0 or false): the installer leaves swap as it is."""
    if value == "auto":
        return value
    if value is False:
        return OFF
    return parse_size(value, label)


def config(value):
    """fleet.json's memory block: fleet-wide limits, the captain and rearAdmiral blocks, swap, and localApi, which
    says the Subspace API runs on this VM (subspace-setup sets it whatever the API's sign-in)."""
    allowed = {*KEYS, "swap", "localApi", *ROLE_KEYS.values()}
    if not isinstance(value, dict) or not value.keys() <= allowed:
        raise Refused("memory must be an object with any of max, high, swapMax, tasksMax, captain, rearAdmiral, swap "
                      "and localApi.")
    if not isinstance(value.get("localApi", False), bool):
        raise Refused("memory.localApi must be true or false.")
    return {"fleet": limits({key: value[key] for key in KEYS if key in value}, "memory"),
            **{role: limits(value.get(key, {}), f"memory.{key}") for role, key in ROLE_KEYS.items()},
            "swap": swap_setting(value.get("swap", "auto")), "localApi": value.get("localApi", False)}


def unit_size(mib):
    """A size as systemd takes it: 20G, 3840M, 0."""
    if mib == 0:
        return "0"
    return f"{mib // GIB}G" if mib % GIB == 0 else f"{mib}M"


def gib(mib):
    """A size for people: 20 GiB, 62.7 GiB, 768 MiB."""
    if mib < GIB:
        return f"{mib} MiB"
    value = mib / GIB
    return f"{value:.0f} GiB" if mib % GIB == 0 else f"{value:.1f} GiB"


def shown(value):
    return "none" if value == OFF else gib(value) if value else "0 (no swap)"


def round_down(mib):
    """To a whole GiB from 4 GiB up, to 256 MiB below."""
    step = GIB if mib >= 4 * GIB else 256
    return max(0, mib) // step * step


def nearest(mib, step):
    return (mib + step // 2) // step * step


def soft(cap):
    """The soft limit: a tenth under the hard cap, to a whole GiB (256 MiB for a cap under 4 GiB)."""
    step = GIB if cap >= 4 * GIB else 256
    return max(256, cap * 9 // 10 // step * step)


def allowance(cap):
    """The swap allowance: a fifth of the hard cap, to the nearest GiB, at least 512 MiB."""
    return max(512, nearest(cap // 5, GIB))


# Sizing ----------------------------------------------------------------------

def total_memory():
    """The VM's memory (MemTotal) in MiB, or None where /proc/meminfo cannot be read."""
    try:
        for line in MEMINFO.read_text().splitlines():
            if line.startswith("MemTotal:"):
                return int(line.split()[1]) // 1024
    except (OSError, ValueError, IndexError):
        pass
    return None


def reserve(api_here=False, irc_here=False):
    return RESERVE + (API_RESERVE if api_here else 0) + (IRC_RESERVE if irc_here else 0)


def captain_cap(total, captains, held):
    """A captain's computed hard cap, and what decided it: its share if up to three ships build at once, what one
    ship can take while everything else keeps its room (the OS and services, the rear admiral up to its own cap,
    every other captain at rest), the ceiling or the floor."""
    share = total // min(captains, BUILDING)
    room = total - held - rear_cap(total) - (captains - 1) * RESTING["captain"]
    cap, basis = min((share, "share"), (room, "room"), (CAPTAIN_CEILING, "ceiling"), key=lambda item: item[0])
    cap = round_down(cap)
    if cap < CAPTAIN_FLOOR:
        return CAPTAIN_FLOOR, "floor"
    return cap, basis


def rear_cap(total):
    """The rear admiral's computed hard cap: an eighth of the VM, between 1 and 4 GiB."""
    return min(REAR_CEILING, max(REAR_FLOOR, nearest(total // 8, 256)))


def minimum_memory(captains, held):
    """The least memory, in whole GiB, at which the sizing gives every captain the BUILD it needs."""
    return next(size for size in range(GIB, 1024 * GIB, GIB) if captain_cap(size, captains, held)[0] >= BUILD)


def plan(settings, officers, total, api_here=False, irc_here=False):
    """Each officer's limits, where each came from, and the warnings to give.

    settings is config()'s result; officers lists {"id", "role", "memory"} with the rear admiral first; total is the
    VM's memory in MiB, or None when unknown (then only limits set in fleet.json apply). Each limit resolves from
    the computed default, then fleet.json's memory, its role block, then the officer's own memory, where auto
    returns to the computed default; a soft limit and a swap allowance nobody set follow the hard cap.
    """
    captains = sum(person["role"] == "captain" for person in officers)
    held = reserve(api_here, irc_here)
    computed, basis = {}, None
    if total:
        if captains:
            computed["captain"], basis = captain_cap(total, captains, held)
        computed["rear_admiral"] = rear_cap(total)
    result, sources = {}, {}
    for person in officers:
        role = person["role"]
        chosen, origin = {}, {}
        for label, level in (("memory", settings["fleet"]), (f"memory.{ROLE_KEYS[role]}", settings[role]),
                             (f"{person['id']}'s memory", person.get("memory") or {})):
            for key, value in level.items():
                if value == AUTO:
                    chosen.pop(key, None)
                    origin.pop(key, None)
                else:
                    chosen[key], origin[key] = value, f"{label}.{key}"
        hard = chosen.get("max", computed.get(role, OFF))
        limited = hard != OFF
        result[person["id"]] = {
            "max": hard,
            "high": chosen["high"] if "high" in chosen else soft(hard) if limited else OFF,
            "swapMax": chosen["swapMax"] if "swapMax" in chosen else allowance(hard) if limited else OFF,
            "tasksMax": chosen.get("tasksMax", TASKS),
        }
        sources[person["id"]] = origin
    return {"total": total, "reserve": held, "captains": captains, "computed": computed, "basis": basis,
            "roles": {person["id"]: person["role"] for person in officers}, "limits": result, "sources": sources,
            "warnings": warnings(total, captains, held, computed, result)}


def warnings(total, captains, held, computed, result):
    found = []
    if total and captains and computed["captain"] < BUILD:
        at_rest = held + RESTING["rear_admiral"] + captains * RESTING["captain"]
        text = (f"this VM's {gib(total)} is small for {count(captains, 'captain')}: the sizing gives each captain a "
                f"hard cap of {gib(computed['captain'])}, below the {gib(BUILD)} a typical build or test run needs, so "
                "builds may be killed for memory.")
        if at_rest > total:
            text += f" The fleet at rest alone needs about {gib(at_rest)}."
        text += (f" Use a VM with at least {gib(minimum_memory(captains, held))} of memory, or fewer captains. The "
                 "install goes ahead.")
        found.append(text)
    caps = [limits["max"] for limits in result.values() if limits["max"] != OFF]
    if total and sum(caps) > total:
        found.append(f"the hard caps add up to {gib(sum(caps))} on this {gib(total)} VM. That is allowed: a cap stops "
                     f"one ship taking the whole machine, and the sizing expects at most {WORDS[BUILDING]} ships to build at "
                     "full stretch at once. If several ships will run heavy builds at the same time, lower the "
                     "captains' cap (subspace-setup --captain-memory SIZE, or memory.captain.max in fleet.json) until "
                     "the caps add up to less than the VM.")
    grouped = {}
    for agent_id, limits in result.items():
        if total and limits["max"] != OFF and limits["max"] > total:
            grouped.setdefault(("above", limits["max"]), []).append(agent_id)
        if OFF not in (limits["high"], limits["max"]) and limits["high"] >= limits["max"]:
            grouped.setdefault(("soft", limits["high"], limits["max"]), []).append(agent_id)
    for key, ids in grouped.items():
        if key[0] == "above":
            found.append(f"the hard cap of {join(ids)}, {gib(key[1])}, is more than the VM's {gib(total)}, so the whole "
                         "VM runs out of memory before the cap takes effect.")
        else:
            found.append(f"the soft limit of {join(ids)}, {gib(key[1])}, is not below the hard cap, {gib(key[2])}, so "
                         "it has no effect.")
    return found


def unit_options(limits):
    """The [Service] lines that apply an officer's limits."""
    lines = [f"OOMPolicy={OOM_POLICY}"]
    for key, directive in (("high", "MemoryHigh"), ("max", "MemoryMax"), ("swapMax", "MemorySwapMax")):
        if limits[key] != OFF:
            lines.append(f"{directive}={unit_size(limits[key])}")
    if limits["tasksMax"] != OFF:
        lines.append(f"TasksMax={limits['tasksMax']}")
    return lines


# Reports ---------------------------------------------------------------------

def count(number, noun):
    return f"{number} {noun}{'' if number == 1 else 'es' if noun.endswith('s') else 's'}"


def join(items):
    items = list(items)
    return items[0] if len(items) == 1 else f"{', '.join(items[:-1])} and {items[-1]}"


FRACTIONS = {1: "all of", 2: "half", 3: "a third of"}
WORDS = {1: "one", 2: "two", 3: "three"}


def plan_lines(result):
    """What the dry run and the install summary say about the engines' limits, and why."""
    total = result["total"]
    lines = []
    if total:
        lines.append(f"Memory: this VM has {gib(total)}. Each officer's engine gets these limits, sized from that and "
                     f"the fleet's {count(result['captains'], 'captain')} (installer README, 'Memory limits and swap'):")
    else:
        lines.append("Memory: the installer cannot read this VM's memory (/proc/meminfo), so only the limits set in "
                     "fleet.json apply:")
    groups = {}
    for agent_id, limits in result["limits"].items():
        role = result["roles"][agent_id]
        key = (role, tuple(limits.items()), tuple(sorted(result["sources"][agent_id].items())))
        groups.setdefault(key, []).append(agent_id)
    for (role, limits, sources), ids in groups.items():
        limits, sources = dict(limits), dict(sources)
        who = (f"rear admiral {ids[0]}" if role == "rear_admiral" else
               f"captain{'' if len(ids) == 1 else 's'} {join(ids)}, crew included")
        parts = []
        for key, label in (("max", "hard cap"), ("high", "soft limit"), ("swapMax", "swap")):
            parts.append(f"{label} {shown(limits[key])}" + (f" ({sources[key]})" if key in sources else ""))
        tasks = "no task limit" if limits["tasksMax"] == OFF else f"at most {limits['tasksMax']} tasks"
        parts.append(tasks + (f" ({sources['tasksMax']})" if "tasksMax" in sources else ""))
        lines.append(f"  {who}: {', '.join(parts)}.")
    lines.append(f"  Every engine has OOMPolicy={OOM_POLICY}: when a process in it is killed for memory, the engine, "
                 "and a captain's other crew, keep running.")
    if total:
        lines.append("  Why: " + why(result))
    lines.append("  The gateway and the model proxy have no cap, so a leak in either cannot take the fleet down by "
                 f"itself; {SHARED_OPTIONS[0]} has the kernel kill an agent's process before them if the whole VM runs "
                 "out of memory.")
    lines.extend(f"Memory warning: {text}" for text in result["warnings"])
    return lines


def why(result):
    computed, held = result["computed"], result["reserve"]
    text = ""
    if "captain" in computed:
        decides = {
            "share": f"its share decides, {FRACTIONS[min(result['captains'], BUILDING)]} the VM",
            "room": "what is left for one ship decides",
            "ceiling": f"the {gib(CAPTAIN_CEILING)} ceiling decides, which no build these fleets run needs to pass",
            "floor": f"the VM is too small for more than the {gib(CAPTAIN_FLOOR)} floor",
        }[result["basis"]]
        text = (f"a captain's hard cap is its share of the VM if up to {WORDS[BUILDING]} ships build at full stretch "
                f"at once, between {gib(CAPTAIN_FLOOR)} and {gib(CAPTAIN_CEILING)}, and never more than leaves "
                f"{gib(held)} for the OS and the fleet's services, the rear admiral its cap and every other captain "
                f"{gib(RESTING['captain'])} at rest; here {decides}. The rear admiral's")
    else:
        text = "the rear admiral's hard cap"
    return (text + f" is an eighth of the VM, between {gib(REAR_FLOOR)} and {gib(REAR_CEILING)}. A soft limit is a "
            "tenth under its hard cap, and a swap allowance a fifth of it.")


# Hand-made limits --------------------------------------------------------------

def systemd_size(value, total):
    """A memory directive's value as systemd reads it, in MiB; OFF for infinity or an empty (reset) value."""
    value = value.strip()
    if value in ("", "infinity"):
        return OFF
    if value.endswith("%") and total:
        try:
            return int(total * float(value[:-1]) / 100)
        except ValueError:
            return None
    match = re.fullmatch(r"([0-9]+(?:\.[0-9]+)?)([KMGTPE]?)", value)
    if not match:
        return None
    power = "KMGTPE".find(match.group(2)) + 1 if match.group(2) else 0
    return int(float(match.group(1)) * 1024 ** power) // MIB


def systemd_value(directive, value, total):
    """A hand-made value in the installer's terms, for comparison: MiB, a task count, OFF or the text itself."""
    if directive == "OOMPolicy":
        return value.strip()
    if directive == "TasksMax":
        value = value.strip()
        return OFF if value in ("", "infinity") else int(value) if value.isdigit() else value
    return systemd_size(value, total)


def dropin_files(unit):
    """The unit's drop-in files outside the installer's unit file, in the order systemd applies them."""
    files = {}
    for directory in DROPIN_DIRS:
        folder = directory / f"{unit}.d"
        try:
            paths = sorted(folder.glob("*.conf")) if folder.is_dir() else []
        except OSError:
            continue
        for path in paths:
            files.setdefault(path.name, path)
    return [files[name] for name in sorted(files)]


def hand_made(unit, total):
    """The limits set on a unit by hand, in drop-ins: {directive: (value, raw text, file)}, the one that applies."""
    found = {}
    for path in dropin_files(unit):
        try:
            text = path.read_text()
        except (OSError, UnicodeError):
            continue
        section = None
        for line in text.splitlines():
            line = line.strip()
            if not line or line[0] in "#;":
                continue
            if line.startswith("["):
                section = line
                continue
            key, equals, value = line.partition("=")
            if section == "[Service]" and equals and key.strip() in DIRECTIVES:
                found[key.strip()] = (systemd_value(key.strip(), value, total), value.strip(), path)
    return found


def only_limits(path):
    """Whether a drop-in holds nothing but limits, so removing it removes only them."""
    try:
        lines = [line.strip() for line in path.read_text().splitlines()]
    except (OSError, UnicodeError):
        return False
    return all(not line or line[0] in "#;" or line == "[Service]" or line.partition("=")[0].strip() in DIRECTIVES
               for line in lines)


def installer_value(limits, directive):
    return OOM_POLICY if directive == "OOMPolicy" else limits[DIRECTIVES[directive]]


def value_text(directive, value):
    if directive in ("OOMPolicy", "TasksMax") or value in (OFF, None) or isinstance(value, str):
        return str(value)
    return unit_size(value)


def hand_made_lines(officers, result):
    """The dry run's and the summary's account of limits set by hand, which win over the installer's because
    systemd applies drop-ins after the unit file, and the commands that hand them to the installer.
    officers maps each officer's ID to its unit."""
    total = result["total"]
    entries, commands = [], []
    for agent_id, unit in officers.items():
        found = hand_made(unit, total)
        if not found:
            continue
        limits = result["limits"][agent_id]
        places, same, files = {}, True, []
        for directive, (value, raw, path) in found.items():
            mine = installer_value(limits, directive)
            same = same and value == mine
            text = f"{directive}={value_text(directive, value) if value is not None else raw}"
            if value != mine:
                text += f" (the installer's: {value_text(directive, mine)})"
            places.setdefault(path.parent, []).append((text, path))
            if not only_limits(path):
                commands.append(f"  edit {path}: remove its {directive} line")
            elif path not in files:
                files.append(path)
        parts = []
        for folder, items in places.items():
            where = folder if len(items) > 1 else items[0][1]
            parts.append(f"{', '.join(text for text, _ in items)} in {where}")
        entries.append(f"  {agent_id}: {'; '.join(parts)}" + ("; the same as the installer's." if same else "."))
        if files:
            commands.append("  sudo rm -- " + " ".join(str(path) for path in files))
    if not entries:
        return []
    return ["Memory: limits set by hand on this VM win over the installer's, because systemd applies drop-ins after "
            "the unit file. The installer leaves them as they are:", *entries,
            "Memory: to hand them to the installer, run these, then the installer again:", *commands,
            "  sudo systemctl daemon-reload"]


# Swap -------------------------------------------------------------------------

def unescape(name):
    return re.sub(r"\\([0-7]{3})", lambda match: chr(int(match.group(1), 8)), name)


def read(path):
    """A system file's text for the installer to inspect, or "" when it cannot be read. Never used to rewrite one."""
    try:
        return path.read_bytes().decode(errors="replace")
    except OSError:
        return ""


def mount_of(path):
    """The filesystem type and mount point that hold path."""
    best = ("", "")
    for line in read(MOUNTS).splitlines():
        parts = line.split()
        if len(parts) < 3:
            continue
        point = unescape(parts[1])
        if (str(path) == point or str(path).startswith(point.rstrip("/") + "/")) and len(point) >= len(best[1]):
            best = (parts[2], point)
    return best


def sysctl_files():
    """Configuration files, other than the installer's, that set vm.swappiness: [(path, value)], in the order
    systemd-sysctl applies them (by file name, /etc/sysctl.conf as 99-sysctl.conf), so the last one wins."""
    found = []
    for location in SYSCTL_PATHS:
        try:
            paths = [location] if location.is_file() else sorted(location.glob("*.conf")) if location.is_dir() else []
        except OSError:
            continue
        for path in paths:
            if path == SYSCTL_FILE:
                continue
            for line in read(path).splitlines():
                key, equals, value = line.strip().lstrip("-").partition("=")
                if equals and key.strip().replace("/", ".") == "vm.swappiness":
                    found.append((path, value.strip()))
    return sorted(found, key=lambda item: "99-sysctl.conf" if item[0].name == "sysctl.conf" else item[0].name)


def swap_state():
    """The VM's swap as the installer finds it: what is on, swap configured but off, a cloud agent's swap, the
    filesystem and free space for the installer's swap file, and vm.swappiness with the files that set it."""
    active = []
    for line in read(PROC_SWAPS).splitlines()[1:]:
        parts = line.split()
        try:
            # The kernel reports a swap area's size in KiB without its header page (an 8 GiB file shows as
            # 8388604), so the size in MiB is rounded up to what the file or partition holds.
            active.append({"name": unescape(parts[0]), "size": -(-int(parts[2]) // 1024),
                           "used": int(parts[3]) // 1024})
        except (IndexError, ValueError):
            continue
    names = {entry["name"] for entry in active}
    configured = []
    for line in read(FSTAB).splitlines():
        parts = line.split()
        if len(parts) >= 3 and not parts[0].startswith("#") and parts[2] == "swap" and unescape(parts[0]) not in names \
                and "noauto" not in (parts[3].split(",") if len(parts) > 3 else []):
            configured.append(unescape(parts[0]))
    for directory in SWAP_UNIT_DIRS:
        try:
            units = sorted(directory.glob("*.swap")) if directory.is_dir() else []
        except OSError:
            units = []
        configured.extend(unit.name for unit in units if not (unit.is_symlink() and os.readlink(unit) == "/dev/null"))
    agent = None
    if re.search(r"^\s*ResourceDisk\.EnableSwap\s*=\s*(y|yes|true)\s*$", read(WAAGENT), re.IGNORECASE | re.MULTILINE):
        agent = f"the Azure Linux agent (ResourceDisk.EnableSwap in {WAAGENT})"
    filesystem, point = mount_of(SWAP_FILE.parent)
    try:
        info = os.statvfs(SWAP_FILE.parent)
        free = info.f_bavail * info.f_frsize // MIB
    except OSError:
        free = 0
    try:
        swappiness = int(read(SWAPPINESS_PROC).strip())
    except ValueError:
        swappiness = None
    return {"active": active, "configured": configured, "agent": agent, "filesystem": filesystem,
            "mount": point or "/", "free": free, "swappiness": swappiness, "swappinessFiles": sysctl_files(),
            "own": SWAP_FILE.exists(), "ownSettings": SYSCTL_FILE.exists()}


def swap_plan(setting, state, total):
    """What the installer does about swap: {"action": create, activate, keep, skip or off, "size", "reason"}."""
    own = [entry for entry in state["active"] if entry["name"] == str(SWAP_FILE)]
    others = [entry for entry in state["active"] if entry["name"] != str(SWAP_FILE)]
    if setting == OFF:
        return {"action": "off", "size": 0, "reason": "the installer leaves swap as it is, as memory.swap is none"}
    wanted = setting
    if setting == "auto":
        wanted = min(SWAP_MAX, max(SWAP_MIN, nearest((total or 0) // 8, GIB)))
    if own:
        # Only a size set in memory.swap is compared with the file: auto's rule depends on the disk's free space,
        # which the file itself and everything since have changed, so it is not a size the file should have now.
        return {"action": "keep", "size": own[0]["size"], "wanted": None if setting == "auto" else setting,
                "reason": "the installer's swap file is on"}
    if others:
        listed = ", ".join(f"{entry['name']} ({gib(entry['size'])})" for entry in others)
        return {"action": "skip", "size": 0, "reason": f"this VM already has swap: {listed}"}
    elsewhere = [name for name in state["configured"] if name != str(SWAP_FILE)]
    if elsewhere:
        return {"action": "skip", "size": 0, "reason": f"swap is configured but not on ({', '.join(elsewhere)}), so "
                "the installer leaves swap to that configuration"}
    if state["agent"]:
        return {"action": "skip", "size": 0, "reason": f"{state['agent']} manages swap on this VM"}
    if str(SWAP_FILE) in state["configured"] and state["own"]:
        return {"action": "activate", "size": wanted, "reason": "the installer's swap file is set up but not on"}
    if state["filesystem"] not in SWAP_FILESYSTEMS:
        return {"action": "skip", "size": 0, "reason": f"{state['mount']} is {state['filesystem'] or 'unknown'}, "
                "which cannot hold a plain swap file; add swap yourself (installer README, 'Memory limits and swap')"}
    free = state["free"]
    if setting == "auto":
        size = min(wanted, free // 4 // GIB * GIB)
        if size < GIB:
            return {"action": "skip", "size": 0, "reason": f"the disk is tight ({gib(free)} free on "
                    f"{state['mount']}), so the installer adds no swap file; free some space and run the installer "
                    "again"}
        basis = (f"an eighth of the VM's memory, between {gib(SWAP_MIN)} and {gib(SWAP_MAX)}, and at most a quarter "
                 f"of the {gib(free)} free on {state['mount']}")
    else:
        size = wanted
        if size > free // 2:
            return {"action": "skip", "size": 0, "reason": f"the disk is tight for the {gib(size)} memory.swap asks "
                    f"for, more than half the {gib(free)} free on {state['mount']}, so the installer adds no swap file"}
        basis = "memory.swap"
    return {"action": "create", "size": size, "reason": basis}


def swappiness_plan(setting, state, has_swap):
    """What the installer does about vm.swappiness: set (its own setting), keep (another file sets it) or leave."""
    now = f"the kernel's vm.swappiness is {state['swappiness']}"
    if setting == OFF or (not has_swap and not state["swappinessFiles"]):
        return {"action": "leave", "before": now}
    if state["swappinessFiles"]:
        path, value = state["swappinessFiles"][-1]
        return {"action": "keep", "before": f"{now}, set to {value} in {path}, which the installer leaves"}
    return {"action": "set",
            "before": f"the installer will set vm.swappiness to {SWAPPINESS} ({SYSCTL_FILE}), so the kernel swaps only "
                      "under pressure"}


def sentence(text):
    return text[0].upper() + text[1:] + ("" if text.endswith(".") else ".")


def installed_swappiness(state):
    """What sets vm.swappiness on the VM now: another file, the installer's own, or nothing."""
    now = f"the kernel's vm.swappiness is {state['swappiness']}"
    if state["swappinessFiles"]:
        path, value = state["swappinessFiles"][-1]
        return f"{now}, set to {value} in {path}, which the installer leaves"
    if state["ownSettings"]:
        return f"{now}, the installer's setting ({SYSCTL_FILE}), so it swaps only under pressure"
    return now


def swap_lines(setting, state, total, installed=False, managed=True):
    """The dry run's swap decision, or after an install (installed) the swap the VM has now and why. managed is
    false for a run that leaves swap alone (--no-systemd), dry or not."""
    if not managed:
        return [f"Swap: {current_swap(state)} With --no-systemd the installer leaves swap as it is."]
    decision = swap_plan(setting, state, total)
    action = decision["action"]
    has_swap = bool(state["active"]) or not installed and action in ("create", "activate")
    tuning = swappiness_plan(setting, state, has_swap)
    if installed:
        text = current_swap(state)
        if action == "off" or action == "skip" and not decision["reason"].startswith("this VM already has swap"):
            text += " " + sentence(decision["reason"])
        elif action in ("create", "activate"):
            text += " The installer could not set it up (see above); run the installer again to retry."
    elif action == "create":
        text = (f"this VM has no swap. The installer will create a swap file of {gib(decision['size'])}, "
                f"{SWAP_FILE} ({decision['reason']}), and turn it on now and at every boot ({FSTAB}).")
    elif action == "activate":
        text = f"{decision['reason']}. The installer will turn it on, now and at every boot."
    elif action == "keep":
        text = f"{SWAP_FILE}, the installer's swap file, is on ({gib(decision['size'])}) and stays."
    else:
        text = f"{decision['reason']}."
    # Sizes within 1 MiB are the same file: /proc/swaps leaves out the header page.
    if action == "keep" and decision["wanted"] is not None and abs(decision["wanted"] - decision["size"]) > 1:
        text += (f" memory.swap asks for {gib(decision['wanted'])}, and the installer never resizes a swap file in "
                 f"use: to resize it, run sudo swapoff {SWAP_FILE} && sudo rm {SWAP_FILE}, then the installer again.")
    text += " " + sentence(installed_swappiness(state) if installed else tuning["before"])
    lines = [f"Swap: {text}"]
    if not has_swap and setting != OFF:
        lines.append("Swap warning: the VM has no swap, so the engines' swap allowances have no effect and a "
                     "runaway process meets its hard cap with nothing to spill into.")
    return lines


def current_swap(state):
    if not state["active"]:
        return "this VM has no swap."
    parts = []
    for entry in state["active"]:
        owner = " (the installer's)" if entry["name"] == str(SWAP_FILE) else ""
        parts.append(f"{entry['name']}{owner}, {gib(entry['size'])} with "
                     f"{gib(entry['used']) if entry['used'] else 'none'} in use")
    return f"this VM has swap: {'; '.join(parts)}."


def write_root_file(path, text, mode=0o644):
    temporary = path.with_name(f".{path.name}.subspace-fleet")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, mode)
    with os.fdopen(descriptor, "w") as stream:
        stream.write(text)
    os.chmod(temporary, mode)
    os.replace(temporary, path)


def read_fstab():
    """/etc/fstab's bytes as they are, or b"" when there is none. Any other failure to read it raises
    FstabUnreadable: the installer then leaves the file untouched rather than write it from what it could not read."""
    try:
        return FSTAB.read_bytes()
    except FileNotFoundError:
        return b""
    except OSError as error:
        raise FstabUnreadable(f"could not read {FSTAB} ({error.strerror or error})") from None


def add_to_fstab():
    """The swap file's entry, appended to /etc/fstab unless it is there already. The file's bytes are never
    decoded or rewritten: whatever encoding or entries it has stay exactly as they are."""
    existing = read_fstab()
    if any(line.split()[:1] == [os.fsencode(SWAP_FILE)] for line in existing.splitlines()):
        return
    entry = f"{FSTAB_MARK}\n{SWAP_FILE} none swap sw 0 0\n".encode()
    if existing and not existing.endswith(b"\n"):
        entry = b"\n" + entry
    created = not existing and not FSTAB.exists()
    descriptor = os.open(FSTAB, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)
    try:
        if created:
            os.fchmod(descriptor, 0o644)
        os.write(descriptor, entry)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def create_swap_file(size, run=subprocess.run):
    """A swap file of size MiB at SWAP_FILE, on now and at every boot. Preallocated first; if swapon refuses that,
    written out in full and tried once more."""
    def write_out():
        with open(SWAP_FILE, "wb") as stream:
            block = bytes(MIB)
            for _ in range(size):
                stream.write(block)

    def turn_on():
        run(["mkswap", str(SWAP_FILE)], check=True, stdout=subprocess.DEVNULL)
        run(["swapon", str(SWAP_FILE)], check=True)

    SWAP_FILE.unlink(missing_ok=True)  # Only ever an inactive leftover of the installer's own.
    descriptor = os.open(SWAP_FILE, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        if hasattr(os, "posix_fallocate"):
            os.posix_fallocate(descriptor, 0, size * MIB)
    finally:
        os.close(descriptor)
    if not hasattr(os, "posix_fallocate"):
        write_out()
    try:
        turn_on()
    except subprocess.CalledProcessError:
        write_out()
        turn_on()
    add_to_fstab()


def apply_swap(setting, state, total, run=subprocess.run):
    """Make sure the VM has swap, as swap_plan decides, and set vm.swappiness. Never raises: a failure is reported
    and the install carries on."""
    decision = swap_plan(setting, state, total)
    lines = []
    try:
        if decision["action"] in ("create", "activate"):
            # Before anything changes: a swap file that cannot be added to /etc/fstab is not set up at all.
            read_fstab()
    except FstabUnreadable as error:
        lines.append(f"Swap warning: {error}, so the installer leaves it untouched and sets up no swap file. The "
                     "install carries on without it; run the installer again once the file can be read.")
        decision = {"action": "skip"}
    try:
        if decision["action"] == "create":
            create_swap_file(decision["size"], run)
            lines.append(f"Swap: created {SWAP_FILE} ({gib(decision['size'])}, {decision['reason']}), on now and at "
                         f"every boot ({FSTAB}).")
        elif decision["action"] == "activate":
            run(["swapon", str(SWAP_FILE)], check=True)
            add_to_fstab()
            lines.append(f"Swap: turned {SWAP_FILE} on, now and at every boot.")
    except (OSError, subprocess.CalledProcessError) as error:
        if decision["action"] == "create":
            try:
                run(["swapoff", str(SWAP_FILE)], check=False, stderr=subprocess.DEVNULL)
                SWAP_FILE.unlink(missing_ok=True)
            except OSError:
                pass
        lines.append(f"Swap warning: could not set up {SWAP_FILE} ({error}). The install carries on without it; run "
                     "the installer again to retry.")
        decision = {"action": "skip"}
    has_swap = bool(state["active"]) or decision["action"] in ("create", "activate")
    tuning = swappiness_plan(setting, state, has_swap)
    try:
        if tuning["action"] == "set":
            write_root_file(SYSCTL_FILE, f"# Written by the Subspace fleet installer: swap only under pressure.\n"
                                         f"vm.swappiness = {SWAPPINESS}\n")
            SWAPPINESS_PROC.write_text(f"{SWAPPINESS}\n")
        elif tuning["action"] == "keep" and SYSCTL_FILE.exists():
            # Another file sets it, which is the operator's choice: the installer's own would only compete.
            SYSCTL_FILE.unlink()
    except OSError as error:
        lines.append(f"Swap warning: could not set vm.swappiness ({error}).")
    return lines


# Live view (subspace-fleetctl memory) --------------------------------------------

PROPERTIES = ("Id", "ActiveState", "MemoryCurrent", "MemoryPeak", "MemorySwapCurrent", "MemoryHigh", "MemoryMax",
              "MemorySwapMax", "TasksCurrent", "TasksMax", "OOMPolicy", "NRestarts", "Result", "ControlGroup")


def show_units(units, run=subprocess.run):
    """systemctl show's properties for each unit: {unit: {property: value}}."""
    output = run(["systemctl", "show", f"--property={','.join(PROPERTIES)}", "--", *units], capture_output=True,
                 text=True, check=False).stdout
    result = {}
    for block in output.split("\n\n"):
        values = dict(line.split("=", 1) for line in block.splitlines() if "=" in line)
        if values.get("Id"):
            result[values["Id"]] = values
    return result


def live_size(value):
    if value in (None, "", "[not set]", "infinity") or not value.isdigit():
        return OFF if value == "infinity" else None
    return int(value) // MIB


def oom_events(control_group):
    """The memory.events oom_kill count of a unit's cgroup, since the unit last started."""
    for line in read(CGROUP / control_group.lstrip("/") / "memory.events").splitlines():
        key, _, value = line.partition(" ")
        if key == "oom_kill" and value.strip().isdigit():
            return int(value)
    return None


def kernel_log(run=subprocess.run):
    """The kernel's messages since boot, from the journal, or None when journalctl cannot read them."""
    try:
        done = run(["journalctl", "-k", "-b", "0", "-o", "short-iso", "--no-pager", "-q"], capture_output=True,
                   text=True, errors="replace", check=False)
    except OSError:
        return None
    return done.stdout if done.returncode == 0 else None


def kills(text):
    """The fleet's processes the kernel killed for memory, from its log: [{time, unit, task, pid, rss, whole,
    limit}]. whole when the whole VM had run out; otherwise limit is the cgroup whose limit the process met, as the
    kernel names it (oom_memcg), or None where its log does not say."""
    found, sizes = [], {}
    for line in text.splitlines():
        killed = re.search(r"Killed process (\d+) \(([^)]*)\).*?anon-rss:(\d+)kB", line)
        if killed:
            sizes[killed.group(1)] = int(killed.group(3)) // 1024
        event = re.search(r"oom-kill:(.*)$", line)
        if not event:
            continue
        details = event.group(1).split(",")
        values = dict(item.split("=", 1) for item in details if "=" in item)
        unit = next((part for part in values.get("task_memcg", "").split("/") if FLEET_UNIT.fullmatch(part)), None)
        if unit:
            found.append({"time": line.split()[0], "unit": unit, "task": values.get("task", "?"),
                          "pid": values.get("pid", "?"), "rss": None,
                          "whole": values.get("constraint") == "CONSTRAINT_NONE" or "global_oom" in details,
                          "limit": values.get("oom_memcg")})
    for kill in found:
        kill["rss"] = sizes.get(kill["pid"])
    return found


def live_lines(officers, result, state, show=None, log=None):
    """`subspace-fleetctl memory`: each engine's limits as systemd applies them now and its use, the VM's swap,
    and every fleet process the kernel has killed for memory since boot. officers maps IDs to units."""
    show = show_units(list(officers.values())) if show is None else show
    total = result["total"]
    lines = [f"Memory: this VM has {gib(total) + ' of memory' if total else 'an unknown amount of memory'}. Each "
             "officer's engine as systemd runs it now:"]
    differences = []
    for agent_id, unit in officers.items():
        values = show.get(unit, {})
        if not values:
            lines.append(f"  {agent_id} ({unit}): not loaded.")
            continue
        used, peak = live_size(values.get("MemoryCurrent")), live_size(values.get("MemoryPeak"))
        live = {"max": live_size(values.get("MemoryMax")), "high": live_size(values.get("MemoryHigh")),
                "swapMax": live_size(values.get("MemorySwapMax"))}
        tasks = values.get("TasksMax", "")
        live["tasksMax"] = OFF if tasks == "infinity" else int(tasks) if tasks.isdigit() else None
        events = oom_events(values.get("ControlGroup", "")) if values.get("ControlGroup") else None
        parts = [f"{values.get('ActiveState', 'unknown')}",
                 f"using {gib(used) if used is not None else 'unknown'}"
                 + (f" (peak {gib(peak)})" if isinstance(peak, int) else ""),
                 f"hard cap {shown(live['max']) if live['max'] is not None else 'unknown'}",
                 f"soft limit {shown(live['high']) if live['high'] is not None else 'unknown'}",
                 f"swap {shown(live['swapMax']) if live['swapMax'] is not None else 'unknown'}",
                 f"tasks {values.get('TasksCurrent', '?')} of {tasks or '?'}",
                 f"OOMPolicy={values.get('OOMPolicy', '?')}",
                 f"restarts {values.get('NRestarts', '?')}, last result {values.get('Result', '?')}"]
        if events:
            # The cgroup's oom_kill count: kills for memory of any kind, at its cap or with the whole VM out of
            # memory. The journal, below, says which.
            parts.append(f"{count(events, 'process')} killed for memory since it started")
        lines.append(f"  {agent_id} ({unit}): {', '.join(parts)}.")
        expected = result["limits"].get(agent_id)
        if expected:
            for key, label in (("max", "hard cap"), ("high", "soft limit"), ("swapMax", "swap"), ("tasksMax", "tasks")):
                if live[key] is not None and live[key] != expected[key]:
                    mine = expected[key] if key == "tasksMax" else shown(expected[key])
                    have = live[key] if key == "tasksMax" else shown(live[key])
                    differences.append(f"{agent_id}'s {label} is {have}, the installer's {mine}")
    if differences:
        lines.append("Memory: these live limits differ from the installer's for this VM: " + "; ".join(differences)
                     + ". A limit set by hand wins (see below); otherwise the VM or the fleet changed since the last "
                     "install: run the installer again.")
    lines.append(f"Swap: {current_swap(state)}"
                 + (f" vm.swappiness is {state['swappiness']}." if state["swappiness"] is not None else ""))
    log = kernel_log() if log is None else log
    if log is None:
        lines.append("Memory: could not read the journal, so the fleet processes killed for memory since boot are "
                     "unknown (sudo journalctl -k -b).")
    else:
        events = kills(log)
        if not events:
            lines.append("Memory: no fleet process has been killed for memory since boot.")
        else:
            lines.append(f"Memory: the kernel killed {count(len(events), 'fleet process')} for memory since boot:")
            for event in events:
                size = f", {gib(event['rss'])} resident" if event["rss"] is not None else ""
                if event["whole"]:
                    scope = "the whole VM had run out of memory, which takes engines down with it; caps stop that"
                elif event["limit"] and event["limit"].rstrip("/").rsplit("/", 1)[-1] == event["unit"]:
                    scope = "it reached its engine's hard cap, which kept the rest of the VM safe"
                elif event["limit"]:
                    scope = f"it reached the memory limit of {event['limit']}"
                else:
                    scope = "the kernel's log does not say which memory limit it reached"
                lines.append(f"  {event['time']} {event['unit']}: {event['task']} (pid {event['pid']}){size}; {scope}.")
    return lines
