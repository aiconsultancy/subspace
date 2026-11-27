"""Offline checks for the engines' memory and task limits and the fleet VM's swap (lib/memory.py).

Run with Python 3.10 or later: python3 -B subspace-installer/tests/memory_test.py
"""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import memory  # noqa: E402
from memory import GIB, OFF  # noqa: E402
import runtime  # noqa: E402

runtime.readable_without_credentials = lambda url: True

# MemTotal of VMs sold as 16, 32, 64 and 128 GB, as /proc/meminfo reports them (in MiB).
VMS = {16: 16036, 32: 32082, 64: 64154, 128: 128307}
EXAMPLE = Path(__file__).resolve().parents[1] / "fleet.example.json"
# Fictional captains, after the example's orion.
CAPTAINS = ["orion", "rigel", "deneb", "mira", "sirius", "castor", "pollux", "spica", "hadar", "naos", "nashira",
            "alcor", "mizar", "alnair", "ankaa", "atria", "avior", "merak"]


def officers(captains, own=None):
    own = own or {}
    return [{"id": "atlas", "role": "rear_admiral", "memory": own.get("atlas", {})},
            *({"id": name, "role": "captain", "memory": own.get(name, {})} for name in CAPTAINS[:captains])]


def sized(total, captains, settings=None, local=False, own=None):
    own = {agent: memory.limits(value, f"{agent}'s memory") for agent, value in (own or {}).items()}
    return memory.plan(memory.config(settings or {}), officers(captains, own), total, api_here=local, irc_here=local)


def shape(limits):
    return tuple(memory.unit_size(limits[key]) if limits[key] != OFF else OFF for key in ("max", "high", "swapMax"))


def fleet(captains=1, **changes):
    """fleet.example.json with the given captains (crew-less, so no repositories are needed), validated."""
    config = json.loads(EXAMPLE.read_text())
    config.update(mockupPreviews={"enabled": False}, impeccable=False)
    config["captains"] = [{"id": name, "name": name.title()} for name in CAPTAINS[:captains]]
    config.update(changes)
    return config


def state(**changes):
    """A VM's swap as swap_state() reads it: by default a fresh cloud image, no swap, ext4 with 100 GiB free."""
    return {"active": [], "configured": [], "agent": None, "filesystem": "ext4", "mount": "/", "free": 100 * GIB,
            "swappiness": 60, "swappinessFiles": [], "own": False, "ownSettings": False, **changes}


