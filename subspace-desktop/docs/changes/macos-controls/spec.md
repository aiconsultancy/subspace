# Spec: macOS controls

## Sizing model

GPUI sets the window's `rem` from the theme's base `font_size`; Subspace sets
that to 14 px × (body text ÷ 13 px), so the default body text gives a 14 px
rem. GPUI Kit's `text_xs`, `text_sm`, `h_6`, `px_2` and similar helpers are
fractions of that rem. Before this change, button text came from GPUI Kit's
size tiers (`xsmall` gave `text_xs`, 10.5 px at rem 14; `small` gave
`text_sm`, 12.25 px), list rows and tabs used absolute pixel tokens, and
radii came from GPUI Kit's `radius` (6 px, 8 px in Cupertino) while the
segmented control used `--radius`. Buttons at the same level therefore had
different text sizes and corners.

After this change every shared button takes four tokens from its **role**:
height, horizontal padding, text size and corner radius. The tokens are
pixels at the reference rem, 14 px (`density::REFERENCE_REM`), and resolve as
`rem(value ÷ 14)`, so they read as pixels in the theme file and designer but
still scale with the body text size. The global icon-button size multiplies
Toolbar, Action and Dialog heights, glyphs, gaps and padding; it never changes
text. Compact is a fixed preset. A radius larger than half the height draws a
capsule. Vertical padding is not a token: the label is centred in the height.

| Role | Constructors | Token prefix | Default |
| --- | --- | --- | --- |
| Toolbar | `IconButton::new`, `action::icon_button`, `toolbar_button`, `menu_button`, shell chrome | `--toolbar-button-` | 21 h, 7 pad, 12.25 text, 6 r |
| Action | `action::action_button`, `CopyDetails` | `--action-button-` | 21 h, 7 pad, 10.5 text, 6 r |
| Dialog | `action::dialog_button`, `.role(ButtonRole::Dialog)` | `--dialog-button-` | 21 h, 7 pad, 12.25 text, 6 r |
| Compact | `.compact()` | `--compact-button-` | 17.5 h, 3.5 pad, 10.5 text, 6 r |

The defaults are the geometry the majority of each role had before, so themes
without the tokens look the same. The minority sites that now match their role
are listed in [delivery](delivery.md#intentional-changes-in-every-theme).

Other tokens in the `controls` group: `--dialog-pad` and `--dialog-gap`
(dashboard dialog frame, default 10.5 and 7, the old `p_3` and `gap_2`),
`--nav-h` (least board-tab height, default 28) and `--liquid-glass` (0 or 1).

## Liquid Glass

With `--liquid-glass` set to 1:

- Toolbar-role buttons that are quiet (ghost) or outlined and carry a glyph
  are drawn by `glass::control`, the same module and tones (`glass`,
  `glass-rim`, `glass-shadow`) as the board-tab capsules: a resting
  translucent fill (70 % of `glass`) with an inset rim stronger at the top,
  the full `glass` tone under the pointer, a `glass-shadow` shade while
  pressed. A latched or selected button is the lit material of the current
  tab. Geometry, focus ring and accessibility stay native.
- Destructive (Danger) buttons are outlined in red instead of filled: macOS
  draws destructive buttons at secondary tint prominence.
- Primary buttons keep the accent fill (the default button), and success,
  warning and neutral buttons keep their native treatment.

GPUI cannot sample or blur content behind an element, so there is no
refraction or backdrop blur. The glass is translucency over the bar, as for
the existing tabs. macOS 27 adds a uniform frosted toolbar behind toolbar
buttons; Subspace's title bar already is one.

## macOS values

Cupertino Light, Cupertino Dark and Subspace One Dark set:

| Token group | Value | macOS reference |
| --- | --- | --- |
| Toolbar | 24 h, 10 pad, 13 text, 12 r (capsule), glass | Toolbar items: medium (24 pt) controls on Liquid Glass |
| Action | 20 h, 8 pad, 11 text, 6 r | Small push button (20 pt, 11 pt text), rounded rectangle |
| Dialog | 24 h, 12 pad, 13 text, 7 r | Medium (regular) push button (24 pt, 13 pt text), rounded rectangle |
| Compact | 16 h, 4 pad, 10 text, 5 r | Mini control (16 pt); text kept at the 10 pt macOS minimum |
| Dialog frame | 20 pad, 12 gap | Sheet and alert margins |
| `--liquid-glass` | 1 | Liquid Glass toolbar and tinted destructive buttons |

Cupertino's body text changes from 14 px to 13 px, the macOS default text
size, which also makes its rem exactly 14 px so the control tokens resolve to
whole pixels. Headings stay 15 px (macOS Title 3). Subspace One Dark keeps its
palette and fonts; only the controls change. All GPUI Kit themes keep the
defaults.

GPUI Kit points map to window pixels one to one at 1× (macOS points).

## Sources

- Apple, Human Interface Guidelines, *Buttons* (updated for Liquid Glass,
  16 December 2025): roles (primary uses the accent colour, destructive uses
  red), push buttons, toolbar items instead of image or square buttons in
  the window frame.
  <https://developer.apple.com/design/human-interface-guidelines/buttons>
- Apple, Human Interface Guidelines, *Typography*: macOS default text 13 pt,
  minimum 10 pt.
  <https://developer.apple.com/design/human-interface-guidelines/typography>
- Apple, WWDC25 session 310, *Build an AppKit app with the new design*: mini
  to extra large control sizes, mini–medium taller than before; mini to
  medium are rounded rectangles, large and extra large are capsules; toolbar
  items sit on glass; the glass bezel style; tint prominence, with
  destructive buttons at secondary prominence and the default button at
  primary. <https://developer.apple.com/videos/play/wwdc2025/310/>
- Apple's macOS 26 Figma kit, as measured by Self-Made DesignOps (secondary
  source): control heights 16, 20, 24, 28 and 36 pt; toolbar buttons 24 pt.
  <https://self-made-orange.github.io/self-made-design-ops/systems/macos.html>
