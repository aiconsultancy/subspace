# Spec: a complete component gallery and clear component rules

## Rules

`docs/ui-components.md#rules` is canonical; AGENTS.md (Components), the pull
request checklist in `docs/ways-of-working.md` and the repository template,
and `docs/board-checklist.md` link or summarise it:

1. Use a GPUI Kit component as it ships, themed by tokens, through its shared
   Subspace treatment where the catalogue lists one.
2. If none fits, build a shared component in `framework/board-kit/dashboard`
   by composing GPUI Kit components.
3. Custom drawing only when no composition can do it, with the reason in the
   catalogue.
4. Everything visible is a component; no one-off styled UI.
5. Every shared component and shell surface has an interactive gallery card
   with its copyable name; a component is not done without it.

## Enforcement (`make check`, gallery crate tests)

| Check | Test |
| --- | --- |
| Every `pub mod` of the dashboard crate is catalogued or listed under "Catalogue entries without a card" | `tests::catalogue::every_public_dashboard_module_is_in_the_catalogue` |
| Every catalogue entry has a card, or a reason; card names cited in reasons exist | `every_catalogue_entry_has_a_gallery_card_or_a_reason` |
| The gallery names table lists exactly the Subspace and shell cards | `the_component_catalogue_lists_every_gallery_name` |
| Every public gpui-component 0.9.1 module has a GPUI Kit card | `every_gpui_kit_component_module_has_a_card` |
| Each GPUI Kit card's used mark matches imports and calls outside the gallery | `kit_usage::the_used_flags_match_the_source_outside_the_gallery` |
| "In Subspace, use" names a Subspace card; shell cards link only to cards | `every_preferred_treatment_is_a_subspace_card`, `shell_surfaces_link_to_gallery_cards` |

The usage scan expands `use gpui_kit::{component,base}::…` trees (braces,
`self`, globs, aliases) and inline paths in `boards`, `clients`,
`framework` and `products`, skipping the gallery, test and example code,
comments and `#[cfg(test)]` modules. A path with `(` is matched as text, for
components used through a method such as `.open_sheet(`.

## Gallery

- Navigation: GPUI Kit's `Sidebar`, grouped by library (Subspace components,
  Shell, GPUI Kit with a used count), one section shown at a time so only its
  samples are built. The chosen section is stored.
- Filter by name in the sidebar header: every word must be in the name,
  source or a GPUI Kit type path; at most 40 cards from all sections.
- `/` item search over every card; choosing one shows its section, scrolls to
  it and turns **Hide unused** off if it would hide it.
- Toolbar: **Hide unused** (`IconButton::latched`), off by default, stored;
  the status shows "N of M GPUI Kit components used".
- Card: name (monospace) with GPUI Kit's `Clipboard` copying the exact name;
  used or not used mark for GPUI Kit; source; note; **In Subspace, use** link
  to the shared treatment; one sample per state.
- New Subspace cards: messaging (`chat_input::ChatInput`, completion list,
  composer, chat bubble and message, sender and member avatars), navigation
  and chrome (glass capsule and control, overflow toolbar, surface header,
  tokens), inputs (form field, heading, note, field error, path input,
  textarea, secret field, reveal button), menus (`anchored::Anchor`, row menu,
  repository picker), lists (`list::render` over a fixture list).
- Shell surfaces: 15 cards linking to the cards they compose and opening the
  real surface through its shell action where one exists.
- GPUI Kit: 84 cards (37 basic, 13 form, 17 layout, 17 advanced) in GPUI
  Kit's documentation groups, plus the story gallery's extra components.
  Samples are interactive with fixture data; controlled values live in a
  gallery `Values` store. Popups go through `anchored` and switches through
  `toggle::switch`, as the architecture checks require.

## Out of scope

- Migrating existing direct controls in boards.
- `panel::frame` (its resize saves the app-wide width) and the live log view
  (real records) are linked, not drawn.
