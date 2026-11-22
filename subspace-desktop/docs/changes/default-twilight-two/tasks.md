# Tasks: Twilight Two for new installs

- [x] Add `shell::record_new_install_theme` to the Framework: record a
      catalogue variant as the saved theme only when neither product file
      exists.
- [x] State `NEW_INSTALL_THEME` (`Twilight Two`) in the Subspace descriptor
      and keep its `default_theme` at Cupertino Dark.
- [x] Record it at desktop start, before the built-in web Spaces write the
      config file, and never for a command-line or headless run.
- [x] Test a new install, each kind of existing install, unreadable and
      malformed files, an unknown theme, and a light-mode system.
- [x] Add `CAPTURE_FIRST_RUN=1` and the `resize:` capture step.
- [x] Update `README.md`, `docs/settings.md`, `docs/capture.md`, `AGENTS.md`
      and the Framework's `docs/appearance.md`.
- [x] Capture a first run at 1440 × 900.
