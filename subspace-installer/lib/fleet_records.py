"""A fleet's records in the Subspace API: its standard channels, ships and officers.

The desktop takes each agent's rank and IRC nick from the API's agent records
and joins the channels the fleet and its ships record, so the installer keeps
them in step with the fleet's roster:

- the fleet records Starbase and the command channel, and the address at which
  a desktop reaches its Stargate Fleet API (fleet_api_address());
- each captain has a ship, named for the ship (the captain's ``name``), which
  records the captain's ship channel;
- the rear admiral and each captain have an agent record with their rank and
  ``ircNick``, their IRC account. In a fleet with plain officer names (the
  default for new fleets) an officer has one name, its ID: its IRC account
  and nick, its agent name and its Stargate registration ID, so the desktop
  finds a captain's ship in the Fleet API by its nick. A fleet installed with
  fleet-prefixed names keeps them until it is renamed (subspace-setup
  rename-officers): a captain's agent name is its account,
  ``<slug>-<captain-id>``, and the rear admiral's is its own name.

Crew have no records: they are not on IRC, and the API attributes their work
to their captain with a crew name and model on each write.

Every route applies them with register(): fleetctl register (the installer's
own fleet), subspace-setup before it runs the installer (so agent dispatch can
sign the rear admiral in) and subspace-setup register-fleet (a further fleet's
registration). Only a human organisation administrator may change fleets and
agents, and only a human may change ships.
"""

import ipaddress
import re
from urllib.parse import quote

import standard_channels

# The API's rules for agent names, IRC nicks, ship names, models and account subjects.
AGENT_NAME = re.compile(r"[A-Za-z0-9-]{1,64}")
IRC_NICK = re.compile(r"[A-Za-z][A-Za-z0-9_-]{0,63}")
CAPTAIN_ID = re.compile(r"[a-z][a-z0-9-]{0,19}")
# A Fleet API address the API accepts (subspace-api src/fleets.ts), limited to forms whose stored form the
# installer can predict: an ASCII host name, an IPv4 address or a bracketed IPv6 address, and a plain path.
FLEET_API_URL = re.compile(r"(?P<scheme>[Hh][Tt][Tt][Pp][Ss]?)://(?P<host>\[[0-9A-Fa-f:.]+\]|[A-Za-z0-9.-]+)"
                           r"(?::(?P<port>[0-9]{1,5}))?(?P<path>(?:/[A-Za-z0-9._~!$&'()*+,;=:@%-]*)*)")
