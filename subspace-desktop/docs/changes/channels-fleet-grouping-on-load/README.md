# Group listed channels by fleet

## Intent

Show the Starbase and ships under their fleet as soon as the channel list and
fleet details are available.

## Spec

The navigator applies the network's roster to the IRC server's listed
channels as well as to its conversation buffers. Fleet channels use their
short names and fleet order. Unjoined channels remain greyed and keep their
user counts and Join action. Channels the fleet does not claim stay under
**Other** on a single-fleet network. The navigator refreshes when either
source changes, using the existing catalogue and IRC events.

## Delivery

The Channels navigator and listing tests passed. They cover both arrival
orders, a fleet detail update after loading and the list on a saved network
with no fleet. The sandbox denied the loopback listener needed for native
fixture captures.

## Tasks

- [x] Apply the fleet roster to the server's listed channels.
- [x] Verify regrouping through the catalogue and IRC event paths.
