# Features and experiments

Settings lists app pages first, then the boards included in the product.
Disabled boards remain configurable. Their main tabs and agent tools disappear
when you save. A board's tab menu (right-click) has **Close Space**, which
saves and applies the same `false` or removed `boards` entry as turning off
its Enabled switch and choosing Save, and **Space settings…**, which opens
its page. The last open board can't be closed. The plus after the last tab,
**Add a Space**, lists the boards that are off; choosing one saves and
applies the same choice as turning on its Enabled switch and choosing Save,
then opens it.

A board omitted from `config.json` uses the product default for the current
build. It is not treated as enabled merely because the key is absent. Release
builds of Subspace default Channels and Issues to disabled. Debug
builds default those boards to enabled. GitHub defaults to enabled. Copilot Agents defaults to disabled.
An explicit `true` or `false` in `boards` overrides that default. Settings
writes `true` when you enable a default-off board.

For example, the Subspace App's Copilot Agents board defaults to disabled in
every build, and its Enabled setting is its only gate. The Actions board
defaults to enabled; its Flow view has no separate feature flag.

A disabled board's feature choices remain saved, but its features resolve to
off. Re-enabling the board restores those choices. App features are independent
of board enablement.

The **Experiments** page lists experimental features grouped by owner. Each
board also shows its experimental features below its ordinary settings. Both
locations edit the same draft choice. **Save** writes the choices, **Cancel**
discards them, and a restart applies them.

Experimental features are off by default. They can change or be removed.
They do not grant extra service permissions or bypass tool approvals.

## Pi chat

Enable **Pi chat** in **Settings > Experiments**, then select
**Pi (Experimental)** as the chat engine in **Settings > Chat**. Save and restart.

Turning Pi off while it is selected changes the draft engine to Off and
shows an explanation. Cancel leaves the saved engine unchanged. Turning Pi on
does not select it automatically. A manually edited configuration that selects
Pi without its opt-in cannot start Pi and reports how to correct the settings.

```json
{
  "chat": { "engine": "pi" },
  "features": { "app.pi-chat": true }
}
```

Pi's existing restrictions still apply. See [Pi chat](pi-chat.md).

## ACP agent chat

Enable **ACP agent chat** (`app.acp-chat`) in **Settings > Experiments**, then
select **ACP agent (Experimental)** as the chat engine and an **Agent** in
**Settings > Chat**. Save and restart. The Agent choice appears only while ACP
is the selected engine.

Turning the experiment off while ACP is selected changes the draft engine to
Off and shows an explanation, as for Pi. A configuration that selects ACP
without the opt-in cannot start an agent.

```json
{
  "chat": { "engine": "acp", "acp": { "agent": "opencode" } },
  "features": { "app.acp-chat": true }
}
```

The agent keeps its own tools and permission rules. See
[ACP agent chat](acp-chat.md).

## Register a board feature

The shared `Dashboard` trait advertises static settings metadata through its
`settings` method. The app reads the metadata from product registrations, not
running board entities. A disabled board can therefore expose its settings
without starting network requests or loading credentials.

Define a feature in the board crate. Use a stable namespaced ID,
`board.<board-id>.<feature-id>`:

```rust
use subspace_core::features::{FeatureSpec, FeatureStage};

const PREVIEW: FeatureSpec = FeatureSpec {
    id: "board.example.preview",
    title: "Preview view",
    description: "Use the board's new preview view. Save and restart to apply.",
    stage: FeatureStage::Experimental,
    default_enabled: false,
    toggleable: true,
};
```

Return the feature from the board's settings declaration. For example, inside
the board's existing `impl Dashboard`:

```rust
fn settings() -> subspace_dashboard::settings::BoardSettings {
    subspace_dashboard::settings::BoardSettings {
        features: &[PREVIEW],
        ..Default::default()
    }
}
```

Check the registered feature before starting its behavior:

```rust
if config.feature_enabled(&PREVIEW) {
    // Start the explicitly enabled feature.
}
```

Registration provides the controls, not the behavior guard. The board must
check the flag at the behavior's entry point, including worker and tool paths
that do not render UI. Saved unknown keys cannot register a feature or create
behavior by themselves.

App features use `app.<feature-id>` IDs. IDs must be unique and board IDs must
match the declaring owner. IDs are at most 128 ASCII bytes. Use nonempty
dot-separated segments containing only ASCII letters, digits or hyphens.
Labels and descriptions are public metadata. Never
put tokens, service responses or user-specific data in declarations.

## Nested settings

`BoardSettings.sections` accepts shared `SettingsSection` declarations.
Sections have an ID, title, icon, child sections and feature declarations.
The settings tree builds its nodes from these declarations, so a new section
does not need a navigation case in the app. Registered feature toggles use the
shared renderer. Existing connection forms keep their private credential
editors.

Keep section IDs stable and unique within their parent. Put ordinary controls
before experimental controls. Do not initialize the board or make service calls
while returning settings metadata.

For example, a board can place its declaration under a nested page:

```rust
use subspace_dashboard::settings::{BoardSettings, SettingsSection};
use subspace_lucide::IconName;

const SECTIONS: &[SettingsSection] = &[SettingsSection {
    id: "preview",
    title: "Preview",
    icon: IconName::Bot,
    children: &[],
    features: &[PREVIEW],
}];

fn settings() -> BoardSettings {
    BoardSettings {
        sections: SECTIONS,
        ..Default::default()
    }
}
```

The feature still appears on the board overview below ordinary settings and on
the global Experiments page. The nested page adds a third view of the same
choice, not another preference. Board navigation uses the board's existing
icon unless its settings metadata declares an override.

## Graduate or remove a feature

To graduate a feature, keep its ID and change its stage to `Stable`. A
toggleable stable feature moves to regular settings and keeps the saved choice.
Its declared default applies only when no saved choice exists. A stable feature
that is no longer toggleable uses its declared behavior instead of a stale
toggle value.

To retire a feature, change its stage to `Removed` while removing its behavior,
or remove the declaration and its guarded implementation together. Removed
declarations do not appear in settings and always resolve to off, even if their
saved key is true. Do not reuse an old ID for a different feature.

Unknown valid flags survive unrelated settings saves for forward compatibility.
They have no effect without a registered implementation. Do not delete unknown
preferences merely to make the current list look clean.
