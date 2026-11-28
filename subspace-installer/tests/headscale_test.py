"""Offline checks for the coordinator and router installer entry points."""

from pathlib import Path
import subprocess
import unittest


INSTALL = Path(__file__).resolve().parents[2] / "install.sh"
HOST = "headscale.example.com"
PUBLIC = "203.0.113.42"
API = "10.20.0.4"
IRC = "10.20.0.5"


def dry_run(command, *arguments):
    return subprocess.run(
        [str(INSTALL), command, "--dry-run", *arguments],
        capture_output=True,
        text=True,
        check=False,
    )


class HeadscaleTest(unittest.TestCase):
    def test_coordinator_exposes_only_coordination_and_service_routes(self):
        result = dry_run("headscale", HOST, PUBLIC, API, IRC)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("TCP 443 only", result.stdout)
        self.assertIn("verified-member DERP", result.stdout)
        self.assertIn(f"TCP {API}:443 and {IRC}:6697 only", result.stdout)

    def test_router_deduplicates_a_shared_service_address(self):
        for irc, routes in ((API, f"{API}/32"), (IRC, f"{API}/32,{IRC}/32")):
            with self.subTest(irc=irc):
                result = dry_run("headscale-router", HOST, API, irc)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"Advertised routes: {routes}\n", result.stdout)

    def test_neither_entry_point_accepts_unscoped_or_invalid_destinations(self):
        for address in ("0.0.0.0", PUBLIC, "10.20.0.0/24", "10.20.0.300", "10.20.0.04"):
            with self.subTest(address=address):
                self.assertNotEqual(dry_run("headscale", HOST, PUBLIC, API, address).returncode, 0)
                self.assertNotEqual(dry_run("headscale-router", HOST, API, address).returncode, 0)

    def test_coordinator_requires_public_ip_and_hostname(self):
        for host, public in (("https://headscale.example.com", PUBLIC), (HOST, API), (HOST, "127.0.0.1")):
            with self.subTest(host=host, public=public):
                self.assertNotEqual(dry_run("headscale", host, public, API, IRC).returncode, 0)


if __name__ == "__main__":
    unittest.main()
