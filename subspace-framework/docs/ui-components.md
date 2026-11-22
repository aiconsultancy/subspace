# UI component catalogue

This catalogue maps the Subspace UI to its owner and is the canonical source
of the component rules. Look a component up here, or on the
[Component gallery](../spaces/component-gallery/help.md) board, before
building anything.

## Rules

These rules decide how every visible part of Subspace is built, in boards,
the shell and the Framework. Apply them in order. Where a component goes in
a Space, and how the Space is laid out, is in the
[design guide](design-guide.md).

1. **GPUI Kit component first.** Use a GPUI Kit component as it ships, themed
   through the active theme and [`Tokens`](../board-kit/dashboard/src/tokens.rs).
   Where this catalogue lists a shared treatment of that component, use the
   treatment: buttons through `action` and `icon_button`, switches through
   `toggle::switch`, popups through `anchored`, tabs through `tabs`. Each
   GPUI Kit card in the gallery names its treatment under **In Subspace, use**.
2. **Build from GPUI Kit components.** If no GPUI Kit component fits, build a
   shared component in
   [`board-kit/dashboard`](../board-kit/dashboard/src/lib.rs)
   by composing GPUI Kit components. Extend an existing shared component with
   an option before adding another one.
3. **Custom last.** Draw a custom element only when no composition of GPUI
   Kit components can do the job. Record the exception and its reason in
   this catalogue, as [Board-owned compositions and exceptions](#board-owned-compositions-and-exceptions)
   does for the Actions job graph.
4. **Everything visible is a component.** Boards and the shell compose GPUI
   Kit and shared components; they do not style one-off controls. Layout
   (`h_flex`, `v_flex`, `div`) is not a component and needs no wrapper.
5. **Every component is in the gallery.** Every shared component and every
   shell surface has an interactive card on the Component gallery board,
   showing its name and source with a button that copies the name. A new or
   changed component is not done until its card shows it and its name is in
   [Component gallery names](#component-gallery-names).

`make check` enforces these with tests in `spaces/component-gallery`:

- every public module of `board-kit/dashboard` is in
  [Shared dashboard surfaces](#shared-dashboard-surfaces) or in
  [Catalogue entries without a card](#catalogue-entries-without-a-card);
- every catalogue entry has a gallery card whose name is the entry, starts
  with `entry::` or ends with `::entry`, unless that table gives the reason;
- [Component gallery names](#component-gallery-names) lists exactly the
  gallery's Subspace and shell cards;
- every public module of gpui-component 0.9.1 has a GPUI Kit card, and each
  card's **Used in Subspace** mark matches the source outside the gallery;
- a GPUI Kit card's **In Subspace, use** names a Subspace card, and a shell
  card links only to gallery cards;
- as before, `Switch::new` appears only in `toggle`, and popup hosts only in
  `anchored`.

Review, not a test, checks rules 3 and 4 (custom drawing and one-off
styling) and whether a card shows every state of its component.

Older code predates these rules in places. The inventory below names direct
controls as migration candidates; migrate one when it changes.

## Ownership rule

The rules above choose the component; these say where the code lives.

- Styling for a shared button, tab, input or dialog treatment is defined in
  [`board-kit/dashboard`](../board-kit/dashboard/src/lib.rs),
  even when GPUI Kit supplies the underlying control and behavior. Boards use
  GPUI Kit components as they ship, or through that treatment, never restyled.
- Do not create a wrapper for every `Div` or duplicate GPUI Kit state and event
  APIs. A shared treatment may be a small constructor, style operation, or
  composition that leaves native behavior visible to the board.
- Extend an existing shared widget when it is missing one generally useful
  option. Add a new shared surface only after the repeated treatment and its
  variants are known.
- Keep board concepts and layouts in `spaces/<id>`. A pull-request diff or job
  graph is not a generic component merely because it contains reusable
  primitives.
- Keep window chrome and application dialogs in
  [`host/app`](../host/app/src/app.rs). They coordinate dashboards but are
  not dashboard widgets.
- Use [`Tokens::current`](../board-kit/dashboard/src/tokens.rs) for semantic
  colours and density-aware dimensions. Buttons take their height, padding,
  text size and radius from their role's tokens (`Tokens::controls`, see
  [Button roles](#button-roles)); other controls use the 4 px
  `control_radius`; dialogs, menus, popups, toasts, cards, chat bubbles and
  feedback surfaces use the 6 px `surface_radius`. Flush domain surfaces stay
  square only when their board entry documents the exception. Use
  `caption_size` and `secondary_size` for supporting text, and the existing
  `heading_font`/`typography` roles for titles and headings.
  `Glyph::rgb` is reserved for a colour supplied by a service.
- Shared tab treatment uses GPUI Kit `Tab` and `TabBar`. New action buttons have
  an appropriate glyph by default. Every control has an accessible label; a
  glyph is decorative unless the label does not communicate its status.
  Glyphs are Lucide. The one exception is a product mark that identifies a
  chat engine, defined once by `ChatPanel::engine_icon` in
  `host/chat` (sources in its `assets/PROVENANCE.md`). Marks are
  single-colour SVGs tinted like any other glyph.
- Use `IconButton::latched(bool)` for an independent on/off mode in a toolbar.
  Its on state uses the shell Wrap button's shared filled treatment; on and off
  expose accessible pressed state. Keep the owner's callbacks and persistence.
  `IconButton::latched_glyph(bool)` is the quieter form for a switch that is
  usually on, such as the workspace's dock switches: while on, only its glyph
  takes the accent colour, with no fill.
  Use `toggle::switch` for booleans in forms, settings and dialogs, including
  settings panels opened from toolbars. Form rows use
  `toggle::toggle_row` with wrapping text on the left and the switch on the
  right. Settings switches edit the draft until Save; a switch does not change
  the owning operation's persistence, permission, or confirmation rules.
  Selection, task completion, disclosure, and checked menu commands retain
  their native controls. This is an explicit product convention for staged
  Settings forms as well as immediately applied board options.

## Button roles

Every shared button has one role. The role decides its height, horizontal
padding, text size and corner radius, read from the theme's `subspace.density`
tokens through `Tokens::controls`. Pick the role by where the button sits, not
by how prominent it should look; prominence is the intent (variant).

| Role | Constructor | Use for | Tokens |
| --- | --- | --- | --- |
| Toolbar | `IconButton::new`, `action::icon_button`, `action::toolbar_button`, `action::menu_button`, `action::filter_button`, `action::main_action` | Title bar, board, panel and surface toolbars; menu pickers; icon-only chrome such as Close; a pane toolbar's labelled actions | `--toolbar-button-*` |
| Action | `action::action_button` | Labelled actions inside board content and forms | `--action-button-*` |
| Dialog | `action::dialog_button`, `.role(ButtonRole::Dialog)` | Dialog and confirmation footers | `--dialog-button-*` |
| Compact | `.compact()` | The inventoried dense groups: segments, status bar commands and dock switches (`.compact().flat()`), inline row icons | `--compact-button-*` |

Each role has `-h`, `-pad-x`, `-font` and `-radius` tokens, in pixels at the
default 14 px interface base (13 px body text); they scale with the body text
size. Toolbar, Action and Dialog heights also follow the global icon-button
size; Compact does not. Vertical padding is not a separate token: the label is
centred in the role's height. `row_button` keeps native geometry because its
content is the caller's.

`IconButton::surface` reports how a button follows the theme's control style:

| Surface | Which buttons | What it draws |
| --- | --- | --- |
| `Quiet(look)` | Quiet (ghost) Toolbar buttons with a glyph, labelled or not: the global, board, panel, overflow and chat toolbars | The finishes of `--toolbar-button-rest` and `--toolbar-button-hover`, each `flat` (default in every bundled theme) or `glass`; see the table below |
| `Quiet(ToolbarLook::FLAT)` | `.flat()` buttons: every status bar command | Always flat, whatever the theme's tokens |
| `Glass` | Outlined Toolbar buttons with a glyph, such as menu pickers, with `--liquid-glass` | A resting `glass::control` capsule |
| `Tinted` | Danger buttons with `--liquid-glass` | Red text and edge, no fill |
| `Native` | Everything else | GPUI Kit's own variant |

| State | `flat` | `glass` |
| --- | --- | --- |
| Rest | No fill, outline or rim; a transparent hairline keeps the size and gives the focus ring its edge | `glass` capsule with its rim |
| Hover | `button-hover` fill; glyph and text to `fg_strong` | `glass` |
| Pressed | `button-hover` at 1.6 times its alpha | `glass-shadow` shade |
| Latched (`.latched(true)`) | `button-on` fill (1.3 times under the pointer) and the accent mixed halfway with `fg_strong` as the glyph; never GPUI Kit's selected style, so it still answers the pointer | Lit glass, as the current board tab |
| Glyph latch (`.latched_glyph(true)`) | The finishes of a button that is off, with the glyph and text in the accent (`icon_button::glyph_latch`) | The resting glass of a button that is off, with the accent glyph |

The rest token chooses the resting finish; the hover token chooses hover,
pressed and latched. `icon_button::paint` resolves them to colours and is what
the tests check. `button-hover` and `button-on` are theme tones (a faint
white or black wash by mode, and the accent at 24% or 16%); the theme
designer edits them under Colours and the two tokens under Controls. Latched
buttons keep `aria-pressed` through `toggled`. The status bar never takes
glass or an outline in any theme or state.

Direct GPUI Kit buttons remain in the Terminal paste confirmation
(`runtimes/terminal`); it is a migration candidate for
`dialog_button`.

## Shared dashboard surfaces

The public modules in
[`board-kit/dashboard/src/lib.rs`](../board-kit/dashboard/src/lib.rs) divide into UI
widgets, presentation helpers, and UI support. The table includes all of them so
callers do not need a generated symbol index. The host-owned chat entries are
listed beside the reusable input and snapshot modules they consume.

| Canonical surface | Purpose and current consumers | Sizing, type, colour, and variants |
| --- | --- | --- |
| [`action`](../board-kit/dashboard/src/action.rs) | Intent-styled action, dialog, icon and menu-picker constructors used by boards and host features. | Every constructor delegates to `IconButton` with a [button role](#button-roles): `action_button` is Action, `dialog_button` is Dialog (dialog footers), `icon_button`, `toolbar_button` and `menu_button` are Toolbar. `main_action` is the Space's one primary action at the trailing end of its pane toolbar, such as **New issue…**: Toolbar role, `Primary`, semibold label; a toolbar has at most one. `filter_button` is a Toolbar-role menu trigger named by its label alone, with the trailing chevron and no leading glyph, such as **Assignee**. `Accept`, `Reject`, `Caution`, `Primary`, `Neutral` and `Quiet` retain their themed variants; with Liquid Glass, `Reject` is tinted rather than filled. `ActionState` owns loading, success and failure. `toolbar_button` keeps optional visible text, accessible name, tooltip and semantic colour. `row_button` is a full-width quiet row control whose content starts at the leading edge, such as an IRC member with an avatar. `card_button` is a native outline Button whose content determines its height, used by IRC issue previews. |
| [`icon_button::IconButton`](../board-kit/dashboard/src/icon_button.rs) | One shared button with an optional glyph and visible text. Used directly or through `action` constructors. | `.role(ButtonRole)` selects the theme's height, padding, text size and radius; new buttons are Toolbar. `.surface(&controls)` and the [button roles](#button-roles) surface table say what it draws; quiet glyph Toolbar buttons follow the theme's `flat` or `glass` finishes and `.flat()` forces flat. Reads `IconButtonSize` at render time and scales the role's height: with the default tokens at rem16, Small/Medium/Large/Extra large use glyph14/20/28/40 and target24/36/48/72. Padding and gaps scale without changing text size. `icon_button::Metrics::current` gives toolbar rows, gaps and the overflow chevron the same scaled geometry. With Liquid Glass, outlined Toolbar buttons with a glyph are `glass::control` capsules. `.latched(bool)` is the controlled toolbar on/off treatment, matching shell Wrap and exposing pressed state; `.latched_glyph(bool)` shows the on state by an accent glyph alone, for the workspace's dock switches. `.compact()` is the Compact role, a fixed preset for inventoried dense groups. Native button behavior and anchored focus remain shared. |
| [`animated_collapsible`](../board-kit/dashboard/src/animated_collapsible.rs) | Measured disclosure for expandable summaries. The owner retains `Disclosure` and supplies the content. | Reversible height and opacity over 200 ms, with reduced-motion support. `on_resize` lets the owner invalidate virtual row measurements. |
| [`anchored`](../board-kit/dashboard/src/anchored.rs) | Shared popup host and native Select, colour select and context-menu adapters. Theme designer and Space Lab use `select`; the theme designer uses `color_select` for every colour; shared text, links, rows, panels and chat sessions use `AnchoredContextMenuExt`. | Retained `Anchor` measures its `trigger`, positions focusable content with GPUI Kit side flipping/clamping, toggles on repeat pointer activation, dismisses on Escape/outside click and returns focus without overriding an action's new focus target. `dropdown_menu(button, builder)` hosts native menu content; `select(&state)` and `anchored_context_menu` preserve native semantic behavior. Explicit retained anchors must be dismissed when their owner deactivates. `floating(content)` draws non-interactive content at its layout position above later siblings, kept 8 px inside the window, such as IRC's card under a focused issue reference. |
| [`subspace-chat`](../host/chat/src/lib.rs) | Host-owned Copilot/Pi conversation pane: tabs, transcript, references, approvals, session list, input, slash commands, attachment drafts, and toolbar. The shell retains it; boards send `ChatRequest` values. | Uses native buttons, inputs, menus, popovers, commands, and shared rich text. File and image drops or clipboard images become bounded, removable drafts; Copilot sends validated image blobs only on explicit submit and Pi reports unsupported image drafts. The changing busy-status line is centered, wraps in a narrow pane, and uses a subtle theme-accent shimmer. GPUI keeps the repeating animation static under reduced motion. Its engine-specific controls belong here because every app uses the same pane. |
| [Host chat canvas](../host/chat/src/agent_chat/canvas.rs) | Copilot's per-chat native Chart, Mermaid Diagram and restricted SVG Drawing area. Shared by every board through the host chat panel. | Flush resizable frame, shared action buttons, chart Data table (chart formatters), Source alternative and gallery thumbnails. Docked drawings take their own aspect up to a square. Thumbnail frames use native GPUI layout with button roles and descriptive labels. Cmd/Ctrl+Alt+G toggles the gallery while chat or canvas has focus. Escape exits; Left/Right navigate only when the canvas itself has focus. Input editing retains arrow keys. |
| [`chat_input`](../board-kit/dashboard/src/chat_input.rs) | Reusable retained message input used by IRC and host chat. | Native `Textarea`, Write/Preview, trimmed Enter submission, disabled/busy/error state and bounded clipboard capture. `set_plain_text` drops Write/Preview and the Markdown hint for plain-text protocols. `set_placeholder` names the recipient, such as `Message #channel`. `set_recall_keys(true)` adds the `SubspaceChatRecall` context, so Tab emits `Complete` for name completion, and, while the draft is empty, one line or an unedited recalled line (`set_recalled`), the `SubspaceChatLineRecall` context, so Up and Down emit `Previous` and `Next` for line recall; in any other multi-line draft they move the cursor between lines. `cursor` and `replace_range` let an owner complete the word before the cursor as an undoable edit. `chat_input::completion` is the one completion list for every chat input (see [Shared messaging controls](#shared-messaging-controls)). Runtime handles and file ingestion stay in host chat. |
| [`chat_bubble`](../board-kit/dashboard/src/chat_bubble.rs) | The transcript bubble (`bubble`) and the native message row (`message`) with a stable identity. Copilot uses the bubble; IRC uses `message` for transcript lines. | Native `Bubble` and `Message`; width, padding, the surface radius and the `select` fill come from tokens. See [Shared messaging controls](#shared-messaging-controls). |
| [`chat_sessions`](../board-kit/dashboard/src/chat_sessions.rs) | GPUI session snapshots published by host chat and observed by Copilot Agents. | Saved session metadata and open/busy tab state; no runtime ownership or service calls. |
| [`alerts`](../board-kit/dashboard/src/alerts.rs) | Stored sound and OS-notification choices plus delivery helpers. Used by Copilot Agents and the shell notification coordinator. | No visual component. Names and defaults are shared UI policy. |
| [`avatar`](../board-kit/dashboard/src/avatar.rs) | Compact profile photos, theme-aware initials and participant stacks; `sender` and `member` for chat transcripts and member lists; `agent`, `external` and `crew_meter` for fleet agents, relayed people and an agent's crew. IRC uses `sender`, `member` and `agent`. | Shared GPUI Kit `Avatar` and `AvatarGroup` treatment, 24 px, with at most two participants in a stack. Groups reserve a fixed 48 px column so titles align, including fallback initials. `sender` is a 32 px rounded square (surface radius) with initials at the secondary size; `member` is Kit's xsmall avatar (16 px) with the control radius. Both use Kit's theme-aware identity colours. `agent(id, AgentRank, name)` is a circle as large as `sender` (`.small()`: as large as `member`) with a one-pixel ring, a faint fill and the rank's Lucide glyph, all in the rank's tone: a captain `Navigation2` in `Tone::Blue`, the rear admiral `Diamond` in `Tone::Accent`; its accessible name is the name and rank. `.crew(busy, total)` replaces the ring with Kit's `ProgressCircle` around a smaller face, one arc for the busy share, rather than the mock-up's separate arcs per crew member, which would need custom drawing. `external(id, name)` is a `sender`-sized square with up to two initials in muted text inside a dashed muted outline and no fill. `crew_meter(id, busy, total)` is a row of 6 px dots, busy ones filled green and idle ones a muted ring, named *3 of 4 crew busy*; above eight members it shows `busy/total` in muted text. `activity_dot(busy, tokens)` is one of those dots on its own, for a single yes-or-no activity such as a ship working or a person here; it has no name of its own, so its caller names it. `agent_icon(avatar, AgentIcons)` is the one seam for the app's **Icons** setting ([appearance](appearance.md#agent-icons)): the avatar as drawn for Large, the same avatar at `MARK_SIZE_PX` (12 px, a row glyph's size) for Small, and `None` for Off. Its dots are token-coloured layout `div`s, as `tabs::unread_dot` is. Boards supply names and already-decoded images, never credential-bearing URLs. |
| [`badge`](../board-kit/dashboard/src/badge.rs) | Shared count pills and key-hint badges. Used by menu digit hints, shell switcher tabs and navigation width calculations. | Owns the 11 px default font, 5 px horizontal padding and 7 px radius. `badge::Pill` is a caption-sized native Tag with muted, warning and danger tones, tinted fill and an accessible status label, used for draft limits; `Pill::hue(Tone)` draws it in any theme tone instead, with the same tinted fill, for role marks such as Channels' `CAPTAIN` in `Tone::Blue` and `REAR ADMIRAL` in `Tone::Accent`. Callers supply semantic foreground and background colours; navigation uses the same constants for width measurement. |
| [`chart`](../board-kit/dashboard/src/chart.rs) | Native charts for a `subspace_charts::ChartModel`: bar (stacked, grouped, horizontal), line, area, pie, doughnut and scatter. Used for the charts agents author with Flint, including chat canvases and Space Lab. | GPUI Kit `plot` shapes, axes, labels and tooltip overlay; the stacked and grouped bar layout is Subspace's own. Series colours come from `Tokens` tones in `chart::palette` order, with a lighter shade on each turn past the end, unless the model fixes a series colour. Tick text scales with `Tokens::heading_size`. The title, subtitle, legend and value format are model fields, not variants. `Chart::height` fixes the height; `Chart::fill` takes what the parent flex container has left, for a chart that should grow with the window; `Chart::label` replaces the generated accessible description when the surroundings name the chart better. `cargo run -p subspace-dashboard --example charts [light\|dark]` shows every kind. |
| [`code`](../board-kit/dashboard/src/code.rs) | Tree-sitter language selection and syntax-highlight runs. Used by GitHub code views. | GPUI Kit `SyntaxHighlighter`; code size comes from `Tokens::mono_size`. Highlight colours come from the active highlighter theme. |
| [`composer`](../board-kit/dashboard/src/composer.rs) | Markdown Write/Preview composer with validation, busy state, limits, and submit events. Used by GitHub and Issues. | Native `Textarea` plus shared rich-text preview and action controls. Retained drafts leave on Escape without discarding; readonly/busy variants freeze input, and submit waits for IME composition. Owners can supply a state hint when the submit shortcut is unavailable. Optional image intake emits bounded `ComposerImagePaste` batches independently of ordinary text paste; its owner validates/uploads the bytes and supplies attachment controls through the accessory slot between editor and submit. Required empty text disables the button; the keyboard path explains that text is required. Labels, optional input and limits are variants, not separate composers. |
| [`context_text`](../board-kit/dashboard/src/context_text.rs) | Converts rows, details, and selected text into labelled chat references. Used by GitHub and Terminal. | No independent visual style. It owns the 16 KiB cap and truncation marker. |
| [`copy_details::CopyDetails`](../board-kit/dashboard/src/copy_details.rs) | One local-copy control for Copilot errors, update errors, and About diagnostics. Owners supply `ErrorDetails`; the shell installs `CopyDiagnostics::new(LocalDiagnostics)`. `copy_details::copy_text` copies any owner text, such as the IRC **Copy agent prompt**. | Shared neutral `action_button` with Copy glyph, native Tab/Enter/Space and global icon sizing. Activation calls the core local-copy projection and `links::copy`, then shows a temporary in-app confirmation. Labels remain stable when the error changes. `diagnostics(id)` omits the error; `with_diagnostics` supplies isolated gallery fixtures. `copy_text(id, label, confirmation, text)` builds its text on activation, never in render, and uses the same confirmation. No environment reads or commands in render or copy. |
| [`dates`](../board-kit/dashboard/src/dates.rs) | Compact age, relative time, and local timestamp formats. Used directly or re-exported by several boards. | No component styling. It keeps time text consistent between rows and details. |
| [`density`](../board-kit/dashboard/src/density.rs) | Stored dimensions and body, heading, tab and monospace font sizes. Used by `Tokens`, settings, and the theme designer. | Supplies the button role tokens (`--toolbar-button-h`, `-pad-x`, `-font`, `-radius` and the same for `action`, `dialog` and `compact`), `--dialog-pad`, `--dialog-gap`, `--nav-h`, `--liquid-glass`, the `flat` or `glass` choices `--toolbar-button-rest` and `--toolbar-button-hover`, `--row-h`, `--row-font`, `--heading-font`, `--tab-font`, `--mono-font`, padding, row inset, top-bar height, panel, lane, tree, motion the two radius tiers (`--radius`, `--surface-radius`) and the navigation capsule's roundness (`--nav-round`, a fraction of half the item height). The workspace shell's `--strip-h` (dock headers and the pane's tab strip, 34 px), `--nav-w` (the navigator, 240 px) and `--inspector-w` (the inspector, 320 px) are in the theme designer's Window group. A theme's `subspace.density` block sets them. |
| [`dialog`](../board-kit/dashboard/src/dialog.rs) | Shared modal backdrop, frame, heading and footer. Used by the quit dialog and Actions, IRC and Web board dialogs. | Native GPUI composition with the shared modal key context, bounded 540, 600 or 780 px frames, `--dialog-pad` padding and `--dialog-gap` spacing, surface radius, the raised `surface` colour with a shadow, a semibold heading and secondary-to-primary footer ordering. Domain state and content stay in the caller. |
| [`feedback`](../board-kit/dashboard/src/feedback.rs) | Skeleton, inline loading line, empty state, full error state, retained-data error banner, Retry, and recovery actions. Used by the shared list and board-specific loading surfaces. | `loading_line` gives in-panel fetches a stable selectable ID, muted secondary text and token styling. Titles and messages use caller-scoped `rich::plain_text` identities. `retry_action` and `recovery_action` use the shared xsmall `action` treatment for Retry, Restart, New and similar actions. `configuration` and `error_box_with_settings` add the shared glyph-bearing `configure_action`, which dispatches `ConfigureBoard` to the shell and opens the visible board's Settings page. `empty_with_actions` is the empty state with caller-supplied setup actions in one centred, wrapping row, such as the Channels Space's **Add network…**, **Copy agent prompt** and **Show in Finder**. |
| [`find_in_page`](../board-kit/dashboard/src/find_in_page.rs) | Local text find within the focused Space or Help surface. | Retained query and match navigation use a native input and shared icon buttons. Native text owns selection and match painting; Escape closes the bar and restores focus. |
| [`fields`](../board-kit/dashboard/src/fields.rs) | Native configuration, compact-table and search inputs, labelled fields, multiline textareas and code editors. Used by Report issue, Search sources, Actions, Space Lab, the shell's Settings pages and board settings editors such as IRC. | Semantic input roles select native sizes. Inline or stacked labels and validation messages use tokens. Callers retain native input state, events and masking. The Settings form treatment is shared: `form_field` (label above control), `heading`, `note` (muted selectable help) and `field_error` (red message under its control); the shell's own Settings helpers delegate to them. `toolbar_search(state, label, cx)` is a workspace toolbar's search field: a Search input with the search glyph and a clear button, filled with the quiet `pane` surface, no border at rest, the control radius, and the native focus ring when focused. `path_input` is a configuration input with a trailing neutral **Choose…** action (FolderOpen glyph) that opens the native file picker and writes the chosen path into the input. |
| [`github_sign_in`](../board-kit/dashboard/src/github_sign_in.rs) | Sign in with GitHub, shared by Settings (the **GitHub accounts** group on GitHub Space pages) and chat (when its account is missing or rejected). `GitHubSignIn` is the app's one sign-in, installed by the shell only when the app supplies an OAuth App (`ProductBuild::github_oauth`); without one `shared` is `None` and neither view offers it. `status` shows it. | `sign_in_button` is a neutral `action_button` with the LogIn glyph, disabled while a sign-in runs. `status(id, phase, tokens, on_cancel, cx)` draws nothing while idle; a `feedback::loading_line` with a quiet **Cancel** while starting or waiting for the browser; the device code in the monospace family at 18 px (scaled by the monospace setting) with **Copy code** (`copy_details::copy_text`), **Open GitHub** (ExternalLink glyph, `links::open`) and **Cancel**; or the failure in `red`. The headless flow is `subspace-github-auth`; the token goes to the credentials' store, never to UI state. Sign-in and sign-out apply at once, outside Settings' Save. |
| [`glass`](../board-kit/dashboard/src/glass.rs) | Liquid Glass navigation capsules for the shell's horizontal board tabs and vertical rail, and `control`, the glass surface for outlined toolbar buttons when a theme sets `--liquid-glass` and for quiet glyph toolbar buttons whose theme chooses `glass` (see [Button roles](#button-roles)). | `control` keeps the button's geometry: a resting glass fill and inset rim, the plain `glass` tone under the pointer, a `glass-shadow` shade while pressed, and the current capsule's lit material when latched or selected. `nav_button(id, selected, height, tokens, cx)` returns a capsule `Button` (`capsule_radius`: half the height times `--nav-round`) with a transparent hairline, so keyboard focus tints the border along the capsule. Unselected capsules have no fill and a faint glass hover. With `--liquid-glass`, the current one has a top-lit gradient fill, a hairline edge, an inset specular rim stronger at the top and a short lifting shadow; without it, the current one has a faint accent fill inside a one-pixel accent ring and an accent label (`accent_material`, `label_color`). Either way it reports pressed to assistive technology. `bleed` gives a capsule shadow room inside a clipping row such as an overflow toolbar. There is no backdrop blur: the glass is translucency over the bar. |
| [`html`](../board-kit/dashboard/src/html.rs) | Converts service HTML to Markdown before shared rich-text rendering. Used by GitHub search indexing. | Presentation-neutral parser and writer; image and link policy is applied by `rich`. |
| [`links`](../board-kit/dashboard/src/links.rs) | Safe external URL opening, clipboard copying, and the shared `external` link. Used across the shell and boards. | `external` uses Base Link for keyboard activation and link semantics, theme tokens for hover and focus, and a native Copy link context menu. `external_marked` adds a trailing decorative external-link glyph for inline values. It opens only destinations accepted by `safe_url`. Use Buttons for in-app commands. |
| [`toolbar_label`](../board-kit/dashboard/src/toolbar_label.rs) | A toolbar fact with nothing to choose. Used by the GitHub Space for the account when only one is signed in; more than one account shows the `menu_button` picker instead. | `label(id, ToolbarLabel { icon, text, accessible }, cx)`: an optional muted decorative glyph (`None` for plain quiet text, such as the Issues Space's **Grouped by state**) and muted text in the Toolbar button role's height, padding and text size, so it shares the band's centre line. No hover, focus or caret, because it promises no action. Its accessible name gives the text context, such as "Signed in to GitHub as octo-dev". In overflow the owner supplies a native menu label item. |
| [`toggle`](../board-kit/dashboard/src/toggle.rs) | Every form/settings/dialog switch, including source policies, grid options and board forms. Toolbar modes use `IconButton::latched` instead. | `switch(id, cx)` builds the native Switch with a contrast-safe on track; build every switch with it (see [Switch contrast](#switch-contrast)). `toggle_row(label, switch)` aligns wrapping label text with a trailing switch. The caller supplies the controlled value, size, disabled state, and callback. Keyboard, focus, and switch semantics remain native. |
| [`list`](../board-kit/dashboard/src/list.rs) | `ListState`, grouped rows, folding, keyboard selection, scrolling, detail opening, and `ListDashboard`. GitHub and Copilot Agents use the full renderer; other boards reuse its row/detail data types. | Rows use `row_height`, `row_font`, `pad_x`, `indent`, `hair`, `pane`, and `select`. The selected row is a fill, never a leading bar. With `row_inset` 0, rows are full width with hairline separators; above 0, hover (`hair`) and selection (`select`) are rounded fills inset by that much, rows have no separators and group headers drop their fill and use muted titles. `Glyph` supports semantic `Tone` or an exceptional service RGB colour. `ListDashboard::group_indicators` is empty by default. A board may supply right-aligned `GroupIndicator` marks after the group name and count. Each mark has an icon, theme tone, accessible label and tooltip, and no click behaviour of its own. `ListDashboard::activates_on_click` makes a row activate on a double click or Enter instead of a click. `ListDashboard::group_captions` is empty by default. A board may return `GroupCaption`s, each a quiet label above the row it names inside a group, such as **SHIPS** and **PEOPLE** in a fleet's group: `caption_size`, semibold and muted, shown in capitals with the text as written as its accessible name, a heading role, indented like the group's rows. A caption is not a row: it has no key in `ListState`, so selection, the list keys, the panel's previous and next and a selection restored by `set_groups` pass over it, and folding the group hides it. It shares the element of the row under it, so the scroll container still has one line per visible key. A caption whose row is not in the group is not drawn. |
| [`measure`](../board-kit/dashboard/src/measure.rs) | Reports the width assigned to an element for responsive compositions. | Invisible canvas probe. Use only when normal flex layout cannot make the decision. `text_width(window, text, family, size)` shapes one line, for fixed lanes that must fit their widest label (the log level lane, rail labels). |
| [`media`](../board-kit/dashboard/src/media.rs) | Shared image-preview tiles and file metadata chips. Owners supply decoded pixels and native Open or Retry actions. | `tile` uses a bounded, accessible frame with token colours and control radius. `file_chip` is a passive metadata label. `thumbnail` is GPUI Kit's `Attachment` (a square tile for an image, a chip otherwise) in a wrapper that carries the button role, label and click, since `Attachment` has neither focus nor an accessible name; `focus_ring` marks the owner's keyboard cursor as an overlay that never shifts layout, also used by `unavailable_chip`. `size_text` uses decimal units. Decoding and commands stay with the owner. |
| [`menu`](../board-kit/dashboard/src/menu.rs) | Searchable keyboard menu body with checked, glyph-bearing and starred options and section headings. Used by board filters and pickers, Logs, and the repository search. A `keep_open` menu stays open after a pick (toggles); an `owner_filtered` menu emits `MenuEvent::Query` and shows the options its owner sets with `set_options`, plus an optional muted note such as **Searching…**. | `MenuAnchors` retains named inline/keyboard triggers and hosts the native `Input` body through `anchored::Anchor`. Overflow replaces its native menu in the same chevron host. Supports arrows, Enter, digits 1–9 and Escape; owners dismiss before hiding or deactivating. No caller supplies popup coordinates or restores board focus unconditionally. |
| [`native_grid`](../board-kit/dashboard/src/native_grid.rs) | Retained bounded result grid, adopted by Space Lab project tables only. | `NativeGrid` owns theme-derived shaded headers, alternating rows, cell boundaries, hover, numeric/string sorting, AND column filters, cell/range selection and scoped TSV copy. Its settings panel retains independent `toggle::switch` options; the toolbar trigger is a disclosure. Rows are virtualized without pagination. `GridExport` supplies an immutable selected/view/all snapshot for native CSV/typed JSON saving; the board owns the file picker and atomic no-clobber write. |
| [`overflow_toolbar`](../board-kit/dashboard/src/overflow_toolbar.rs) | Measured single-row toolbar with atomic inline units and explicit native popup projections. Used by shell tabs, global actions and board toolbars. | Keeps a visible prefix in source order and puts the trailing suffix behind a chevron. Reserves the trigger and gap before choosing the cutoff. Hidden units are measured without painting or registering focus or hit targets. Stable keys retain measurements. Open-menu callbacks use current owner state without rebuilding unchanged native keyboard selection. Anchored entries receive the retained chevron host for chooser replacement. Icons stay fixed for a stable unit key and local entry index. |
| [`panel`](../board-kit/dashboard/src/panel.rs) | Shared right-side detail frame with title bar, service link, previous/next, context menu, scrolling body, and draggable edge. Used by the shared list. IRC opts into `PanelBody::content_owns_scroll` for its native MessageScroller and uses `ListDashboard::panel_footer` for the fixed composer. `panel::service_link` composes the same safe external-link action for board-owned panel extras with a caller-supplied element ID. | Width is `panel_width`, clamped to 320 px and 75% of the window. Uses `surface`, `line`, heading font, and density storage. Existing callers keep a scrolling body without a footer. |
| [`workspace`](../board-kit/dashboard/src/workspace.rs) | The workspace shell that lays out every Space. `Workspace` tiles an optional `Navigator` (the leading dock: the Space's name, then `nav_section` labels and `NavRow` rows for its views, projects, networks or repositories), a `Pane` (a tab strip for what is open, a `Toolbar` for search, filters and actions, then the content) and an optional `Inspector` (the trailing dock: the detail of the selection in `section` groups of `fact` rows, with the actions for it in a footer). `Workspace::detail` places a panel that owns its width, such as the shared list's detail panel, where the inspector goes; `pane_head` is the tab strip and toolbar alone, for a list drawn with `list::render_under`. Used by every Space's layout. | Panels tile to the window's edges and to each other, separated by hairlines. The room is inside each panel, on one text edge: `text_edge` (`pad_x` plus `row_inset`) starts dock headers, rows, the toolbar and the content. Dock headers and the tab strip are at least `strip_height` high, so their hairlines meet. The docks are `navigator_width` and `inspector_width` wide, held to at least 180 px and 260 px and at most 30% and 40% of the window; dragging a dock's inner edge stores its width in the density settings, and a double click restores the theme's width. The toolbar is at least `toolbar_height` high: leading items wrap in a narrow pane, trailing items stay on the first line. The dock switches are not the toolbar's: a Space reports its docks with `Dashboard::docks` (a `SpaceDocks`, each dock `None` when the Space has none, else whether it shows) and takes a switch's click with `Dashboard::toggle_dock` (a `SpaceDock`); the shell's status bar draws `dock_switch` for each dock reported, the navigator's at its leading end and the inspector's at its trailing end, quiet panel glyphs that take the accent while their dock shows (`latched_glyph`), with the ids `workspace-navigator-toggle` and `workspace-inspector-toggle` and the names **Hide navigator** or **Show navigator** and **Hide inspector** or **Show inspector**. The navigator's title, the Space's name, takes the accent. Navigator rows are `row_height` high; a numeric count is in the monospace family. The selected row is a `select` fill and hover is `hair`, inset by `row_inset`, never a leading bar. `NavRow` takes a glyph or a leading element, a count, a trailing mark, and `selected`, `quiet`, `strong` and `indent`, and optionally a menu through `NavRow::menu`, built from the standard groups; a right-click runs the row's click first, so the menu acts on the highlighted row. The navigator and the tab strip use `sidebar`, and the navigator's hairlines use `sidebar_line`; the inspector uses `surface`; the tab strip, toolbar and inspector hairlines use `line`. Dock titles use the heading font at the row size, and section labels (`nav_section`, `section`) use `caption_size`, semibold and muted, shown in capitals while their accessible names and ids keep the text as written; GPUI has no letter-spacing, so they are not tracked. The tab strip's bottom hairline lies behind its tabs, so the open tab of a `tabs::pane_tab_bar` covers it and joins the content. Reading content is no wider than `READING_WIDTH_PX` (760 px). `Inspector::tabs(tabs, selected, on_select)` puts a tab list in place of the inspector's title, the one approved exception in the [design guide](design-guide.md#inspector-tabs): each `InspectorTab` has a label and an optional count, at most `INSPECTOR_MAX_TABS` (3) show, and the title becomes the tab list's accessible name. The tab list is a `segmented` unit with the `TabList` role; each tab is a focusable button with the `Tab` role and selected state, chosen by a click, Enter or Space as the other segmented controls are, and its count uses the navigator's count mark. The body takes the `TabPanel` role, named after the open tab. Close stays the last header control, and without tabs the inspector is unchanged. |
| [`prefs`](../board-kit/dashboard/src/prefs.rs) | Dashboard preference keys and cache files beside `state.json`. | No visual component. It is the storage boundary for UI choices and large caches. Shell navigation and chat defaults use the same `state.json` store. |
| [`reference_card`](../board-kit/dashboard/src/reference_card.rs) | Shared reference buttons, passive hover cards, status pills and assignee avatars. | `surface` preserves native Button activation and focus; `frame` uses the shared popover elevation. Outline, accent-edge and chip treatments use tokens, with glyphs to distinguish status without colour alone. |
| [`reveal`](../board-kit/dashboard/src/reveal.rs) | Show a local file in the system file manager. Used by Settings > General and the Channels Space for `config.json`. | `reveal::button(id, path)` is a neutral `action_button` with the FolderOpen glyph, labelled per platform by `reveal::label()`: **Show in Finder**, **Show in File Explorer** or **Show in Folder**. It selects the file with GPUI's `reveal_path`, or opens the nearest existing folder when the file is missing. |
| [`secret`](../board-kit/dashboard/src/secret.rs) | The saved-credential field used by Settings tokens, board-declared secret fields and board settings editors such as IRC passwords. | Masked native input that never shows a saved value. A saved secret shows a green Lock state and stays read-only; the ghost **Clear** action (Eraser glyph, **Undo clear** with RotateCcw) makes it editable and a yellow note says what Save does. Clicking the read-only input shows the muted Clear hint; the input's accessible label carries the same explanation. `TOKEN` and `PASSWORD` select the wording. The owner keeps the input, the saved flag, the Clear choice and the hinted row. |
| [`rich`](../board-kit/dashboard/src/rich.rs) | Selectable plain text and Markdown rendering with context menus, links, and images. Used in details, help, feedback, chat, diffs and IRC messages. `rich_text_with_links` lets the owner claim links first, such as IRC's channel and member references; other links follow the shared policy. `rich_text_prepared` does the same for Markdown its owner has already limited to a safe subset with no HTML and no images, skipping `prepare_markdown`, which reads tags inside code as HTML. | GPUI Kit `TextView`; caller supplies the surrounding font size. `Text`, `Markdown`, and converted service HTML are the content variants. Non-control presentation text (errors, diagnostics, empty states, explanatory text and read-only values) uses `rich::plain_text` with a stable unique ID so pointer or keyboard selection can be copied without activating surrounding controls. `plain_text_with` and `text_menu_with` take `MenuChat::Off` for a board whose content stays out of chat, such as Issues: the menu offers **Copy** only and holds no selection for the chat. Buttons, menus, tabs and row actions remain native controls. |
| [`repositories`](../board-kit/dashboard/src/repositories.rs) | Favourite repositories (`repos`) and `RepositoryPicker`, the shared repository search. Used by the PRs board's **Repositories** control and Settings' **Add repository…**. | A kept-open, owner-filtered `menu`: **Favourites only** (when the owner has the filter), **Favourites** (filled stars), then **Your repositories** or **GitHub results** (outlined stars). The owner supplies the blocking search and applies each `RepositoryPickerEvent`: the board saves at once (`write`, `apply`, `DashboardEvent::ConfigChanged`), Settings edits its draft. |
| [`row_menu`](../board-kit/dashboard/src/row_menu.rs) | The standard right-click menu, built from seven groups in a fixed order: Open, Act, Copy, Chat, View, Remove and Place. Used by shared list rows, group headers, the detail panel, `workspace::NavRow`, and the Space tab and navigation rail. | `GroupedMenu` holds `MenuCommand`s, each with its `MenuGroup`, label, Lucide glyph and action; `build` turns it into GPUI Kit's native `PopupMenu` in group order, with a separator between groups that have commands, the bound key beside a command made with `key` or `action` (Open shows Enter), a disabled command's reason as a label line under it, and the Remove group's glyphs in the theme's danger colour (labels keep the menu's colour so they remain the items' accessible names). `outline` lists the lines without building them. `RowMenu` facts give the standard commands (`row_menu_items`, `panel_menu_items`, then `grouped`); a `RowMenu` whose `chat` is `MenuChat::Off` leaves out **Add to chat** and **Add selection to chat**, for a board whose content stays out of chat. |
| [`log_view`](../board-kit/dashboard/src/log_view.rs) | Framework log viewer, hosted by the shell in place of the board. | `surface_header` titled **Logs** with Close, then a flush overflow toolbar: shared tab bar for the level, shared action buttons (target menu, follow, copy, copy log path, open folder, capture level) and a native filter input. Close is never an overflow unit. The line list uses the theme's monospace family with token colours per level; the level lane is measured to fit `ERROR`. Header, toolbar and lines share the `pad_x` inset. Escape closes an open menu first, then the view, from anywhere inside it. |
| [`search_view`](../host/search-ui/src/search_view.rs) | Host full-text Search view, composed directly by the shell. Board indexing and retained-content helpers stay in dashboard. | `surface_header` titled **Search** with its description, **Sources & model** and Close. Native search input, shared action buttons and token-based result/source layout on the `pad_x` inset. Current/all scope, keyboard result selection, cached excerpts, source policies and explicit model installation. All storage and model work runs in background tasks. |
| [`segmented`](../board-kit/dashboard/src/segmented.rs) | Compact exclusive choices such as filters and layout modes. | `control(id, items, selected, tokens, on_select)` returns one atomic inline unit. `selected` is `None` while a required choice without a default is unmade. Each focusable segment has an accessible label and pressed/selected state, optional Lucide glyph, control-tier radius and an index callback. Place the returned group in one `OverflowUnit`; do not split its segments across overflow. The inspector's tabs (`workspace::Inspector::tabs`) are drawn with the same segments, with tab roles. |
| [`settings`](../board-kit/dashboard/src/settings.rs) | Board-provided settings and feature-section descriptors consumed by the shell settings tree. | Data contract only. The shell owns navigation and form rendering. |
| [`slider`](../board-kit/dashboard/src/slider.rs) | Labelled numeric controls for runtime-backed Space Lab projects. | Retained `NumericSliderState` maps finite domain values to native integer ticks. `NumericSliderEvent::Commit` reports user edits; external synchronization is silent. The shared treatment supplies the accessible name, keyboard navigation and disabled enforcement around GPUI Kit's native slider parts. Tokens define presentation. Native accessibility-action dispatch remains a verification gap. |
| [`surface_header`](../board-kit/dashboard/src/surface_header.rs) | Title bar of a Framework view that the shell shows in place of the board: Logs and Search. | `SurfaceHeader::new(icon, title, close_label, on_close)`, optional `description` and trailing `action`s. Muted glyph and semibold heading-family title at `heading_font`, `toolbar_height` minimum, `pad_x` inset, `line` bottom hairline. Close is a quiet shared `icon_button` with the X glyph, the caller's accessible name ("Close logs") and a tooltip naming Esc; it always stays visible after the actions and never moves into overflow. The owning view binds Escape and emits its own close event. Boards do not use it; they cannot be closed. |
| [`tabs`](../board-kit/dashboard/src/tabs.rs) | Canonical native section and resource tabs. Used by GitHub navigation, the Issues timeline and the log view's levels. | GPUI Kit `Tab` and `TabBar`, native pointer selection and accessibility, `small` segmented treatment at its natural width, glyph prefixes by default, accessible labels, optional count/busy text, and disabled state. `TabSpec::plain(label)` omits the glyph for named panel regions. `TabSpec::key` shows an opened item's key, such as `SUB-12`, after the glyph in the monospace family and muted colour, before its title. `pane_tab_bar` is the bar of a workspace pane's tab strip: each tab as tall as the strip on the strip's surface, the open tab on the content's surface over the strip's bottom hairline with the native trailing hairline. In every bar the selected tab is told apart by the surface it takes and its stronger, medium-weight text; no bar draws an accent line on any edge. `tabs::unread_dot` is the passive unread mark; `tabs::dock::area` applies the same treatment to native dock splits, dragging and close callbacks. |
| [`tokens`](../board-kit/dashboard/src/tokens.rs) | Semantic colours, density-aware dimensions and type roles, radius tiers, service tones, and One Dark defaults. Used throughout the app and boards. | `caption_size` and `secondary_size` scale from the row font; titles use `heading_font` or `heading_size`. `control_radius` (default 4 px) and `surface_radius` (default 6 px) come from the density tokens. `nav_round` (`--nav-round`, default 1) shapes navigation capsules. Active theme colours override defaults: `chrome` and `chrome_line` (title bar) for the top bar, `sidebar` and `sidebar_line` for navigation, and `surface` (popover) for dialogs, menus, toasts and side panels. `glass`, `glass_rim` and `glass_shadow` take the dark or light set by theme mode for the current navigation capsule. Theme metadata may override dashboard tones, including `glass`, `glass-rim` and `glass-shadow`. |
| [`truncated`](../board-kit/dashboard/src/truncated.rs) | Single-line truncation with a delayed tooltip containing the full text. Used by rows, headings, toolbars, and diff chrome. | Supports plain or pre-styled text and wrapped/unwrapped variants. It measures actual available width rather than guessing from characters. |
| [`typography`](../board-kit/dashboard/src/typography.rs) | Heading font and configurable group-heading role. Used by grouped rows in shared lists, GitHub diff, settings, and the theme preview. | Group headings have configurable family, size, bold, and italic values. Other heading sizes scale from `Tokens::heading_font`; body and code use token roles. |

### Catalogue entries without a card

These public modules and catalogue entries have no Component gallery card of
their own. A shell surface's card is named in its reason. The gallery tests
read this table: an entry here needs no card, and a card name in a reason
must exist.

| Entry | Why it has no card |
| --- | --- |
| `alerts` | No visual component: stored sound and notification choices. |
| `artwork` | No visual component: board-owned embedded assets. |
| `chat_sessions` | No visual component: session snapshots for host chat. |
| `code` | A presentation helper that returns syntax-colour runs; its output shows in the GitHub diff and in `gpui_kit::component::input::Editor`. |
| `context_text` | No visual component: chat reference text. |
| `contracts` | No visual component: board-to-board messages. |
| `contrast` | No visual component: WCAG contrast arithmetic behind `toggle::switch`. |
| `dates` | No visual component: date and age formats. |
| `density` | No visual component: the stored density tokens behind `tokens::Tokens`. |
| `dock_panel` | No visual component: the contract through which the shell docks the product's terminal panel. The panel's header composes `action` buttons. |
| `help` | No visual component: help image resolution for board guides, shown by `shell::help`. |
| `html` | No visual component: HTML to Markdown before `rich::rich_text`. |
| `images` | No visual component: bounded raster decoding. |
| `measure` | No visual component: an invisible width probe. |
| `notifications` | No visual component: OS notification requests. |
| `prefetch` | No visual component: background detail fetching. |
| `prefs` | No visual component: stored choices and caches. |
| `refresh` | No visual component: automatic refresh timing. |
| `retained` | No visual component: retained-content permits. |
| `search_adapter` | No visual component: full-text search helpers. |
| `settings` | No visual component: settings descriptors; the shell draws them, see `shell::settings_dialog`. |
| `subspace-chat` | A host crate boards cannot depend on; see the shell surface `shell::chat_panel`. |
| `Host chat canvas` | A host crate boards cannot depend on; see the shell surface `shell::chat_canvas`. |
| `search_view` | A host crate boards cannot depend on; see the shell surface `shell::search_view`. |
| `log_view` | It shows the app's real log, not fixture data; see the shell surface `shell::log_view`, which opens it. |

The crate root also owns shared UI interfaces rather than additional visual
components:

| Root export | Purpose |
| --- | --- |
| `Dashboard`, `DashboardHandle`, `EventSink` | Board lifecycle, toolbar/status contribution, search, modal state, reload, and type-erased shell integration. |
| `DashboardEvent`, `Toast`, `ToastTone`, `ToastLink` | Board-to-shell requests and the data rendered by the shell toast stack. |
| `PaletteCommand`, `PaletteOption`, `SubList`, `Search`, `ItemSearch` | Commands and item-search results rendered by the shell palette. |
| `Modal`, `DASHBOARD_CONTEXT`, `MODAL_CONTEXT`, `view_keys`, `KeyHelp` | Shared keyboard and modal boundaries. |
| `Wrap`, `Selection`, `Services`, `CurrentProduct` | Globals set by the shell and read by boards and shared widgets. |
| `AgentContext`, `AgentTool`, `Catalog`, `ToolEnvironment`, `ChatRequest` | Shared chat and agent-tool integration contracts. |

The definitions and re-exports are in
[`board-kit/dashboard/src/lib.rs`](../board-kit/dashboard/src/lib.rs).

### Product issue reporting

[`subspace-report-issue`](../host/report-issue/src/lib.rs) owns the
retained Report issue workflow for every product. The shell supplies explicit
diagnostic facts and opens the same form from its toolbar and status bar.
Both entry points use `IconButton` with the global size and a visible label.
The feature composes native form controls, the Kit dialog, shared actions,
selectable readonly text and a bounded strip of up to five image previews.
Each preview has a stable identity and its own shared Remove action. The
count, picker limit, sequential clipboard handoff and explicit folder save
belong to the retained report owner, not a new shared control variant.
This is a host-owned
application form, not a new board-local treatment or gallery component.
GitHub requests and raster decoding run off the UI thread.
The diagnostics use the shared default issue allowlist. The form's **Include
paths** boolean uses `toggle::toggle_row` with the label on the left and the
native `toggle::switch`. It starts off for each report and is not persisted.
Only explicit opt-in appends the sanitized path allowlist; either transition
invalidates the exact preview. See [Report an issue](report-issue.md).
Automatic diagnostics use native disclosure behavior, collapsed initially,
with a field count and a compact two-column readonly grid when expanded.
Typography and spacing use shared tokens. Disclosure state changes only the
presentation, never the diagnostic projection or exact submission preview.

The shared `copy_details::CopyDetails` error composition keeps **Copy details**
and **Show log** together. Both use the canonical action-button geometry and
native keyboard behavior. **Show log** dispatches the existing `OpenLogView`
command. The error-free About variant has only **Copy diagnostics**. Logging
belongs to each error owner's acceptance handler, not to this renderer or its
copy callback; both use the same sanitized core diagnostics projection.
About and Settings route this command through their dialog owners. Settings
keeps its existing unsaved-change guard. The log view wraps structured fields
below the message so a long copied block cannot hide the message column.
The report's multiline description, reproduction steps and exact preview use
GPUI Kit `Textarea` directly. The Issues editor uses `fields::textarea`
for its multiline description; both keep Kit's native editing and selection.

### Native result grid

Retain `Entity<NativeGrid>` in the owning view, with IDs containing the project
and component identity. Build typed `GridCell` values and a `GridSnapshot` when
an accepted result changes, not during render. Keep the existing 500-row and
8,000-cell boundary. Call `replace` for a new result evaluation. It clears the
range anchor and selection while retaining the entity, options, sort and filters.
Space Lab detects replacement from the result's values, scope, evaluation time
or session, without changing the worker schema.

Click selects a cell. Shift-click selects a rectangle. Arrows move the active
cell; Shift-arrows extend the rectangle, and Home/End move within the row.
Command-C or Control-C copies selected cells. Selection follows original row
identity across sorting and filtering, including selected records hidden by a
filter. Selected output follows sort order and omits headers on the clipboard.
View output follows filtered display order. All output follows original source
order. View/all clipboard output and CSV include headers. JSON keeps the
`columns`/`rows` structure and original number/string values.

Spreadsheet output quotes delimiters, quotes and line breaks. Formula-like
strings and headers receive an apostrophe before their original value, including
leading whitespace/control-character bypasses. Numeric cells remain numbers.
Export captures its values before the native save dialog opens, so later worker
updates cannot mix evaluations. An existing destination is not overwritten.

GPUI Kit 0.9.1 `TableState` has single-cell positional selection and does not
forward click modifiers in its selection event. Its `bordered` option describes
the outer frame, not every cell. The shared grid therefore uses GPUI's
`uniform_list` with grid-owned identity, range, cell boundaries and keyboard
behavior. Horizontal scrolling keeps headers, filters and cells together;
relative column minima and row sizing follow interface zoom. Drag selection is
not included. The supported rectangular gesture is Shift-click or Shift-arrow.

### Switch contrast

A switch must be identifiable in each state on each surface it can sit on, in
every bundled theme, light and dark. The rule follows WCAG 2.2 success
criterion 1.4.11 (non-text contrast):

- The outline of the off track has a contrast ratio of at least 3:1 against
  each surface.
- The on track has at least 3:1 against each surface.
- The knob has at least 3:1 against the off track and against the on track.
- A disabled switch is visibly faded, but it stays identifiable: its track
  keeps at least 1.5:1 against each surface, and the knob keeps at least 1.5:1
  against the faded track.

The surfaces are the theme's `background` (window), `muted` (panel and
dialog), `secondary`, `sidebar`, `popover`, `accent` (selected row) and
`list_active`, each composited over `background`.

GPUI Kit's Switch has a transparent border, so the track's fill is its
outline. Its colours come from three places, and Subspace enforces the rule at
each one with
[`toggle::track`](../board-kit/dashboard/src/toggle/track.rs):

| Part | GPUI Kit source | Where Subspace applies the rule |
| --- | --- | --- |
| Off track | `switch.background` theme token | When Subspace applies a theme, including custom themes and the theme designer preview |
| Knob | `switch.thumb.background` theme token | When Subspace applies a theme |
| On track | `primary`, unless the switch sets its own colour | `toggle::switch` sets the colour on each switch |

A colour that already meets the rule is kept. A colour that does not is moved
in small steps towards the theme's text colour (tracks) or window background
(knob), then towards black or white, until it passes. If a custom theme's
surfaces make the rule impossible, the step with the highest worst-case
contrast is used. Switch tokens are written as solid colours, because the rule
cannot measure the stops of a gradient. The theme's `primary` colour is not
changed for other controls. A test computes every ratio above for every bundled theme and fails
below the minimum. The Component gallery item `toggle::switch` shows every
state on every surface.

`list::header` draws a title and its muted subtitle on the same text
baseline. The subtitle follows the title's width and truncates when space is
limited. No Space uses it for its own title any more: the Space's name is the
`workspace::Navigator` title, and its context goes to the status bar. PRs and
Actions start their list rows at `workspace::text_edge` through
`ListDashboard::list_side_inset`.

`ListDashboard::list_side_inset` optionally overrides row content edges for
one board. `group_count_after_name` keeps a count beside its group name.
Both preserve the existing theme inset and trailing count by default; the Issues
list opts into the approved NETT layout's 24 px edge and adjacent count. Activity
keeps its existing defaults. The scroll container and selection fills retain
their shared ownership.

`anchored::form_popover` hosts a native form with nested pickers, as used by
Actions Filters. It positions the form from its trigger. A picker menu may
extend beyond the form, so the form closes from its trigger, Escape or its
close button.

## Current direct GPUI Kit controls

### Shared messaging controls

- `ChatInput` and `ChatInputEvent` are exported at the crate root. IRC uses
  `ChatInput::message`, the same `InputGroup`/`Textarea` composition as
  Copilot, with send controls instead of agent settings. `set_plain_text`
  removes its Markdown Write/Preview. Only a plain Enter sends: Kit reports
  Shift+Enter as a `PressEnter` too, after inserting the line break, and the
  input ignores it. The message input's hint names the platform's key,
  `Return to send · Shift+Return for a new line` on macOS and `Enter to send ·
  Shift+Enter for a new line` elsewhere.
  `composer::Composer` remains the Write/Preview Markdown editor.
- `chat_input::completion::CompletionList` is the completion list above a
  chat input, shown with Kit's `Command` list with search and filtering off.
  The owner works out the choices: the agent chat's `/` commands
  (`SlashList`) and the IRC composer's `@member`, `/command`, argument and
  `#channel` choices. The owner places the list directly above the input
  and calls `ChatInput::set_completing` while the list is open. Then Up and
  Down emit `Previous` and `Next`, Enter and Tab emit `Complete` and Escape
  emits `Cancel`; the owner answers with `step` and `active`. A click emits
  `CompletionEvent::Accept`. The list never takes focus.
  `CompletionItem::new(label)` is a plain row.
  `CompletionItem::command(label, hint, detail)` adds muted arguments and a
  description at the end of the row. `CompletionItem::person(name, mark,
  away)` adds the shared member initials and a muted trailing mark, such as
  a mode prefix where member lists show it, and fades the row while away.
  `spoken` sets the accessible name when it should say more than the label.
  `word_before(text, cursor)` gives the range of the word to replace. Do not
  build a second popup for completion.
- Both inputs share editor and toolbar minimum dimensions. Their outer frame
  draws only the top border; the panel and status bar own the side and bottom
  dividers. The input group has no second outline or clipped focus ring.
- Message inputs have an accessible top resize handle. Dragging or arrow keys
  change the height; Home or a double-click resets it. The board supplies its
  measured height, and the shared sizing policy caps the input at half that
  height. Write and Preview keep the same draft and size.
- `ListDashboard::min_list_width` reserves list space when the inspector
  shrinks.
- `ListDashboard::panel_header_label` substitutes friendly text for an opaque
  header ID. `PanelHeader::label` changes display only; Copy id retains the
  original service identifier. Other boards keep their existing headers.
- `ListDashboard::row_commands` adds a board's own commands to a row's or
  group header's context menu, each in its group of the
  [standard menu](design-guide.md#context-menus). IRC adds **Open to the
  right** and **Open below** (Open), **Join** and **Refresh channel list**
  (Act) and **Leave channel**, **Remove from list** or **Close conversation**
  (Remove); PRs adds **Add `owner/name` to favourites** or **Remove … from
  favourites** (Act), also on repository headers through `group_menu`;
  Actions adds **Refresh repository** (Act). The older `extend_row_menu` and
  `row_submenus` still work; what they add lands at the end of Act.
- `ListDashboard::empty_actions` replaces the empty state's **Configure** with
  the board's own next step. PRs offers **Add repositories…** when All open,
  or Favourites only, has no favourite repositories.
- [`chat_bubble`](../board-kit/dashboard/src/chat_bubble.rs) owns the bubble
   native `Bubble` width, padding, radius and background used by Copilot.
   Its `message` helper composes native `Message` slots with stable IDs; IRC
   uses it for transcript lines.
- [`images`](../board-kit/dashboard/src/images.rs) owns bounded raster
  decoding for chat attachments and Report issue images.
- `ListDashboard::activates_on_click` is `true` by default. A board returns
  `false` for a row whose activation changes something, so a click only
  selects it and a double click or Enter activates it. Channels does this for
  channels from the server's list that you are not in (a muted, `done` row),
  where activating joins. The Component gallery's `list::render` card has one
  such muted row.
- `ListDashboard::row_leading` lets a board supply its own leading content,
  such as a shared avatar or IRC's buffer glyph, without replacing the list
  frame.
- Shared rich text refuses automatic remote image loads.
  `html::to_markdown_without_images` avoids a second image decoder path when
  a board renders authenticated media separately.

These uses describe current code. A direct use inside `board-kit/dashboard` is the
implementation of a shared surface. A direct use in a board is a migration
candidate unless this catalogue records an exception. Layout-only `h_flex`,
`v_flex`, and `Div` use is already within the target boundary.

### GPUI-Kit messaging reuse

The pinned 0.9.1 library already supplies `Message` and its named slots,
`Bubble`, `MessageGroup`, `Marker`, `Attachment`, and `MessageScroller` with
tail-following, anchored history prepending and a jump-to-latest button.
Use these for new or changed messaging compositions through shared dashboard
helpers. Do not replace the native `chat_bubble` helper with a styled `Div` or recreate transcript
scrolling. The existing `ChatInput` already composes native `InputGroup` and
`Textarea`; keep its draft, Markdown and resize behavior.

Kit also provides badge dots/counts, native list-item drag/drop hooks, popup
menus, resizable panels and `Notification` with in-app/system delivery. Boards
still own unread meaning, polling and account isolation. Neither Sidebar nor
Message supplies a synchronization model.

`ListDashboard::panel_content_owns_scroll` disables the frame's body scrolling
while leaving the composer footer fixed, so a native MessageScroller owns the
transcript's scrolling. Other panels retain their existing scrolling defaults.

IRC composes the native messaging pieces like a chat client. Each transcript
row is a `chat_bubble::message` (native `Message`, `ListItem` role, stable ID).
A message row has a gutter as wide as `avatar::sender`: the sender's avatar
for the first message of a group, else the time, shown on hover. Beside it are
the bold nick in its theme tone with the time, then the text. Every row's text
is one selectable `TextView`, so drags across rows copy their text in reading
order. Plain and mIRC text uses escaped literal Markdown: mIRC colours become
16 % theme-tone background tints, other formatting becomes inline tags and
monospace uses inline code. Compact lines' sender names are bold without their
nick tone. Literal links and references use native inline objects within the
text view, retaining the shared link policy, reference styling and cards;
each is an atomic selectable span. `rich::text_menu` gives every row its shared
Copy menu. Formatted text from anyone (Slack-style
`*bold*`, `_italic_`, `~strike~`, code, quotes, lists and links) goes through
`rich::rich_text_prepared` after the board rewrites it to a safe subset with
no HTML and no images, so code that shows a tag keeps its text.
Highlighted messages use the `select` fill with the control radius. Unread
rows use a 12 % accent fill (18 % under the pointer), after a `New` divider.
Other rows but the separator labels are tinted under the pointer, as in Slack,
with the foreground at 6 % opacity and the control radius: the list's `hair` hover
does not show on the `surface` colour in every theme. Compact server lines
and condensed membership runs keep the time in the same gutter.
Issue references (through the `issues` contract) share the reference look,
accent on the light accent tint. An issue its tracker reports as unavailable
is muted with a muted underline, never tinted. The keyboard's link or
reference is strong text on a stronger accent tint, underlined in the
accent, and the focused message has a 2 px ring in the theme's `ring` colour
drawn over the row, so the text does not move. An issue reference's card is
Kit's `Tooltip::element`: the identifier and *Archived* in muted caption
text, the title in medium weight, then the state's glyph (the Issues board's
state icons and tones) with *state · Tracker · assignee · profile* in muted
caption text, at most 320 px wide. Under the pointer it is the
`InteractiveText` tooltip; for the keyboard's reference it floats under the
message through `anchored::floating`. The Channels Space's local plugin preserves
the literal path's reference presentation. Formatted Markdown references
remain ordinary links without cards or unavailable styling.
Day separators are a pill (`line` border, `surface` fill, full radius) on
`line` hairlines. The `MessageScroller` row style removes the chat gap; a new
sender group adds its own space above. The conversation's header row (the
name, then the network and topic in muted text cut with a tooltip, and the
member count) has a `line` divider under it.

IRC is the one board that hosts its views in a GPUI Kit `DockArea` with
`DockSkin`, nested inside the shell's own Help dock: the shared list in the
left dock (`list::render` with the board as its `ListDashboard`; activating a
row opens a panel instead of the detail panel), each conversation as a
closable centre panel in tabs and splits, and the member list in the right
dock. A conversation panel draws no dock title bar of its own; its tab title
carries a shared `badge` with the unread count (accent when it mentions you).
The member list has a bar-height header with **Members · N** and a close
button, and its own scrolling; its rows are `action::row_button`s with
`avatar::member`. It fills its dock, which owns the width. A joined channel
row's menu adds **Leave channel**, a channel we are no longer in adds
**Remove from list**, and a private conversation's adds **Close
conversation**, after the open items and a separator; the toolbar's **More**
menu offers the same items. This exception keeps IRC
formatting, which Markdown cannot express, without a new shared variant;
within-line text selection is available only in Markdown messages.

`ListDashboard::row_emphasis` and `row_accessible_label` default to normal text
and the row title. IRC uses them for bold titles on highlighted or unread
private buffers and an unread label.

Registered boards opt into generic notification settings through
`BoardSettings::notifications`, and may declare the choices that apply until
the person saves their own with `BoardSettings::notification_defaults` (Off
unless declared). Shared preferences define modes, in-app styles
and allowlisted native sounds. Kit `Notification` is explicitly in-app. The app's
single raw GPUI OS-response owner retains opaque routes across window closure;
do not use Kit `.system()` alongside that router. OS posts contain generic text
unless the board passes `Notice::described`, whose text names only who and
where, never message content.

| Component family | Current use and representative paths | Target treatment and migration priority |
| --- | --- | --- |
| `Button` and variants | Shared [`action`](../board-kit/dashboard/src/action.rs); direct shell use remains in [toolbar](../host/app/src/app/toolbar.rs) and [settings dialog](../host/app/src/app/settings_dialog.rs) | **High for remaining changed controls.** Reuse or extend `action` for intent, size, glyph, state, tooltip, and accessibility. Shell-only chrome may remain shell-owned, but treatment shared with boards belongs in `dashboard`. |
| `Tab` and `TabBar` | Shared [`tabs`](../board-kit/dashboard/src/tabs.rs) treatment used by GitHub (Subspace App: `spaces/github/src/lib.rs`) | **Consolidated for resource and section navigation.** Boards provide domain labels, glyph prefixes by default, status and callbacks; GPUI Kit retains native selection and accessibility. `TabSpec::plain` is the named-panel exception. The pinned library does not yet provide compound arrow-key navigation. Other tab-like controls remain migration candidates when changed. |
| `Input`, `Textarea`, `Editor` | Shared composition in [composer](../board-kit/dashboard/src/composer/view.rs); direct board inputs in GitHub diff (Subspace App: `spaces/github/src/diff/view/chrome.rs`) | **Medium.** First catalogue field roles: search, labelled form field, Markdown, code/JSON, and compact filter. Share their visual treatment without hiding GPUI Kit state or events. |
| `Switch`, `Select`, `Slider` | Every switch comes from `toggle::switch`; forms and the theme designer place it with `toggle::toggle_row` | **Shared boolean treatment.** The architecture check rejects other `Switch::new` calls. Keep native controlled state, keyboard focus, disabled behavior, and the owner's Save or immediate-apply boundary. Select and Slider retain their existing state models. |
| Appearance pickers and `NumberInput` | Settings > Appearance in [`settings/pickers.rs`](../host/app/src/settings/pickers.rs) and [`settings/sizes.rs`](../host/app/src/settings/sizes.rs) | **Shell-owned exception.** Theme rows and the trigger show the name with decorative swatches read from that theme's own document, outlined with the `line` token; a small `ThemeList` delegate right-aligns them and draws the check. Font rows and the trigger preview each font in its own family; Automatic uses the UI font. Item titles stay the plain name for search and accessibility, and values stay `String`. Each role's size is a native `NumberInput` with a `px` suffix on the select's line, in a group labelled "<role> font size". In-range entries preview live; leaving the field or Save clamps it. |
| Selection and completion controls | GitHub Viewed completion, exclusive log-level tabs, and checked menu commands | **Semantic exceptions.** These select objects, record completion, or represent menu commands rather than independent toolbar modes. Do not replace these controls with form switches. |
| Dialog components | Shared [`dialog`](../board-kit/dashboard/src/dialog.rs) frames for the quit dialog and board dialogs; chat confirmation uses native dialog parts | **Medium for remaining modals.** Keep domain contents local and reuse the shared frame, footer order, spacing and action treatment. |
| `PopupMenu`, context and dropdown menus, `Popover`, `Select` | Shared [`anchored`](../board-kit/dashboard/src/anchored.rs) construction boundary; [`row_menu`](../board-kit/dashboard/src/row_menu.rs) and board owners supply semantic entries | **Shared host required.** Do not add direct native hosts or fixed/click-offset popup positions outside `anchored`. Native Select and invocation-point context semantics are delegated through its adapters. The architecture test covers all board/framework source, including the excluded Terminal crate, with no legacy allowlist. |
| `Command`, `Tree`, `ListItem` | Shell [palette](../host/app/src/app/palette_view.rs) and [settings navigation](../host/app/src/settings/view/navigation.rs) | **No board migration identified.** These are shell-owned compositions. Reuse their data contracts, not their layout, unless a board gains the same interaction. |
| `Scrollbar` and scroll traits | Shell [settings](../host/app/src/settings/view.rs) and board viewers | **Low.** Scroll behavior remains native. Add shared visual treatment only if visible scrollbar configuration diverges. |
| `Spinner`, `Progress`, `StatusBar` | Shell [status bar](../host/app/src/status_bar.rs) | **Low.** Status bar is shell-owned. Board busy/progress colour and size should come from shared feedback/tokens when changed. |
| `Tooltip` | Shared [`truncated`](../board-kit/dashboard/src/truncated.rs) | **Low.** Keep native tooltip behavior; centralize delay, control label, and style only when they are product policy. |
| `SyntaxHighlighter` and `TextView` | Shared [`code`](../board-kit/dashboard/src/code.rs) and [`rich`](../board-kit/dashboard/src/rich.rs) | **Already at the target boundary.** Boards consume shared presentation and retain domain-specific surrounding layout. |
| `h_flex`, `v_flex`, and `Div` | Layout and board-specific compositions throughout | **Not migration candidates by themselves.** Wrap only an interactive or visual pattern that needs one treatment across call sites. |

## Shell-owned compositions

The `shell::toolbar` Component gallery card covers the title bar, including
its product mark. Themed one-tone marks use GPUI's `svg()` at the 14 px slot's
device-pixel size, with the artwork's baked dark/light brand tone. These are
asset colours, not general interface tokens. Compatibility `product.svg`
artwork uses `img()` to retain its own colours. No custom drawing is needed.

| Composition | Canonical path | Why it stays in the shell |
| --- | --- | --- |
| Window frame and main split layout | [`app/shell.rs`](../host/app/src/app/shell.rs) | Composes the selected dashboard, optional shared chat pane, status bar, and overlays. Vertical board navigation sits left of workspace content, below the toolbar and above the status bar. |
| Dashboard switcher, dashboard controls, command search, chat toggle, and chrome | [`app/toolbar.rs`](../host/app/src/app/toolbar.rs) | Coordinates registered dashboards and global actions. Horizontal tabs are the default. Switcher tabs are shared `glass` capsules, vertically centred in the title bar with shadow room between them; only the current board's tab is glass, and none while Logs or Search replaces the board. On macOS the title-bar row and the traffic lights share one centre line, and the logo starts 20 pt after the zoom button. Switcher buttons use product icons, descriptive tooltips, accessible labels and dashboard badges. Each tab's right-click menu ([`app/space_menu.rs`](../host/app/src/app/space_menu.rs), the standard `row_menu` groups through `anchored_context_menu`) has **Close Space** in the Remove group, which saves and applies the same choice as the Space's Enabled switch and Save (the current Space hands over to the tab before it, or the next one when it was first), and **Space settings…** in the Place group; for the last open Space, Close Space is disabled with the reason in the menu. A plus after the last tab ([`app/add_space.rs`](../host/app/src/app/add_space.rs), the shared ghost `IconButton` with the Plus glyph and the accessible name **Add a Space**, opening a native menu through `anchored::dropdown_menu`) lists the Spaces that are switched off, each with its glyph, title and description, then **Manage Spaces…** after a separator; with every Space on, the menu says so in one quiet line. Choosing a Space saves and applies the same choice as its Enabled switch and Save, then switches to it. The plus is the tab strip's last overflow unit, so it is the first to move into the chevron menu, as its last entry **Add a Space…**, which opens the same menu in the chevron's place. The palette command **Add a Space…** offers the same choices as a palette list. Global actions stay right-anchored on the first row. Tabs yield space first; tabs and globals each use shared chevron overflow when needed. The complete board control group sits beside the tabs when it fits and moves below them as one horizontal row otherwise. Board adapters provide their own explicit overflow actions without changing `Dashboard::render_toolbar`. Global labels default off and remain independently configurable in Settings > Layout or each inline button's right-click menu. Commands uses the Command glyph, not Search. The chat toggle shows the selected engine's mark. An available update changes About to a Download glyph in both inline and overflow controls. Activation still opens About. |
| Board navigation rail | [`app/navigation.rs`](../host/app/src/app/navigation.rs) | Optional left rail and icon-only collapse, configurable in Settings > Layout or through the palette action **Toggle vertical navigation**. Destinations are the same `glass` capsules as the horizontal tabs, with the same right-click menu, and the list ends with the same **Add a Space** plus. Tab content uses a full-width, left-aligned horizontal row inside a 32 px capsule, with separate row gaps and larger padding. Stored keys are `app.board-navigation` (`horizontal` or `vertical`) and `app.board-navigation-collapsed` (`1` or `0`). Short windows scroll the tab list and keep the collapse control. |
| Command palette | [`app/palette_view.rs`](../host/app/src/app/palette_view.rs) and [`app/palette_lines.rs`](../host/app/src/app/palette_lines.rs) | Combines shell commands with each dashboard's command and search contracts. |
| Help and board help overlays | [`app/help.rs`](../host/app/src/app/help.rs) and [`app/board_help.rs`](../host/app/src/app/board_help.rs) | Combine global keys with active-board help. |
| Settings dialog, tree, pages, and forms | [`app/settings_dialog.rs`](../host/app/src/app/settings_dialog.rs) and [`settings/view.rs`](../host/app/src/settings/view.rs) | General owns startup behavior, including the sole automatic-update preference. Layout owns board order, board navigation, the terminal's placement, bottom width and button text (with a terminal), the collapsed toolbar menu, and toolbar labels. Terminal (with a terminal) hosts the terminal's font editor. The themed left pane fills the dialog's top, left, and bottom interior edges. Application and Boards use one inert named-group heading treatment. The shared update-status Button stays inside the bottom of that pane, with native text truncation and a full-status tooltip. The right pane owns the Settings title, scrolling page, and Save/Cancel actions. Body and navigation scroll independently. Chat settings stay on Chat. Dirty dismissal offers Save and exit or Exit without saving. |
| GitHub account and token Settings | [`settings/view.rs`](../host/app/src/settings/view.rs) and [`settings/extensions.rs`](../host/app/src/settings/extensions.rs) | One Settings-owned credential composition serves PRs, other GitHub-backed board pages and opted-in product Settings extensions. Every page uses the same retained Account select, masked token draft, saved-token state and Clear action. Select construction uses `anchored`; Save and Cancel retain their existing boundaries. Shared help states when saved account and token changes take effect. Extensions request the controls explicitly rather than copying them or identifying another board. |
| About updates | [`app/about_dialog/update_panel`](../host/app/src/app/about_dialog/update_panel.rs) | A themed GroupBox separates updates from build metadata. Status and the applicable Check or Install action share a wrapping row. Available versions use the shared View on GitHub link with Copy link. About, Settings, toolbar, and status bar observe one retained updater. Native Progress and Spinner report real work. This product-specific composition remains shell-owned. |
| Theme designer | [`jig/view.rs`](../host/app/src/jig/view.rs) and [`jig/view/sections.rs`](../host/app/src/jig/view/sections.rs) | Edits shared tokens, fonts, and theme files; it is application tooling rather than a dashboard widget. A native `Accordion` holds the Theme, Typography, Spacing and density, Controls, Colours, and Preview and contrast sections; each section is built only while open and has its own **Reset to theme default**. Density groups render as sliders, unitless tokens such as `--liquid-glass` as `toggle_row` switches, and choice tokens such as `--toolbar-button-rest` (`flat` or `glass`) as shared `segmented` controls. Each colour is `anchored::color_select` (GPUI Kit's `ColorSelect`) with its own `ColorPickerState`, written back as rounded hex. Every edit previews in the whole window. |
| Docked terminal | [`app/shell/dock.rs`](../host/app/src/app/shell/dock.rs), [`app/terminal_dock.rs`](../host/app/src/app/terminal_dock.rs) and the product's panel (the Subspace App's `terminal/panel/src/header.rs`) | The shell places the product's one dock panel (`subspace_dashboard::dock_panel`) at the left, the right or the bottom (beside the chat or full width), reusing the dock frames, drag handles and 60% share, or fills the body when maximized. The panel's header composes the shared tab bar, `action_button`s and `action::toolbar_button` glyphs, latched for the current placement and full width, whose text shows only while the label choice is on; Hide is `action::icon_button`. No new component, so no gallery card: the gallery cannot build the Zig terminal. |
| Toast stack | [`app/toasts.rs`](../host/app/src/app/toasts.rs) | The shared `Toast` contract is in `dashboard`; stacking, timing, and placement belong to the window shell. |
| Window status | [`status_bar.rs`](../host/app/src/status_bar.rs) | Combines active-dashboard state, chat state, build metadata, and global shortcuts. The chat state is the engine's mark at the compact status-glyph size, then its name and state ("Codex ready"), both in the state's colour. Shared compact `.flat()` Buttons open About, Report issue (with the `report-issue` feature and a repository), Commands, and Help through existing actions; all share one treatment, flat in every theme and state (no fill, outline or glass at rest, the `button-hover` fill under the pointer, stronger while pressed). The version adds a Download indication when an update is available. |

## Board-owned compositions and exceptions

Space Lab's native project view composes shared actions, fields, sliders and
charts. Its versioned definition owns layout and bindings. Its project workspace
owns source selection, execution consent and private Project Settings; the root
service owns workers independently of view selection.

The dynamic project selector uses `anchored::select` with GPUI Kit's retained
`SelectState`. The shared adapter preserves the standard native selection
treatment, and Space Lab adds no local style variant. Labels, the standard
small size and theme tokens stay consistent with
the surrounding fields. The project view records the trigger's state generation
so a deferred selection cannot be accepted after synchronization. Pinned GPUI
Kit does not expose a disabled accessibility flag on every control; prevented
activation and accessible labels are separate from native disabled-state
readback.

The Actions board composes the workspace shell (`workspace::Navigator` and
`workspace::Toolbar`), shared lists/details, tabs, menu buttons, segmented dispatch choices, labelled fields, feedback and confirmation
dialogs. Its job DAG uses GPUI layout and painting for job nodes and edges, with
theme Tokens, surface radii and accessible job labels. The graph is a domain
visualization, not a board-local button style. The `flow-graph` example uses the
same graph component with offline fixtures. Lists can supply a
`ListDashboard::group_note` for loading or empty group rows.
`feedback::empty_line` supplies an accessible empty row without a group.
`ListDashboard::comfortable_rows` lets Issues give populated and loading
rows the same taller height. The issue page and path bar share a centred
column capped at 80 rem and use truncated values in Properties.

| Board | Local compositions | Shared surfaces it reuses | Why the local UI is not a generic widget |
| --- | --- | --- | --- |
| GitHub | Pull-request/issue controls and the diff tree, code rows, image diff, and diff chrome in `spaces/github/src/diff` (Subspace App: `spaces/github/src/diff/mod.rs`) | Full shared list/detail frame, tabs, composer, menu, code, rich text, actions, and row menus | Diff navigation, viewed files, hunks, and image comparison are GitHub domain views. |
| Copilot Agents | Task filters, cloud-session rows, logs, and saved-reference details in `spaces/github-copilot-agents/src` (Subspace App: `spaces/github-copilot-agents/src/lib.rs`) | Full shared list/detail frame, actions, menu, dates, alerts, and shared chat session data | Cloud task state and log/reference bodies are specific to agent sessions. |
| Issues | Team workflow (Subspace App: `spaces/issues/src/view/issue/workflow.rs`) and Properties (Subspace App: `spaces/issues/src/view/issue/properties.rs`) on the issue page. | GPUI Kit icons and layout, shared row button, anchored menu, and truncated values. | The workflow trail uses a canvas to keep its solid and dotted segments aligned with equal-width state stops as the page resizes. GPUI Kit has no matching workflow primitive. The end-state picker uses the shared menu, while the canvas draws only the trail. |
| Channels (IRC) | Conversation view (header, grouped message rows, compact lines, day pill, typing line), member list view and Markdown/reference rewriting in `spaces/irc/src` (Subspace App: `spaces/irc/src/lib.rs`); the Settings > IRC network list and form in `spaces/irc/src/editor` (Subspace App: `spaces/irc/src/editor.rs`) | Full shared list/detail frame, `MessageScroller`, `chat_bubble::message`, `ChatInput::message` (plain text, recall keys), dialog, fields (`form_field`, `path_input`, `heading`, `note`, `field_error`), `secret`, `toggle`, `anchored::select`, actions, `copy_details::copy_text`, `reveal`, `IconButton::latched`, dropdown menu, feedback and notifications | mIRC formatting, condensed membership runs and member prefixes are IRC domain views. The network list follows the web board manager's rows: a semibold name, a muted summary, Edit… and Remove…, and an inline removal confirmation that names the network instead of a second dialog over Settings. |

## Concrete inconsistencies

These are bounded consolidation candidates, not evidence that all UI should be
centralized.

1. **Resource and section tabs are consolidated.**
   [`tabs`](../board-kit/dashboard/src/tabs.rs) now supplies the native GPUI Kit
   treatment used by GitHub. Counts, loading or
   failure marks, disabled state, labels and selection still come from each
   board. This does not claim that every button, input, dialog or older
   tab-like control has migrated.
2. **Feedback and detail-panel controls use shared native action treatment.**
   [`feedback`](../board-kit/dashboard/src/feedback.rs) uses `action_button` for
   Retry and recovery actions, while its titles and messages use selectable
   `rich::plain_text` with distinct instance IDs. This keeps repeated feedback
   surfaces selectable without changing their actions.
   [`panel`](../board-kit/dashboard/src/panel.rs) uses
   [`action::icon_button`](../board-kit/dashboard/src/action.rs) for arrows and
   Close, with tooltips and accessible labels. The older
   feedback-local bordered button has been removed; remaining shell-only
   controls are separate migration candidates.
3. **The shell palette and toast affordances use labelled native buttons.**
   Toast dismiss uses the shared quiet `icon_button` with the Lucide X glyph,
   tooltip and accessible label **Dismiss notification**. The palette uses the
   same native small ghost treatment as the other application controls; its
   tooltip includes the palette shortcut and its accessible name remains
   present when its label is hidden.

Do not combine these fixes with a new component crate, a rename of
`subspace-dashboard`, or a mechanical repository-wide replacement of direct
GPUI Kit calls. Migrate a control when it changes or when a focused
consolidation proves the shared variants. The existing crate boundary is
sufficient; the missing work is a stricter styling boundary inside it.

## Component gallery names

The developer [Component gallery](../spaces/component-gallery/help.md) board
(`component-gallery`) shows each component below in its states, with fixture
data, and copies its name with the button beside it. Use these names in
requests, such as "use `action::icon_button`". A name is the component or
helper name in code, relative to `subspace_dashboard`, unless it starts with
`shell::` (a shell surface), `gpui_kit::`, a board ID or `inconsistencies::`.
A board test checks that this table lists every Subspace and shell card. To
add a card, see the board's guide.

| Section | Gallery names |
| --- | --- |
| Buttons | `icon_button::IconButton`, `icon_button::latched`, `action::action_button`, `action::ActionState`, `action::menu_button`, `action::header_button`, `action::icon_button`, `action::toolbar_button`, `feedback::retry_action`, `feedback::recovery_action`, `feedback::configure_action`, `panel::service_link` |
| Choices | `segmented::control`, `tabs::tab_bar`, `tabs::unread_dot`, `tabs::dock::area`, `toggle::switch`, `toggle::toggle_row`, `slider::control` |
| Inputs | `fields::input`, `fields::labelled_input`, `fields::code_editor`, `fields::form_field`, `fields::heading`, `fields::note`, `fields::field_error`, `fields::path_input`, `fields::textarea`, `secret::secret_field`, `reveal::button` |
| Menus | `anchored::dropdown_menu`, `anchored::select`, `anchored::color_select`, `menu::MenuAnchors`, `anchored::Anchor`, `anchored::form_popover`, `row_menu::RowMenu`, `repositories::RepositoryPicker` |
| Dialogs | `dialog::frame`, `dialog::footer`, `dialog::backdrop` |
| Feedback | `copy_details::CopyDetails`, `github_sign_in::status`, `feedback::skeleton`, `feedback::loading_line`, `feedback::empty_line`, `feedback::empty`, `feedback::configuration`, `feedback::error_box`, `feedback::error_box_with_settings`, `feedback::error_box_with_actions`, `feedback::error_banner`, `feedback::error_banner_with_actions`, `Toast` |
| Content | `animated_collapsible::AnimatedCollapsible`, `reference_card::surface`, `reference_card::frame`, `media::tile`, `media::file_chip`, `media::thumbnail`, `badge::badge`, `badge::Pill`, `links::external`, `links::external_marked`, `links::external_marked_with_label`, `truncated::text`, `rich::plain_text`, `rich::rich_text`, `find_in_page::surface`, `typography::group_heading`, `toolbar_label::label`, `avatar::person`, `avatar::group` |
| Messaging | `chat_input::ChatInput`, `chat_input::completion::CompletionList`, `composer::Composer`, `chat_bubble::bubble`, `chat_bubble::message`, `avatar::sender`, `avatar::member`, `avatar::agent`, `avatar::external`, `avatar::crew_meter` |
| Navigation and chrome | `glass::nav_button`, `glass::control`, `overflow_toolbar::OverflowToolbar`, `surface_header::SurfaceHeader`, `workspace::Navigator`, `workspace::Toolbar`, `workspace::dock_switch`, `workspace::Pane`, `workspace::Inspector`, `workspace::InspectorTab`, `tokens::Tokens` |
| Lists and tables | `list::render`, `list::GroupCaption`, `native_grid::NativeGrid`, `chart::Chart` |
| Shell surfaces | `shell::window`, `shell::toolbar`, `shell::navigation_rail`, `shell::status_bar`, `shell::command_palette`, `shell::help`, `shell::settings_dialog`, `shell::about_dialog`, `shell::report_issue`, `shell::theme_designer`, `shell::toasts`, `shell::chat_panel`, `shell::chat_canvas`, `shell::search_view`, `shell::log_view` |

Shell cards cannot draw the host's surfaces, because boards may not depend on
the host. Each links to the cards of the components the surface is built
from and, where the shell has a command for it, opens the real surface.

The GPUI Kit sections (Basic, Form, Layout and Advanced components, grouped as
[GPUI Kit's component index](https://gpui-kit.com/component) groups them,
with the extra components of its 0.9.1 story gallery) give every GPUI Kit
component a card named by its full path, such as
`gpui_kit::component::button::Button`. They are not listed here: a test
checks them against the public modules of gpui-component 0.9.1 instead. Each
card says whether Subspace uses the component outside the gallery, and the
board's **Hide unused** toolbar toggle hides the ones it does not.

`panel::frame` is not in the gallery: its resize handle saves the app-wide
panel width. A custom detail pane can use `panel::resizable_left_edge` with
the same width token. Open a detail panel in a list board to see it.

`workspace::Workspace` is not in the gallery for the same reason: dragging a
dock's edge stores the app-wide navigator or inspector width. Its parts have
the cards `workspace::Navigator`, `workspace::Toolbar`,
`workspace::dock_switch`, `workspace::Pane` and `workspace::Inspector`. Open
any Space to see them together, with its dock switches in the status bar.

The live `icon_button::IconButton` sample follows the global size and shows its
current name. In **Settings > Appearance**, choose **Small**, **Medium**,
**Large** or **Extra large** to view each size in turn. The gallery does not
override individual button sizes or change the preference.

### Gallery inconsistencies

These hand-rolled one-offs do not use a shared component. The gallery renders
the closest shared component beside each entry, not a copy of the board code.
The `inconsistencies::size_and_radius_strip` item draws plain replicas of the
measured heights, radii and padding side by side. These entries are
consolidation candidates. Migrate one when it changes; this list does not
authorize a mechanical replacement.

| Gallery name | Source | Closest shared component |
| --- | --- | --- |
| `github::mode_switch` | `spaces/github/src/detail.rs` | `segmented::control` |
| `github::check_run_link` | `spaces/github/src/detail/panel.rs` | `links::external` |
| `space-lab::row_actions` | `spaces/space-lab/src/project_settings/view.rs` | `segmented::control` |
| `inline-confirmations` | Web manager | `dialog::frame` with `dialog::footer` |
| `inconsistencies::size_and_radius_strip` | `spaces/component-gallery/src/samples/inconsistencies/strip.rs` | Replicas of measured values |
