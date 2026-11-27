"""Offline existing-network setup, TLS, lockdown and audit checks."""

from contextlib import ExitStack
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

VM = Path(__file__).resolve().parents[1] / "vm"
sys.path.insert(0, str(VM))
import fleet_setup  # noqa: E402
import private_network  # noqa: E402
from private_network import IngressPlan, Network, PrivateNetworkError, check_certificate, check_running_api  # noqa: E402


CHOICE = "fleet.example.com,eth0,10.20.1.4,10.21.0.0/24"
V6 = "fleet.example.com,example0,fd00:20::4,fd00:21::/64"
SSO = {"NODE_ENV": "production", "ENABLE_DEV_AUTH": "false", "AUTH_PROVIDERS": "workos", "AGENT_PORT": ""}


class SetupTests(unittest.TestCase):
    def test_network_is_opt_in_and_kept_until_replaced_or_disabled(self):
        args = fleet_setup.parse_args(["--non-interactive", "--name", "Example Fleet",
                                       "--rear-admiral", "Atlas", "--captains", "Orion", "--access", "cloudflare"])
        with tempfile.TemporaryDirectory() as inputs, patch.object(fleet_setup, "INPUTS", Path(inputs)):
            answers = fleet_setup.gather(args, {})
            self.assertIsNone(Network.parse(answers.get("privateNetwork")))
            args.private_network = CHOICE
            answers = fleet_setup.gather(args, answers)
            fleet_setup.save_answers(answers)
            args.private_network = None
            saved = fleet_setup.read_json(Path(inputs) / "answers.json")
            rerun = fleet_setup.gather(args, saved)
            self.assertEqual(rerun["privateNetwork"], CHOICE)
            self.assertEqual(rerun["access"], "cloudflare")
            args.private_network = "off"
            self.assertIsNone(Network.parse(fleet_setup.gather(args, rerun)["privateNetwork"]))

    def test_invalid_targets_fail_instead_of_widening_the_scope(self):
        for value in (
            CHOICE.replace("10.20.1.4", "0.0.0.0"), CHOICE.replace("10.20.1.4", "127.0.0.1"),
            CHOICE.replace("10.20.1.4", "203.0.113.4"), CHOICE.replace("eth0", "lo"),
            CHOICE.replace("10.21.0.0/24", "0.0.0.0/0"), CHOICE.replace("10.21.0.0/24", "10.0.0.0/7"),
            CHOICE.replace("10.21.0.0/24", "fd00::/8"), CHOICE.replace("eth0", 'a"b'),
            CHOICE.replace("10.21.0.0/24", "10.20.1.0/24"), V6.replace("fd00:21", "fd00:20"),
            CHOICE.replace("fleet.example.com", "a.localhost"), CHOICE.replace("fleet.example.com", "10.20.1.4"),
        ):
            with self.subTest(value=value), self.assertRaises(PrivateNetworkError):
                Network.parse(value)

    def test_only_the_proxy_user_may_relay_to_the_existing_api_port(self):
        answers = {"api": True, "operator": "operator", "privateNetwork": CHOICE, "agentDispatch": False}
        with tempfile.TemporaryDirectory() as inputs, patch.object(fleet_setup, "INPUTS", Path(inputs)), \
                patch.object(fleet_setup, "metadata_users", return_value=[]), \
                patch.object(fleet_setup.pwd, "getpwnam", side_effect=KeyError):
            args = fleet_setup.isolation_args(answers, IngressPlan(Network.parse(CHOICE), 3000, "", ""))
        self.assertIn("3000:subspace-ingress", args)
        self.assertNotIn("443", args)
        self.assertNotIn("6697", args)
        self.assertIn("5432", args)

    def test_preflight_will_not_proxy_the_default_dev_backend_even_on_a_dry_run(self):
        answers = {"api": True, "irc": "local", "lockdown": True, "agentDispatch": False, "allowPorts": []}
        with tempfile.TemporaryDirectory() as inputs, patch.object(fleet_setup, "INPUTS", Path(inputs)), \
                patch.object(fleet_setup, "FLEET_ETC", Path(inputs) / "installed"):
            with self.assertRaisesRegex(PrivateNetworkError, "development sign-in"):
                fleet_setup.planned_private_ingress(Network.parse(CHOICE), answers)
            with self.assertRaisesRegex(fleet_setup.SetupError, "lockdown enabled"):
                fleet_setup.planned_private_ingress(Network.parse(CHOICE), {**answers, "lockdown": False})

    def test_startup_checks_the_actual_running_api_not_the_desired_env_file(self):
        for changes, allowed in (({}, True), ({"AUTH_PROVIDERS": "entra"}, True),
                                 ({"AUTH_PROVIDERS": "workos,dev"}, False), ({"AGENT_PORT": "3001"}, False),
                                 ({"ENABLE_DEV_AUTH": "true"}, False), ({"NODE_ENV": "development"}, False)):
            with self.subTest(changes=changes), patch("private_network.command", side_effect=[
                "mock-id", json.dumps([f"{key}={value}" for key, value in {**SSO, **changes}.items()]),
            ]):
                if allowed:
                    check_running_api()
                else:
                    with self.assertRaises(PrivateNetworkError):
                        check_running_api()

    def test_plan_requires_explicit_acknowledgement_for_an_installed_local_api_block(self):
        answers = {"api": True, "irc": "local", "lockdown": True, "agentDispatch": False, "allowPorts": []}
        network = Network.parse(CHOICE)
        flag = "--accept-officer-tracker-access-loss"
        for document, api in (("inputs", {"url": "http://127.0.0.1:3000", "agentUrl": "http://127.0.0.1:3001"}),
                              ("installed", {"url": "http://127.0.0.1:3000", "enrolment": True})):
            with self.subTest(document=document), tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
                root = Path(directory)
                inputs, installed = root / "inputs", root / "installed"
                fleet_setup.write_json(root / document / "fleet.json", {"api": api})
                binary = root / "haproxy"
                binary.touch()
                stack.enter_context(patch.object(fleet_setup, "INPUTS", inputs))
                stack.enter_context(patch.object(fleet_setup, "FLEET_ETC", installed))
                stack.enter_context(patch.object(private_network, "HAPROXY", binary))
                stack.enter_context(patch.object(fleet_setup, "api_environment", return_value=(SSO, False)))
                interface = stack.enter_context(patch.object(Network, "check_interface"))
                stack.enter_context(patch.object(fleet_setup, "secret_file", side_effect=["chain", "key"]))
                stack.enter_context(patch.object(private_network, "check_certificate"))
                run = stack.enter_context(patch.object(fleet_setup, "run"))
                say = stack.enter_context(patch.object(fleet_setup, "say"))
                with self.assertRaisesRegex(fleet_setup.SetupError, f"tracker access.*{flag}"):
                    fleet_setup.planned_private_ingress(network, answers)
                interface.assert_not_called()
                args = fleet_setup.parse_args([flag])
                ingress = fleet_setup.planned_private_ingress(
                    network, answers, accept_officer_tracker_access_loss=args.accept_officer_tracker_access_loss,
                )
                self.assertIn("mode tcp", ingress.config)
                self.assertEqual(ingress.bundle, "chain\nkey\n")
                run.assert_called_once()
                self.assertIn("signed enrolment", say.call_args_list[0].args[0])

    def test_guard_stops_the_proxy_when_firewall_or_api_verification_fails(self):
        for failures in ("allow-list missing", "unsafe API sign-in"):
            with self.subTest(failures=failures), patch.object(private_network, "verify",
                    side_effect=PrivateNetworkError(failures)), patch.object(private_network, "command") as command:
                with self.assertRaisesRegex(PrivateNetworkError, failures):
                    private_network.main(["verify", CHOICE])
                command.assert_not_called()
                with self.assertRaisesRegex(PrivateNetworkError, failures):
                    private_network.main(["verify", CHOICE, "--stop-on-failure"])
                command.assert_called_once_with("systemctl", "stop", "--no-block", private_network.UNIT.name)
        with patch.object(private_network, "verify"), patch.object(private_network, "command") as command:
            private_network.main(["verify", CHOICE, "--stop-on-failure"])
            command.assert_not_called()

    def test_a_blocked_retirement_warns_and_setup_still_reaches_its_audit_and_sync(self):
        """Off stops the proxy and isolates before retiring the account, so only an unrelated failure stops the run
        before the host can confirm its lockdown. The warning is repeated last, after the summary and audit, and a
        clean retirement prints none."""
        member = (None, SimpleNamespace(gr_gid=902, gr_mem=["example"]))
        account = SimpleNamespace(pw_uid=901, pw_gid=902, pw_gecos=private_network.ACCOUNT[0],
                                  pw_dir=private_network.ACCOUNT[1], pw_shell=private_network.ACCOUNT[2])
        cases = {
            "a clean retirement": ((account, SimpleNamespace(gr_gid=902, gr_mem=[])), {"userdel": "", "groupdel": ""},
                                   ""),
            "a group with a member": (member, {}, lambda: private_network.members_blocked(member[1])),
            "a failing userdel": ((account, SimpleNamespace(gr_gid=902, gr_mem=[])), {"userdel": 6},
                                  lambda: private_network.retirement_blocked("userdel failed with exit code 6")),
            "a failing groupdel": ((None, SimpleNamespace(gr_gid=902, gr_mem=[])), {"groupdel": 8},
                                   lambda: private_network.retirement_blocked(
                                       "groupdel failed with exit code 8, for example because the group is still "
                                       "another account's primary group")),
            "an unreadable receipt": (PermissionError(13, "Permission denied"), {}, None),
            "another private network error": (PrivateNetworkError("unexpected"), {}, None),
        }
        for name, (entries, status, warning) in cases.items():
            with self.subTest(name), tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
                root = Path(directory)
                for path in ("INPUTS", "REFERENCES", "AGENT_HOMES"):
                    stack.enter_context(patch.object(fleet_setup, path, root / path.lower()))
                receipt = root / "private-ingress-account"
                receipt.write_text("901:902\n")
                stack.enter_context(patch.object(private_network, "ACCOUNT_RECEIPT", receipt))
                # A receipt setup cannot read at all, or any other failure, still stops the run.
                failing = "account_receipt" if isinstance(entries, PermissionError) else "account_entries"
                lookup = {"side_effect": entries} if isinstance(entries, Exception) else {"return_value": entries}
                stack.enter_context(patch.object(private_network, failing, **lookup))
                order, said = [], []
                # Setup calls remove() itself, and remove() has its own command runner and unlinks the proxy's
                # units, configuration and TLS bundle: never reach the host's.
                stack.enter_context(patch.object(private_network, "remove",
                                                 side_effect=lambda **_: order.append("remove")))
                stack.enter_context(patch.object(private_network, "command", side_effect=AssertionError))

                def run(*command, **_options):
                    order.append(" ".join(str(part).rsplit("/", 1)[-1] for part in command[:2] if part != "--"))
                    return status.get(command[0], "")

                def summary(*_):
                    order.append("summary")
                    return {"SUBSPACE_CHANNELS": "#starbase #command #orion", "SUBSPACE_FLEET_NAME": "Example Fleet",
                            "SUBSPACE_FLEET_SLUG": "example-fleet", "SUBSPACE_PORTS": "6667"}
                for target, attribute, value in (
                        (fleet_setup.pwd, "getpwnam", {}), (fleet_setup.os, "geteuid", {"return_value": 0}),
                        (fleet_setup, "say", {"side_effect": said.append}), (fleet_setup, "print", {"create": True}),
                        (fleet_setup, "installer", {}),
                        (fleet_setup, "run", {"side_effect": run}), (fleet_setup, "check_outbound", {}),
                        (fleet_setup, "irc_source", {"return_value": "a" * 40}),
                        (fleet_setup, "confirm_fleet_api", {"return_value": False}),
                        (fleet_setup, "installed_stargate_source", {}),
                        (fleet_setup, "summary", {"side_effect": summary})):
                    stack.enter_context(patch.object(target, attribute, **value))
                argv = ["--non-interactive", "--operator", "operator", "--name", "Example Fleet", "--rear-admiral",
                        "Atlas", "--captains", "Orion", "--crew-claude", "0", "--crew-codex", "0", "--inputs",
                        str(root / "inputs"), "--stargate-ref", fleet_setup.runtime.STARGATE_REF, "--no-demo",
                        "--no-api", "--irc-auth", "password"]
                args = fleet_setup.parse_args(argv)
                if warning is None:
                    with self.assertRaises(Exception) as stopped:
                        fleet_setup.setup(args)
                    self.assertIs(stopped.exception, entries)
                else:
                    fleet_setup.setup(args)
                if warning == "":
                    self.assertFalse(receipt.exists())
                else:
                    self.assertEqual(receipt.read_text(), "901:902\n")
                # Off disables the proxy before it records off or replaces isolation.
                self.assertEqual(order[0], "remove")
                self.assertIn("lockdown.sh isolate", order)
                if warning is None:
                    self.assertEqual(order[-1], "lockdown.sh applied-id")
                    continue
                self.assertEqual(order[order.index("lockdown.sh applied-id"):],
                                 ["lockdown.sh applied-id", *status, "summary", "audit.sh", "sync",
                                  "runtime.py memory-summary", "providers.sh status"])
                self.assertIn("fleet Example Fleet (example-fleet) is set up.", said)
                if warning == "":
                    self.assertEqual([line for line in said if "retiring" in line or "None" in line], [])
                    self.assertFalse(said[-1].startswith("WARNING"), said[-1])
                    continue
                warning = str(warning())
                self.assertEqual([line for line in said if warning in line], [f"WARNING: {warning}"] * 2)
                self.assertEqual(said[-1], f"WARNING: {warning}")
                self.assertIn(f"{receipt}, proves", warning)
                self.assertIn("See Recovering the proxy account in docs/private-access.md.", warning)


class LifecycleTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        stack = ExitStack()
        self.addCleanup(stack.close)
        for name in ("UNIT", "GUARD", "TIMER", "CONFIG", "TLS", "ACCOUNT_RECEIPT"):
            path = getattr(private_network, name)
            stack.enter_context(patch.object(private_network, name, root / path.parent.name / path.name))
        self.paths = [getattr(private_network, name) for name in ("UNIT", "GUARD", "TIMER", "CONFIG", "TLS")]

    def test_apply_writes_config_credentials_and_units_before_arming_timer_then_starting_proxy(self):
        def run(*_args):
            self.assertTrue(all(path.is_file() for path in self.paths))

        with patch.object(fleet_setup, "run", side_effect=run) as start:
            IngressPlan(Network.parse(CHOICE), 3000, "config", "tls").enable(
                "change-id", write_private=fleet_setup.write_private, run=start)
        self.assertEqual([call.args[1:] for call in start.call_args_list], [
            ("daemon-reload",), ("enable", "--now", private_network.TIMER.name),
            ("start-private-ingress", "change-id"),
        ])
        self.assertEqual(start.call_args.args[0], VM / "lockdown.sh")

    def test_off_stops_timer_checker_and_proxy_before_removal(self):
        for path in self.paths:
            path.parent.mkdir(exist_ok=True)
            path.touch()

        def systemctl(*args):
            self.assertTrue(all(path.exists() == (args[1] != "daemon-reload") for path in self.paths))

        with patch.object(private_network, "command", side_effect=systemctl) as stop:
            private_network.remove()
        self.assertEqual([call.args[1:] for call in stop.call_args_list], [
            ("disable", "--now", private_network.TIMER.name), ("stop", private_network.GUARD.name),
            ("disable", "--now", private_network.UNIT.name), ("daemon-reload",),
        ])
        self.assertFalse(private_network.CONFIG.parent.exists())

    def test_uninstall_keeps_the_receipt_while_the_uid_may_still_be_exempt(self):
        private_network.ACCOUNT_RECEIPT.parent.mkdir()
        private_network.ACCOUNT_RECEIPT.write_text("901:902\n")
        for path in self.paths:
            path.parent.mkdir(exist_ok=True)
            path.touch()
        with patch.object(private_network, "account_entries") as lookup:
            private_network.main(["remove", "--no-systemd"])
        lookup.assert_not_called()
        self.assertTrue(all(not path.exists() for path in self.paths))
        self.assertEqual(private_network.ACCOUNT_RECEIPT.read_text(), "901:902\n")

    def test_removing_an_unconfigured_proxy_does_not_touch_systemd_or_the_filesystem(self):
        with patch.object(private_network, "command") as systemctl, patch.object(Path, "rmdir") as rmdir, \
                patch.object(Path, "unlink") as unlink:
            private_network.remove()
        systemctl.assert_not_called()
        rmdir.assert_not_called()
        unlink.assert_not_called()


class AccountTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.receipt = Path(directory.name) / "private-ingress-account"
        self.account = SimpleNamespace(pw_uid=901, pw_gid=902, pw_gecos="Subspace private ingress",
                                       pw_dir="/nonexistent", pw_shell="/usr/sbin/nologin")
        self.group = SimpleNamespace(gr_gid=902, gr_mem=[])
        self.entries = (None, None)
        stack = ExitStack()
        self.addCleanup(stack.close)
        stack.enter_context(patch.object(private_network, "ACCOUNT_RECEIPT", self.receipt))
        for module, lookup, index in ((private_network.pwd, "getpwnam", 0), (private_network.grp, "getgrnam", 1)):
            stack.enter_context(patch.object(module, lookup, side_effect=lambda _, i=index: self.lookup(i)))
        stack.enter_context(patch.object(private_network, "disable"))
        self.run = stack.enter_context(patch.object(fleet_setup, "run", side_effect=self.change_account))
        self.ingress = IngressPlan(Network.parse(CHOICE), 3000, "", "")

    def lookup(self, index):
        if self.entries[index] is None:
            raise KeyError(private_network.USER)
        return self.entries[index]

    def change_account(self, command, *_, **__):
        if command == "useradd":
            self.entries = (self.account, self.group)
        elif command == "userdel":
            self.entries = (None, self.group)  # USERGROUPS_ENAB=no
        elif command == "groupdel":
            self.entries = (None, None)
        else:
            self.fail(f"Unexpected command: {command}")

    def quiesce(self):
        self.ingress.quiesce(write_private=fleet_setup.write_private, run=self.run)

    def test_refuses_a_login_an_identical_unrecorded_account_and_a_group_on_its_own(self):
        login = SimpleNamespace(pw_uid=1001, pw_gid=902, pw_gecos="Example User",
                                pw_dir="/home/example", pw_shell="/bin/bash")
        for entries in ((login, self.group), (self.account, self.group), (None, self.group)):
            self.entries = entries
            with self.subTest(entries=entries), self.assertRaisesRegex(PrivateNetworkError, "will not adopt"):
                self.quiesce()
        self.run.assert_not_called()
        self.assertFalse(self.receipt.exists())

    def test_creates_and_reuses_only_the_recorded_account_and_group(self):
        self.quiesce()
        self.assertEqual(self.receipt.read_text(), "901:902\n")
        self.assertEqual(self.receipt.stat().st_mode & 0o777, 0o600)
        self.run.assert_called_once_with("useradd", "--system", "--user-group", "--no-create-home", "--comment",
                                         self.account.pw_gecos, "--home-dir", self.account.pw_dir, "--shell",
                                         self.account.pw_shell, "--", private_network.USER)
        self.run.reset_mock()
        self.quiesce()
        self.run.assert_not_called()

    def test_off_retires_only_the_recorded_account_and_group(self):
        private_network.retire_account(run=self.run)
        self.entries = (self.account, self.group)  # Identical, but unrecorded: not setup's to remove.
        private_network.retire_account(run=self.run)
        self.run.assert_not_called()
        self.receipt.write_text("901:902\n")
        private_network.retire_account(run=self.run)
        self.assertEqual([call.args for call in self.run.call_args_list],
                         [("userdel", "--", private_network.USER), ("groupdel", "--", private_network.USER)])
        self.assertEqual(self.entries, (None, None))
        self.assertFalse(self.receipt.exists())

    def test_off_finishes_a_retirement_that_stopped_part_way_and_setup_can_enable_again(self):
        for step in ("userdel", "groupdel", "receipt"):
            with self.subTest(step=step):
                self.entries = (None, None)
                self.receipt.unlink(missing_ok=True)
                self.quiesce()

                def stop(command, *args, **options):
                    self.change_account(command, *args, **options)
                    if command == step:
                        raise KeyboardInterrupt

                with patch.object(fleet_setup, "run", side_effect=stop) as interrupted, \
                        patch.object(Path, "unlink", side_effect=KeyboardInterrupt), \
                        self.assertRaises(KeyboardInterrupt):
                    private_network.retire_account(run=interrupted)
                self.assertTrue(self.receipt.exists())
                self.run.reset_mock()
                private_network.retire_account(run=self.run)
                self.assertEqual([call.args[0] for call in self.run.call_args_list],
                                 {"userdel": ["groupdel"], "groupdel": [], "receipt": []}[step])
                self.assertEqual(self.entries, (None, None))
                self.assertFalse(self.receipt.exists())
                self.run.reset_mock()
                self.quiesce()
                self.assertEqual([call.args[0] for call in self.run.call_args_list], ["useradd"])
                self.assertEqual(self.receipt.read_text(), "901:902\n")

    def altered(self):
        """Partial or altered account states that no receipt can prove setup created as they are."""
        account, group = self.account, self.group
        login = SimpleNamespace(**{**vars(account), "pw_gecos": "Example User", "pw_shell": "/bin/bash"})
        return {
            "account without its group": ((account, None), "901:902\n"),
            "group with a member": ((account, SimpleNamespace(gr_gid=902, gr_mem=["example"])), "901:902\n"),
            "another profile": ((login, group), "901:902\n"),
            "another UID": ((account, group), "903:902\n"),
            "another GID": ((account, group), "901:903\n"),
            "another primary group": ((SimpleNamespace(**{**vars(account), "pw_gid": 903}), group), "901:902\n"),
        }

    def test_setup_and_the_guard_refuse_every_partial_or_altered_state(self):
        states = {
            **self.altered(),
            "group without its account": ((None, self.group), "901:902\n"),
            "receipt without either": ((None, None), "901:902\n"),
            # Receipts setup never writes, which off reports as unreadable.
            "root's UID": ((SimpleNamespace(**{**vars(self.account), "pw_uid": 0}), self.group), "0:902\n"),
            "root's GID": ((SimpleNamespace(**{**vars(self.account), "pw_gid": 0}),
                            SimpleNamespace(gr_gid=0, gr_mem=[])), "901:0\n"),
            "unrecorded account without a group": ((self.account, None), None),
        }
        for name, (entries, receipt) in states.items():
            with self.subTest(name), patch.object(Network, "audit_firewall") as firewall:
                self.entries = entries
                self.receipt.unlink(missing_ok=True)
                if receipt:
                    self.receipt.write_text(receipt)
                with self.assertRaisesRegex(PrivateNetworkError, "will not adopt"):
                    self.quiesce()
                with self.assertRaisesRegex(PrivateNetworkError, str(private_network.ACCOUNT_RECEIPT)):
                    private_network.verify(CHOICE)
                firewall.assert_not_called()
                self.run.assert_not_called()
                self.assertEqual(private_network.account_receipt(), receipt)

    def test_useradd_must_leave_exactly_the_expected_account(self):
        self.account.pw_shell = "/bin/bash"
        with self.assertRaisesRegex(PrivateNetworkError, "will not adopt"):
            self.quiesce()
        self.assertFalse(self.receipt.exists())

    def test_off_reports_and_keeps_every_state_it_cannot_prove(self):
        states = {
            **self.altered(),
            "group of another GID": ((None, SimpleNamespace(gr_gid=903, gr_mem=[])), "901:902\n"),
            "group with a member, without its account": ((None, SimpleNamespace(gr_gid=902, gr_mem=["example"])),
                                                          "901:902\n"),
        }
        for name, (entries, receipt) in states.items():
            with self.subTest(name):
                self.entries = entries
                self.receipt.write_text(receipt)
                with self.assertRaisesRegex(PrivateNetworkError, rf"Cannot finish retiring.*{self.receipt}"):
                    private_network.retire_account(run=self.run)
                self.run.assert_not_called()
                self.assertEqual(self.entries, entries)
                self.assertEqual(self.receipt.read_text(), receipt)

    def test_off_keeps_a_group_that_gains_a_member_once_the_account_is_gone(self):
        self.quiesce()
        self.run.reset_mock()
        self.run.side_effect = lambda command, *_, **__: setattr(
            self, "entries", (None, SimpleNamespace(gr_gid=902, gr_mem=["example"])))
        with self.assertRaisesRegex(PrivateNetworkError, "lists example as members.*gpasswd -d MEMBER"):
            private_network.retire_account(run=self.run)
        self.assertEqual([call.args[0] for call in self.run.call_args_list], ["userdel"])
        self.assertTrue(self.receipt.exists())

    def test_off_names_the_members_that_keep_a_recorded_group(self):
        self.quiesce()
        self.run.reset_mock()
        joined = SimpleNamespace(gr_gid=902, gr_mem=["example", "other"])
        for entries in ((self.account, joined), (None, joined)):
            with self.subTest(account=entries[0] is not None):
                self.entries = entries
                with self.assertRaisesRegex(private_network.RetirementBlocked,
                                            "lists example, other as members. Remove each with 'gpasswd -d MEMBER "
                                            "subspace-ingress' and rerun"):
                    private_network.retire_account(run=self.run)
                self.run.assert_not_called()
                self.assertTrue(self.receipt.exists())
        # Removing members would not make another profile match, so it is not offered.
        self.entries = (SimpleNamespace(**{**vars(self.account), "pw_shell": "/bin/bash"}), joined)
        with self.assertRaisesRegex(private_network.RetirementBlocked, "does not match the receipt") as other:
            private_network.retire_account(run=self.run)
        self.assertNotIn("gpasswd", str(other.exception))
        # Without its members the group matches again: off retires it.
        self.entries = (self.account, self.group)
        private_network.retire_account(run=self.run)
        self.assertEqual(self.entries, (None, None))
        self.assertFalse(self.receipt.exists())

    def test_a_failing_userdel_or_groupdel_blocks_retirement_and_keeps_the_receipt(self):
        for step in ("userdel", "groupdel"):
            with self.subTest(step=step):
                self.entries = (None, None)
                self.receipt.unlink(missing_ok=True)
                self.quiesce()

                def refuse(command, *args, **options):
                    self.assertEqual(options, {"check": False})
                    if command == step:
                        return 8
                    return self.change_account(command, *args)
                with patch.object(fleet_setup, "run", side_effect=refuse) as run, \
                        self.assertRaisesRegex(private_network.RetirementBlocked, f"{step} failed with exit code 8"):
                    private_network.retire_account(run=run)
                self.assertEqual(private_network.account_receipt(), "901:902\n")

    def test_a_malformed_receipt_is_never_read_as_ownership_and_says_how_to_recover(self):
        for entries in ((None, None), (self.account, self.group)):
            for receipt in ("901:902", "0:902\n", "garbage\n", "\udcff901:902\n".encode("utf-8", "surrogateescape")):
                with self.subTest(entries=entries, receipt=receipt):
                    self.entries = entries
                    (self.receipt.write_bytes if isinstance(receipt, bytes) else self.receipt.write_text)(receipt)
                    with self.assertRaises(private_network.RetirementBlocked) as off:
                        private_network.retire_account(run=self.run)
                    with self.assertRaises(PrivateNetworkError) as enable:
                        self.quiesce()
                    with self.assertRaises(PrivateNetworkError) as guard:
                        private_network.verify(CHOICE)
                    self.run.assert_not_called()
                    self.assertTrue(self.receipt.exists())
                    self.assertEqual(self.entries, entries)
                    for error in (off, enable, guard):
                        self.assertRegex(str(error.exception), rf"Cannot read the receipt {self.receipt}, .* "
                                         "Check 'getent passwd subspace-ingress' and 'getent group subspace-ingress'.*"
                                         r"move the receipt out of the active path, for example to /root/"
                                         r"private-ingress-account\.bad")
                        self.assertNotIn("run off again", str(error.exception))
                    self.assertIn("Lockdown no longer exempts the proxy's UID.", str(off.exception))
                    for error in (enable, guard):
                        self.assertIn("Rerun setup with --private-network off first", str(error.exception))
                        self.assertNotIsInstance(error.exception, private_network.RetirementBlocked)

    def test_guard_refuses_a_replaced_account_before_checking_firewall_or_api(self):
        self.quiesce()
        self.receipt.write_text("903:902\n")
        with patch.object(Network, "audit_firewall") as firewall, patch.object(private_network, "check_running_api") as api:
            with self.assertRaisesRegex(PrivateNetworkError, "Cannot verify ownership"):
                private_network.verify(CHOICE)
            firewall.assert_not_called()
            api.assert_not_called()


