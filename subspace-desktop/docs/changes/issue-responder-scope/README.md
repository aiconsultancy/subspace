# Issue reference scope and hub replacement

A card in memory must only be visible while the current issue account can read it. An open Issues board must continue to refresh when the provider is restarted. The broker should report a failed registration or announcement in release builds.

## Behaviour

- The hub projects an organisation and cached cards only while it has an organisation and no account error. Sign-out, disabled issue packs, lost permissions and lost membership withhold both. Transient read errors keep the current organisation.
- The contract answers `SignedOut` for a signed-out current profile and `Unavailable` when an organisation cannot be read. Missing issues and individual read failures retain their existing answers when the scope is usable.
- While open, the Issues board follows hub installation and removal. It observes the current responder's timer for list and detail updates after a replacement.
- The contract broker logs rejected registrations and announcements with the board and message name. The Issues responder adds no issue data to those logs.

## Verification

- Test the real loopback pack-off response after a card has been resolved and the hub projection with populated caches after permission loss.
- Test a 50-identifier contract batch with a cached card, a duplicate, a missing issue and an unknown team. Count HTTP reads and reuse the existing reference-priority test.
- Replace the hub while the board is open. Hold its first group page until the board has switched hubs, then let the responder timer deliver the page.

## Issues board checklist

- **Contracts and lifecycle.** Keep the declared `issues@1` messages and follow the local Issues hub global. The board uses the broker for cross-board communication.
- **Caching and refresh.** Keep reference cards in memory. Leave the existing reader priorities and shared responder timer in place.
- **Diagnostics.** Use the broker's existing content-free warnings.
- **Tests and evidence.** Cover the changed access and refresh paths with the fixture tests above. The change has no new visual treatment to capture.
- **Other board integration.** The existing Issues registration, layout, controls, settings, search, agent tools, chat context, notices and Help continue through their existing integrations.
