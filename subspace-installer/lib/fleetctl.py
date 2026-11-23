#!/usr/bin/python3
"""Operate an installed fleet and apply Subspace API registrations."""

import argparse
import json
import os
import stat
import sys
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener

import fleet_records
from runtime import APP, ETC, UUID, Invalid, pattern, read_json, require, validate


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_args):
        return None


class ApiError(Invalid):
    def __init__(self, method, status):
        self.status = status
        super().__init__(f"Subspace API returned HTTP {status} for {method}.")


class Api:
    def __init__(self, cfg):
        require(cfg["api"], "Configure the Subspace API before assigning initiatives.")
        self.base = cfg["api"]["url"] + "/v1"
        self.headers = {
            "Authorization": "Bearer " + cfg["token"], "Accept": "application/json", "Content-Type": "application/json",
        }
        self.opener = build_opener(NoRedirect())

    def request(self, method, path, body=None):
        payload = None if body is None else json.dumps(body).encode()
        request = Request(self.base + path, data=payload, headers=self.headers, method=method)
        try:
            with self.opener.open(request, timeout=20) as response:
                value = json.load(response)
        except HTTPError as error:
            error.close()
            raise ApiError(method, error.code) from None
        except (URLError, OSError, ValueError):
            raise Invalid(f"Could not read the Subspace API response for {method}.") from None
        require(isinstance(value, (dict, list)), "Unexpected Subspace API response.")
        return value

    def assign(self, initiative, fleet_id):
        self.request("PUT", f"/initiatives/{initiative}/fleet", {"fleetId": fleet_id})
        print(f"Assigned initiative {initiative}." if fleet_id else f"Unassigned initiative {initiative}.")


def register(cfg):
    """Register the fleet, its standard channels, ships and officers (fleet_records), then its initiatives."""
    if not cfg["api"]:
        return
    api = Api(cfg)
    fields = fleet_records.fleet_fields(cfg)
    try:
        fleet = fleet_records.register(api.request, fields, fleet_records.roster(cfg))
    except fleet_records.Refused as error:
        raise Invalid(str(error)) from None
    fleet_id = pattern(fleet.get("id"), UUID, "API fleet UUID")
    print(f"Fleet {cfg['slug']} registered as {fleet_id}, with its standard channels, ships and officers.")
    if fields["fleetApiUrl"]:
        print(f"Desktops reach its Stargate Fleet API at {fields['fleetApiUrl']}; the desktop app asks for its token once.")
    for initiative in cfg["api"]["initiatives"]:
        current = api.request("GET", f"/initiatives/{initiative}")
        if current.get("fleetId") != fleet_id:
            api.assign(initiative, fleet_id)


def attach(member):
    member = pattern(member, r"[a-z][a-z0-9-]{0,19}", "rear-admiral or captain ID")
    units = (ETC / "units").read_text().splitlines()
    require(f"subspace-fleet-engine-{member}.service" in units,
            "No installed OpenCode engine for this member. Crew run on their captain's server: "
            "attach to the captain and open the crew member's session from its session list.")
    settings = ETC / f"engine-{member}.json"
    require(settings.is_file(), "Unknown fleet member.")
    os.execvp("runuser", ["runuser", "--pty", "-u", f"ss-{member}", "--",
                         str(APP / "lib/runtime.py"), "engine", str(settings), "attach"])


def api_token():
    """Print Stargate's Fleet API token, which only the primary gateway's user can read."""
    cfg = read_json(ETC / "stargate.json")
    require("api" in cfg, "Stargate's Fleet API is not enabled. Set stargate.api in the fleet config and upgrade.")
    checks = read_json(ETC / "gateway-checks.json", private=True)
    user, settings = next(((user, path) for user, path in checks if path.endswith("/gateway.json")), (None, None))
    require(user, "No primary Stargate gateway is installed.")
    os.execvp("runuser", ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "gateway", settings, "api", "token"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("register", help="sync the configured fleet, its channels, ships, officers and initiatives")
    assign = sub.add_parser("assign", help="assign an initiative to a fleet")
    assign.add_argument("initiative")
    assign.add_argument("fleet", help="fleet slug, UUID, or 'none'")
    tty = sub.add_parser("attach", help="open the rear admiral's or a captain's OpenCode tmux session (crew sessions are on the captain's server)")
    tty.add_argument("member")
    sub.add_parser("api-token", help="print the Stargate Fleet API token for an operator's client")
    args = parser.parse_args()
    require(os.geteuid() == 0, "Run fleetctl as root.")
    os.umask(0o077)
    if args.command == "attach":
        attach(args.member)
    if args.command == "api-token":
        api_token()
    private = ETC / "secrets.json"
    require(stat.S_IMODE(private.stat().st_mode) == 0o600, "Installed secrets must have mode 0600.")
    cfg = validate(read_json(ETC / "fleet.json"), read_json(private, private=True))
    if args.command == "register":
        register(cfg)
    elif args.command == "assign":
        initiative = pattern(args.initiative, UUID, "initiative UUID")
        api = Api(cfg)
        fleet_id = None
        if args.fleet != "none":
            fleet = pattern(args.fleet, r"[a-zA-Z0-9][a-zA-Z0-9-]{0,63}", "fleet ID or slug")
            fleet_id = pattern(api.request("GET", f"/fleets/{fleet}").get("id"), UUID, "API fleet UUID")
        api.assign(initiative, fleet_id)


if __name__ == "__main__":
    try:
        main()
    except Invalid as error:
        sys.exit(f"subspace-fleet: {error}")
    except (OSError, ValueError):
        sys.exit("subspace-fleet: fleetctl could not read the installed configuration or execute the request.")
