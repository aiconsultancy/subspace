//! The palette's entries: in
//! commands mode the matching commands, then matching items; in item mode
//! (`/`) items only; in a sub-list its matching options. Before the user types,
//! the items are the dashboard's recent ones. Matching and ranking come from
//! `subspace_core::palette` and the dashboard's item index.

use gpui_kit::SharedString;
use subspace_core::palette::filter_commands;
use subspace_core::search::MAX_RESULTS;
use subspace_dashboard::{
    PaletteCommand, PaletteMode, PaletteOption, Row, Search, SearchDoc, SubList,
};

/// Stored labels of commands used most recently, newest first.
pub const RECENT_COMMANDS_KEY: &str = "palette.commands.recent";

/// One line of the palette.
pub enum Entry {
    Command(PaletteCommand),
    /// An item of the dashboard's search: its document id and its row.
    Item(SharedString, Row),
    Option(PaletteOption),
}

/// A titled run of entries.
pub struct Section {
    pub title: SharedString,
    pub entries: Vec<Entry>,
}

/// What the palette must list.
pub struct Request<'a> {
    pub query: &'a str,
    pub mode: PaletteMode,
    pub sub: Option<&'a SubList>,
    pub commands: &'a [PaletteCommand],
    pub recent_commands: &'a [String],
    pub search: Option<&'a Search>,
    pub recent: &'a [String],
}

/// What the palette lists for the request's query.
pub fn sections(request: &Request<'_>) -> Vec<Section> {
    let query = request.query.trim();
    match request.sub {
        Some(sub) => vec![sub_section(sub, query)],
        None => list_sections(request, query),
    }
}

/// A sub-list shows its own matching options and nothing else.
fn sub_section(sub: &SubList, query: &str) -> Section {
    let lower = query.to_lowercase();
    let entries = sub
        .options
        .iter()
        .filter(|option| lower.is_empty() || option.label.to_lowercase().contains(&lower))
        .cloned()
        .map(Entry::Option)
        .collect();
    Section {
        title: "".into(),
        entries,
    }
}

/// The commands, then the items, with the empty sections dropped.
fn list_sections(request: &Request<'_>, query: &str) -> Vec<Section> {
    let commands = (request.mode == PaletteMode::Commands)
        .then(|| command_section(request.commands, query, request.recent_commands))
        .into_iter();
    let items = request
        .search
        .map(|search| item_section(search, query, request.recent));
    commands
        .chain(items)
        .filter(|section| !section.entries.is_empty())
        .collect()
}

fn command_section(commands: &[PaletteCommand], query: &str, recent: &[String]) -> Section {
    let mut shown = filter_commands(commands, query, |command| {
        (command.label.as_ref(), command.keywords.as_ref())
    });
    if query.is_empty() {
        order_recent_commands(&mut shown, commands, recent);
    }
    Section {
        title: "Commands".into(),
        entries: shown
            .into_iter()
            .map(|index| Entry::Command(commands[index].clone()))
            .collect(),
    }
}

fn order_recent_commands(shown: &mut [usize], commands: &[PaletteCommand], recent: &[String]) {
    shown.sort_by_key(|index| {
        let command = &commands[*index];
        let label = command.label.as_ref();
        let key = command_recent_key(command);
        recent
            .iter()
            .position(|recent| recent == key || recent == label)
            .unwrap_or(recent.len())
    });
}

/// Stable stored identity for commands whose action label changes with
/// state: the command's own [`PaletteCommand::recent_key`], or a known
/// toggle's.
pub fn command_recent_key(command: &PaletteCommand) -> &str {
    if let Some(key) = &command.recent_key {
        return key;
    }
    match command.label.as_ref() {
        "Wrap titles" | "Truncate titles" => "toggle-wrap",
        "Open theme designer" | "Close theme designer" => "toggle-theme-designer",
        other => other,
    }
}

/// Before the user types, the dashboard's recent items; after, the matches.
fn item_section(search: &Search, query: &str, recent: &[String]) -> Section {
    let entries: Vec<Entry> = if query.is_empty() {
        recent
            .iter()
            .filter_map(|id| search.items.find(id))
            .take(MAX_RESULTS)
            .map(item_entry)
            .collect()
    } else {
        search
            .items
            .search(query, MAX_RESULTS)
            .into_iter()
            .map(item_entry)
            .collect()
    };
    let title = if query.is_empty() { "Recent" } else { "Items" };
    Section {
        title: title.into(),
        entries,
    }
}

fn item_entry((doc, row): (&SearchDoc, &Row)) -> Entry {
    Entry::Item(doc.id.clone().into(), row.clone())
}

