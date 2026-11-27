# AgentsView Space

## Intent

Open the selected fleet's AgentsView session browser inside Subspace. A
connection provisioned by the installer or saved in Settings opens directly
into AgentsView.

## Specification

- Standard macOS and Windows builds include AgentsView and its Settings
  panel. The default Cargo feature `agentsview` can be omitted from a
  custom build. The Space is enabled by default when it is included. The
  pre-push desktop checks exercise both Cargo configurations, and release
  metadata lists the Space for macOS and Windows archives.
- **Settings › Spaces › AgentsView** edits one fleet's address and masked
  API key. The pair lives in the desktop's owner-only board configuration.
  Changing the address requires clearing or replacing an existing key.
- The desktop installer accepts `--agentsview-url` and
  `--agentsview-key-file` on the `desktop` command. It writes the pair
  together without putting the key on the command line. Rerunning setup for
  the same address keeps the saved key. `--agentsview-clear-key` removes it.
- Fleet setup recommends installing AgentsView and asks the operator to
  choose. If the operator opts out, the host installer hides the Space on
  the desktop's next launch.
- The isolated web view applies the key to the configured server's API
  requests at document start. The Space shows the page after the upstream
  settings request succeeds. If the fleet rejects the key, the Space closes
  the page and directs the user to Settings. The key stays out of the view's URLs and browser storage.
- The embedded page adopts Twilight Two when Subspace selects it. Changing
  the Subspace theme updates the current page without reloading the session.
  AgentsView's Light mode, high contrast and custom message colours still
  apply. The standalone page keeps its own appearance.

## Delivery and tasks

- [x] Wire the build flag, settings, Space and installer.
- [x] Check the token boundary and configuration with focused tests.
- [x] Verify the authenticated and rejected native views against the fleet's
  pinned AgentsView release using local fixture data.
- [x] Check the embedded Twilight Two palette against a live, authenticated
  AgentsView instance and cover its appearance overrides with focused tests.

### Board checklist

- [x] Identity, registration and icon use the product descriptor and shared
  bot glyph. Disabled Spaces have no native process to retain.
- [x] The shared workspace pane holds the upstream page. The page owns its
  layout, search, filters and detail. Subspace reports connection state in
  the status bar and provides shared Configure, Reload and Retry actions.
  Docks, list hooks, extra menus, modal state and quit warnings are not
  applicable to the embedding.
- [x] One board-owned Settings editor with the shell's Save and Cancel.
  Notifications and background updates are not applicable to this Space.
- [x] The owner-only board file keeps the connection across restarts.
  Session data stays with the fleet, so Subspace's cache, refresh settings
  and indexing do not apply.
- [x] Agent tools, chat attachments, notifications and board contracts are
  not applicable to the embedded view. It observes only shared settings.
- [x] [User guide](../../../spaces/web/agentsview-help.md) registered in the
  app and linked from the product's board guide list.
- [x] Navigation stays within the configured site. Connection failures
  display fixed messages without including server responses or credentials.
- [x] Check headless coverage and capture fixture screenshots.

## Verification

The macOS capture build opened AgentsView 0.47.0 from a private loopback
fixture under a URL prefix. It displayed a fictional session after the
installer provisioned its address and key. With a rejected key, the Space
closed the web view and showed Configure. Replacing the key in Settings and
saving opened the session browser in the same launch.

| State | Capture |
| --- | --- |
| Switcher on the base revision | [Before](evidence/before.png) |
| No connection configured | [Setup](evidence/setup.png) |
| Authenticated fleet | [Connected](evidence/connected.png) |
| Narrow window | [Narrow](evidence/narrow.png) |
| Key rejected | [API key required](evidence/key-required.png) |
| Masked replacement | [Settings](evidence/settings.png) |
| Fleet opts out at install time | [Space hidden](evidence/disabled.png) |
| Twilight Two shell with the default AgentsView palette | [Before theme matching](evidence/twilight-before.png) |
| Twilight Two shell and embedded page | [After theme matching](evidence/twilight-after.png) |

Windows PowerShell parsing and installer command regression checks run on
macOS. Windows WebView2 and Windows file ACL behaviour need a Windows run.