- WWDC26 coverage of macOS 27 (secondary): consistent corner radii, a
  uniform frosted toolbar and a user slider from clear to tinted glass; no
  change to control sizes was announced.
  <https://www.cultofmac.com/news/liquid-glass-changes-ios-27-macos-27>

Apple does not publish exact corner radii for push buttons; 6 and 7 px are
estimates from the rounded-rectangle shapes, kept below half the height.

## Live preview

The designer's density sliders already wrote the `Density` global; edits now
go through `Jig::set_density`, which refreshes every window, so the
Component gallery (or any board) re-renders with the edit on the next frame.
Cancel restores the snapshot taken when the designer opened. Save persists as
before.

## Theme designer

The designer stays a 520 px panel at the window's right edge, so the board
beside it, such as the Component gallery, shows each edit at once. Its body
is a native GPUI Kit `Accordion` (several sections may be open; only open
sections are built), with the existing footer: **Reset all**, **Copy JSON**,
**Cancel** and **Save**.

| Section | Controls | What reads them |
| --- | --- | --- |
| Theme | Base theme, theme name | Theme catalogue, Save |
| Typography | Heading, body and code families; `--heading-font`, `--row-font`, `--mono-font`; group heading family, size, bold, italic | `Typography`, `Tokens`, GPUI Kit `font_size` (the rem) |
| Spacing and density | Rows (`--row-h`, `--pad-x`, `--indent`, `--row-inset`), Boards (`--lane-w`, `--done-w`, `--head-h`, `--tree-w`), Window (`--chrome-h`, `--toolbar-h`, `--panel-w`, `--motion`), Shape (`--radius`, `--surface-radius`, `--nav-round`) | `Tokens` |
| Controls | Toolbar, Action, Dialog and Compact buttons (height, horizontal padding, text size, corner radius); `--dialog-pad`, `--dialog-gap`, `--nav-h`; **Liquid glass**; GPUI Kit `radius` (fields and menus) and `radius.lg` (large surfaces) | `Tokens::controls`, GPUI Kit theme |
| Colours | Surfaces, Lines, Text, Actions and selection, Status, Glass (24 keys), then contrast checks | `Tokens`, GPUI Kit theme, `glass` tones |
| Preview and contrast | Font samples and all contrast checks | — |

Each section but Theme and Preview has **Reset to theme default**, which
returns that section's density tokens, colours, radii or typography to the
base theme (the saved theme after Save) as a live preview. A colour or radius
that the base theme leaves derived is written to the theme only when it
changes. Save writes the custom theme file as before, with the new tokens in
`subspace.density`, colours in `colors` or `subspace.tones` and the GPUI Kit
radii at the variant's top level.

### Tokens that exist but are not in the designer

- Caption and secondary text sizes are fixed ratios of the body size
  (`Tokens::caption_size`, `secondary_size`), not tokens.
- Line height, font weight and letter spacing have no tokens; GPUI Kit's
  defaults apply.
- Input heights, padding and text follow GPUI Kit's `Size`; only their radius
  (GPUI Kit `radius`) is themeable. The focus ring has only its colour
  (`ring`).
- The colour tones `accent`, `red`, `green`, `yellow`, `blue`, `cyan`,
  `magenta`, `orange` and `media-stage` override GPUI Kit colours for the
  dashboard tokens; the designer edits the GPUI Kit colours instead.
- The vertical rail's 32 px item height is a constant, not `--nav-h`.