/// The index of the option to highlight when a sub-list opens: the checked
/// one, else the first.
pub fn checked_index(sub: &SubList) -> usize {
    sub.options.iter().position(|o| o.checked).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use subspace_dashboard::{
        Glyph, ItemSearch, PaletteCommand, PaletteMode, PaletteOption, Row, Search, SearchDoc,
        SubList, Tone,
    };

    use super::{Entry, Request, sections};
    use crate::app::Reload;

    fn row(key: &str, title: &str) -> Row {
        Row {
            key: key.to_string().into(),
            icon: Glyph::new("■", Tone::Yellow),
            id: None,
            title: title.to_string().into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        }
    }

    fn search() -> Search {
        let items = [
            ("1", "Plan crew onboarding"),
            ("2", "Tidy the sample rules"),
        ]
        .map(|(id, title)| (SearchDoc::new(id, title), row(id, title)));
        Search {
            placeholder: "Search this board".into(),
            items: Rc::new(ItemSearch::new(items)),
        }
    }

    fn labels(sections: &[super::Section]) -> Vec<(String, Vec<String>)> {
        sections
            .iter()
            .map(|section| {
                let entries = section
                    .entries
                    .iter()
                    .map(|entry| match entry {
                        Entry::Command(command) => command.label.to_string(),
                        Entry::Item(id, _) => format!("item {id}"),
                        Entry::Option(option) => option.label.to_string(),
                    })
                    .collect();
                (section.title.to_string(), entries)
            })
            .collect()
    }

    #[test]
    fn commands_come_before_items_and_item_mode_hides_commands() {
        let commands = vec![
            PaletteCommand::new("Wrap titles", Reload).keywords("truncate"),
            PaletteCommand::new("Reload", Reload),
        ];
        let search = search();
        let found = labels(&sections(&Request {
            query: "onbo",
            mode: PaletteMode::Commands,
            sub: None,
            commands: &commands,
            recent_commands: &["Reload".into()],
            search: Some(&search),
            recent: &[],
        }));
        assert_eq!(found, [("Items".to_string(), vec!["item 1".to_string()])]);

        let found = labels(&sections(&Request {
            query: "wr",
            mode: PaletteMode::Commands,
            sub: None,
            commands: &commands,
            recent_commands: &[],
            search: Some(&search),
            recent: &[],
        }));
        assert_eq!(
            found[0],
            ("Commands".to_string(), vec!["Wrap titles".to_string()])
        );

        let found = labels(&sections(&Request {
            query: "",
            mode: PaletteMode::Items,
            sub: None,
            commands: &commands,
            recent_commands: &[],
            search: Some(&search),
            recent: &["2".into(), "gone".into()],
        }));
        assert_eq!(found, [("Recent".to_string(), vec!["item 2".to_string()])]);
    }

    #[test]
    fn a_command_with_a_changing_label_is_recent_by_its_stable_key() {
        let back = |label: &'static str| PaletteCommand {
            recent_key: Some("Back to issues".into()),
            ..PaletteCommand::new(label, Reload)
        };
        // What is stored names no channel or Space.
        assert_eq!(
            super::command_recent_key(&back("Back to #discovery")),
            "Back to issues"
        );
        assert_eq!(
            super::command_recent_key(&back("Back to PRs")),
            "Back to issues"
        );
        assert_eq!(
            super::command_recent_key(&PaletteCommand::new("Reload", Reload)),
            "Reload"
        );
        // Stored under the key, it still comes first under another label.
        let commands = vec![PaletteCommand::new("Reload", Reload), back("Back to #ops")];
        let empty = labels(&sections(&Request {
            query: "",
            mode: PaletteMode::Commands,
            sub: None,
            commands: &commands,
            recent_commands: &["Back to issues".into()],
            search: None,
            recent: &[],
        }));
        assert_eq!(
            empty[0].1,
            ["Back to #ops".to_string(), "Reload".to_string()]
        );
    }

    #[test]
    fn an_empty_query_puts_recent_commands_first_without_changing_search_ranking() {
        let commands = vec![
            PaletteCommand::new("Wrap titles", Reload).keywords("truncate"),
            PaletteCommand::new("Reload", Reload),
            PaletteCommand::new("Show logs", Reload),
        ];
        let empty = labels(&sections(&Request {
            query: "",
            mode: PaletteMode::Commands,
            sub: None,
            commands: &commands,
            recent_commands: &["Show logs".into(), "Reload".into()],
            search: None,
            recent: &[],
        }));
        assert_eq!(
            empty[0].1,
            [
                "Show logs".to_string(),
                "Reload".to_string(),
                "Wrap titles".to_string()
            ]
        );

        let searched = labels(&sections(&Request {
            query: "re",
            mode: PaletteMode::Commands,
            sub: None,
            commands: &commands,
            recent_commands: &["Show logs".into()],
            search: None,
            recent: &[],
        }));
        assert_eq!(
            searched[0].1,
            ["Reload".to_string(), "Wrap titles".to_string()]
        );

        let toggled = labels(&sections(&Request {
            query: "",
            mode: PaletteMode::Commands,
            sub: None,
            commands: &[PaletteCommand::new("Truncate titles", Reload)],
            recent_commands: &["toggle-wrap".into()],
            search: None,
            recent: &[],
        }));
        assert_eq!(toggled[0].1, ["Truncate titles".to_string()]);
    }

    #[test]
    fn a_sub_list_shows_its_matching_options_only() {
        let sub = SubList::new(
            "Pick a sprint",
            vec![
                PaletteOption::new("s1", "Sprint 41"),
                PaletteOption::new("s2", "Sprint 42").checked(true),
            ],
            |_, _, _| {},
        );
        let found = labels(&sections(&Request {
            query: "42",
            mode: PaletteMode::Commands,
            sub: Some(&sub),
            commands: &[],
            recent_commands: &[],
            search: None,
            recent: &[],
        }));
        assert_eq!(found, [(String::new(), vec!["Sprint 42".to_string()])]);
        assert_eq!(super::checked_index(&sub), 1);
    }
}
