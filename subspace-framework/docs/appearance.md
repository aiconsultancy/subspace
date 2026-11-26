# Appearance

**Cupertino Dark** is the framework default for Subspace and products that
do not declare another default. A new Subspace install opens in
[Twilight Two](#a-theme-for-new-installs) instead. **Subspace One Dark**, **Cupertino Light**,
[Subspace Channel](#subspace-channel) Light and Dark,
[macOS Classic Dark Remix](#macos-classic-dark-remix) and
[Twilight Two](#twilight-two) remain available.
Existing saved theme choices still win when available. If a saved theme has
been removed, the default is used. Subspace does not switch
themes with the system appearance; choose **Cupertino Light** for a light
window.
Select a theme in **Settings > Appearance**, or open **View > Theme designer**.
The backtick shortcut and the eye toolbar button also open the designer.

Three independent font choices control headings and tabs, body text, and code
and logs. Automatic choices use the selected theme's defaults and installed
fonts. The app does not download fonts; it can embed some, as
[Embedded fonts](#embedded-fonts) describes.

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

## Embedded fonts

An app can embed font files and list them in `ProductBuild::fonts`, each a
`BundledFont` with its family name and bytes. The shell registers them with
the text system when the app starts, before it applies the saved theme, so
the window, Settings and every screenshot from the capture build draw them on
macOS, Windows and Linux without the fonts being installed. Embed static
files, one per weight and style: the macOS text system registers a variable
font as a single face at its default weight.

An embedded family counts as installed when a theme, Settings or the theme
designer names it, and appears in their font lists. Automatic choices never
take a family only the app provides, so a theme that names no font keeps the
platform's fonts. A family the machine also has installed stays an automatic
choice, as before. If the platform refuses the files, the app shows a notice
and themes fall back as they do for any family that is not installed.

The Framework embeds no fonts. In an app that embeds none, a theme naming a
family the machine lacks still starts with a notice naming the role and the
fallback, which **Settings > Appearance** repeats. The app carries the
fonts' licences in its own notices.

The Subspace App embeds **Inter** (400, 500, 600 and 700, with 400 and 700
italic) and **JetBrains Mono** (the same weights and italics), both under
the SIL Open Font License 1.1.

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

## Twilight Two

**Twilight Two** is the look of the workspace shell's approved mock-ups, and
the theme screenshots of the app are taken in. A new Subspace App install
opens in it (see [A theme for new installs](#a-theme-for-new-installs)). It
has one dark variant: a
warm near-black canvas (`#131312`); one warm dark surface (`#1a1a18`) for
the docks, the tab strip, the title bar and the status bar; warm hairlines
(`#2b2a27`); and Twilight's gold (`#cfa95e`) for the accent, primary
actions, links and focus. A selected row and a latched button take a warm
dark gold fill. It asks for Inter for headings and body text and JetBrains
Mono for code. The Subspace App embeds both (see
[Embedded fonts](#embedded-fonts)), so it starts in them, with no font
notice, whether or not they are installed. In an app that embeds neither,
the automatic fonts are used where they are not installed, and the app and
Settings say so.

It also sets the mock-ups' sizes: 13 px body text and 11.5 px code, 32 px
rows whose selection is inset 8 px with 6 px corners, text 16 px in from
every panel's edge, 34 px tab strip and dock headers, a 36 px toolbar whose
buttons are 24 px high with 12.5 px text and 6 px corners, a 38 px title bar
with 26 px Space tabs and 13 px tab labels, and flat buttons without Liquid
Glass. Other themes keep their own sizes and colours.

Its syntax colours, and other colours the mock-ups do not set, come from
Twilight in the GPUI Kit catalogue (Apache 2.0; notice in
`licenses/themes/`).

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

## Agent icons

Open **Settings > Application > Appearance** and choose **Icons**: **Large**,
**Small** or **Off**. It sets how fleet agents' and ships' icons show in
Spaces. **Small** is the default (`avatar::DEFAULT_AGENT_ICONS`), and a state
file without the choice reads as it. **Save** keeps the choice under
`app.agent-icons` and applies it without a restart; **Cancel** leaves the
saved choice unchanged.

- **Large** draws each icon as its surface always has, such as the agent
  avatar beside a ship in a list.
- **Small** draws the same avatar at 12 px (`avatar::MARK_SIZE_PX`), the size
  of a row's leading glyph, where that glyph would sit.
- **Off** leaves the icon out, and the text beside it closes up.

A Space reads the choice with `AgentIcons::current` and passes each agent
avatar through the one helper, `avatar::agent_icon`, so every surface
follows it the same way. People's initials, message authors' avatars and
text role marks are not agent icons and do not change.

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
     panel, the pane strip height, the navigator and inspector widths,
     motion, and the control, surface and tab radii.
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

Bundled names are reserved, and so are `Default Light` and `Default Dark`,
which GPUI Kit registers itself when it starts. Invalid custom files and duplicate names produce
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

### Theme documents a product supplies

A product can embed its own theme documents beside the Framework's catalogue.
Set `ProductBuild.product_themes` to `&[("example-dark", include_str!("../themes/example-dark.json"))]`,
a list of `(id, json)` pairs, and use `&[]` for none. Then:

- List the id in `ProductDescriptor.themes`, as for a Framework document.
- Name one of its variants as `ProductDescriptor.default_theme` to make it
  the default. `record_new_install_theme` takes the same slice as its last
  argument, so a new install can start in it.
- Its variants appear in Settings and the theme picker like any other, and
  Report issue names the theme by the document's id.

Each document must parse as a theme set with at least one variant, whether
or not the descriptor lists it. An id that is `custom` (the id Report issue
gives a user's own theme), repeats a Framework document's id or another
product document's id, or invalid JSON, including a repeated field, stops
the app at startup with an error that names the id. So does a variant name
that repeats, ignoring case, a Framework or GPUI Kit variant, GPUI Kit's own
`Default Light` or `Default Dark`, or a variant in another product document;
the error names the document's id and the variant. Variant names are reserved like the
Framework's, so a custom theme file cannot reuse them. The Framework ships no
product theme files.

In an app's tests, `test_support::install_product_themes` checks the
documents as startup does, apart from the product's default, and installs
them, so `offline_shell_fixture` loads them; call it before the fixture.

### A theme for new installs

A product can give new installs a theme other than its default, without
moving existing installs. Before the desktop starts, and only when it starts
the desktop app, the product's entrypoint calls
`shell::record_new_install_theme(&context, "Twilight Two", product_themes)`,
with the same documents as `ProductBuild.product_themes` (`&[]` for none).
When the product has neither a config file nor a state file, a new install,
it saves that variant as `appearance.theme` in a new state file, so it is the
user's saved choice from then on and a later change of default never moves
it. An install with either file is left alone, including one whose files are
unreadable or malformed, so an install with no saved theme keeps showing the
product's default. The product's theme documents must pass the checks
startup makes, and the variant must be in the product's theme catalogue; if
either fails, or the files' folder cannot be read, nothing is written and
the call returns the reason. The Subspace App records Twilight Two this way and
keeps Cupertino Dark as its default; a build that sets
`SUBSPACE_DEFAULT_THEME` records nothing. Command-line and headless runs
never call it.