def swaps(*entries):
    """swap_state() on a VM whose /proc/swaps lists entries, each (name, size in KiB as the kernel reports it), and
    which has no other swap configuration."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "swaps").write_text("Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n"
                                    + "".join(f"{name}\tfile\t\t{kib}\t\t0\t\t-2\n" for name, kib in entries))
        with patch.multiple(memory, PROC_SWAPS=root / "swaps", FSTAB=root / "fstab", WAAGENT=root / "waagent.conf",
                            MOUNTS=root / "mounts", SYSCTL_PATHS=(), SWAPPINESS_PROC=root / "swappiness",
                            SWAP_UNIT_DIRS=()):
            return memory.swap_state()


class SizingTests(unittest.TestCase):
    def test_the_rule_for_common_vm_sizes_and_fleet_shapes(self):
        # (hard cap, soft limit, swap allowance) for a captain, with a hosted API and a shared IRC server.
        expected = {
            (16, 1): ("11G", "9G", "2G"), (16, 4): ("5G", "4G", "1G"), (16, 8): ("4G", "3G", "1G"),
            (32, 1): ("25G", "22G", "5G"), (32, 4): ("10G", "9G", "2G"), (32, 8): ("10G", "9G", "2G"),
            (64, 1): ("32G", "28G", "6G"), (64, 4): ("20G", "18G", "4G"), (64, 8): ("20G", "18G", "4G"),
            (128, 1): ("32G", "28G", "6G"), (128, 4): ("32G", "28G", "6G"), (128, 8): ("32G", "28G", "6G"),
        }
        rear = {16: ("2G", "1792M", "512M"), 32: ("4G", "3G", "1G"), 64: ("4G", "3G", "1G"), 128: ("4G", "3G", "1G")}
        for (size, captains), caps in expected.items():
            with self.subTest(size=size, captains=captains):
                result = sized(VMS[size], captains)
                self.assertEqual({shape(result["limits"][name]) for name in CAPTAINS[:captains]}, {caps})
                self.assertEqual(shape(result["limits"]["atlas"]), rear[size])
                self.assertEqual({limits["tasksMax"] for limits in result["limits"].values()}, {4096})
                # Never refused, and never a warning that the VM is small, at these sizes.
                self.assertFalse(any("is small for" in text for text in result["warnings"]))
        # Every hard cap is well below the machine, and leaves the OS, the services and the rear admiral room.
        for size, total in VMS.items():
            for captains in (1, 2, 3, 4, 8):
                cap = sized(total, captains)["limits"]["orion"]["max"]
                self.assertLessEqual(cap, total - memory.RESERVE - memory.rear_cap(total))

    def test_the_owners_model_is_what_a_64_gb_vm_with_four_captains_gets(self):
        result = sized(VMS[64], 4)
        self.assertEqual(memory.unit_options(result["limits"]["orion"]),
                         ["OOMPolicy=continue", "MemoryHigh=18G", "MemoryMax=20G", "MemorySwapMax=4G", "TasksMax=4096"])
        self.assertEqual(memory.unit_options(result["limits"]["atlas"]),
                         ["OOMPolicy=continue", "MemoryHigh=3G", "MemoryMax=4G", "MemorySwapMax=1G", "TasksMax=4096"])
        self.assertEqual(result["basis"], "share")
        # The caps add up to more than the machine, on purpose, and the operator hears so.
        [warning] = result["warnings"]
        self.assertIn("the hard caps add up to 84 GiB on this 62.7 GiB VM. That is allowed", warning)

    def test_services_on_the_vm_leave_less_room_on_a_small_one(self):
        self.assertEqual(memory.reserve(), 2 * GIB)
        self.assertEqual(memory.reserve(api_here=True, irc_here=True), 3 * GIB + 512)
        self.assertEqual(shape(sized(VMS[16], 1, local=True)["limits"]["orion"]), ("10G", "9G", "2G"))
        self.assertEqual(shape(sized(VMS[32], 1, local=True)["limits"]["orion"]), ("23G", "20G", "5G"))
        # On a big VM the share decides, wherever the services run.
        self.assertEqual(sized(VMS[64], 4, local=True)["limits"]["orion"], sized(VMS[64], 4)["limits"]["orion"])

    def test_a_small_vm_gets_sane_values_and_a_plain_warning(self):
        # The local VM's default size with the example's two captains, the API and Ergo on it.
        result = sized(7936, 2, local=True)
        self.assertEqual(shape(result["limits"]["orion"]), ("2304M", "2G", "512M"))
        self.assertEqual(shape(result["limits"]["atlas"]), ("1G", "768M", "512M"))
        small = next(text for text in result["warnings"] if "is small for" in text)
        self.assertIn("this VM's 7.8 GiB is small for 2 captains: the sizing gives each captain a hard cap of "
                      "2.2 GiB, below the 4 GiB a typical build or test run needs", small)
        self.assertIn("Use a VM with at least 10 GiB of memory, or fewer captains. The install goes ahead.", small)
        self.assertEqual(memory.captain_cap(10 * GIB, 2, memory.reserve(True, True))[0], 4 * GIB)
        # Too small even for the fleet at rest: the floor, and the warning says so; still not refused.
        result = sized(3900, 2, local=True)
        self.assertEqual((result["limits"]["orion"]["max"], result["basis"]), (2 * GIB, "floor"))
        self.assertIn("The fleet at rest alone needs about 6 GiB.", result["warnings"][0])
        self.assertIn("the VM is too small for more than the 2 GiB floor", memory.why(result))

    def test_many_captains_share_the_machine(self):
        result = sized(VMS[64], 18)
        self.assertEqual({shape(limits) for agent, limits in result["limits"].items() if agent != "atlas"},
                         {("20G", "18G", "4G")})
        self.assertIn("the hard caps add up to 364 GiB", result["warnings"][-1])
        # On a smaller VM, eighteen captains at rest leave one ship less room than its share.
        result = sized(VMS[32], 18, local=True)
        self.assertEqual((shape(result["limits"]["orion"]), result["basis"]), (("6G", "5G", "1G"), "room"))
        self.assertIn("what is left for one ship decides", memory.why(result))

    def test_unknown_memory_renders_only_what_fleet_json_sets(self):
        result = sized(None, 1)
        self.assertEqual(memory.unit_options(result["limits"]["orion"]), ["OOMPolicy=continue", "TasksMax=4096"])
        result = sized(None, 1, {"captain": {"max": "8G"}})
        self.assertEqual(shape(result["limits"]["orion"]), ("8G", "7G", "2G"))
        self.assertEqual(result["warnings"], [])
        self.assertIn("cannot read this VM's memory", memory.plan_lines(result)[0])
        with patch.object(memory, "MEMINFO", Path("/nonexistent/meminfo")):
            self.assertIsNone(memory.total_memory())
        with tempfile.NamedTemporaryFile("w", suffix="meminfo") as stream:
            stream.write("MemTotal:       65694012 kB\nMemFree:         1000 kB\n")
            stream.flush()
            with patch.object(memory, "MEMINFO", Path(stream.name)):
                self.assertEqual(memory.total_memory(), 64154)


class OverrideTests(unittest.TestCase):
    def test_the_officer_beats_its_role_which_beats_the_fleet_which_beats_the_sizing(self):
        settings = {"max": "16G", "tasksMax": 2048, "captain": {"max": "24G", "swapMax": "2G"}}
        own = {"rigel": {"max": "40G", "high": "30G"}}
        result = sized(VMS[64], 2, settings, own=own)
        self.assertEqual(result["limits"]["orion"], {"max": 24 * GIB, "high": 21 * GIB, "swapMax": 2 * GIB,
                                                     "tasksMax": 2048})
        self.assertEqual(result["sources"]["orion"], {"max": "memory.captain.max", "swapMax": "memory.captain.swapMax",
                                                      "tasksMax": "memory.tasksMax"})
        self.assertEqual(result["limits"]["rigel"], {"max": 40 * GIB, "high": 30 * GIB, "swapMax": 2 * GIB,
                                                     "tasksMax": 2048})
        self.assertEqual(result["sources"]["rigel"]["max"], "rigel's memory.max")
        # The fleet-wide max reaches the rear admiral, whose role sets nothing.
        self.assertEqual(shape(result["limits"]["atlas"]), ("16G", "14G", "3G"))
        lines = memory.plan_lines(result)
        self.assertIn("  captain orion, crew included: hard cap 24 GiB (memory.captain.max), soft limit 21 GiB, swap "
                      "2 GiB (memory.captain.swapMax), at most 2048 tasks (memory.tasksMax).", lines)

    def test_auto_is_the_sizing_over_a_less_specific_setting(self):
        sizing = sized(VMS[64], 4)["limits"]
        # subspace-setup --captain-memory auto on a fleet with a fleet-wide cap: captains get the VM-sized cap, the
        # rear admiral, whose role sets nothing, the fleet-wide one, and an officer's own cap stays.
        result = sized(VMS[64], 4, {"max": "16G", "captain": {"max": "auto"}}, own={"rigel": {"max": "24G"}})
        self.assertEqual(result["limits"]["orion"], sizing["orion"])
        self.assertEqual(result["sources"]["orion"], {})
        self.assertEqual(shape(result["limits"]["rigel"]), ("24G", "21G", "5G"))
        self.assertEqual(shape(result["limits"]["atlas"]), ("16G", "14G", "3G"))
        self.assertIn("  captains orion, deneb and mira, crew included: hard cap 20 GiB, soft limit 18 GiB, swap 4 GiB, at "
                      "most 4096 tasks.", memory.plan_lines(result))
        # auto in an officer's own memory beats its role's, for any limit.
        result = sized(VMS[64], 4, {"captain": {"max": "12G", "high": "10G", "tasksMax": 1024}},
                       own={"orion": {"max": "auto", "high": "Auto", "tasksMax": "auto"}})
        self.assertEqual(result["limits"]["orion"], sizing["orion"])
        self.assertEqual(shape(result["limits"]["rigel"]), ("12G", "10G", "2G"))

    def test_none_or_zero_turns_a_limit_off_and_a_zero_swap_allowance_means_no_swap(self):
        result = sized(VMS[64], 1, {"captain": {"max": "none", "tasksMax": 0}, "rearAdmiral": {"swapMax": 0}})
        self.assertEqual(memory.unit_options(result["limits"]["orion"]), ["OOMPolicy=continue"])
        self.assertEqual(memory.unit_options(result["limits"]["atlas"]),
                         ["OOMPolicy=continue", "MemoryHigh=3G", "MemoryMax=4G", "MemorySwapMax=0", "TasksMax=4096"])
        result = sized(VMS[64], 1, {"max": 0, "high": "none", "swapMax": "none", "tasksMax": "none"})
        for limits in result["limits"].values():
            self.assertEqual(memory.unit_options(limits), ["OOMPolicy=continue"])
        self.assertIn("  rear admiral atlas: hard cap none (memory.max), soft limit none (memory.high), swap none "
                      "(memory.swapMax), no task limit (memory.tasksMax).", memory.plan_lines(result))
        # A soft limit alone, with no hard cap.
        result = sized(VMS[64], 1, {"captain": {"max": "none", "high": "12G"}})
        self.assertEqual(memory.unit_options(result["limits"]["orion"]),
                         ["OOMPolicy=continue", "MemoryHigh=12G", "TasksMax=4096"])

    def test_sizes_are_read_as_systemd_reads_them(self):
        for text, mib in (("20G", 20 * GIB), ("20GiB", 20 * GIB), ("20 GB", 20 * GIB), ("1.5G", 1536),
                          ("512M", 512), ("1T", 1024 * GIB), ("2g", 2 * GIB)):
            self.assertEqual(memory.parse_size(text, "x"), mib, text)
        self.assertEqual(memory.parse_size("256M", "x", minimum=0, zero=0), 256)
        self.assertEqual(memory.unit_size(1536), "1536M")
        self.assertEqual(memory.unit_size(4 * GIB), "4G")

    def test_bad_values_are_refused_naming_the_setting(self):
        for value, message in (
                ({"captain": {"max": "lots"}}, "memory.captain.max must be a size such as 20G"),
                ({"captain": {"max": "100M"}}, "memory.captain.max must be at least 512M"),
                ({"captain": {"max": 20}}, "memory.captain.max must be a size"),
                ({"rearAdmiral": {"tasksMax": 10}}, "memory.rearAdmiral.tasksMax must be a number of tasks from 256"),
                ({"captain": {"cpu": 2}}, "memory.captain must be an object with any of max, high"),
                ({"crew": {}}, "memory must be an object with any of"),
                ({"swap": "big"}, "memory.swap must be a size"),
                ({"swap": "100M"}, "memory.swap must be at least 512M")):
            with self.subTest(value):
                with self.assertRaisesRegex(runtime.Invalid, message):
                    runtime.validate(fleet(memory=value), {"proxyKey": "fixture-model-proxy-key"})
        config = fleet()
        config["captains"][0]["memory"] = {"max": "tiny"}
        with self.assertRaisesRegex(runtime.Invalid, "orion's memory.max must be a size"):
            runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"})
        config["captains"][0]["memory"] = {"max": "24G"}
        cfg = runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"})
        self.assertEqual(cfg["members"][1]["memory"], {"max": 24 * GIB})
        self.assertEqual(cfg["memory"]["swap"], "auto")
        self.assertEqual(memory.config({"swap": False})["swap"], OFF)
        self.assertEqual(memory.config({"swap": "8G"})["swap"], 8 * GIB)

    def test_overrides_that_cannot_work_are_warned_about_not_refused(self):
        result = sized(VMS[16], 2, {"captain": {"max": "40G", "high": "48G"}})
        self.assertIn("the hard cap of orion and rigel, 40 GiB, is more than the VM's 15.7 GiB, so the whole VM runs "
                      "out of memory before the cap takes effect.", result["warnings"])
        self.assertIn("the soft limit of orion and rigel, 48 GiB, is not below the hard cap, 40 GiB, so it has no "
                      "effect.", result["warnings"])


class ReportTests(unittest.TestCase):
    def test_the_dry_run_says_what_each_engine_gets_and_why(self):
        lines = memory.plan_lines(sized(VMS[64], 4))
        self.assertEqual(lines[0], "Memory: this VM has 62.7 GiB. Each officer's engine gets these limits, sized from "
                                   "that and the fleet's 4 captains (installer README, 'Memory limits and swap'):")
        self.assertEqual(lines[1], "  rear admiral atlas: hard cap 4 GiB, soft limit 3 GiB, swap 1 GiB, at most 4096 "
                                   "tasks.")
        self.assertEqual(lines[2], "  captains orion, rigel, deneb and mira, crew included: hard cap 20 GiB, soft limit "
                                   "18 GiB, swap 4 GiB, at most 4096 tasks.")
        self.assertIn("OOMPolicy=continue: when a process in it is killed for memory, the engine", lines[3])
        self.assertIn("its share of the VM if up to three ships build at full stretch at once, between 2 GiB and 32 "
                      "GiB", lines[4])
        self.assertIn("here its share decides, a third of the VM", lines[4])
        self.assertIn("The gateway and the model proxy have no cap", lines[5])
        self.assertTrue(lines[6].startswith("Memory warning: the hard caps add up to 84 GiB"))
        self.assertIn("subspace-setup --captain-memory SIZE", lines[6])
        # A single ship on a big VM: the ceiling decides, and the caps fit the machine.
        result = sized(VMS[64], 1)
        self.assertIn("the 32 GiB ceiling decides", memory.why(result))
        self.assertEqual(result["warnings"], [])

    def test_the_dry_run_and_the_summary_through_runtime(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)
        (root / "fleet.json").write_text(json.dumps(fleet(4, memory={"swap": "8G"})))
        (root / "fleet.secrets.json").write_text(json.dumps({"proxyKey": "fixture-model-proxy-key"}))
        (root / "fleet.secrets.json").chmod(0o600)
        output = io.StringIO()
        with patch.object(runtime, "ETC", root / "etc"), patch.object(runtime, "STATE", root / "state"), \
                patch.object(runtime, "SESSIONS", root / "etc/sessions"), \
                patch.object(memory, "total_memory", return_value=VMS[64]), \
                patch.object(memory, "swap_state", return_value=state()), \
                patch.object(memory, "DROPIN_DIRS", (root / "control",)), contextlib.redirect_stdout(output):
            runtime.main(["plan", str(root / "fleet.json"), str(root / "fleet.secrets.json"), "", "false"])
        lines = [line for line in output.getvalue().splitlines() if line.startswith(("Memory", "Swap", "  "))]
        self.assertIn("  captains orion, rigel, deneb and mira, crew included: hard cap 20 GiB, soft limit 18 GiB, swap "
                      "4 GiB, at most 4096 tasks.", lines)
        swap = next(line for line in lines if line.startswith("Swap:"))
        self.assertIn("The installer will create a swap file of 8 GiB, /subspace-fleet.swap (memory.swap)", swap)
        # A dry run with --no-systemd says what that run does: leave swap alone.
        output = io.StringIO()
        with patch.object(runtime, "ETC", root / "etc"), patch.object(runtime, "STATE", root / "state"), \
                patch.object(runtime, "SESSIONS", root / "etc/sessions"), \
                patch.object(memory, "total_memory", return_value=VMS[64]), \
                patch.object(memory, "swap_state", return_value=state()), \
                patch.object(memory, "DROPIN_DIRS", (root / "control",)), contextlib.redirect_stdout(output):
            runtime.main(["plan", str(root / "fleet.json"), str(root / "fleet.secrets.json"), "", "false", "no-systemd"])
        self.assertEqual([line for line in output.getvalue().splitlines() if line.startswith("Swap")],
                         ["Swap: this VM has no swap. With --no-systemd the installer leaves swap as it is."])
        # The install summary reads the installed fleet: the swap it has now.
        etc = root / "etc"
        etc.mkdir()
        for name, source in (("fleet.json", "fleet.json"), ("secrets.json", "fleet.secrets.json")):
            (etc / name).write_text((root / source).read_text())
            (etc / name).chmod(0o600)
        output = io.StringIO()
        installed = state(active=[{"name": str(memory.SWAP_FILE), "size": 8 * GIB, "used": 0}], own=True,
                          swappiness=10, ownSettings=True)
        with patch.object(runtime, "ETC", etc), patch.object(memory, "total_memory", return_value=VMS[64]), \
                patch.object(memory, "swap_state", return_value=installed), \
                patch.object(memory, "DROPIN_DIRS", (root / "control",)), contextlib.redirect_stdout(output):
            runtime.main(["memory-summary"])
        self.assertIn("Swap: this VM has swap: /subspace-fleet.swap (the installer's), 8 GiB with none in use. The "
                      "kernel's vm.swappiness is 10, the installer's setting", output.getvalue())
        # An install with --no-systemd leaves swap alone, and says so.
        output = io.StringIO()
        with patch.object(runtime, "ETC", etc), patch.object(memory, "total_memory", return_value=VMS[64]), \
                patch.object(memory, "swap_state", return_value=state()), \
                patch.object(memory, "DROPIN_DIRS", (root / "control",)), contextlib.redirect_stdout(output):
            runtime.main(["memory-summary", "no-systemd"])
        self.assertEqual(output.getvalue().splitlines()[-1],
                         "Swap: this VM has no swap. With --no-systemd the installer leaves swap as it is.")

    def test_the_gateway_line_says_which_gateway_takes_the_rear_admirals_limits(self):
        def plan(config):
            cfg = runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"})
            with patch.object(memory, "total_memory", return_value=VMS[64]), \
                    patch.object(memory, "swap_state", return_value=state()), patch.object(memory, "DROPIN_DIRS", ()):
                return [line for line in runtime.memory_lines(cfg) if "no cap" in line]

        config = fleet(2)
        self.assertEqual(plan(config), ["  The gateway and the model proxy have no cap, so a leak in either cannot take "
                                        "the fleet down by itself; OOMScoreAdjust=-500 has the kernel kill an agent's "
                                        "process before them if the whole VM runs out of memory."])
        # A Claude Agent SDK rear admiral's work runs in the fleet's own gateway, which takes its limits; the OpenCode
        # captains share another gateway, which has no cap.
        config["rearAdmiral"]["harness"] = {"type": "claude-sdk", "family": "claude"}
        self.assertEqual(plan(config), [
            "  The rear admiral's work runs in the fleet's gateway, subspace-fleet-gateway.service, which takes the "
            "rear admiral's limits. The model proxy and the OpenCode officers' gateway, "
            "subspace-fleet-gateway-opencode.service, have no cap, so a leak there cannot take the fleet down by "
            "itself; OOMScoreAdjust=-500 has the kernel kill an agent's process before them if the whole VM runs out "
            "of memory."])
        for captain in config["captains"]:
            captain.update(harness={"type": "claude-sdk", "family": "claude"},
                           crew={"claude": {"count": 0}, "codex": {"count": 0}})
        self.assertEqual(plan(config), [
            "  The rear admiral's work runs in the fleet's gateway, subspace-fleet-gateway.service, which takes the "
            "rear admiral's limits. The model proxy has no cap, so a leak there cannot take the fleet down by itself; "
            "OOMScoreAdjust=-500 has the kernel kill an agent's process before it if the whole VM runs out of "
            "memory."])

    def test_install_sh_tells_the_dry_run_about_no_systemd(self):
        """install.sh --dry-run hands --no-systemd to runtime.py plan, so the plan says what that run does."""
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)
        installer = root / "subspace-installer"
        (installer / "lib").mkdir(parents=True)
        # install.sh as it is, on a VM it accepts (Ubuntu 24.04, amd64), with a runtime.py that prints its arguments.
        release = root / "os-release"
        release.write_text("ID=ubuntu\nVERSION_ID=24.04\n")
        script = (Path(__file__).resolve().parents[1] / "install.sh").read_text()
        for old, new in (("/etc/os-release", str(release)), ("$(dpkg --print-architecture)", "amd64")):
            self.assertEqual(script.count(old), 1, old)
            script = script.replace(old, new)
        (installer / "install.sh").write_text(script)
        (installer / "lib/runtime.py").write_text("import sys\nprint(' '.join(sys.argv[1:]))\n")
        (root / "fleet.json").write_text(EXAMPLE.read_text())
        for flags, last in (([], "false"), (["--no-systemd"], "no-systemd")):
            with self.subTest(flags):
                result = subprocess.run(["bash", str(installer / "install.sh"), "--config", str(root / "fleet.json"),
                                         "--dry-run", *flags], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                plan = result.stdout.splitlines()[0].split()
                self.assertEqual((plan[0], plan[-1]), ("plan", last))

    def test_which_unit_each_officer_runs_in(self):
        config = fleet(2)
        config["captains"][1]["harness"] = {"type": "claude-sdk", "family": "claude"}
        config["captains"][1]["crew"] = {"claude": {"count": 0}, "codex": {"count": 0}}
        cfg = runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"})
        self.assertEqual([runtime.officer_unit(cfg, person) for person in cfg["members"]],
                         ["subspace-fleet-engine-atlas.service", "subspace-fleet-engine-orion.service",
                          "subspace-fleet-gateway-agent-rigel.service"])
        config["rearAdmiral"]["harness"] = {"type": "claude-sdk", "family": "claude"}
        cfg = runtime.validate(config, {"proxyKey": "fixture-model-proxy-key"})
        self.assertEqual(runtime.officer_unit(cfg, cfg["members"][0]), "subspace-fleet-gateway.service")
        # The API on this VM, and Ergo, count against the room left for the engines.
        local = fleet(1, api={"url": "http://127.0.0.1:3000"}, irc={"mode": "local", "allowedAccounts": ["admiral"],
                                                                     "compose": {"ref": "a" * 40, "operator": "ircop"}})
        cfg = runtime.validate(local, {"proxyKey": "fixture-model-proxy-key", "apiToken": "aaa.bbb.admin",
                                       "ircOperatorPassword": "fixture-operator-password",
                                       "ircPasswords": {"atlas": "fixture-password", "orion": "fixture-password"}},
                               check_files=False)
        self.assertEqual(runtime.memory_plan(cfg, VMS[16])["reserve"], memory.reserve(True, True))
        self.assertEqual(runtime.memory_plan(runtime.validate(fleet(1), {"proxyKey": "fixture-model-proxy-key"}),
                                             VMS[16])["reserve"], memory.RESERVE)
        # An API that signs in with WorkOS or Entra has no api block, and still runs here: memory.localApi says so.
        cfg = runtime.validate(fleet(1, memory={"localApi": True}), {"proxyKey": "fixture-model-proxy-key"})
        self.assertIsNone(cfg["api"])
        self.assertEqual(runtime.memory_plan(cfg, VMS[16])["reserve"], memory.reserve(api_here=True))
        self.assertEqual(shape(runtime.memory_plan(cfg, VMS[16])["limits"]["orion"]), ("10G", "9G", "2G"))
        with self.assertRaisesRegex(runtime.Invalid, "memory.localApi must be true or false"):
            runtime.validate(fleet(1, memory={"localApi": "yes"}), {"proxyKey": "fixture-model-proxy-key"})


SET_PROPERTY = "# This is a drop-in unit file extension, created via \"systemctl set-property\"\n" \
               "# or an equivalent operation. Do not edit.\n[Service]\n"


class HandMadeTests(unittest.TestCase):
    """Limits set on the VM by hand, before or beside the installer's: systemctl set-property or drop-in files."""

    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)
        self.control, self.runtime_control, self.system = root / "control", root / "run-control", root / "system"
        dirs = (self.control, self.runtime_control, root / "transient", self.system, root / "run-system")
        patcher = patch.object(memory, "DROPIN_DIRS", dirs)
        patcher.start()
        self.addCleanup(patcher.stop)

    def dropin(self, base, unit, name, text):
        folder = base / f"{unit}.d"
        folder.mkdir(parents=True, exist_ok=True)
        (folder / name).write_text(text)
        return folder / name

    def by_hand(self, captains):
        """The owner's limits, as `systemctl set-property` (without --runtime) leaves them, and an OOMPolicy drop-in."""
        files = []
        for agent, values in (("atlas", {"MemoryHigh": 3 * GIB, "MemoryMax": 4 * GIB, "MemorySwapMax": GIB}),
                              *((name, {"MemoryHigh": 18 * GIB, "MemoryMax": 20 * GIB, "MemorySwapMax": 4 * GIB})
                                for name in CAPTAINS[:captains])):
            unit = f"subspace-fleet-engine-{agent}.service"
            for key, mib in (*values.items(), ("TasksMax", None)):
                value = 4096 if mib is None else mib * memory.MIB
                files.append(self.dropin(self.control, unit, f"50-{key}.conf", f"{SET_PROPERTY}{key}={value}\n"))
            files.append(self.dropin(self.system, unit, "60-oom-policy.conf", "[Service]\nOOMPolicy=continue\n"))
        return files

    def test_hand_made_limits_win_and_the_installer_says_so_with_the_commands_to_hand_them_over(self):
        files = self.by_hand(4)
        result = sized(VMS[64], 4)
        units = {name: f"subspace-fleet-engine-{name}.service" for name in ["atlas", *CAPTAINS[:4]]}
        found = memory.hand_made(units["orion"], VMS[64])
        self.assertEqual({key: value[0] for key, value in found.items()},
                         {"MemoryHigh": 18 * GIB, "MemoryMax": 20 * GIB, "MemorySwapMax": 4 * GIB, "TasksMax": 4096,
                          "OOMPolicy": "continue"})
        lines = memory.hand_made_lines(units, result)
        self.assertEqual(lines[0], "Memory: limits set by hand on this VM win over the installer's, because systemd "
                                   "applies drop-ins after the unit file. The installer leaves them as they are:")
        # Every one of them is what the installer would set now, so each officer's line says so.
        folder = self.control / "subspace-fleet-engine-orion.service.d"
        self.assertEqual(lines[2], f"  orion: MemoryHigh=18G, MemoryMax=20G, MemorySwapMax=4G, TasksMax=4096 in "
                                   f"{folder}; OOMPolicy=continue in "
                                   f"{self.system / 'subspace-fleet-engine-orion.service.d/60-oom-policy.conf'}; the "
                                   "same as the installer's.")
        self.assertTrue(all(line.endswith("the same as the installer's.") for line in lines[1:6]))
        # One command per officer removes exactly the files that hold nothing but limits.
        self.assertEqual(lines[6], "Memory: to hand them to the installer, run these, then the installer again:")
        self.assertEqual(lines[7:12], ["  sudo rm -- " + " ".join(str(path) for path in files[start:start + 5])
                                       for start in range(0, 25, 5)])
        self.assertEqual(lines[12:], ["  sudo systemctl daemon-reload"])

    def test_a_tighter_hand_made_limit_is_kept_and_reported_never_loosened(self):
        self.dropin(self.control, "subspace-fleet-engine-orion.service", "50-MemoryMax.conf",
                    f"{SET_PROPERTY}MemoryMax={10 * GIB * memory.MIB}\n")
        result = sized(VMS[64], 4)
        line = memory.hand_made_lines({"orion": "subspace-fleet-engine-orion.service"}, result)[1]
        self.assertTrue(line.startswith("  orion: MemoryMax=10G (the installer's: 20G) in "), line)
        self.assertNotIn("the same as", line)

    def test_systemd_order_decides_which_hand_made_value_applies(self):
        unit = "subspace-fleet-engine-orion.service"
        # A runtime set-property (until the next reboot) and a persistent one of the same name: /etc wins.
        self.dropin(self.runtime_control, unit, "50-MemoryMax.conf", f"{SET_PROPERTY}MemoryMax=8G\n")
        self.dropin(self.control, unit, "50-MemoryMax.conf", f"{SET_PROPERTY}MemoryMax=12G\n")
        # A later name wins over an earlier one, whatever its directory; an empty value resets to no limit.
        mixed = self.dropin(self.system, unit, "70-local.conf",
                            "[Unit]\nMemoryMax=1G\n[Service]\nEnvironment=FOO=1\nMemoryHigh=\nTasksMax=50%\n")
        found = memory.hand_made(unit, VMS[64])
        self.assertEqual(found["MemoryMax"][0], 12 * GIB)
        self.assertEqual(found["MemoryHigh"][0], OFF)
        self.assertEqual(found["TasksMax"][0], "50%")
        self.assertFalse(memory.only_limits(mixed))
        lines = memory.hand_made_lines({"orion": unit}, sized(VMS[64], 1))
        self.assertEqual(lines[3:], [f"  edit {mixed}: remove its MemoryHigh line",
                                     f"  edit {mixed}: remove its TasksMax line",
                                     f"  sudo rm -- {self.control / (unit + '.d') / '50-MemoryMax.conf'}",
                                     "  sudo systemctl daemon-reload"])
        self.assertIn("MemoryMax=12G (the installer's: 32G) in ", lines[1])
        self.assertIn("MemoryHigh=none (the installer's: 28G), TasksMax=50% (the installer's: 4096) in ", lines[1])
        # A toolchain drop-in that sets no limit is not the installer's business.
        self.dropin(self.system, "subspace-fleet-engine-rigel.service", "50-agent-toolchains.conf",
                    "[Service]\nExecStart=\nExecStart=/usr/local/bin/engine\n")
        self.assertEqual(memory.hand_made_lines({"rigel": "subspace-fleet-engine-rigel.service"}, sized(VMS[64], 2)),
                         [])
        self.assertEqual(memory.systemd_size("30%", 10 * GIB), 3 * GIB)
        self.assertEqual(memory.systemd_size("2097152K", None), 2 * GIB)


