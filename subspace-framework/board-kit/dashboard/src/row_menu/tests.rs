//! The standard commands and the seven groups' order.

use std::rc::Rc;

use gpui_kit::SharedString;
use subspace_lucide::IconName;

use super::{
    GroupedMenu, MenuChat, MenuCommand, MenuGroup, MenuLine, RowMenu, RowMenuItem, grouped,
    panel_menu_items, row_menu_items,
};
use crate::Callback;
use crate::list::{Glyph, Row};
use crate::tokens::Tone;

fn row() -> Row {
    Row {
        key: "octo/app#12".into(),
        icon: Glyph::new("●", Tone::Green),
        id: Some("#12".into()),
        title: "Port the board".into(),
        trailing: Vec::new(),
        done: false,
        indent: 0,
    }
}

fn command(group: MenuGroup, label: &'static str) -> MenuCommand {
    MenuCommand::new(group, label, IconName::Circle, |_, _| {})
}

/// An enabled, untinted line.
fn line(group: MenuGroup, label: &'static str) -> MenuLine {
    MenuLine::Command {
        group,
        label: SharedString::from(label),
        enabled: true,
        danger: false,
    }
}

/// The labels of a menu's command lines, in order.
fn labels(menu: &GroupedMenu) -> Vec<String> {
    menu.outline()
        .into_iter()
        .filter_map(|line| match line {
            MenuLine::Command { label, .. } => Some(label.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn groups_keep_their_order_whatever_order_commands_arrive_in() {
    let mut menu = GroupedMenu::new();
    for group in MenuGroup::ORDER.iter().rev() {
        menu.push(command(*group, "x"));
    }
    let groups: Vec<MenuGroup> = menu
        .outline()
        .into_iter()
        .filter_map(|line| match line {
            MenuLine::Command { group, .. } => Some(group),
            _ => None,
        })
        .collect();
    assert_eq!(groups, MenuGroup::ORDER);
}

#[test]
fn a_separator_falls_only_between_groups_that_have_commands() {
    let menu = GroupedMenu::new()
        .with(command(MenuGroup::Place, "Space settings…"))
        .with(command(MenuGroup::Open, "Open"))
        .with(command(MenuGroup::Open, "Open in GitHub"));
    assert_eq!(
        menu.outline(),
        [
            line(MenuGroup::Open, "Open"),
            line(MenuGroup::Open, "Open in GitHub"),
            MenuLine::Separator,
            line(MenuGroup::Place, "Space settings…"),
        ]
    );
    assert_eq!(GroupedMenu::new().outline(), []);
    assert!(GroupedMenu::new().is_empty());
}

#[test]
fn commands_in_one_group_keep_the_order_they_were_added_in() {
    let menu = GroupedMenu::new()
        .with(command(MenuGroup::Act, "Approve"))
        .with(command(MenuGroup::Copy, "Copy link"))
        .with(command(MenuGroup::Act, "Comment…"));
    assert_eq!(labels(&menu), ["Approve", "Comment…", "Copy link"]);
}

#[test]
fn a_disabled_command_stays_in_place_with_its_reason_under_it() {
    let menu = GroupedMenu::new()
        .with(command(MenuGroup::Place, "Space settings…"))
        .with(
            command(MenuGroup::Remove, "Close Space")
                .disabled(true, "The last open Space can't be closed."),
        );
    assert_eq!(
        menu.outline(),
        [
            MenuLine::Command {
                group: MenuGroup::Remove,
                label: "Close Space".into(),
                enabled: false,
                danger: false,
            },
            MenuLine::Reason("The last open Space can't be closed.".into()),
            MenuLine::Separator,
            line(MenuGroup::Place, "Space settings…"),
        ]
    );
    let enabled = command(MenuGroup::Remove, "Close Space").disabled(false, "unused");
    assert_eq!(enabled.reason(), None);
}

#[test]
fn only_enabled_remove_commands_are_tinted_as_danger() {
    let menu = GroupedMenu::new()
        .with(command(MenuGroup::Remove, "Leave channel"))
        .with(command(MenuGroup::Act, "Join"));
    let danger: Vec<bool> = menu
        .outline()
        .into_iter()
        .filter_map(|line| match line {
            MenuLine::Command { danger, .. } => Some(danger),
            _ => None,
        })
        .collect();
    assert_eq!(danger, [false, true]);
    assert!(MenuGroup::Remove.is_danger());
    assert!(
        MenuGroup::ORDER
            .iter()
            .filter(|group| group.is_danger())
            .eq([MenuGroup::Remove].iter())
    );
}

#[test]
fn a_plain_row_offers_open_its_copies_and_the_chat() {
    let items = row_menu_items(&RowMenu::from_row(&row()));
    assert_eq!(
        items,
        [
            RowMenuItem::OpenDetails,
            RowMenuItem::CopyTitle("Port the board".into()),
            RowMenuItem::CopyId("#12".into()),
            RowMenuItem::AddToChat,
        ]
    );
}

#[test]
fn every_standard_command_lands_in_its_group() {
    let open: Callback = Rc::new(|_, _| {});
    let facts = RowMenu::from_row(&row()).link("GitHub", "https://github.com/octo/app/pull/12");
    let menu = grouped(row_menu_items(&facts), Some(open));
    assert_eq!(
        menu.outline(),
        [
            line(MenuGroup::Open, "Open"),
            line(MenuGroup::Open, "Open in GitHub"),
            MenuLine::Separator,
            line(MenuGroup::Copy, "Copy link"),
            line(MenuGroup::Copy, "Copy title"),
            line(MenuGroup::Copy, "Copy id"),
            MenuLine::Separator,
            line(MenuGroup::Chat, "Add to chat"),
        ]
    );
}

#[test]
fn without_an_open_callback_there_is_no_open() {
    let menu = grouped(row_menu_items(&RowMenu::from_row(&row())), None);
    assert_eq!(labels(&menu), ["Copy title", "Copy id", "Add to chat"]);
}

#[test]
fn a_linked_row_offers_its_service_and_its_link() {
    let facts = RowMenu::from_row(&row()).link("GitHub", "https://github.com/octo/app/pull/12");
    let items = row_menu_items(&facts);
    assert!(items.contains(&RowMenuItem::OpenInBrowser {
        label: "Open in GitHub".into(),
        url: "https://github.com/octo/app/pull/12".into(),
    }));
    assert!(items.contains(&RowMenuItem::CopyLink(
        "https://github.com/octo/app/pull/12".into()
    )));
}

#[test]
fn only_browser_links_are_offered() {
    let facts = RowMenu::new("Row").link("GitHub", "javascript:alert(1)");
    assert_eq!(facts.url, None);
    let items = row_menu_items(&facts);
    assert!(!items.iter().any(|item| matches!(
        item,
        RowMenuItem::OpenInBrowser { .. } | RowMenuItem::CopyLink(_)
    )));
    let mail = RowMenu::new("Row").link("Mail", "mailto:user@example.com");
    assert_eq!(mail.url, None, "a mail link is not a page to open");
}

#[test]
fn a_row_without_details_or_text_still_offers_the_chat() {
    let facts = RowMenu::new("  ").opens_details(false);
    assert_eq!(row_menu_items(&facts), [RowMenuItem::AddToChat]);
}

#[test]
fn a_group_header_row_offers_what_it_has() {
    let facts = RowMenu::new("work/repo")
        .id("12 risks")
        .opens_details(false);
    assert_eq!(
        row_menu_items(&facts),
        [
            RowMenuItem::CopyTitle("work/repo".into()),
            RowMenuItem::CopyId("12 risks".into()),
            RowMenuItem::AddToChat,
        ]
    );
}

#[test]
fn the_panel_offers_the_selection_in_its_groups_and_never_its_own_details() {
    let facts = RowMenu::from_row(&row()).link("GitHub", "https://github.com/octo/app/pull/12");
    let items = panel_menu_items(&facts, " select 1 ");
    assert_eq!(
        items,
        [
            RowMenuItem::OpenInBrowser {
                label: "Open in GitHub".into(),
                url: "https://github.com/octo/app/pull/12".into(),
            },
            RowMenuItem::CopySelection,
            RowMenuItem::CopyLink("https://github.com/octo/app/pull/12".into()),
            RowMenuItem::CopyTitle("Port the board".into()),
            RowMenuItem::CopyId("#12".into()),
            RowMenuItem::AddToChat,
            RowMenuItem::AddSelectionToChat,
        ]
    );
    assert!(!items.contains(&RowMenuItem::OpenDetails));
}

#[test]
fn the_panel_without_a_selection_offers_no_selection_commands() {
    let items = panel_menu_items(&RowMenu::from_row(&row()), "   ");
    assert_eq!(items[0], RowMenuItem::CopyTitle("Port the board".into()));
    assert!(!items.contains(&RowMenuItem::CopySelection));
    assert!(!items.contains(&RowMenuItem::AddSelectionToChat));
}

#[test]
fn menus_offer_the_chat_unless_the_board_turns_it_off() {
    assert_eq!(RowMenu::from_row(&row()).chat, MenuChat::Offered);
    assert_eq!(RowMenu::default().chat, MenuChat::Offered);
    let facts = RowMenu {
        chat: MenuChat::Off,
        ..RowMenu::from_row(&row())
    };
    assert_eq!(
        row_menu_items(&facts),
        [
            RowMenuItem::OpenDetails,
            RowMenuItem::CopyTitle("Port the board".into()),
            RowMenuItem::CopyId("#12".into()),
        ]
    );
    let bare = RowMenu {
        chat: MenuChat::Off,
        ..RowMenu::new("  ").opens_details(false)
    };
    assert_eq!(row_menu_items(&bare), []);
}

#[test]
fn a_chat_free_panel_keeps_copy_and_leaves_out_both_chat_entries() {
    let facts = RowMenu {
        chat: MenuChat::Off,
        ..RowMenu::from_row(&row())
    };
    assert_eq!(
        panel_menu_items(&facts, "selected words"),
        [
            RowMenuItem::CopySelection,
            RowMenuItem::CopyTitle("Port the board".into()),
            RowMenuItem::CopyId("#12".into()),
        ]
    );
    assert_eq!(
        panel_menu_items(&facts, "  ")[0],
        RowMenuItem::CopyTitle("Port the board".into())
    );
}
