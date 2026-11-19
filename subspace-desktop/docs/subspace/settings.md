# Subspace settings

Open **Settings** from the command palette or use Command+, on macOS and
Control+, on Windows or Linux.

To mute every notification sound, open **Application > Notifications**, turn
off **Play notification sounds** and choose **Save**. Individual Space sound
choices are kept. **Preview sound** still plays explicitly; **Test notification**
follows the saved master choice.

Use the shared [Settings and appearance guide](../settings.md) for service
connections, credential priority, enabled Spaces, Space order, automatic
refresh, terminal placement, themes and fonts. The
[appearance guide](https://github.com/aiconsultancy/subspace/blob/main/subspace-framework/docs/appearance.md) covers additional appearance controls.

To use GitHub without the GitHub CLI or a token, open a GitHub Space's page,
such as **Spaces > PRs**, and choose **Sign in with GitHub**. Sign-in and
**Sign out** apply at once; see
[Sign in with GitHub](../settings.md#sign-in-with-github).

Saved service and credential changes require an app restart. Appearance
preferences and the documented immediate settings apply on Save. Cancel
discards the draft. Keep credentials in the documented private config or
environment, not in UI state, screenshots or shared notes.

For updates, open **About > Check for updates**. The
[installation reference](../installation.md#update-an-installed-app) explains
verification, authentication and channel selection.

## About

**About** shows the theme-aware product logo, product name, full build version
and build channel. Updates stay together below the identity. The repository
link and local diagnostics help with support. Product credits appear unchanged
in a muted section at the bottom; products that supply no credits omit it.

Subspace supplies no credits, so its About has no credits section. A product
lists its credits in `ProductDescriptor::credits`.
