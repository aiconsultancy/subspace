"""The standard IRC channels every Subspace fleet has.

Each fleet, whatever else it adds, has exactly these channels, and the desktop
reads them from the fleet's records in the Subspace API:

- Starbase, ``#<slug>-starbase``: people talk to the rear admiral and to each other.
- The fleet command channel, ``#<slug>-command``: the rear admiral and the
  captains coordinate, and agent dispatch offers and answers go there.
- One ship channel per captain. In a fleet with plain officer names (the
  default for new fleets), it is named after the captain, ``#<captain-id>``,
  which is also the captain's IRC account. A fleet installed with fleet-prefixed
  names keeps ``#<slug>-<captain-id>`` until it is renamed.

Starbase and the command channel keep the fleet's slug: several fleets can
share one IRC server, and every fleet has both. The fleet installer
(runtime.py) and subspace-setup (vm/fleet_setup.py) both take the names from
here, so the command channel can be renamed in one edit.
"""

STARBASE_SUFFIX = "starbase"
# The command channel's suffix, #<slug>-command. To rename the channel, change it here and nowhere
# else in the code (the documentation names #<slug>-command).
COMMAND_SUFFIX = "command"
# Stargate refuses channel names over 50 bytes (a common IRC CHANNELLEN); Ergo allows 64.
MAX_CHANNEL_BYTES = 50
# Stargate and Ergo limit nicks and SASL accounts to 32 characters.
MAX_ACCOUNT = 32
# How a fleet names its officers: "plain" (each officer's IRC account, API name and Stargate ID are its ID,
# and a ship channel is #<captain-id>) or "prefixed" (accounts carry the fleet slug, as subspace-setup named
# them before plain names, and a ship channel is #<slug>-<captain-id>).
NAMING = ("plain", "prefixed")


def starbase(slug):
    return f"#{slug}-{STARBASE_SUFFIX}"


def command(slug):
    return f"#{slug}-{COMMAND_SUFFIX}"


def ship(slug, captain_id, plain=False):
    return f"#{captain_id}" if plain else f"#{slug}-{captain_id}"


def standard(slug, captain_ids, plain=False):
    """Starbase, the command channel, then each captain's ship channel, in roster order."""
    return [starbase(slug), command(slug), *(ship(slug, captain_id, plain) for captain_id in captain_ids)]


def summary(slug, plain=False):
    """The standard names, for messages and summaries."""
    return (f"{starbase(slug)} (Starbase), {command(slug)} (fleet command) and "
            f"{ship(slug, '<captain-id>', plain)} for each captain")


def problem(slug, captain_ids, plain=False):
    """Why the standard channels cannot be made for this roster, or None when they can."""
    names = standard(slug, captain_ids, plain)
    for name in names:
        if len(name.encode()) > MAX_CHANNEL_BYTES:
            return (f"The standard channel {name} is {len(name.encode())} characters, but IRC channel names "
                    f"are limited to {MAX_CHANNEL_BYTES}. Use a shorter fleet slug or captain ID.")
    for captain_id in captain_ids:
        if ship(slug, captain_id, plain) in names[:2]:
            return (f"Captain ID {captain_id} would give a ship channel named like a standard fleet channel "
                    f"({ship(slug, captain_id, plain)}). Choose another captain ID.")
    return None
