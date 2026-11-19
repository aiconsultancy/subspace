# Spec: glass capsule board tabs and title-bar spacing

## Title bar (macOS)

- AppKit's window buttons on macOS 26 and 27 are 14 pt squares with 9 pt
  between them. This was measured with `NSWindow::standardWindowButton` on
  macOS 27.0.1. Subspace places the close button 14 pt from the leading edge,
  so the zoom button ends at 74 pt.
- Title-bar content starts 20 pt after the zoom button, at 94 pt from the
  window edge. This is the wide end of the unified-toolbar spacing that Finder
  and Xcode use. It replaces a 76 pt reserve plus the bar's own `--pad-x`, so
  the inset no longer depends on `--pad-x`. In full screen the lights are
  hidden and the reserve is zero.
- The first row's height is the largest of the theme's `--chrome-h`, a tab
  capsule plus 5 px above and below, and the global toolbar's row height. With
  the default fonts this is 38 px in every theme. Both navigation modes use
  the same row.
- The traffic lights sit on the row's centre line. Subspace sets their
  position from the laid-out row height and moves them again only when that
  height changes, such as after a theme, density or zoom change.
- The logo, the tabs, the board's controls and the global actions share that
  centre line.

## Navigation capsules (`subspace_dashboard::glass`)

- The horizontal board tabs and the vertical rail's items use the same shared
  control, `glass::nav_button`.
- **Shape:** a capsule whose radius is half the item's height times the new
  `--nav-round` density token (a fraction from 0 to 1; default 1, a full
  capsule). The radius stays a full capsule at every font size and density.
  Horizontal tabs are 28 px tall, or more for larger heading fonts. Rail items
  are 32 px tall, and a collapsed rail item is a circle.
- **Current board:** a glass capsule with:
  - a translucent fill from the `glass` tone, brighter at the top
    (a gradient from 1.5× to 0.85× of the tone's alpha);
  - a 1 px hairline edge from `glass-rim` at 45% strength;
  - an inset specular rim, `glass-rim` at full strength along the top inner
    edge and 30% along the bottom;
  - a soft lifting shadow from `glass-shadow`: 1 px down with a 3 px blur.
- **Other boards:** no fill. Hovering shows a faint glass fill (55% of the
  tone's alpha), and pressing shows a stronger one (80%). Hover reinforces
  the capsule; it is not the only affordance, since every tab keeps its glyph
  and label.
- **Labels:** the current item uses `fg_strong`, the others `fg`. The glyph
  matches the label. Badges keep one accent treatment.
- **Accessibility:** each item keeps its accessible label and tooltip. The
  current item reports itself as pressed (toggled), like a segmented choice.
  GPUI Kit's `Button` cannot expose `aria-selected`.
- **Focus:** every capsule has a 1 px hairline, which is transparent unless
  the item is current. GPUI Kit tints that border with the focus-ring colour,
  so the focus indicator follows the capsule.
- **Unchanged rules:** while Logs or Search replaces the board, no item is
  current (`SubspaceApp::shows_current`). The tabs keep their overflow
  chevron. Selection is a fill, never a bar or an underline.
- Each horizontal tab sits in `glass::bleed`, which leaves 4 px above and
  below and 3 px at each side. The overflow toolbar clips its row, and this
  room keeps the shadow from being cut off.

## Theme tones

| Theme | `glass` | `glass-rim` | `glass-shadow` |
| --- | --- | --- | --- |
| Subspace One Dark and other dark themes | `#ffffff1a` | `#ffffff40` | `#00000080` |
| Cupertino Light | `#ffffffc7` | `#ffffff` | `#00000033` |
| Cupertino Dark | `#ffffff1f` | `#ffffff4d` | `#00000099` |
| Other light themes | `#ffffffb8` | `#ffffff` | `#0000002e` |

- Themes without these tones get the dark or light set from their mode.
  `one-dark.json` states the dark set explicitly, and a test checks that it
  matches `Tokens::one_dark()`.
- The `switcher` tone and the `nav_select` and `nav_select_fg` tokens are
  removed. They only coloured the old square tabs and the rail's accent fill.
- In each Subspace theme, a test checks two contrast ratios. The glass must be
  visible but quiet on its title bar, between 1.05:1 and 2:1. The label must
  reach at least 4.5:1 on the glass.

## What is real and what is not

| Liquid Glass trait | Here |
| --- | --- |
| Capsule shape | Yes: GPUI rounded corners at half the height |
| Translucency | Yes: an alpha fill over the title bar or sidebar |
| Specular rim, stronger at the top | Yes: inset shadows and a hairline |
| Lift | Yes: a soft drop shadow |
| Light gathered at the top | Yes: a vertical gradient |
| Backdrop blur, refraction, lensing | No |

GPUI cannot sample and blur the content behind an element inside a window.
Subspace does not use `WindowBackgroundAppearance::Blurred`, and this change
does not turn on window-level blur. The title bar and sidebar are opaque, so
blur would show nothing there anyway.