class RenderTests(unittest.TestCase):
    def test_lockdown_accepts_distinct_private_ports_in_either_order(self):
        source = (VM / "lockdown.sh").read_text()
        parse = re.search(r"^parse_private_ingress\(\) \{.*?^\}$", source, re.M | re.S).group()
        program = 'die() { exit 1; };\n' + parse + '\nparse_private_ingress "$1"'
        target = Network.parse(CHOICE).firewall_target.rsplit(",", 2)[0]
        for ports, allowed in (("443,6697", True), ("65535,1", True), ("443,443", False),
                               ("65536,443", False), ("443,65536", False), ("0,443", False), ("443,0", False)):
            with self.subTest(ports=ports):
                result = subprocess.run(["bash", "-ec", program, "bash", f"{target},{ports}"])
                self.assertEqual(result.returncode, int(not allowed))
        # The checker accepts the rule whichever order nft lists the set in, and nothing else.
        functions = [re.search(rf"^{name}\(\) \{{.*?^\}}$", source, re.M | re.S).group()
                     for name in ("private_ingress_rules", "input_prefix", "check_private_ingress")]
        checker = "\n".join([
            'die() { echo "$*" >&2; exit 1; }', 'live_chain() { printf "%s\\n" "${chains[$1]}"; }',
            *re.findall(r"^input_(?:hook|policy)=.*$", source, re.M), parse, *functions,
            'declare -A chains=([private_ingress]=$2 [input]=$3)', 'check_private_ingress "$1"'])
        allow_list = 'iifname "eth0" ip saddr 10.21.0.0/24 accept\ndrop'
        for ports, listed, allowed in (("443,6697", "443, 6697", True), ("6697,443", "443, 6697", True),
                                       ("6697,443", "6697, 443", True), ("443,6697", "6697, 443", True),
                                       ("6697,443", "443, 6698", False), ("6697,443", "443, 6697, 22", False),
                                       ("6697,443", "443", False)):
            inbound = ("type filter hook input priority filter - 10; policy drop;\niif \"lo\" accept\n"
                       f"ct state invalid drop\nip daddr 10.20.1.4 tcp dport {{ {listed} }} jump private_ingress\n"
                       "ct state established,related accept")
            with self.subTest(ports=ports, listed=listed):
                result = subprocess.run(["bash", "-ec", checker, "bash", f"{target},{ports}", allow_list, inbound],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, int(not allowed), result.stderr)
                if not allowed:
                    self.assertIn("does not apply", result.stderr)

    def test_proxy_uses_one_bound_interface_and_cidr_for_both_tls_listeners(self):
        config = Network.parse(V6).proxy_config(3000, 6667)
        self.assertIn('bind [fd00:20::4]:443 interface example0 ssl crt ', config)
        self.assertIn('bind [fd00:20::4]:6697 interface example0 ssl crt ', config)
        self.assertEqual(config.count("tcp-request connection reject unless { src fd00:21::/64 }"), 2)
        self.assertIn("server loopback 127.0.0.1:3000", config)
        self.assertIn("server loopback 127.0.0.1:6667", config)
        self.assertNotIn("5432", config)
        self.assertIn("global\n    log stdout format raw local0", config)
        self.assertIn("log global\n    option tcplog", config)

    def test_systemd_renders_isolation_startup_check_and_periodic_guard(self):
        network = Network.parse(CHOICE)
        service = private_network.unit_text(network)
        for directive in ("ExecStartPre=+", f"verify {CHOICE}\n", "CAP_NET_RAW", "PrivateDevices=yes",
                          "ProtectKernelTunables=yes", "ProtectKernelModules=yes", "ProtectKernelLogs=yes",
                          "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6", "SystemCallFilter=@system-service"):
            self.assertIn(directive, service)
        guard = private_network.guard_text(network)
        self.assertIn(f"is-active --quiet {private_network.UNIT.name}", guard)
        self.assertIn(f"verify {CHOICE} --stop-on-failure", guard)
        self.assertEqual(private_network.TIMER.with_suffix(".service"), private_network.GUARD)
        self.assertIn("OnActiveSec=60s\nOnUnitActiveSec=60s", private_network.timer_text())

    def test_lockdown_stops_the_proxy_before_rolling_back_firewall_rules(self):
        source = (VM / "lockdown.sh").read_text()
        functions = [re.search(rf"^{name}\(\) \{{.*?^\}}$", source, re.M | re.S).group()
                     for name in ("stop_private_ingress", "rollback")]
        private_rules = "    chain private_ingress {\n        drop\n    }\n"
        for pending, confirmed, action, firewall in (
                (True, private_rules, "stop", "restore rules"),
                (True, "    chain input {\n    }\n", "disable --now", "restore rules"),
                (True, None, "disable --now", "remove rules"),
                (False, private_rules, "disable --now", "remove rules")):
            with self.subTest(pending=pending, confirmed=confirmed), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / private_network.UNIT.name).touch()
                if pending:
                    (root / "pending.nft").write_text(private_rules)
                if confirmed is not None:
                    (root / "rules.nft").write_text(confirmed)
                result = subprocess.run(["bash", "-euc", "\n".join([
                    "state=$1; pending=$state/pending.nft; rules=$state/rules.nft",
                    f"private_ingress_unit={private_network.UNIT.name}",
                    'sshd_conf=$state/sshd.conf; backup=$state/backup; isolation=$state/isolation; boot_unit=$state/boot',
                    ': > "$isolation"',
                    'lock_state() { :; }; cancel_rollback() { :; }; stop_rollback_timer() { :; }; sync() { :; }',
                    'say() { :; }; sshd() { return 1; }; restore_pending() { rm -- "$pending"; }',
                    'systemctl() { echo "$*"; }; nft() { echo "restore rules"; }; delete_table() { echo "remove rules"; }',
                    *(function.replace("/etc/systemd/system/", '$state/') for function in functions),
                    f"rollback {'--from-timer' if pending else ''}",
                ]), "bash", str(root)], capture_output=True, check=True, text=True)
                self.assertEqual(result.stdout.splitlines(), [f"{action} {private_network.UNIT.name}", firewall])

    def test_lockdown_starts_ingress_under_the_state_lock_only_for_the_current_change(self):
        source = (VM / "lockdown.sh").read_text()
        functions = [re.search(rf"^{name}\(\) \{{.*?^\}}$", source, re.M | re.S).group()
                     for name in ("lock_state", "start_private_ingress")]
        for pending, confirmed, timed_out in (("new", "old", False), (None, "new", False),
                                              (None, "old", False), ("later", "new", False),
                                              ("new", "old", True)):
            with self.subTest(pending=pending, confirmed=confirmed, timed_out=timed_out), \
                    tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "applied.id").write_text("new\n")  # Still present after rollback.
                (root / "rules.id").write_text(f"{confirmed}\n")
                if pending:
                    (root / "pending.id").write_text(f"{pending}\n")
                    (root / "pending.nft").touch()
                result = subprocess.run(["bash", "-euc", "\n".join([
                    'state=$1; pending=$state/pending.nft; locked=false; timed_out=$2',
                    f"private_ingress_unit={private_network.UNIT.name}",
                    'die() { echo "$*" >&2; exit 1; }',
                    'timeout() { shift 3; "$@"; }',
                    'systemctl() { if flock -n "$state/lock" true; then exit 99; fi; echo "$*"; '
                    'if [[ $timed_out == yes && $1 == enable ]]; then return 124; fi; }',
                    *functions,
                    'start_private_ingress "$(<"$state/applied.id")"',
                ]), "bash", str(root), "yes" if timed_out else "no"], capture_output=True, text=True)
                if (pending or confirmed) != "new":
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("rolled back or was replaced", result.stderr)
                    self.assertEqual(result.stdout, "")
                else:
                    self.assertEqual(result.returncode, int(timed_out), result.stderr)
                    expected = [f"enable --now {private_network.UNIT.name}"]
                    if timed_out:
                        expected.append(f"disable --now --no-block {private_network.UNIT.name}")
                        self.assertIn("Could not start private ingress", result.stderr)
                    self.assertEqual(result.stdout.splitlines(), expected)

    def test_lockdown_route_is_exact_and_checked_before_the_old_allows(self):
        source = (VM / "lockdown.sh").read_text()
        helper = re.search(r"^unique_list\(\).*?$", source, re.M).group()
        assignments = re.findall(r"^input_(?:hook|policy)=.*$", source, re.M)
        functions = [re.search(rf"^{name}\(\) \{{.*?^\}}$", source, re.M | re.S).group()
                     for name in ("parse_private_ingress", "private_ingress_rules", "input_prefix", "render_lockdown")]
        for choice, family in ((CHOICE, "ip"), (V6, "ip6"),
                               (CHOICE.replace("/24", "/32"), "ip"), (V6.replace("/64", "/128"), "ip6"), (None, "")):
            with self.subTest(choice=choice), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                path = root / "rules.nft"
                network = Network.parse(choice)
                target = network.firewall_target if network else ""
                subprocess.run(["bash", "-ec", "\n".join([
                    helper, *assignments, *functions, "ssh_ports=(22) allow=() v4=() v6=() operators=(operator)",
                    "tailscale=false cloudflare=false", f"parse_private_ingress '{target}'" if network else "private_ip=''",
                    'render_lockdown "$1" "$2"',
                ]), "bash", str(path), str(path.with_suffix(".ssh"))], check=True)
                rules = path.read_text()
                if network is None:
                    self.assertNotIn("private_ingress", rules)
                else:
                    rule = f'{family} daddr {network.address} tcp dport {{ 443, 6697 }} jump private_ingress'
                    self.assertLess(rules.index(rule), rules.index("ct state established,related accept"))
                    self.assertLess(rules.index(rule), rules.index("tcp dport { 22 } accept"))
                    self.assertIn(f'iifname "{network.interface}" {family} saddr {network.clients} accept\n        drop', rules)
                    chain, inbound = rules.split("table inet subspace_lockdown {", 1)[1].split("    chain input", 1)
                    inbound = "chain input" + inbound
                    if network.clients.num_addresses == 1:
                        chain = chain.replace(str(network.clients), str(network.clients.network_address))
                    # Exercise the actual read-only dispatch with fixture nft output, not live VMs.
                    nft = root / "nft"
                    nft.write_text('#!/bin/bash\n[[ $* == "list chain inet subspace_lockdown $5" ]] || exit 2\n'
                                   'cat -- "$FIXTURE/$5"\n')
                    nft.chmod(0o755)
                    checker = source.replace("export PATH=", f'export PATH="{root}":', 1)
                    checker = checker.replace("(( EUID == 0 )) || die 'Run as root.'", "")
                    script = root / "lockdown.sh"
                    script.write_text(checker)

                    def check():
                        return subprocess.run(["bash", str(script), "check-private-ingress", target],
                                              text=True, capture_output=True, env={**os.environ, "FIXTURE": directory})

                    (root / "private_ingress").write_text(chain)
                    for incoming, error in ((inbound, None), ("", "does not apply"),
                                            (inbound.replace(rule, ""), "does not apply"),
                                            (inbound.replace(rule, f"accept\n        {rule}"), "does not apply"),
                                            (inbound.replace("policy drop", "policy accept"), "does not apply")):
                        (root / "input").write_text(incoming)
                        result = check()
                        self.assertEqual(result.returncode, int(error is not None), result.stderr)
                        if error:
                            self.assertIn(error, result.stderr)
                    (root / "private_ingress").write_text(chain.replace("        drop", "        accept\n        drop"))
                    result = check()
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn("allow-list", result.stderr)
                    with patch("private_network.command") as command:
                        network.audit_firewall()
                        self.assertEqual(command.call_args.args[1:], ("check-private-ingress", target))

    @unittest.skipUnless(shutil.which("openssl"), "OpenSSL needed for TLS fixtures")
    def test_tls_preflight_checks_dns_name_and_matching_key(self):
        with tempfile.TemporaryDirectory() as scratch:
            cert, key, other_key = (Path(scratch) / name for name in ("cert.pem", "key.pem", "other.pem"))
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                            "-subj", "/CN=fleet.example.com", "-addext", "subjectAltName=DNS:fleet.example.com",
                            "-out", cert, "-keyout", key], check=True, capture_output=True)
            subprocess.run(["openssl", "genpkey", "-algorithm", "EC", "-pkeyopt", "ec_paramgen_curve:P-256",
                            "-out", other_key], check=True, capture_output=True)
            check_certificate(Network.parse(CHOICE), cert, key)
            with self.assertRaises(PrivateNetworkError):
                check_certificate(Network.parse(CHOICE.replace("fleet.", "other.")), cert, key)
            with self.assertRaisesRegex(PrivateNetworkError, "do not match"):
                check_certificate(Network.parse(CHOICE), cert, other_key)


