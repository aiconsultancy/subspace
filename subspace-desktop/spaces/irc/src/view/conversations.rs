//! The conversation menu the toolbar leads with while the navigator is
//! hidden, so every network's buffers stay one click away.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{AnyElement, IntoElement as _, SharedString};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Dashboard as _, Group, anchored};

use super::details::kind_icon;
use crate::{Irc, rows};

/// What the button says while no conversation is open.
const NONE_OPEN: &str = "Conversations";

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// A network's name, above its conversations.
    Network(SharedString),
    /// A conversation, by its row key; `current` marks the one in front.
    Conversation {
        key: SharedString,
        title: SharedString,
        current: bool,
    },
}

/// The menu's lines: each network, then the rows `held` accepts, in the
/// navigator's order. The server's other channels, which `held` refuses,
/// are joined from the navigator or with Join….
fn entries(
    groups: &[Group],
    held: impl Fn(&SharedString) -> bool,
    current: Option<&SharedString>,
) -> Vec<Entry> {
    let mut lines = Vec::new();
    for group in groups {
        lines.push(Entry::Network(group.name.clone()));
        let shown = group.rows.iter().filter(|row| held(&row.key));
        lines.extend(shown.map(|row| Entry::Conversation {
            key: row.key.clone(),
            title: row.title.clone(),
            current: current == Some(&row.key),
        }));
    }
    lines
}

/// The menu's button: it names the conversation in front and lists every
/// network's conversations. Choosing one shows it, as a search result does.
pub(super) fn button(board: &Irc) -> AnyElement {
    let focused = board.dock.focused.as_ref();
    let label = focused.map_or_else(|| NONE_OPEN.into(), |buffer| board.buffer_name(buffer));
    let icon = focused.map_or(IconName::Hash, |buffer| kind_icon(&buffer.target));
    let current = focused.map(rows::row_key);
    let held = |key: &SharedString| board.buffers.contains_key(key);
    let lines = entries(board.list.groups(), held, current.as_ref());
    let owner = board.me.clone();
    let button = menu_button("irc-conversation-menu", icon, label.clone(), false)
        .accessibility_label(format!("Conversation: {label}"));
    anchored::dropdown_menu(button, move |menu, _, _| {
        lines.iter().fold(menu, |menu, line| match line {
            Entry::Network(name) => menu.label(name.clone()),
            Entry::Conversation {
                key,
                title,
                current,
            } => {
                let (owner, key) = (owner.clone(), key.clone());
                let item = PopupMenuItem::new(title.clone()).checked(*current);
                menu.item(item.on_click(move |_, window, cx| {
                    _ = owner.update(cx, |board, cx| board.open_search_result(&key, window, cx));
                }))
            }
        })
    })
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui_kit::SharedString;
    use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

    use super::{Entry, entries};

    fn row(key: &'static str, title: &'static str) -> Row {
        Row {
            key: key.into(),
            icon: Glyph::new("#", Tone::Muted),
            id: None,
            title: title.into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        }
    }

    fn group(name: &'static str, rows: Vec<Row>) -> Group {
        Group {
            key: name.into(),
            icon: Glyph::new("●", Tone::Muted),
            name: name.into(),
            count: None,
            rows,
        }
    }

    fn conversation(key: &'static str, title: &'static str, current: bool) -> Entry {
        Entry::Conversation {
            key: key.into(),
            title: title.into(),
            current,
        }
    }

    #[test]
    fn the_menu_lists_each_network_s_conversations_and_marks_the_one_in_front() {
        let lab = vec![
            row("lab-s", "Server"),
            row("lab-ops", "#ops"),
            row("lab-listed", "#rust"),
        ];
        let groups = [
            group("Lab", lab),
            group("Home", vec![row("home-s", "Server")]),
        ];
        let current = SharedString::from("lab-ops");
        let held = |key: &SharedString| key.as_ref() != "lab-listed";
        assert_eq!(
            entries(&groups, held, Some(&current)),
            [
                Entry::Network("Lab".into()),
                conversation("lab-s", "Server", false),
                conversation("lab-ops", "#ops", true),
                Entry::Network("Home".into()),
                conversation("home-s", "Server", false),
            ],
            "a channel we are not in is left to the navigator"
        );
    }

    #[test]
    fn nothing_is_marked_while_no_conversation_is_open() {
        let groups = [group("Lab", vec![row("lab-s", "Server")])];
        assert_eq!(
            entries(&groups, |_| true, None),
            [
                Entry::Network("Lab".into()),
                conversation("lab-s", "Server", false),
            ]
        );
        assert!(entries(&[], |_| true, None).is_empty());
    }
}
