---
status: done
owner: Starbuck
approval: Jack, Slack message 1795019557.224699
---

# Linux reference proof

Use the pinned GPUI Kit version and its headless/test rendering APIs to capture
real representative gallery cards in both approved themes. Record renderer,
font hashes, dimensions and exact source revision with each capture batch.

Acceptance: real nonblank deterministic PNGs, no display or real app settings,
with an executable test that fails when renderer support is unavailable.
If build or renderer prerequisites cannot be satisfied without unapproved host
changes, record the observed failure and stop for the captain.

The captain installed the authorised Vulkan and native link prerequisites.
The direct renderer initialises with the pinned Lavapipe ICD. All six real
gallery PNGs render successfully and are byte-identical on an independent
repeat. The test now sets the approved font families before GPUI initialisation
and asserts its actual 2× display scale. See [spike evidence](linux-render-proof.md).

This proves native reference capture, not React parity. References are under
`evidence/` and the web package's `public/references/`.
