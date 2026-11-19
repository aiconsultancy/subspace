# Delivery: macOS controls

## Checks

- `make check`, `make interfaces` and `mise run quality` (no new findings).
- Dashboard tests: `icon_button::tests::roles` renders every role at the
  reference rem with the default tokens (21, 21, 21, 21 and 17.5 px frames;
  12.25 and 10.5 px text) and with the macOS tokens (24, 20, 20, 24 and
  16 px; 13, 11 and 10 px text); `Geometry::resolve` keeps the nominal scale,
  doubles Large and caps the radius at half the height; only quiet or
  outlined Toolbar buttons with a glyph become glass and Danger is tinted
  only with Liquid Glass.
- Host tests: `theme::tests::macos_themes_set_liquid_glass_controls_and_others_keep_the_defaults`
  reads the bundled files; `jig::tests::a_control_token_edit_previews_in_other_views_until_cancel`
  drives a designer slider and measures a Dialog button in another window:
  21 px, then 30 px with no Save, then 21 px after Cancel.
- Toolbar, rail and Help layout tests clear the theme's control tokens
  (`preview_tests::nominal_controls`) because they measure the global
  icon-button scale, not one theme.

## Screenshots

Captured from the capture build (`cargo build -p subspace --features
subspace-shell/capture`) in the background with an isolated `HOME`, fixture
state (Component gallery selected, update checks off) and no credentials.
Before is `cacbe80`; after is this change. Each image is 1600 px wide.

### Cupertino Dark

![Before: Cupertino Dark Component gallery with flat toolbar icons and 10.5 px action text](evidence/cupertino-dark-gallery-before.png)
![After: Cupertino Dark gallery with glass toolbar capsules, 20 px actions and a tinted Reject](evidence/cupertino-dark-gallery-after.png)
![Before: Cupertino Dark Settings with a ghost Cancel](evidence/cupertino-dark-settings-before.png)
![After: Cupertino Dark Settings with 24 px regular Cancel and Save](evidence/cupertino-dark-settings-after.png)

### Cupertino Light

![Before: Cupertino Light gallery](evidence/cupertino-light-gallery-before.png)
![After: Cupertino Light gallery with light glass toolbar capsules](evidence/cupertino-light-gallery-after.png)
![Before: Cupertino Light Settings](evidence/cupertino-light-settings-before.png)
![After: Cupertino Light Settings with regular footer buttons](evidence/cupertino-light-settings-after.png)

### Subspace One Dark

![Before: Subspace One Dark gallery](evidence/subspace-one-dark-gallery-before.png)
![After: Subspace One Dark gallery with glass toolbar capsules in its own palette](evidence/subspace-one-dark-gallery-after.png)
![Before: Subspace One Dark Settings](evidence/subspace-one-dark-settings-before.png)
![After: Subspace One Dark Settings with regular footer buttons](evidence/subspace-one-dark-settings-after.png)

### A theme without the tokens: Solarized

Solarized keeps its look. The only differences are the intentional
consistency fixes below: the text-only **No icon** action is now as tall as
the other actions, and the Settings Cancel is outlined like every other
dialog's Cancel.

![Before: Solarized Dark gallery](evidence/solarized-dark-gallery-before.png)
![After: Solarized Dark gallery, unchanged except the No icon action height](evidence/solarized-dark-gallery-after.png)
![Before: Solarized Dark Settings](evidence/solarized-dark-settings-before.png)
![After: Solarized Dark Settings with an outlined Cancel](evidence/solarized-dark-settings-after.png)
![Before: Solarized Light gallery](evidence/solarized-light-gallery-before.png)
![After: Solarized Light gallery](evidence/solarized-light-gallery-after.png)
![Before: Solarized Light Settings](evidence/solarized-light-settings-before.png)
![After: Solarized Light Settings](evidence/solarized-light-settings-after.png)

## Theme designer

Tests: `jig::sections::tests` checks that every density token belongs to
exactly one section; `jig::colors::tests` that every colour field is a theme
colour with a current value; `jig::tests::each_section_previews_live_saves_and_resets`
edits the body size and row height, turns on Liquid glass through its switch,
sets the GPUI Kit radius and a derived title-bar colour, checks each one live
without Save, saves and reads them back from the file, then changes them again
and resets the Spacing, Controls and Colours sections to the saved values. The
existing designer tests open the Typography section by its disclosure.

Live preview in Cupertino Dark: the Toolbar button height raised from 24 to
30.5 px resizes the gallery's toolbar buttons, the latched pair and the title
bar's controls before Save; Close (Cancel) restores them.

![Before the edit: Controls open, Toolbar height 24 px](evidence/live-1-before-edit.png)
![After clicking the Height slider: 30.5 px toolbar buttons in the gallery and title bar](evidence/live-2-toolbar-height-30.png)
![After closing the designer without saving: back to 24 px](evidence/live-3-cancelled.png)

Each section in Cupertino Dark:

![Theme section](evidence/designer-1-theme.png)
![Typography: font families](evidence/designer-2-typography.png)
![Typography: sizes and group headings](evidence/designer-2-typography-more.png)
![Spacing and density: rows and boards](evidence/designer-3-spacing.png)
![Spacing and density: window and shape](evidence/designer-3-spacing-more.png)
![Controls: toolbar and action buttons](evidence/designer-4-controls.png)
![Controls: dialog buttons](evidence/designer-4-controls-more.png)
![Controls: compact buttons](evidence/designer-4-controls-more2.png)
![Controls: dialogs and tabs, Liquid glass and GPUI Kit radii](evidence/designer-4-controls-end.png)
![Colours: surfaces and lines](evidence/designer-5-colours.png)
![Colours: text, actions and status](evidence/designer-5-colours-more.png)
![Colours: glass and contrast checks](evidence/designer-5-colours-end.png)
![Preview and contrast](evidence/designer-6-preview.png)

## Intentional changes in every theme

Each was the minority treatment of its role:

- Text-only action buttons were 17.5 px GPUI Kit `xsmall` buttons; icon
  actions were 21 px. All Action buttons are now one height.
- `toolbar_button` and `menu_button` labels used 10.5 px text beside 12.25 px
  shell toolbar labels; they are Toolbar buttons now.
- Dialog footers used three treatments: Settings (21 px ghost Cancel,
  12.25 px), board dialogs (21 px, 10.5 px) and the quit dialog and theme
  designer (28 px GPUI Kit medium). All use `dialog_button` now, with an
  outlined Cancel and a glyph.
- The status bar's chat mark and text follow the Compact role, like the
  commands beside it, and **⌘K Commands** is Compact too.

## Follow-ups

- Migrate the Terminal paste confirmation and Terminal board dialog to
  `dialog_button` (separate Zig workspaces).
- macOS groups adjacent toolbar buttons on one piece of glass; Subspace draws
  one capsule per button.
- Input heights, padding and text follow GPUI Kit sizes; only their radius is
  themeable (GPUI Kit `radius`).