class AuditTests(unittest.TestCase):
    def audit(self, sockets, published="", *, choice=CHOICE, guard=True, near_expiry=False):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            mock = bin_dir / "fixture"
            mock.write_text('''#!/bin/bash
case ${0##*/} in
    ss) printf '%s\\n' "$AUDIT_SOCKETS" ;;
    jq) if [[ $* == *answers.json* ]]; then printf '%s\\n' "$AUDIT_CHOICE"; else exec /usr/bin/jq "$@"; fi ;;
    nft) case "$*" in
             *'chain inet subspace_lockdown prerouting') echo 'tcp dport 9443 ip saddr != { 10.0.0.0/8 } drop' ;;
             *'table inet subspace_lockdown') echo 'policy drop' ;;
             *) exit 1 ;;
         esac ;;
    python3) if [[ $3 == listeners ]]; then exec /usr/bin/python3 -B "$AUDIT_HELPER" listeners "$4";
             else test "$AUDIT_GUARD" = yes; fi ;;
    openssl) test "$AUDIT_NEAR_EXPIRY" = no ;;
    docker) case $1 in
                ps) echo mock-id ;;
                inspect) printf '%s\\n' "$AUDIT_PUBLISHED" ;;
            esac ;;
    ip|systemctl) exit 0 ;;
    curl|sshd) exit 1 ;;
esac
''')
            mock.chmod(0o755)
            for command in ("ss", "jq", "nft", "python3", "docker", "ip", "systemctl", "curl", "sshd", "openssl"):
                (bin_dir / command).symlink_to("fixture")
            (root / "answers.json").touch(mode=0o600)
            (root / "subspace-private-network").mkdir()
            (root / "subspace-private-network/tls.pem").touch(mode=0o600)
            program = (VM / "audit.sh").read_text().replace("export PATH=", f'export PATH="{bin_dir}":', 1)
            program = re.sub(r"^\(\( EUID == 0 \)\).*$", "", program, flags=re.M)
            # Use only fixtures, never live host files or firewall. The fixture owns its answers, not root.
            program = program.replace('/root/fleet', str(root)).replace('[[ $owner != root ]]',
                                                                         '[[ $owner != "$(id -un)" ]]')
            for path in ("/etc/subspace-lockdown", "/etc/subspace-fleet", "/etc/subspace-private-network", "/root/irc"):
                program = program.replace(path, str(root / Path(path).name))
            return subprocess.run(["bash", "-c", program], text=True, capture_output=True, env={
                **os.environ, "AUDIT_CHOICE": choice, "AUDIT_GUARD": "yes" if guard else "no",
                "AUDIT_HELPER": str(VM / "private_network.py"),
                "AUDIT_SOCKETS": sockets, "AUDIT_PUBLISHED": published or "{}",
                "AUDIT_NEAR_EXPIRY": "yes" if near_expiry else "no",
            })

    def test_recorded_listener_pair_passes_and_an_unexpected_listener_or_publish_fails(self):
        allowed = ('tcp LISTEN 0 20 10.20.1.4:443 0.0.0.0:* users:(("haproxy",pid=123,fd=6))\n'
                   'tcp LISTEN 0 20 10.20.1.4:6697 0.0.0.0:* users:(("haproxy",pid=123,fd=7))\n'
                   'tcp LISTEN 0 20 127.0.0.1:3000 0.0.0.0:* users:(("docker-proxy",pid=456,fd=4))\n')
        okay = self.audit(allowed, '{"3000/tcp":[{"HostIp":"127.0.0.1","HostPort":"3000"}]}', near_expiry=True)
        self.assertEqual(okay.returncode, 0, okay.stdout + okay.stderr)
        self.assertIn("WARN  The private ingress TLS certificate expires within 30 days", okay.stdout)
        fixtures = [
            (allowed + 'tcp LISTEN 0 20 10.20.1.4:8443 *:* users:(("rogue",pid=9,fd=4))', "", True),
            (allowed.replace("10.20.1.4:443", "0.0.0.0:443"), "", True),
            (allowed.replace("10.20.1.4:443", "10.20.1.5:443"), "", True),
            (allowed.replace('"haproxy"', '"other"'), "", True),
            (allowed, '{"5432/tcp":[{"HostIp":"10.20.1.4","HostPort":"5432"}]}', True),
            (allowed, '{"5432/tcp":[{"HostIp":"10.20.1.4","HostPort":"9443"}]}', True),
            (allowed, '{"3001/tcp":[{"HostIp":"::","HostPort":"3001"}]}', True),
            (allowed, '{"3000/tcp":[{"HostIp":"","HostPort":"3000"}]}', True),
            (allowed, '{"9000/tcp":[{"HostPort":"9443"}]}', True),
            (allowed, "", False),
        ]
        for sockets, published, guard in fixtures:
            with self.subTest(sockets=sockets, published=published, guard=guard):
                result = self.audit(sockets, published, guard=guard)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn("FAIL", result.stdout)

    def test_tunnel_and_explicit_site_listener_layouts_keep_working(self):
        socket = 'tcp LISTEN 0 20 10.20.1.4:9443 0.0.0.0:* users:(("nginx",pid=123,fd=6))'
        for sockets in ("", socket):
            with self.subTest(sockets=sockets):
                result = self.audit(sockets, choice="")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_ipv6_interface_suffix_is_not_part_of_the_listener_address(self):
        sockets = '\n'.join(f'tcp LISTEN 0 20 [fd00:20::4]%example0:{port} [::]:* users:(("haproxy",pid=1,fd=6))'
                            for port in (443, 6697))
        for choice in (V6, V6.replace("fd00:20::4", "FD00:0020:0:0:0:0:0:0004")):
            with self.subTest(choice=choice):
                result = self.audit(sockets, choice=choice)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