HOST_LABEL = re.compile(r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?")
LOOPBACK_HOSTS = ("localhost", "127.0.0.1", "[::1]")
FLEET_API_URL_LIMIT = 2048
OFFICERS = ("rearAdmiral", "captain")
PAGE = 200


class Refused(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise Refused(message)


def line(value, maximum):
    return isinstance(value, str) and 1 <= len(value) <= maximum and value.strip() == value and value.isprintable()


# Fleet agents' default account subjects: fictional development sign-in addresses, <account>@agents.example.com.
# subspace-setup signs the rear admiral in with its address to mint agent dispatch's token.
AGENT_EMAIL_DOMAIN = "agents.example.com"


def fleet_api_url(value, label="Fleet API address"):
    """A Fleet API address in the form the API stores it, or Refused.

    The API takes an absolute https URL on any host, or http only on loopback, without credentials, query or
    fragment, and stores it with a lower-case scheme and host, no default port and no trailing slash. The desktop
    sends the fleet's token to this address, so plain http is refused anywhere but on the desktop itself.
    """
    problem = (f"Invalid {label}: use https://<host>[:port][/path], or http only on localhost, 127.0.0.1 or "
               "[::1] (such as the SSH tunnel's end), without credentials, a query or a fragment.")
    match = FLEET_API_URL.fullmatch(value) if isinstance(value, str) and len(value) <= FLEET_API_URL_LIMIT else None
    require(match, problem)
    scheme, host, port, path = (match["scheme"].lower(), match["host"].lower(), match["port"], match["path"])
    if host.startswith("["):
        try:
            host = f"[{ipaddress.IPv6Address(host[1:-1]).compressed}]"
        except ValueError:
            raise Refused(problem) from None
    elif re.fullmatch(r"[0-9.]+", host):
        # Only a full dotted quad: the API's URL parser reads shorter numeric forms differently.
        parts = host.split(".")
        require(len(parts) == 4 and all(part.isdigit() and str(int(part)) == part and int(part) < 256
                                        for part in parts), problem)
    else:
        labels = host.split(".")
        require(all(HOST_LABEL.fullmatch(label) for label in labels) and not labels[-1].isdigit(), problem)
    require(scheme == "https" or host in LOOPBACK_HOSTS, problem)
    if port is not None:
        require(1 <= int(port) <= 65535, problem)
        port = None if int(port) == {"https": 443, "http": 80}[scheme] else str(int(port))
    segments = path.split("/")[1:]
    require(all(segment.lower().replace("%2e", ".") not in (".", "..") for segment in segments), problem)
    address = f"{scheme}://{host}{f':{port}' if port else ''}{path}".rstrip("/")
    require(len(address) <= FLEET_API_URL_LIMIT, problem)
    return address


def fleet_api_address(cfg):
    """The address a desktop reaches the fleet's Stargate Fleet API at, or None when it has none.

    An address set in api.fleetApiUrl (an HTTPS name through Tailscale or Cloudflare, say) comes first.
    Otherwise it is the SSH tunnel's end on the desktop: every way of reaching a fleet (the SSH tunnel itself,
    over Tailscale or through Cloudflare) forwards the Fleet API's port to the same port on the desktop's
    loopback. Without stargate.api there is no Fleet API, so the fleet records none.
    """
    stargate_api = cfg["stargate"].get("api")
    if not stargate_api:
        return None
    return (cfg["api"] or {}).get("fleetApiUrl") or f"http://127.0.0.1:{stargate_api['port']}"


def subject(account):
    """An officer's default account subject (the API's serviceTokenClientId)."""
    return f"{account.lower()}@{AGENT_EMAIL_DOMAIN}"


def fleet_fields(cfg):
    """The fleet's own API fields for a validated fleet config, as the desktop reaches its IRC server."""
    irc = cfg["irc"].get("advertise", cfg["irc"])
    return {
        "slug": cfg["slug"], "name": cfg["name"], "ircHost": irc["host"], "ircPort": irc["port"],
        "ircTls": irc["tls"], "ircWebsocketUrl": irc.get("websocketUrl"), "vmHost": (cfg["api"] or {}).get("vmHost"),
        "fleetApiUrl": fleet_api_address(cfg),
    }


def agent_name(cfg, person):
    """An officer's agent name in the Subspace API: its ID, which is also its IRC account, with plain names;
    with fleet-prefixed ones the rear admiral's own name and a captain's account."""
    if cfg.get("officerNames") == "plain":
        return person["account"]
    return person["name"] if person["role"] == "rear_admiral" else person["account"]


def roster(cfg):
    """The records for a validated fleet config (runtime.validate)."""
    rear, *captains = cfg["members"]

    def officer(person):
        return {"ircNick": person["account"], "defaultModel": person["harness"]["model"],
                "serviceTokenClientId": person.get("serviceTokenClientId") or subject(person["account"])}

    return {
        "starbaseChannel": cfg["irc"]["starbaseChannel"], "commandChannel": cfg["irc"]["commandChannel"],
        "rearAdmiral": {"name": agent_name(cfg, {**rear, "role": "rear_admiral"}), **officer(rear)},
        "captains": [{"name": agent_name(cfg, {**person, "role": "captain"}), "ship": person["name"],
                      "channel": person["channel"], **officer(person)} for person in captains],
    }


def check(slug, records):
    """Refuse records the API would refuse or that are not this fleet's standard set; return them."""
    require(isinstance(records, dict), "The fleet's records must be an object.")
    require(records.get("starbaseChannel") == standard_channels.starbase(slug)
            and records.get("commandChannel") == standard_channels.command(slug),
            f"The fleet's records must name its standard channels: {standard_channels.summary(slug)}.")
    rear, captains = records.get("rearAdmiral"), records.get("captains")
    require(isinstance(rear, dict) and isinstance(captains, list), "The records need a rearAdmiral and a captains array.")
    officers = [rear, *captains]
    for person in officers:
        require(isinstance(person, dict), "Each officer's record must be an object.")
        nick = person.get("ircNick")
        require(isinstance(nick, str) and IRC_NICK.fullmatch(nick), "Each officer needs an IRC nick (its IRC account).")
        require(line(person.get("defaultModel"), 100), f"{nick}'s model must be one line of at most 100 characters.")
        require(line(person.get("serviceTokenClientId"), 200),
                f"{nick}'s serviceTokenClientId must be one line of at most 200 characters.")
    require(isinstance(rear.get("name"), str) and AGENT_NAME.fullmatch(rear["name"]),
            f"The rear admiral's name, {rear.get('name')!r}, is also its name in the Subspace API, which allows "
            "only letters, digits and hyphens, at most 64. Rename the rear admiral.")
    for person in captains:
        nick = person["ircNick"]
        # A captain's agent name is its account (a further fleet's registration may leave it out), so the account
        # must also be an API agent name.
        person.setdefault("name", nick)
        require(person["name"] == nick and AGENT_NAME.fullmatch(nick),
                f"Captain {nick}'s IRC account is also its agent name in the Subspace API, which allows only letters, "
                "digits and hyphens. Choose an account without underscores.")
        require(line(person.get("ship"), 64), f"Captain {nick}'s ship name must be one line of at most 64 characters.")
        channel = person.get("channel")
        # A ship channel is the standard one: named after the captain with plain names, #<captain-id> (its
        # account), else #<slug>-<captain-id>.
        plain = isinstance(channel, str) and channel == f"#{nick.lower()}" and CAPTAIN_ID.fullmatch(nick)
        captain_id = channel.removeprefix(f"#{slug}-") if isinstance(channel, str) and channel.startswith(f"#{slug}-") else ""
        require((plain and standard_channels.problem(slug, [nick], plain=True) is None)
                or (CAPTAIN_ID.fullmatch(captain_id) and standard_channels.problem(slug, [captain_id]) is None),
                f"Captain {nick}'s ship channel must be its standard ship channel: #{nick.lower()} in a fleet with "
                f"plain officer names, else #{slug}-<captain-id>.")
    names = [rear["name"], *(person["name"] for person in captains)]
    nicks = [person["ircNick"] for person in officers]
    ships = [person["ship"] for person in captains]
    channels = [person["channel"] for person in captains]
    for values, label in ((names, "agent names (the rear admiral's name and the captains' accounts)"),
                          (nicks, "IRC accounts"), (ships, "ship names"), (channels, "ship channels")):
        require(len({value.lower() for value in values}) == len(values), f"The fleet's {label} must differ, ignoring case.")
    return records


def same(a, b):
    return isinstance(a, str) and isinstance(b, str) and a.lower() == b.lower()


def holder(row):
    """An API agent record in words: its name and fleet."""
    if not row:
        return "another agent"
    fleet = row.get("fleet")
    return f"agent {row.get('name')}" + (f" of fleet {fleet}" if fleet else "") + (
        "" if row.get("isActive", True) else " (deactivated)")


def status(error):
    return getattr(error, "status", None)


class Writer:
    """API writes that explain the refusals an installer can meet."""

    def __init__(self, call):
        self.call = call

    def __call__(self, method, path, body, conflict, invalid=""):
        try:
            return self.call(method, path, body)
        except Exception as error:  # The callers' API errors carry the HTTP status; anything else passes.
            # The API's own reason, such as which agent and fleet hold a name, when the caller read it.
            detail = getattr(error, "detail", "") or ""
            detail = f" The API says: {detail}" if detail else ""
            if status(error) == 403:
                raise Refused("The Subspace API refused to change the fleet's records: registering a fleet, its ships "
                              "and its agents needs a human organisation administrator's token.") from None
            if status(error) == 409 and conflict:
                raise Refused(conflict + detail) from None
            if status(error) in (400, 422):
                raise Refused(f"The Subspace API refused {method} {path} as invalid. {invalid or conflict}{detail}") \
                    from None
            raise


def register(call, fields, records, say=print):
    """Create or update the fleet with its standard channels, then reconcile() its ships and officers.

    fields are the fleet's own API fields (fleet_fields(), or a further fleet's registration). Returns the
    fleet as the API has it before the ships and agents changed.
    """
    write = Writer(call)
    fields = {**fields, "starbaseChannel": records["starbaseChannel"], "commandChannel": records["commandChannel"]}
    path = f"/fleets/{fields['slug']}"
    try:
        fleet = call("GET", path, None)
    except Exception as error:
        if status(error) != 404:
            raise
        try:
            fleet = write("POST", "/fleets", fields, None, "Check the fleet's IRC settings.")
        except Exception as conflict:  # Another run registered it first.
            if status(conflict) != 409:
                raise
            fleet = call("GET", path, None)
    changes = {key: value for key, value in fields.items() if fleet.get(key) != value}
    if changes:
        fleet = write("PATCH", path, changes, f"Fleet {fields['slug']} changed while it was being registered. Try again.")
    reconcile(call, fleet, records, say)
    return fleet


def agents(call):
    rows, cursor = [], ""
    while True:
        page = call("GET", f"/agents?limit={PAGE}" + (f"&cursor={quote(cursor, safe='')}" if cursor else ""), None)
        rows += page.get("items", [])
        cursor = page.get("nextCursor") or ""
        if not cursor:
            return rows


def reconcile(call, fleet, records, say=print):
    """Bring the fleet's ships and officer agents in step with records (see check()).

    call(method, path, body) sends one request under /api/v1 and returns the decoded JSON; its errors carry
    `status`. Existing records are found by IRC nick, else by account subject, and updated only when they
    differ, so a re-run sends no writes. An officer the roster no longer has is deactivated (a later run that
    has it again reactivates it), and its ship's channel is cleared. Returns the number of writes.
    """
    write = Writer(call)
    fleet_id = fleet["id"]
    ships = {row["id"]: row for row in call("GET", f"/fleets/{fleet_id}/ships", None)}
    known = agents(call)
    writes, used_ships, handled = 0, set(), set()
    for rank, person in [("rearAdmiral", records["rearAdmiral"]), *(("captain", row) for row in records["captains"])]:
        nick, account_subject = person["ircNick"], person["serviceTokenClientId"]
        existing = (next((row for row in known if same(row.get("ircNick"), nick)), None)
                    or next((row for row in known if same(row.get("serviceTokenClientId"), account_subject)), None))
        require(existing is None or same(existing.get("serviceTokenClientId"), account_subject),
                f"The Subspace API already has an agent with IRC nick {nick} under another account subject. Give the "
                "officer that agent's serviceTokenClientId, or clear that agent's nick, then register again.")
        require(existing is None or existing.get("fleetId") in (None, fleet_id),
                f"The Subspace API's agent for {nick} (by IRC nick or account subject) belongs to another fleet. "
                "Each fleet's IRC accounts and account subjects must be its own.")
        name = person["name"]
        # As the API compares: a name or nick another agent holds as either, where this one is new or changes.
        new_name = existing is None or not same(existing.get("name"), name)
        new_nick = existing is None or not same(existing.get("ircNick"), nick)
        clash = next((row for row in known if row is not existing and (
            same(row.get("name"), name) or (new_name and same(row.get("ircNick"), name))
            or (new_nick and same(row.get("name"), nick)))), None)
        require(clash is None, f"The name {name} is taken in the Subspace API: {holder(clash)} has it. An agent has one "
                "name, and agent names and IRC nicks are unique in an organisation, ignoring case. "
                + ("Rename the rear admiral." if rank == "rearAdmiral" else "Choose another name for the captain."))
        ship_id = None
        if rank == "captain":
            ship_id, changed = ship(write, fleet_id, ships, used_ships, person, existing)
            writes += changed
        body = {"name": name, "rank": rank, "fleetId": fleet_id, "shipId": ship_id, "defaultModel": person["defaultModel"],
                "serviceTokenClientId": account_subject, "ircNick": nick}
        conflict = f"Another agent already has {nick}'s name, IRC nick or account subject."
        invalid = (f"Check {nick}'s serviceTokenClientId: the API takes the identity provider's subject (an Entra "
                   "object ID, a WorkOS subject or a development sign-in email).")
        role = "the rear admiral" if rank == "rearAdmiral" else "captain"
        if existing is None:
            write("POST", "/agents", body, conflict, invalid)
            say(f"Registered {role} {name} with the Subspace API.")
            writes += 1
            continue
        handled.add(existing["userId"])
        if not existing.get("isActive", True):
            existing = write("POST", f"/agents/{existing['userId']}/reactivate", {"version": existing["version"]}, conflict)
            say(f"Reactivated {role} {name} in the Subspace API.")
            writes += 1
        current = {key: existing.get(key) for key in body}
        if same(current["serviceTokenClientId"], account_subject):
            current["serviceTokenClientId"] = account_subject
        if current != body:
            write("PUT", f"/agents/{existing['userId']}", {**body, "version": existing["version"]}, conflict, invalid)
            say(f"Updated {role} {name} in the Subspace API.")
            writes += 1
    for row in known:
        if (row["userId"] in handled or row.get("fleetId") != fleet_id or row.get("rank") not in OFFICERS
                or not row.get("isActive", True)):
            continue
        write("POST", f"/agents/{row['userId']}/deactivate", {"version": row["version"]},
              f"Agent {row.get('name')} changed while it was being retired. Register again.")
        say(f"Deactivated agent {row.get('name')} in the Subspace API: the fleet's roster no longer has it.")
        writes += 1
        old = ships.get(row.get("shipId"))
        if old and old["id"] not in used_ships and old.get("channel"):
            write("PATCH", f"/ships/{old['id']}", {"name": old["name"], "channel": None, "version": old["version"]},
                  f"Ship {old['name']} changed while its channel was being cleared. Register again.")
            old["channel"] = None
            writes += 1
    return writes


def ship(write, fleet_id, ships, used, person, existing):
    """The captain's ship, found by its channel, the agent's ship or its name, made to match. (ID, writes)."""
    name, channel = person["ship"], person["channel"]
    free = [row for row in ships.values() if row["id"] not in used]
    found = (next((row for row in free if same(row.get("channel"), channel)), None)
             or next((row for row in free if existing and row["id"] == existing.get("shipId")), None)
             or next((row for row in free if same(row.get("name"), name)), None))
    conflict = f"Another ship in the fleet is already named {name}."
    if found is None:
        found = write("POST", f"/fleets/{fleet_id}/ships", {"name": name, "channel": channel}, conflict)
        ships[found["id"]] = found
        used.add(found["id"])
        return found["id"], 1
    used.add(found["id"])
    if found.get("name") == name and found.get("channel") == channel:
        return found["id"], 0
    updated = write("PATCH", f"/ships/{found['id']}", {"name": name, "channel": channel, "version": found["version"]},
                    conflict)
    ships[found["id"]] = updated
    return found["id"], 1


def usernames(call):
    """The usernames of the organisation's people (GET /users, which lists active members): a person's username
    is their IRC account, so the API refuses it as an agent's name or nick."""
    names, cursor = [], ""
    while True:
        page = call("GET", f"/users?limit={PAGE}" + (f"&cursor={quote(cursor, safe='')}" if cursor else ""), None)
        names += [row["username"] for row in page.get("items", [])
                  if row.get("kind") == "human" and isinstance(row.get("username"), str)]
        cursor = page.get("nextCursor") or ""
        if not cursor:
            return names


def taken_names(rows, slug, officers, people=()):
    """Officers whose name or IRC nick another API agent or a person holds, as messages naming that agent and its
    fleet (never the person).

    rows: the organisation's agents (GET /agents). officers: (agent name, IRC nick, own nicks) per officer,
    where own nicks are the nicks this officer's own record may have now (its current one, and its old one
    during a rename): such a record of this fleet is the officer itself. people: people's usernames (usernames()).
    """
    problems = []
    for name, nick, own in officers:
        mine = [row for row in rows if row.get("fleet") == slug and any(same(row.get("ircNick"), value) for value in own)]
        # A name the officer's own record already has is not new: the API accepts it as it stands, even where an
        # older record shares it (the API compares only names that change).
        wanted = [value for value in (name, nick)
                  if not any(same(row.get("name"), value) or same(row.get("ircNick"), value) for row in mine)]
        found = None
        for row in rows:
            if row in mine:
                continue
            taken = [value for value in wanted if same(row.get("name"), value) or same(row.get("ircNick"), value)]
            if taken:
                found = f"{taken[0]} is taken in the Subspace API: {holder(row)} has it"
                break
        person = next((value for value in wanted if any(same(username, value) for username in people)), None)
        if found is None and person:
            found = f"{person} is taken in the Subspace API: a person in the organisation has it as their username"
        if found:
            problems.append(found)
    return problems


def rename(call, slug, renames, say=print):
    """Rename a fleet's officers' API agent records in place, keeping their IDs, keys and history: each from its
    old IRC nick to its new name and nick, and a captain's ship channel from its old name to its new one.

    renames: [{"old": old nick, "name": new agent name, "nick": new nick, "channel": (old, new) or None}].
    Uses GET /fleets/{slug}, GET /agents, PUT /agents/{id} and PATCH /ships/{id}, an organisation administrator's
    routes. A record already renamed is left alone, so a re-run after a partial failure finishes the rest.
    Returns the number of writes.
    """
    write = Writer(call)
    fleet = call("GET", f"/fleets/{slug}", None)
    known = agents(call)
    mine = [row for row in known if row.get("fleetId") == fleet["id"]]
    ships = {row["id"]: row for row in call("GET", f"/fleets/{fleet['id']}/ships", None)}
    writes = 0
    for item in renames:
        row = (next((row for row in mine if same(row.get("ircNick"), item["nick"])), None)
               or next((row for row in mine if same(row.get("ircNick"), item["old"])), None))
        require(row is not None, f"Fleet {slug} has no agent record with IRC nick {item['old']} or {item['nick']} in "
                "the Subspace API, so there is nothing to rename. Register the fleet first.")
        clash = next((other for other in known if other is not row and any(
            same(other.get(key), value) for key in ("name", "ircNick") for value in (item["name"], item["nick"]))),
            None)
        require(clash is None, f"Cannot rename {item['old']} to {item['nick']}: {holder(clash)} has that name in the "
                "Subspace API. Agent names and IRC nicks are unique in an organisation, ignoring case.")
        if row.get("name") != item["name"] or row.get("ircNick") != item["nick"]:
            body = {"name": item["name"], "rank": row["rank"], "fleetId": fleet["id"], "shipId": row.get("shipId"),
                    "defaultModel": row["defaultModel"], "serviceTokenClientId": row["serviceTokenClientId"],
                    "ircNick": item["nick"], "version": row["version"]}
            write("PUT", f"/agents/{row['userId']}", body,
                  f"Could not rename {item['old']} to {item['nick']} in the Subspace API.",
                  f"Check {item['old']}'s record in the Subspace API.")
            say(f"Renamed agent {row.get('name')} (IRC nick {row.get('ircNick')}) to {item['name']} (IRC nick "
                f"{item['nick']}) in the Subspace API; its ID {row['userId']}, keys and history are kept.")
            writes += 1
        ship_row = ships.get(row.get("shipId"))
        # The ship's recorded channel moves with it; a ship with none (enrolment makes ships without one) gets the
        # new standard channel. A channel set to something else by hand stays.
        if item.get("channel") and ship_row and (ship_row.get("channel") is None
                                                 or same(ship_row.get("channel"), item["channel"][0])):
            write("PATCH", f"/ships/{ship_row['id']}", {"name": ship_row["name"], "channel": item["channel"][1],
                                                        "version": ship_row["version"]},
                  f"Ship {ship_row['name']} changed while its channel was being moved. Run the rename again.")
            say(f"Moved ship {ship_row['name']}'s channel from {ship_row['channel']} to {item['channel'][1]} in the "
                "Subspace API." if ship_row.get("channel") else
                f"Gave ship {ship_row['name']} its channel {item['channel'][1]} in the Subspace API.")
            writes += 1
    return writes
