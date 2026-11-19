# Spec: flat controls

## Theme tokens

Two choice tokens in a theme's `subspace.density` block choose how quiet
glyph toolbar buttons look. Each is `flat` (the default) or `glass`:

| Token | Chooses |
| --- | --- |
| `--toolbar-button-rest` | The resting finish |
| `--toolbar-button-hover` | Hover, pressed and latched |

No bundled theme sets them, so every bundled theme is flat. `Density` stores
a choice token by value name and holds it as the index, so a stored or theme
value other than `flat` or `glass` is rejected like any invalid density value.

Two colour tones, `button-hover` and `button-on`, set the flat fills. Without
them, `button-hover` is `#ffffff17` on dark themes and `#00000012` on light
ones, and `button-on` is the theme's accent at 24% (dark) or 16% (light).

## Which buttons

`IconButton::surface(&Controls)` decides:

| Surface | Buttons |
| --- | --- |
| `Quiet(look)` | Quiet (ghost) Toolbar-role buttons with a glyph, labelled or not: the global, board, panel, overflow and chat toolbars |
| `Quiet(ToolbarLook::FLAT)` | Buttons built with `.flat()`: every status bar command, in every theme |
| `Glass` | Outlined Toolbar buttons with a glyph, such as menu pickers, when `--liquid-glass` is on (unchanged) |
| `Tinted` | Danger buttons when `--liquid-glass` is on (unchanged) |
| `Native` | Everything else |

`--liquid-glass` no longer puts quiet buttons on glass. Board tabs and the
navigation rail keep their glass capsules: they were already bare at rest.

## States

| State | `flat` | `glass` |
| --- | --- | --- |
| Rest | No fill; a transparent hairline keeps the size and the focus ring's edge | The glass capsule and rim |
| Hover | `button-hover` fill; glyph and text turn to the strong text colour | `glass` fill |
| Pressed | `button-hover` at 1.6× its alpha | `glass-shadow` shade |
| Latched | `button-on` fill, 1.3× under the pointer and 1.6× pressed; the glyph and text are the accent mixed halfway with the strong text colour | The lit glass of the current board tab |

A latched flat button does not use GPUI Kit's selected style, which ignores
the pointer. It keeps its accessible pressed state (`toggled`). The keyboard
focus ring is GPUI Kit's, drawn along the button's radius. `icon_button::paint`
resolves a look and latched state to colours.

## Status bar

About (the version), Report issue, Commands and Help are all
`action_button(Quiet).compact().flat()`, built by `status_bar::commands`.
Report issue is no longer a toolbar-size button with its own constructor. The
chat state is text, not a button.

## Theme designer

Controls > Control style shows **Toolbar buttons at rest** and **Toolbar
buttons on hover**, each a shared segmented **Flat**/**Glass** choice that
previews live. Colours gains **Flat buttons**: **Button hover** and
**Button on**. Save writes them to the theme file; Reset returns them to the
base theme's values.

## Contrast

In every Subspace theme, on the title bar and on the page, the strong text
on the hover and pressed fills, and the latched glyph on the latched fills,
are at least 4.5:1.
