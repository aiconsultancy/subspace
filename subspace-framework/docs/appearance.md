# Appearance

**Cupertino Dark** is the framework default for Subspace and products that
do not declare another default. **Subspace One Dark**, **Cupertino Light**,
[Subspace Channel](#subspace-channel) Light and Dark, and
[macOS Classic Dark Remix](#macos-classic-dark-remix) remain available.
Existing saved theme choices still win when available. If a saved theme has
been removed, the default is used. Subspace does not switch
themes with the system appearance; choose **Cupertino Light** for a light
window.
Select a theme in **Settings > Appearance**, or open **View > Theme designer**.
The backtick shortcut and the eye toolbar button also open the designer.

Three independent font choices control headings and tabs, body text, and code
and logs. Automatic choices use the selected theme's defaults and installed
fonts. The app does not download fonts.

The theme designer also has a separate **Group headings** section for titles
that fold or unfold dashboard row groups. It controls the installed font
family, size, bold and italic style. These choices do not style the group's
chevron, icons, counters or body rows.

If a requested family is absent, the application reports the missing role and
uses an installed fallback. Old single-font preferences still apply until you
save independent font choices.

In **Settings > Appearance**, each font role has a size field with a pixel
value and minus/plus buttons. Sizes change in 0.5 px steps. Headings accept 10
to 24 px, body text accepts 10 to 18 px, and code and logs accept 10 to 20 px.
**Tabs**, the board tabs in the toolbar, have their own size, 10 to 24 px
(15 px by default), and use the headings font. These are reference sizes;
existing heading levels and smaller text retain their relative sizes.

Changes preview in the live window. **Save** keeps the sizes for the next launch
without a restart. **Cancel**, Esc or closing Settings restores the previous
appearance. Sizes also appear under Typography in the theme designer and are
included in its exported theme JSON.

## Cupertino

**Cupertino Light** and **Cupertino Dark** give Subspace a native macOS look.
Light mode uses silver and warm grey materials; dark mode uses graphite and
warm black. System blue marks only primary actions, selection and focus.

The theme also sets sizes, not only colours. It uses the system font at 13 px
for body text, as macOS does, Menlo for code, a 38 px top bar, 32 px rows on a
4 px grid, 6 px control and 12 px surface corners, and inset rounded row
selection without separators. Its sidebar and top bar share one material, and
dialogs, menus and side panels use the raised surface colour. It pairs well
with the vertical board navigation rail in **Settings > Application >
Layout**.

Cupertino Light, Cupertino Dark and Subspace One Dark draw buttons in the
macOS 26 style (see [Buttons](#buttons)): menu pickers are glass capsules,
board actions are small push buttons and dialog buttons are regular push
buttons. Their glyph toolbar buttons are flat, like every theme's.

## Subspace Channel

**Subspace Channel Dark** sets a blue-black canvas, title bar and sidebar
against warm graphite lists, tabs and status bar, with system blue for
selection and focus and Xcode's dark syntax colours. It uses the Cupertino
sizes, 24 px board action buttons, and JetBrainsMono Nerd Font Mono for code
when it is installed.

**Subspace Channel Light** is its counterpart: a white canvas with cool
silver chrome, warm light grey lists and status bar, and Xcode's light syntax
colours. Its text, status and link colours meet WCAG AA contrast on white.

## macOS Classic Dark Remix

**macOS Classic Dark Remix** is huacnlee's
[macOS Classic](https://github.com/huacnlee/zed-theme-macos-classic) theme
for Zed (MIT; notice in `licenses/themes/`), remixed with Subspace sizes:
graphite surfaces, system blue (`#077cfd`) for primary actions and focus,
18 px headings, 30 px rows, 24 px buttons and 14 px surface corners. It has
no light variant. Three values differ from the remix as supplied, so it
passes the bundled-theme accessibility checks: the off switch track is
`#808080`, strong text (`caret`) matches body text (`#dedede`), and it uses
Cupertino Dark's glass tones.

## Buttons

A theme sets each kind of button's height, horizontal padding, text size and
corner radius, so buttons of one kind match wherever they appear. The kinds
are:

| Kind | Where | Default (all other themes) | macOS themes |
| --- | --- | --- | --- |
| Toolbar | Title bar, board and panel toolbars, menu pickers | 21 px high, 12.25 px text, 6 px corners | 24 px capsule, 13 px text |
| Action | Labelled actions in board content, such as Retry or Allow | 21 px high, 10.5 px text | 20 px, 11 px text, 6 px corners |
| Dialog | Cancel, Save and other dialog footer buttons | 21 px high, 12.25 px text | 24 px, 13 px text, 7 px corners |
| Compact | Segmented choices, status bar commands, dense inline icons | 17.5 px high, 10.5 px text | 16 px, 10 px text, 5 px corners |

The sizes are pixels at the default 13 px body text. Like the rest of the
interface, they grow and shrink when you change the body text size. The
**Icon buttons** size below still multiplies toolbar, action and dialog
button heights and glyphs; it does not change their text. A radius larger
than half the height draws a capsule.

Two theme tokens choose how glyph toolbar buttons, such as Search, Wrap or
Settings, look: `--toolbar-button-rest` at rest and `--toolbar-button-hover`
under the pointer, while pressed and while latched. Each is `flat` or
`glass`, and every bundled theme uses `flat` for both:

| | `flat` | `glass` |
| --- | --- | --- |
| At rest | Only the glyph: no fill, outline or rim | A glass capsule: a translucent fill with a bright upper rim |
| Under the pointer | The `button-hover` fill, and the glyph turns to the strong text colour | The glass brightens |
| Pressed | A slightly stronger `button-hover` fill | The glass is shaded |
| Latched, such as Wrap when on | The `button-on` fill (the accent, faint) and an accent-tinted glyph, deeper under the pointer | The lit glass of the current board tab |

A latched button also reports its pressed state to assistive technology. The
`button-hover` and `button-on` colours are theme tones: by default a faint
white (dark themes) or black (light themes) wash, and the theme's accent at
24% (dark) or 16% (light).

The status bar's commands (About, Report issue, Commands and Help) are
always flat, whatever these tokens say.

The **Liquid glass** switch draws outlined toolbar buttons, such as menu
pickers, as glass capsules, and tints destructive actions red rather than
filling them. Primary actions keep the accent fill. GPUI cannot blur what lies
behind a window's content, so glass is translucency over the bar, not a lens.

A theme also sets the dialog frame's padding and gap and the least height of
a board tab. The theme designer edits all of these and previews them live.

On macOS, every theme draws the toolbar as the window's title bar: the window
controls sit at its leading edge, and you can drag or double-click its
background as you would a title bar.

## Icon buttons

Open **Settings > Application > Appearance** and choose **Small**, **Medium**,
**Large** or **Extra large** for icon buttons throughout the app. **Small** matches
the existing global toolbar size and remains the default. Select **Save** to
apply and keep the choice without a restart. Cancel leaves the saved choice
unchanged.

The icons, button hit areas, padding, gaps and toolbar rows grow together. The
same choice applies with horizontal or vertical board tabs. The overflow menu
keeps actions available when the window is too narrow for the complete group.
Text labels keep their normal size. Deliberate compact controls retain their
documented compact size.

At a 16 px interface base, the choices use these nominal layout dimensions:

| Choice | Glyph | Button target |
| --- | --- | --- |
| Small | 14 px | 24 px |
| Medium | 20 px | 36 px |
| Large | 28 px | 48 px |
| Extra large | 40 px | 72 px |

These dimensions follow the interface's relative scale. Changing the icon-button
choice does not change the interface base font or visible button text size. The
theme, interface zoom and display rounding determine the resolved dimensions.

## Design a theme

1. Open the theme designer and select a base theme. Solarized Light and Solarized Dark
   are separate choices. The designer does not generate an opposite-mode palette.
2. Enter a name. Use ASCII letters, digits, spaces, hyphens or underscores.
3. Open a section and edit it. The designer is a panel at the window's
   right edge, so the board beside it, such as the Component gallery,
   shows each change as you make it:
   - **Typography**: the heading, body and code font families and sizes,
     and group headings. Body text size is the interface base.
   - **Spacing and density**: rows, board columns, the window's bars and
     panel, motion, and the control, surface and tab radii.
   - **Controls**: each button kind's height, padding, text size and radius
     (see [Buttons](#buttons)), dialog padding and spacing, board-tab height,
     the **Liquid glass** switch, **Flat** or **Glass** toolbar buttons at
     rest and on hover, and GPUI Kit's radii for fields, menus and large
     surfaces.
   - **Colours**: surfaces (including lists, secondary and group-box
     backgrounds), tabs, the status bar, inputs, lines, text, actions and
     selection, status colours, the glass tones and the flat button hover and
     on fills, with contrast checks. Each colour is a field showing its swatch
     and hex value; select it to pick from the palette, adjust hue,
     saturation, lightness and alpha, or type a hex value. In dark themes
     GPUI Kit fills inputs with the **Input border and dark fill** colour at
     30% opacity.
   - **Preview and contrast**: font samples and every contrast check.

   **Reset to theme default** returns one section to the base theme.
   Colours the base theme derives stay derived until you change them.
4. Select **Save** to create the named theme and make it active. If its file
   exists, confirm the overwrite. A changed file cancels that confirmation.

**Cancel**, closing the panel, Esc or opening Settings restores the appearance
from before the preview. After a successful Save, Cancel keeps that saved
appearance. **Reset all** restores the selected base draft, without writing
files.

The designer does not offer line height, font weight or letter spacing:
Subspace has no theme tokens for them yet. Input heights, padding and text
follow GPUI Kit's sizes; only their radius is in the theme.
**Copy JSON** exports reloadable theme JSON to the clipboard without saving.

Custom files are in `themes/` beside the product's `config.json`. Save stores
the chosen light or dark variant, font roles, density and application text
colours together. It does not change service configuration or credentials.
Restart the app to reload manually added files.

Bundled names are reserved. Invalid custom files and duplicate names produce
warnings; they do not stop the application. Saves refuse symlinks, files over
1 MiB and competing writers. The theme file and active preferences have
separate save boundaries. A partial-save message states which one succeeded.

## Choose a default for a build

The product descriptor can declare its own default theme by the display name
of one variant, such as `Some("Solarized Dark".into())`. One of the product's
declared theme documents must contain that variant. Leaving
`ProductDescriptor.default_theme` as `None` selects the shared framework
default, **Cupertino Dark**, and adds its `cupertino` document to that
product's theme catalogue. In a new product entrypoint, set
`ProductBuild.default_theme_override` to `option_env!("SUBSPACE_DEFAULT_THEME")`,
as the shipped product does.

To override a product's default for a local build, create an ignored
`mise.local.toml` in the app's folder, as the Subspace App does:

```toml
[env]
SUBSPACE_DEFAULT_THEME = "Solarized Dark"
```

Run the normal build or packaging command from that folder. The product
captures this value at compile time. It does not read it from the environment
when the installed app runs. Cargo tracks changes to the value on later builds.
Use the variant's display name, as Settings shows it, not the theme document
ID. `Subspace One Dark` selects **Subspace One Dark**; `Solarized Dark` selects
**Solarized Dark**.

The build override must name a variant in one of the product's declared theme
documents. A blank or padded name fails product validation; an unknown name
stops the app at startup. Saved user theme choices still take priority over the
compiled default. An explicit product default comes next when no build override
exists, followed by the framework fallback.
