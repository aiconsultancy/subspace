//! The conversation menu the toolbar leads with while the navigator is
//! hidden, so every network's buffers stay one click away.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{AnyElement, IntoElement as _, SharedString, WeakEntity, div};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Dashboard as _, Group, anchored};

use super::details::kind_icon;
use crate::{Irc, rows};

/// What the button says while no conversation is open.
const NONE_OPEN: &str = "Conversations";
const BUTTON_ID: &str = "irc-conversation-menu";
/// Units are measured when they first draw.
const UNMEASURED_WIDTH: f32 = 0.;

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

/// The lines of the menu for `board`, the conversation in front checked.
fn lines(board: &Irc) -> Vec<Entry> {
    let current = board.dock.focused.as_ref().map(rows::row_key);
    let held = |key: &SharedString| board.buffers.contains_key(key);
    entries(board.list.groups(), held, current.as_ref())
}

/// One line as a menu item: a network's name, or a conversation that shows
/// when chosen, as a search result does.
fn item(line: &Entry, owner: &WeakEntity<Irc>) -> PopupMenuItem {
    match line {
        Entry::Network(name) => PopupMenuItem::label(name.clone()),
        Entry::Conversation {
            key,
            title,
            current,
        } => {
            let (owner, key) = (owner.clone(), key.clone());
            PopupMenuItem::new(title.clone())
                .checked(*current)
                .on_click(move |_, window, cx| {
                    _ = owner.update(cx, |board, cx| board.open_search_result(&key, window, cx));
                })
        }
    }
}

/// The menu's button: it names the conversation in front and lists every
/// network's conversations.
fn button(board: &Irc) -> AnyElement {
    let focused = board.dock.focused.as_ref();
    let label = focused.map_or_else(|| NONE_OPEN.into(), |buffer| board.buffer_name(buffer));
    let icon = focused.map_or(IconName::Hash, |buffer| kind_icon(&buffer.target));
    let lines = lines(board);
    let owner = board.me.clone();
    let button = menu_button(BUTTON_ID, icon, label.clone(), false)
        .accessibility_label(format!("Conversation: {label}"));
    anchored::dropdown_menu(button, move |menu, _, _| {
        lines
            .iter()
            .fold(menu, |menu, line| menu.item(item(line, &owner)))
    })
    .into_any_element()
}

/// The menu as a unit of the toolbar's leading overflow: the button while
/// it fits, else its lines in the overflow menu.
pub(super) fn unit(board: &Irc) -> OverflowUnit {
    let owner = board.me.clone();
    let entries = lines(board)
        .into_iter()
        .map(|line| {
            let owner = owner.clone();
            PopupEntry::new(move || item(&line, &owner))
        })
        .collect();
    OverflowUnit::inline(
        BUTTON_ID,
        UNMEASURED_WIDTH,
        move |_, cx| {
            owner
                .upgrade()
                .map_or_else(|| div().into_any_element(), |board| button(board.read(cx)))
        },
        entries,
    )
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
