---
status: approved
owner: Jack
approved: 2026-11-18
approval: Slack C0CEQ7JRZND, thread 1795017294.971179, message 1795019557.224699
---

# Slice 1 specification

- Render representative real gallery cards offscreen on Linux into PNGs.
- Render each case in Subspace One Dark and Cupertino Dark.
- Use Inter and JetBrainsMono Nerd Font in both renderers.
- Show matching interactive React/shadcn cases in a built web Space.
- Capture the web cases with headless Chromium.
- Compare equal-size PNGs with zero non-anti-aliased pixel differences allowed.
- Save reference, web and difference images and a per-case report.
- Fail missing images, dimension differences and non-edge differences.
- Keep fixture inputs isolated from real credentials and application settings.
- No WASM, macOS runner, system package installation or Linux web-view port.

Only representative cases are required in this slice. Full-gallery parity is
later work. If Linux GPUI capture cannot run, stop before the web implementation
and report the exact blocker with evidence.
