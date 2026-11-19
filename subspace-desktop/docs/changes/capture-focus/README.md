# Reliable capture dispatch

## Intent

Fix jt's 16 November 2026 fork-review item 3: a focused chat composer could
swallow capture actions and navigation keys, and unhandled actions silently
succeeded.

## Specification and delivery

- A debug capture-only focus handle in the window registry tracks the shell's
  root element. Its small builder lives with the shell's layout helpers so
  capture does not enlarge the app constructor or root rendering module. Capture
  draws the current dispatch tree, checks the named action has a handler on
  that root path, then dispatches synchronously through the handle. This
  bypasses focused inputs without changing focus or overriding a handler's
  focus choice. Registered actions handled only in a descendant are rejected
  too. Unknown and unhandled actions name the action in the error.
- Each `key:` step first focuses the active Space, draws its dispatch tree,
  then sends the entire sequence normally. Keys go to the focused element;
  Tab can move focus within a sequence. `type:` preserves focus so a click can
  select a composer before entering a draft. This keeps existing GPUI input
  dispatch and avoids duplicating keyboard routing.
- A capture error logs `capture failed` and exits with status 1. The launcher
  preserves logs and treats that diagnostic as failure even when macOS `open`
  returns success; it also preserves a launcher error status.
- Capture starts on the first Space unless fixture state sets `app.dashboard`.
  The agent instructions, launcher header and capture module document this.

## Verification and tasks

- [x] Headless test with the real shared `ChatInput` focused: the shell's
  ToggleChat action reaches its handler and does not steal focus.
- [x] Headless unknown action, registered unhandled action, and action handled
  only by a descendant all fail with their names.
- [x] Headless key step reaches the active Space while preserving a typed
  composer draft; typing still reaches the composer before navigation.
- [x] Child-process test verifies the failure diagnostic and exit status 1.
- [x] Portable launcher fixtures verify app failure despite successful `open`,
  failed `open`, retained failure logs and cleanup. These run in the ordinary
  script suite on Linux as well as macOS.
- [x] No visible UI change: this changes the debug capture tool itself;
  before/after screenshots are not required.

The required workspace, quality, interfaces, audit and full pre-push results
are recorded in the PR body. No runtime dependencies or contracts change.

See [capture instructions](../../../AGENTS.md#ui-change-evidence).
