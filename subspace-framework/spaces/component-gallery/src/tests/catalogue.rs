//! The component rules, enforced against `docs/ui-components.md`:
//!
//! - every public module of `framework/board-kit/dashboard` is in the
//!   catalogue (the shared surfaces table) or in the table of entries
//!   without a card, with a reason;
//! - every catalogue entry has a gallery card, unless that table says why not;
//! - the gallery names tables list exactly the Subspace and shell cards;
//! - every public GPUI Kit 0.9.1 component module has a GPUI Kit card.

use std::collections::{BTreeSet, HashSet};

use crate::catalog::{Library, SECTIONS, items};

const CATALOGUE: &str = include_str!("../../../../docs/ui-components.md");
const DASHBOARD: &str = include_str!("../../../../board-kit/dashboard/src/lib.rs");

/// The public modules of gpui-component 0.9.1 (`src/lib.rs`). When GPUI Kit
/// is updated, refresh this list and give each new component a card.
const KIT_MODULES: [&str; 66] = [
    "accordion",
    "alert",
    "attachment",
    "avatar",
    "badge",
    "breadcrumb",
    "bubble",
    "button",
    "carousel",
    "chart",
    "checkbox",
    "clipboard",
    "collapsible",
    "color_picker",
    "combobox",
    "command",
    "description_list",
    "dialog",
    "diff",
    "dock",
    "empty",
    "form",
    "grid_layout",
    "group_box",
    "highlighter",
    "history",
    "hover_card",
    "input",
    "kbd",
    "label",
    "link",
    "list",
    "marker",
    "menu",
    "message",
    "message_scroller",
    "native_menu",
    "notification",
    "pagination",
    "plot",
    "popover",
    "progress",
    "questionnaire",
    "radio",
    "rating",
    "resizable",
    "scroll",
    "searchable_list",
    "select",
    "separator",
    "setting",
    "sheet",
    "shimmer",
    "sidebar",
    "skeleton",
    "slider",
    "speech",
    "spinner",
    "status_bar",
    "stepper",
    "switch",
    "tab",
    "table",
    "tag",
    "text",
    "theme",
];

/// gpui-component modules that hold infrastructure rather than a component.
const KIT_INFRASTRUCTURE: [(&str, &str); 4] = [
    ("global_state", "window-wide component state"),
    (
        "highlighter",
        "syntax colours, shown through the Editor and Diff cards",
    ),
    ("history", "undo and navigation history"),
    (
        "searchable_list",
        "the item model behind Select and Combobox",
    ),
];

/// The body rows of the first Markdown table in `text`, as trimmed cells.
fn first_table(text: &str) -> Vec<Vec<&str>> {
    text.lines()
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .skip(2)
        .map(|row| row.trim_matches('|').split('|').map(str::trim).collect())
        .collect()
}

/// The text of `text` from the heading `start` to the next heading of the
/// same or a higher level.
fn section<'a>(text: &'a str, start: &str) -> &'a str {
    let level = start.split(' ').next().unwrap_or_default();
    let line = format!("{start}\n");
    let at = if text.starts_with(&line) {
        Some(0)
    } else {
        text.find(&format!("\n{line}")).map(|at| at + 1)
    };
    let Some(at) = at else {
        return "";
    };
    let rest = &text[at + line.len()..];
    let end = rest
        .match_indices("\n#")
        .find(|(at, _)| {
            let hashes = rest[at + 1..].chars().take_while(|c| *c == '#').count();
            hashes <= level.len()
        })
        .map_or(rest.len(), |(at, _)| at);
    &rest[..end]
}

/// The backticked names in one cell, such as "`a`, `b`".
fn backticked(cell: &str) -> Vec<&str> {
    cell.split(',')
        .filter_map(|part| part.trim().strip_prefix('`')?.split('`').next())
        .collect()
}

/// Every backticked span in `text`.
fn spans(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

/// The entry a catalogue row names: the link text, without backticks.
fn entry(cell: &str) -> &str {
    let text = cell
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .map_or(cell, |(text, _)| text);
    text.trim_matches('`')
}

/// The gallery names that `docs/ui-components.md` lists: the second column of
/// the section table, then the first column of the inconsistencies table.
fn documented_gallery_names(catalogue: &str) -> Vec<&str> {
    let names = section(catalogue, "## Component gallery names");
    let (shared, inconsistencies) = names
        .split_once("### Gallery inconsistencies")
        .unwrap_or_default();
    let shared = first_table(shared)
        .into_iter()
        .flat_map(|cells| backticked(cells.get(1).copied().unwrap_or_default()));
    let one_offs = first_table(inconsistencies)
        .into_iter()
        .flat_map(|cells| backticked(cells.first().copied().unwrap_or_default()));
    shared.chain(one_offs).collect()
}

/// The entries of the shared surfaces table.
fn catalogue_entries(catalogue: &str) -> Vec<&str> {
    first_table(section(catalogue, "## Shared dashboard surfaces"))
        .into_iter()
        .filter_map(|cells| cells.first().map(|cell| entry(cell)))
        .collect()
}

/// The entries without a card, with their reasons.
fn without_card(catalogue: &str) -> Vec<(&str, &str)> {
    first_table(section(catalogue, "### Catalogue entries without a card"))
        .into_iter()
        .filter_map(|cells| Some((entry(cells.first()?), *cells.get(1)?)))
        .collect()
}

/// The `pub mod` names of a Rust source, outside `#[cfg(...)]` items.
fn public_modules(source: &str) -> Vec<&str> {
    let mut gated = false;
    let mut modules = Vec::new();
    for line in source.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix("pub mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            && !gated
        {
            modules.push(name);
        }
        gated = line.starts_with("#[cfg(");
    }
    modules
}

/// Whether a card's name refers to the catalogue entry `entry`.
fn covers(name: &str, entry: &str) -> bool {
    name == entry
        || name.starts_with(&format!("{entry}::"))
        || name.ends_with(&format!("::{entry}"))
}

fn shared_card_names() -> Vec<&'static str> {
    SECTIONS
        .iter()
        .filter(|section| section.library != Library::Kit)
        .flat_map(|section| section.items)
        .map(|item| item.name)
        .collect()
}

