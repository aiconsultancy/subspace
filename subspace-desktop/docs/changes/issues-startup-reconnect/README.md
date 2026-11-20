# Reconnect Issues after native account restoration

## Intent

Opening Issues while the native account is restoring can leave it on
**Sign-in required** after the account is usable. Pressing **Retry** starts
the same API connection again. Recover automatically from that startup race.

## Specification

The retained Issues responder checks whether the native account for its hub
is usable. If the initial connection asked for sign-in, it queues one
replacement bootstrap for the same account generation. The product reports
readiness from local account state. The existing API transport obtains
tokens on its worker. A persistent refusal leaves the existing **Retry**
action available. An earlier successful connection closes the startup
retry window.

## Delivery and tasks

- Cover delayed account readiness, a persistent refusal and a superseded
  account generation with local tests.
- Update the [Issues connection guide](../../issues.md#connect-and-sign-in).
- Board checklist: existing registration, descriptor, contracts, wiring
  and Help UI apply. This change uses the responder's existing poll and
  adds no component, configuration or tool.
