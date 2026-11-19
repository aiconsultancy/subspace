# Tasks: glass capsule board tabs and title-bar spacing

- [x] Measure AppKit's traffic-light geometry on macOS 27 (14 pt buttons,
      9 pt apart).
- [x] Start title-bar content 20 pt after the zoom button in both navigation
      modes, independent of `--pad-x`.
- [x] Size the first row to hold a tab capsule with clear space, and centre
      the traffic lights on it when its height changes.
- [x] Add `subspace_dashboard::glass`: the capsule navigation button, its
      material, label colours and shadow bleed.
- [x] Add the `--nav-round` density token and the `glass`, `glass-rim` and
      `glass-shadow` tokens and theme tones. Remove `switcher`, `nav_select`
      and `nav_select_fg`.
- [x] Set glass tones for Subspace One Dark (matching the default tokens),
      Cupertino Light and Cupertino Dark.
- [x] Use the capsule for the horizontal tabs and the vertical rail.
- [x] Tests: traffic-light centring and clearance, inset tabs with gaps, only
      the current board pressed (none while Logs is open), glass material,
      tokens, and theme contrast.
- [x] Update `docs/ui-components.md` and `AGENTS.md`.
- [x] Capture before and after evidence in One Dark, Cupertino Light and
      Cupertino Dark, plus vertical navigation.