#[test]
fn the_table_readers_find_cells_entries_and_modules() {
    let catalogue = "## Component gallery names\n\nText.\n\n| Section | Gallery names |\n| --- | --- |\n| A | `a::b`, `C` |\n\n### Gallery inconsistencies\n\n| Gallery name | Source | Closest |\n| --- | --- | --- |\n| `x::y` | `src/x.rs`, more | `a::b` |\n\n## Next\n";
    assert_eq!(documented_gallery_names(catalogue), ["a::b", "C", "x::y"]);
    assert_eq!(entry("[`action`](../a.rs)"), "action");
    assert_eq!(entry("[Host chat canvas](../c.rs)"), "Host chat canvas");
    assert_eq!(
        public_modules("pub mod a;\n#[cfg(test)]\npub mod b;\nmod c;\npub mod d;"),
        ["a", "d"]
    );
    assert!(covers("action::icon_button", "action"));
    assert!(covers("icon_button::IconButton", "IconButton"));
    assert_eq!(
        spans("a `b::c`, then `d` and `e::f`."),
        ["b::c", "d", "e::f"]
    );
    assert!(!covers("actions::x", "action"));
}

/// The catalogue's gallery names tables list exactly the Subspace and shell cards.
#[test]
fn the_component_catalogue_lists_every_gallery_name() {
    let documented = documented_gallery_names(CATALOGUE);
    let unique: HashSet<_> = documented.iter().copied().collect();
    assert_eq!(
        unique.len(),
        documented.len(),
        "docs/ui-components.md repeats a gallery name: {documented:?}"
    );
    let shown: HashSet<_> = shared_card_names().into_iter().collect();
    let missing: BTreeSet<_> = shown.difference(&unique).collect();
    let extra: BTreeSet<_> = unique.difference(&shown).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "Add to docs/ui-components.md: {missing:?}; remove: {extra:?}"
    );
}

/// Every public dashboard module is catalogued, with a card or a reason.
#[test]
fn every_public_dashboard_module_is_in_the_catalogue() {
    let entries: HashSet<_> = catalogue_entries(CATALOGUE).into_iter().collect();
    let exempt: HashSet<_> = without_card(CATALOGUE)
        .into_iter()
        .map(|(entry, _)| entry)
        .collect();
    let catalogued = |module: &str| {
        entries
            .iter()
            .chain(&exempt)
            .any(|entry| *entry == module || entry.starts_with(&format!("{module}::")))
    };
    let missing: Vec<_> = public_modules(DASHBOARD)
        .into_iter()
        .filter(|module| !catalogued(module))
        .collect();
    assert!(
        missing.is_empty(),
        "List these modules in docs/ui-components.md's shared surfaces table, or under \
         \"Catalogue entries without a card\" with a reason: {missing:?}"
    );
}

/// Every catalogue entry has a gallery card, unless the catalogue says why not.
#[test]
fn every_catalogue_entry_has_a_gallery_card_or_a_reason() {
    let names = shared_card_names();
    let exempt = without_card(CATALOGUE);
    for (entry, reason) in &exempt {
        assert!(!reason.is_empty(), "{entry} has no reason");
        for named in spans(reason) {
            let is_card = items().any(|item| item.name == named) || !named.contains("::");
            assert!(is_card, "{entry} cites {named}, which is not a card");
        }
    }
    let exempt: HashSet<_> = exempt.into_iter().map(|(entry, _)| entry).collect();
    let uncovered: Vec<_> = catalogue_entries(CATALOGUE)
        .into_iter()
        .filter(|entry| !exempt.contains(entry))
        .filter(|entry| !names.iter().any(|name| covers(name, entry)))
        .collect();
    assert!(
        uncovered.is_empty(),
        "Give these catalogue entries a Component gallery card, or say why not under \
         \"Catalogue entries without a card\": {uncovered:?}"
    );
}

/// Every public GPUI Kit component module has a GPUI Kit card.
#[test]
fn every_gpui_kit_component_module_has_a_card() {
    let covered: HashSet<String> = items()
        .filter_map(|item| item.kit.as_ref())
        .flat_map(|kit| kit.paths.iter())
        .filter_map(|path| path.split("::").next())
        .map(str::to_owned)
        .collect();
    let infrastructure: HashSet<_> = KIT_INFRASTRUCTURE.iter().map(|(name, _)| *name).collect();
    let missing: Vec<_> = KIT_MODULES
        .iter()
        .filter(|module| !covered.contains(**module) && !infrastructure.contains(*module))
        .collect();
    assert!(
        missing.is_empty(),
        "GPUI Kit modules without a card: {missing:?}"
    );
}
