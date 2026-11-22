"""The standard IRC channels every Subspace fleet has.

Each fleet, whatever else it adds, has exactly these channels, named from its
slug, and the desktop recognises them by name:

- Starbase, ``#<slug>-starbase``: people talk to the rear admiral and to each other.
- The fleet command channel, ``#<slug>-command``: the rear admiral and the
  captains coordinate, and agent dispatch offers and answers go there.
- One ship channel per captain, ``#<slug>-<captain-id>``.

The fleet installer (runtime.py) and subspace-setup (vm/fleet_setup.py) both
take the names from here, so the command channel can be renamed in one edit.
"""

STARBASE_SUFFIX = "starbase"
# The command channel's suffix. The owner has yet to choose between "command"
# and "fleet-command"; change it here and nowhere else.
COMMAND_SUFFIX = "command"
# Stargate refuses channel names over 50 bytes (a common IRC CHANNELLEN); Ergo allows 64.
MAX_CHANNEL_BYTES = 50
# Stargate and Ergo limit nicks and SASL accounts to 32 characters.
MAX_ACCOUNT = 32


def starbase(slug):
    return f"#{slug}-{STARBASE_SUFFIX}"


def command(slug):
    return f"#{slug}-{COMMAND_SUFFIX}"


def ship(slug, captain_id):
    return f"#{slug}-{captain_id}"


def standard(slug, captain_ids):
    """Starbase, the command channel, then each captain's ship channel, in roster order."""
    return [starbase(slug), command(slug), *(ship(slug, captain_id) for captain_id in captain_ids)]


def summary(slug):
    """The standard names, for messages and summaries."""
    return (f"{starbase(slug)} (Starbase), {command(slug)} (fleet command) and "
            f"{ship(slug, '<captain-id>')} for each captain")


def problem(slug, captain_ids):
    """Why the standard channels cannot be made for this roster, or None when they can."""
    names = standard(slug, captain_ids)
    for name in names:
        if len(name.encode()) > MAX_CHANNEL_BYTES:
            return (f"The standard channel {name} is {len(name.encode())} characters, but IRC channel names "
                    f"are limited to {MAX_CHANNEL_BYTES}. Use a shorter fleet slug or captain ID.")
    for captain_id in captain_ids:
        if ship(slug, captain_id) in names[:2]:
            return (f"Captain ID {captain_id} would give a ship channel named like a standard fleet channel "
                    f"({ship(slug, captain_id)}). Choose another captain ID.")
    return None
