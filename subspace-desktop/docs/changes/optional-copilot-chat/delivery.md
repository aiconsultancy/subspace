# Delivery

These captures use isolated macOS debug builds in Cupertino Dark, a fresh
temporary home and the IRC board with no networks configured. The before
build uses the desktop feature branch at `b6032a2`.

![Before, opening Chat defaults to Copilot and shows its sign-in requirement](evidence/before-chat.png)

![After, opening Chat with no engine shows setup guidance and Chat off](evidence/after-chat-off.png)

![Before, Chat Settings selects Copilot by default](evidence/before-settings.png)

![After, Chat Settings selects Off on a fresh configuration](evidence/after-settings.png)

![After, the engine picker offers Off, Copilot, Pi and ACP](evidence/after-engine-options.png)

![After, a saved Copilot selection opens Copilot and shows its sign-in requirement](evidence/after-chat-copilot.png)

## Checks

- `make check` passes, including workspace tests, Clippy and script fixtures.
- `mise run quality` passes with no new findings.
- Regression tests cover the default engine, saved engine choices, experiment
  fallback and opening Chat without a runtime.
- Both capture builds produced the same macOS linker warning about the size of
  the unwind metadata.