class SwapTests(unittest.TestCase):
    def test_a_fresh_vm_gets_a_swap_file_sized_from_memory_and_disk(self):
        for size, total in VMS.items():
            with self.subTest(size):
                decision = memory.swap_plan("auto", state(), total)
                self.assertEqual((decision["action"], decision["size"]),
                                 ("create", {16: 2, 32: 4, 64: 8, 128: 16}[size] * GIB))
        # At most a quarter of the free space: 20 GiB free gives 5 GiB, not 8.
        self.assertEqual(memory.swap_plan("auto", state(free=20 * GIB), VMS[64])["size"], 5 * GIB)
        # A tight disk gets none, with a clear note; an explicit size more than half the free space neither.
        tight = memory.swap_plan("auto", state(free=3 * GIB), VMS[64])
        self.assertEqual(tight["action"], "skip")
        self.assertIn("the disk is tight (3 GiB free on /)", tight["reason"])
        self.assertEqual(memory.swap_plan(12 * GIB, state(free=20 * GIB), VMS[64])["action"], "skip")
        self.assertEqual(memory.swap_plan(8 * GIB, state(free=20 * GIB), VMS[64])["size"], 8 * GIB)
        self.assertEqual(memory.swap_plan("auto", state(), None)["size"], memory.SWAP_MIN)

    def test_swap_someone_else_manages_is_left_alone(self):
        cases = (
            (state(active=[{"name": "/swapfile", "size": 8 * GIB, "used": 100}]),
             "this VM already has swap: /swapfile (8 GiB)"),
            (state(active=[{"name": "/dev/zram0", "size": 4 * GIB, "used": 0}]), "already has swap: /dev/zram0"),
            (state(configured=["UUID=0000-EXAMPLE"]), "swap is configured but not on (UUID=0000-EXAMPLE)"),
            (state(configured=["swapfile.swap"]), "swap is configured but not on (swapfile.swap)"),
            (state(agent="the Azure Linux agent (ResourceDisk.EnableSwap in /etc/waagent.conf)"),
             "the Azure Linux agent (ResourceDisk.EnableSwap in /etc/waagent.conf) manages swap on this VM"),
            (state(filesystem="btrfs"), "/ is btrfs, which cannot hold a plain swap file"),
            (state(filesystem="overlay"), "/ is overlay, which cannot hold a plain swap file"),
        )
        for current, reason in cases:
            with self.subTest(reason):
                decision = memory.swap_plan("auto", current, VMS[64])
                self.assertEqual(decision["action"], "skip")
                self.assertIn(reason, decision["reason"])
        # Off: nothing is created, whatever the VM has.
        self.assertEqual(memory.swap_plan(OFF, state(), VMS[64])["action"], "off")

    def test_the_installers_own_swap_file_is_kept_never_resized(self):
        # The installer's 8 GiB swap file as /proc/swaps reports it, without its header page: 4 KiB, or 64 KiB on
        # an arm64 kernel with 64 KiB pages.
        for reported in (8388604, 8388544):
            with self.subTest(reported):
                own = {**swaps((str(memory.SWAP_FILE), reported)), "own": True, "swappiness": 10}
                self.assertEqual(own["active"][0]["size"], 8 * GIB)
                self.assertEqual(memory.swap_plan("auto", own, VMS[64])["action"], "keep")
                # Made by the sizing or at the size memory.swap sets, it is the size it should be: nothing to do.
                # A VM grown since, or a disk filled since, changes what auto would make, but not this file.
                for setting, total in (("auto", VMS[64]), (8 * GIB, VMS[64]), ("auto", VMS[128])):
                    [line] = memory.swap_lines(setting, own, total)
                    self.assertTrue(line.startswith("Swap: /subspace-fleet.swap, the installer's swap file, is on (8 "
                                                    "GiB) and stays. The installer will set vm.swappiness"), line)
                [line] = memory.swap_lines(16 * GIB, own, VMS[64])
                self.assertIn("/subspace-fleet.swap, the installer's swap file, is on (8 GiB) and stays. memory.swap "
                              "asks for 16 GiB, and the installer never resizes a swap file in use: to resize it, run "
                              "sudo swapoff /subspace-fleet.swap && sudo rm /subspace-fleet.swap, then the installer "
                              "again.", line)
        # Within 1 MiB is the same size, however the kernel rounds.
        close = state(active=[{"name": str(memory.SWAP_FILE), "size": 8 * GIB - 1, "used": 0}], own=True)
        self.assertNotIn("never resizes", memory.swap_lines(8 * GIB, close, VMS[64])[0])
        # Set up but off, after a reboot that lost it: turned on again.
        idle = state(configured=[str(memory.SWAP_FILE)], own=True)
        self.assertEqual(memory.swap_plan("auto", idle, VMS[64])["action"], "activate")

    def test_the_summary_says_what_the_vm_has_after_the_install(self):
        # The swap step failed, or never ran: the summary says the VM has none, and how to retry.
        lines = memory.swap_lines("auto", state(), VMS[64], installed=True)
        self.assertEqual(lines, ["Swap: this VM has no swap. The installer could not set it up (see above); run the "
                                 "installer again to retry. The kernel's vm.swappiness is 60.",
                                 "Swap warning: the VM has no swap, so the engines' swap allowances have no effect "
                                 "and a runaway process meets its hard cap with nothing to spill into."])
        lines = memory.swap_lines(OFF, state(), VMS[64], installed=True)
        self.assertEqual(lines, ["Swap: this VM has no swap. The installer leaves swap as it is, as memory.swap is "
                                 "none. The kernel's vm.swappiness is 60."])

    def test_swappiness_is_set_only_where_nothing_else_sets_it(self):
        self.assertEqual(memory.swappiness_plan("auto", state(), True)["action"], "set")
        kept = memory.swappiness_plan("auto", state(swappinessFiles=[(Path("/etc/sysctl.d/99-local.conf"), "30")],
                                                    swappiness=30), True)
        self.assertEqual(kept["action"], "keep")
        self.assertIn("set to 30 in /etc/sysctl.d/99-local.conf, which the installer leaves", kept["before"])
        self.assertEqual(memory.swappiness_plan(OFF, state(), True)["action"], "leave")
        self.assertEqual(memory.swappiness_plan("auto", state(), False)["action"], "leave")
        lines = memory.swap_lines("auto", state(filesystem="btrfs"), VMS[64])
        self.assertEqual(lines[1], "Swap warning: the VM has no swap, so the engines' swap allowances have no effect "
                                   "and a runaway process meets its hard cap with nothing to spill into.")

    def test_the_vms_swap_is_read_from_the_system(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)
        (root / "swaps").write_text("Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n"
                                    "/swap\\040file                         file\t\t8388604\t\t1024\t\t-2\n")
        (root / "fstab").write_text("# <file system> <mount point> <type>\nUUID=0000 / ext4 defaults 0 1\n"
                                    "/swap\\040file none swap sw 0 0\n/dev/sdb2 none swap noauto 0 0\n"
                                    "UUID=1111 none swap sw 0 0\n")
        (root / "waagent.conf").write_text("ResourceDisk.Format=y\nResourceDisk.EnableSwap=y\n")
        (root / "mounts").write_text("/dev/root / ext4 rw 0 0\n/dev/sdb1 /mnt ext4 rw 0 0\ntmpfs /run tmpfs rw 0 0\n")
        (root / "sysctl.d").mkdir()
        (root / "sysctl.d/99-local.conf").write_text("# local\nvm.swappiness = 30\n")
        (root / "sysctl.d/60-subspace-fleet-swap.conf").write_text("vm.swappiness = 10\n")
        (root / "swappiness").write_text("30\n")
        units = root / "units"
        units.mkdir()
        (units / "data-swap.swap").write_text("[Swap]\nWhat=/data/swap\n")
        (units / "masked.swap").symlink_to("/dev/null")
        with patch.multiple(memory, PROC_SWAPS=root / "swaps", FSTAB=root / "fstab", WAAGENT=root / "waagent.conf",
                            MOUNTS=root / "mounts", SYSCTL_PATHS=(root / "sysctl.d",),
                            SYSCTL_FILE=root / "sysctl.d/60-subspace-fleet-swap.conf",
                            SWAPPINESS_PROC=root / "swappiness", SWAP_UNIT_DIRS=(units,)):
            found = memory.swap_state()
        # An 8 GiB file: the kernel leaves its 4 KiB header page out of the size.
        self.assertEqual(found["active"], [{"name": "/swap file", "size": 8 * GIB, "used": 1}])
        # The noauto entry is not set to come on; the masked unit is off.
        self.assertEqual(found["configured"], ["UUID=1111", "data-swap.swap"])
        self.assertIn("ResourceDisk.EnableSwap", found["agent"])
        self.assertEqual((found["filesystem"], found["mount"]), ("ext4", "/"))
        self.assertEqual(found["swappiness"], 30)
        self.assertEqual(found["swappinessFiles"], [(root / "sysctl.d/99-local.conf", "30")])
        with patch.object(memory, "MOUNTS", root / "mounts"):
            self.assertEqual(memory.mount_of(Path("/mnt/resource")), ("ext4", "/mnt"))

    def test_making_swap_turns_it_on_now_and_at_every_boot(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)
        (root / "fstab").write_text("UUID=0000 / ext4 defaults 0 1")
        (root / "swappiness").write_text("60\n")
        commands = []

        def run(command, **_options):
            commands.append(command)
            return subprocess.CompletedProcess(command, 0)

        names = dict(SWAP_FILE=root / "subspace-fleet.swap", FSTAB=root / "fstab", SWAPPINESS_PROC=root / "swappiness",
                     SYSCTL_FILE=root / "60-subspace-fleet-swap.conf")
        with patch.multiple(memory, **names):
            # A small file, so the test writes little: an explicit size, in MiB.
            lines = memory.apply_swap(8, state(), 64, run=run)
            self.assertEqual(lines, [f"Swap: created {names['SWAP_FILE']} (8 MiB, memory.swap), on now and at every "
                                     f"boot ({names['FSTAB']})."])
            self.assertEqual(names["SWAP_FILE"].stat().st_size, 8 * memory.MIB)
            self.assertEqual(names["SWAP_FILE"].stat().st_mode & 0o777, 0o600)
            self.assertEqual(commands, [["mkswap", str(names["SWAP_FILE"])], ["swapon", str(names["SWAP_FILE"])]])
            self.assertEqual(names["FSTAB"].read_text(), f"UUID=0000 / ext4 defaults 0 1\n{memory.FSTAB_MARK}\n"
                                                         f"{names['SWAP_FILE']} none swap sw 0 0\n")
            self.assertEqual(names["SYSCTL_FILE"].read_text().splitlines()[-1], "vm.swappiness = 10")
            self.assertEqual(names["SWAPPINESS_PROC"].read_text(), "10\n")
            # Run again with the file on: nothing new, no second fstab entry.
            commands.clear()
            on = state(active=[{"name": str(names["SWAP_FILE"]), "size": 8, "used": 0}], own=True)
            self.assertEqual(memory.apply_swap(8, on, 64, run=run), [])
            self.assertEqual(commands, [])
            memory.add_to_fstab()
            self.assertEqual(names["FSTAB"].read_text().count("none swap"), 1)
            # Another file now sets vm.swappiness: the installer's own setting goes, so the two never compete.
            memory.apply_swap(8, {**on, "swappinessFiles": [(Path("/etc/sysctl.conf"), "30")]}, 64, run=run)
            self.assertFalse(names["SYSCTL_FILE"].exists())

    def test_fstab_is_only_ever_appended_to_and_never_written_from_a_failed_read(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)
        commands = []

        def run(command, **_options):
            commands.append(command)
            return subprocess.CompletedProcess(command, 0)

        names = dict(SWAP_FILE=root / "subspace-fleet.swap", FSTAB=root / "fstab", SWAPPINESS_PROC=root / "swappiness",
                     SYSCTL_FILE=root / "60-subspace-fleet-swap.conf")
        entry = f"{memory.FSTAB_MARK}\n{names['SWAP_FILE']} none swap sw 0 0\n".encode()
        # Not UTF-8 (a Latin-1 comment), with a data volume and no newline at the end: every byte stays.
        original = b"# Donn\xe9es\nUUID=0000 / ext4 defaults 0 1\nUUID=1111 /data ext4 defaults,nofail 0 2"
        names["FSTAB"].write_bytes(original)
        with patch.multiple(memory, **names):
            memory.apply_swap(8, state(), 64, run=run)
        self.assertEqual(names["FSTAB"].read_bytes(), original + b"\n" + entry)
        # A file that exists but cannot be read: no swap file, no command, the file as it was, and the reason.
        names["SWAP_FILE"].unlink()
        names["FSTAB"].write_bytes(original)
        commands.clear()
        real = {"read_bytes": Path.read_bytes, "read_text": Path.read_text}

        def refused(method):
            def read(path, *args, **kwargs):
                if path == names["FSTAB"]:
                    raise PermissionError(13, "Permission denied", str(path))
                return real[method](path, *args, **kwargs)
            return read

        with patch.multiple(memory, **names), patch.object(Path, "read_bytes", refused("read_bytes")), \
                patch.object(Path, "read_text", refused("read_text")):
            lines = memory.apply_swap(8, state(), 64, run=run)
            lines += memory.apply_swap("auto", state(configured=[str(names["SWAP_FILE"])], own=True), 64, run=run)
        self.assertEqual(lines, [f"Swap warning: could not read {names['FSTAB']} (Permission denied), so the installer "
                                 "leaves it untouched and sets up no swap file. The install carries on without it; run "
                                 "the installer again once the file can be read."] * 2)
        self.assertEqual((commands, names["SWAP_FILE"].exists()), ([], False))
        self.assertEqual(names["FSTAB"].read_bytes(), original)
        # No /etc/fstab at all: one is made, readable by everyone as fstab is.
        names["FSTAB"].unlink()
        with patch.multiple(memory, **names):
            memory.apply_swap(8, state(), 64, run=run)
        self.assertEqual((names["FSTAB"].read_bytes(), names["FSTAB"].stat().st_mode & 0o777), (entry, 0o644))

    def test_a_swap_file_that_cannot_be_turned_on_never_stops_the_install(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        root = Path(work.name)

        def run(command, **_options):
            if command[0] == "swapon":
                raise subprocess.CalledProcessError(255, command)
            return subprocess.CompletedProcess(command, 0)

        with patch.multiple(memory, SWAP_FILE=root / "subspace-fleet.swap", FSTAB=root / "fstab",
                            SWAPPINESS_PROC=root / "swappiness", SYSCTL_FILE=root / "60-subspace-fleet-swap.conf"):
            lines = memory.apply_swap(4, state(), 64, run=run)
        self.assertTrue(lines[0].startswith(f"Swap warning: could not set up {root / 'subspace-fleet.swap'}"))
        self.assertIn("The install carries on without it", lines[0])
        self.assertFalse((root / "subspace-fleet.swap").exists())
        self.assertFalse((root / "fstab").exists())
        # Without swap there is nothing for vm.swappiness to tune.
        self.assertFalse((root / "60-subspace-fleet-swap.conf").exists())


class LiveTests(unittest.TestCase):
    """subspace-fleetctl memory: the limits systemd applies now, and what the kernel killed for memory since boot."""

    # A kernel log like the one an engine without a cap leaves when one of its crew's test runs takes the VM's
    # memory, then a kill at a captain's own cap.
    LOG = "\n".join([
        "2026-01-02T03:04:05+0000 example-vm kernel: node invoked oom-killer: gfp_mask=0x140cca(GFP_HIGHUSER_MOVABLE)",
        "2026-01-02T03:04:05+0000 example-vm kernel: oom-kill:constraint=CONSTRAINT_NONE,nodemask=(null),cpuset=/,"
        "mems_allowed=0,global_oom,task_memcg=/system.slice/subspace-fleet-engine-orion.service,task=MainThread,"
        "pid=4242,uid=990",
        "2026-01-02T03:04:05+0000 example-vm kernel: Out of memory: Killed process 4242 (MainThread) "
        "total-vm:70000000kB, anon-rss:60000000kB, file-rss:0kB, shmem-rss:0kB, UID:990 pgtables:120000kB "
        "oom_score_adj:0",
        "2026-01-02T05:06:07+0000 example-vm kernel: oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),cpuset=/,"
        "mems_allowed=0,oom_memcg=/system.slice/subspace-fleet-engine-rigel.service,"
        "task_memcg=/system.slice/subspace-fleet-engine-rigel.service,task=cargo,pid=5151,uid=991",
        "2026-01-02T05:06:07+0000 example-vm kernel: Memory cgroup out of memory: Killed process 5151 (cargo) "
        "total-vm:21000000kB, anon-rss:20900000kB, file-rss:0kB, shmem-rss:0kB, UID:991 pgtables:40000kB",
        "2026-01-02T05:07:00+0000 example-vm kernel: oom-kill:constraint=CONSTRAINT_NONE,global_oom,"
        "task_memcg=/user.slice/user-1000.slice/session-1.scope,task=python3,pid=6000,uid=1000",
    ])

    def show(self):
        common = {"ActiveState": "active", "MemoryCurrent": str(600 * memory.MIB), "MemoryPeak": str(900 * memory.MIB),
                  "TasksCurrent": "42", "NRestarts": "0", "Result": "success", "OOMPolicy": "continue"}
        return {
            "subspace-fleet-engine-atlas.service": {**common, "Id": "subspace-fleet-engine-atlas.service",
                                                    "MemoryHigh": str(3 * GIB * memory.MIB),
                                                    "MemoryMax": str(4 * GIB * memory.MIB),
                                                    "MemorySwapMax": str(GIB * memory.MIB), "TasksMax": "4096"},
            "subspace-fleet-engine-orion.service": {**common, "Id": "subspace-fleet-engine-orion.service",
                                                    "MemoryHigh": "infinity", "MemoryMax": "infinity",
                                                    "MemorySwapMax": "infinity", "TasksMax": "71012",
                                                    "OOMPolicy": "stop", "NRestarts": "1", "Result": "oom-kill"},
        }

    def test_kills_since_boot_are_read_from_the_kernel_log(self):
        found = memory.kills(self.LOG)
        self.assertEqual(found, [
            {"time": "2026-01-02T03:04:05+0000", "unit": "subspace-fleet-engine-orion.service", "task": "MainThread",
             "pid": "4242", "rss": 58593, "whole": True, "limit": None},
            {"time": "2026-01-02T05:06:07+0000", "unit": "subspace-fleet-engine-rigel.service", "task": "cargo",
             "pid": "5151", "rss": 20410, "whole": False, "limit": "/system.slice/subspace-fleet-engine-rigel.service"},
        ])

    def test_the_live_view_shows_limits_use_differences_and_kills(self):
        result = sized(VMS[64], 1)
        units = {"atlas": "subspace-fleet-engine-atlas.service", "orion": "subspace-fleet-engine-orion.service"}
        lines = memory.live_lines(units, result, state(), show=self.show(), log=self.LOG)
        self.assertEqual(lines[0], "Memory: this VM has 62.7 GiB of memory. Each officer's engine as systemd runs it "
                                   "now:")
        self.assertEqual(lines[1], "  atlas (subspace-fleet-engine-atlas.service): active, using 600 MiB (peak 900 "
                                   "MiB), hard cap 4 GiB, soft limit 3 GiB, swap 1 GiB, tasks 42 of 4096, "
                                   "OOMPolicy=continue, restarts 0, last result success.")
        self.assertIn("hard cap none, soft limit none, swap none, tasks 42 of 71012, OOMPolicy=stop, restarts 1, last "
                      "result oom-kill", lines[2])
        self.assertIn("orion's hard cap is none, the installer's 32 GiB", lines[3])
        self.assertEqual(lines[4], "Swap: this VM has no swap. vm.swappiness is 60.")
        self.assertEqual(lines[5], "Memory: the kernel killed 2 fleet processes for memory since boot:")
        self.assertIn("subspace-fleet-engine-orion.service: MainThread (pid 4242), 57.2 GiB resident; the whole VM had "
                      "run out of memory", lines[6])
        self.assertIn("cargo (pid 5151), 19.9 GiB resident; it reached its engine's hard cap", lines[7])
        quiet = memory.live_lines(units, result, state(), show=self.show(), log="")
        self.assertIn("Memory: no fleet process has been killed for memory since boot.", quiet)
        with patch.object(memory, "kernel_log", return_value=None):
            unread = memory.live_lines(units, result, state(), show={})
        self.assertIn("  atlas (subspace-fleet-engine-atlas.service): not loaded.", unread)
        self.assertEqual(unread[-1], "Memory: could not read the journal, so the fleet processes killed for memory "
                                     "since boot are unknown (sudo journalctl -k -b).")

    def test_the_report_says_only_what_the_kernel_and_the_journal_say(self):
        # journalctl that fails, as without a journal or the right to read it: unknown, never "no kills".
        failed = lambda command, **_options: subprocess.CompletedProcess(command, 1, "", "No journal files.\n")  # noqa: E731
        self.assertIsNone(memory.kernel_log(run=failed))
        read = lambda command, **_options: subprocess.CompletedProcess(command, 0, self.LOG, "")  # noqa: E731
        self.assertEqual(memory.kernel_log(run=read), self.LOG)
        # The cgroup's oom_kill count is every kill for memory, the whole VM's included: not "at its cap".
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        cgroup = Path(work.name)
        events = cgroup / "system.slice/subspace-fleet-engine-atlas.service/memory.events"
        events.parent.mkdir(parents=True)
        events.write_text("low 0\nhigh 12\nmax 3\noom 2\noom_kill 2\noom_group_kill 0\n")
        show = self.show()
        show["subspace-fleet-engine-atlas.service"]["ControlGroup"] = "/system.slice/subspace-fleet-engine-atlas.service"
        units = {"atlas": "subspace-fleet-engine-atlas.service"}
        with patch.object(memory, "CGROUP", cgroup):
            lines = memory.live_lines(units, sized(VMS[64], 1), state(), show=show, log="")
        self.assertTrue(lines[1].endswith(", last result success, 2 processes killed for memory since it started."),
                        lines[1])
        # The journal names the cgroup whose limit a process met (oom_memcg): its own engine's, another's, or none.
        log = "\n".join([
            "2026-01-02T05:06:07+0000 example-vm kernel: oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),"
            "cpuset=/,mems_allowed=0,oom_memcg=/system.slice/subspace-fleet-engine-rigel.service,"
            "task_memcg=/system.slice/subspace-fleet-engine-rigel.service,task=cargo,pid=5151,uid=991",
            "2026-01-02T06:00:00+0000 example-vm kernel: oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),"
            "cpuset=/,mems_allowed=0,oom_memcg=/system.slice,"
            "task_memcg=/system.slice/subspace-fleet-engine-deneb.service,task=node,pid=6161,uid=992",
            "2026-01-02T07:00:00+0000 example-vm kernel: oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),"
            "cpuset=/,mems_allowed=0,task_memcg=/system.slice/subspace-fleet-engine-mira.service,task=go,pid=7171,"
            "uid=993",
        ])
        lines = memory.live_lines({}, sized(VMS[64], 1), state(), show={}, log=log)
        self.assertEqual(lines[-3:], [
            "  2026-01-02T05:06:07+0000 subspace-fleet-engine-rigel.service: cargo (pid 5151); it reached its engine's "
            "hard cap, which kept the rest of the VM safe.",
            "  2026-01-02T06:00:00+0000 subspace-fleet-engine-deneb.service: node (pid 6161); it reached the memory "
            "limit of /system.slice.",
            "  2026-01-02T07:00:00+0000 subspace-fleet-engine-mira.service: go (pid 7171); the kernel's log does not say "
            "which memory limit it reached.",
        ])

    def test_systemctl_show_is_read_per_unit(self):
        output = ("Id=subspace-fleet-engine-atlas.service\nMemoryMax=4294967296\nTasksMax=4096\n\n"
                  "Id=subspace-fleet-engine-orion.service\nMemoryMax=infinity\nMemoryPeak=[not set]\n")
        run = lambda command, **_options: subprocess.CompletedProcess(command, 0, output, "")  # noqa: E731
        shown = memory.show_units(["subspace-fleet-engine-atlas.service", "subspace-fleet-engine-orion.service"], run)
        self.assertEqual(shown["subspace-fleet-engine-atlas.service"]["MemoryMax"], "4294967296")
        self.assertEqual(memory.live_size(shown["subspace-fleet-engine-orion.service"]["MemoryMax"]), OFF)
        self.assertIsNone(memory.live_size(shown["subspace-fleet-engine-orion.service"]["MemoryPeak"]))


if __name__ == "__main__":
    unittest.main()
