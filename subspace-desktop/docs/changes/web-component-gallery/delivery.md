---
status: approved
owner: Jack
approved: 2026-11-18
approval: Slack C0CEQ7JRZND, thread 1795017294.971179, message 1795019557.224699
---

# Slice 1 delivery

1. Prove Linux offscreen capture through GPUI's test platform and its real
   offscreen renderer. Use existing gallery constructors and theme documents.
   Do not approximate GPUI with custom raster drawing or substitute React PNGs.
2. If capture works, build React/shadcn examples and both presets under
   `subspace-desktop/web-spaces/examples/component-gallery/`.
3. Add deterministic headless Chromium capture and exact non-edge comparison.
   Preserve image differences in the report; do not widen tolerance.
4. Run focused tests and normal repository checks. Obtain one Opus 5.5 visual
   review. Commit, push and open a draft PR when slice 1 works, then stop.

The earlier [plan](plan.md) retains source research. Its approved phase 2
section overrides the former WASM and macOS options. The
[inventory and themes](inventory-and-themes.md) remain the coverage reference.

Observed checks and limits are in [slice 1 results](slice-1-results.md).

The owner's 19 November expansion and current full-catalogue work are recorded
in [full gallery visual review](full-gallery-visual.md). It does not change the
historical slice 1 strict metrics or claim completed native/delivery gates.
