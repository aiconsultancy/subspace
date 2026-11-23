//! A line's menu acts on the line it was opened over, or on nothing.

use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render, SharedString,
    TestAppContext, Window, div,
};
use subspace_lucide::IconName;

use super::{BLOCKED_REASON, line_commands};
use crate::list::{Detail, ListDashboard, ListState};
use crate::row_menu::{GroupedMenu, MenuCommand, MenuGroup, MenuLine, RowMenu};
use crate::{Dashboard, DashboardEvent};

/// A list board whose write dialog can block navigation, with one command
/// of its own that names the selected line.
struct Guarded {
    focus: FocusHandle,
    list: ListState,
    navigable: bool,
}

impl EventEmitter<DashboardEvent> for Guarded {}

impl Focusable for Guarded {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Guarded {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Dashboard for Guarded {
    const ID: &'static str = "list-menu-tests";
    const TITLE: &'static str = "Guarded list";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            list: ListState::default(),
            navigable: true,
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

impl ListDashboard for Guarded {
    fn can_navigate(&self, _: &App) -> bool {
        self.navigable
    }

    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    fn detail(&self, _: &str) -> Option<Detail> {
        None
    }

    fn row_commands(board: &Entity<Self>, cx: &App) -> Vec<MenuCommand> {
        let selected = board
            .read(cx)
            .list()
            .selected()
            .cloned()
            .unwrap_or_default();
        vec![MenuCommand::new(
            MenuGroup::Act,
            format!("Archive {selected}"),
            IconName::Circle,
            |_, _| {},
        )]
    }
}

/// A board with `row-a` selected, then navigation blocked when `blocked`.
fn board(cx: &mut TestAppContext, blocked: bool) -> Entity<Guarded> {
    let window = cx.add_window(Guarded::new);
    let board = window.root(cx).unwrap();
    board.update(cx, |board, _| {
        board.list.select("row-a".into());
        board.navigable = !blocked;
    });
    board
}

fn facts() -> RowMenu {
    RowMenu::new("Second row").id("B-2").opens_details(true)
}

/// The menu for `key`, built as opening it builds it.
fn menu_for(
    cx: &mut TestAppContext,
    board: &Entity<Guarded>,
    key: &str,
    header: bool,
) -> GroupedMenu {
    let entity = board.downgrade();
    let key = SharedString::from(key);
    cx.update(|cx| line_commands(&entity, &key, &facts(), header, cx))
}

/// Whether the command labelled `label` is in the menu, and enabled.
fn command(menu: &GroupedMenu, label: &str) -> Option<bool> {
    menu.outline().into_iter().find_map(|line| match line {
        MenuLine::Command {
            label: shown,
            enabled,
            ..
        } if shown.as_str() == label => Some(enabled),
        _ => None,
    })
}

fn selected(cx: &mut TestAppContext, board: &Entity<Guarded>) -> Option<SharedString> {
    cx.update(|cx| board.read(cx).list().selected().cloned())
}

#[gpui_kit::test]
fn a_menu_selects_its_line_and_offers_the_boards_commands_for_it(cx: &mut TestAppContext) {
    let board = board(cx, false);
    let menu = menu_for(cx, &board, "row-b", false);
    assert_eq!(selected(cx, &board).as_deref(), Some("row-b"));
    assert_eq!(command(&menu, "Archive row-b"), Some(true));
    assert_eq!(command(&menu, "Open"), Some(true));
    assert_eq!(command(&menu, "Add to chat"), Some(true));
    assert!(menu.outline().contains(&MenuLine::Extension));
}

#[gpui_kit::test]
fn a_blocked_board_keeps_its_selection_and_the_menu_never_acts_on_it(cx: &mut TestAppContext) {
    let board = board(cx, true);
    let menu = menu_for(cx, &board, "row-b", false);
    assert_eq!(selected(cx, &board).as_deref(), Some("row-a"));
    assert_eq!(
        command(&menu, "Archive row-a"),
        None,
        "no command for the other line"
    );
    assert_eq!(command(&menu, "Archive row-b"), None);
    assert!(!menu.outline().contains(&MenuLine::Extension));
    assert_eq!(command(&menu, "Open"), Some(false));
    assert_eq!(command(&menu, "Add to chat"), Some(false));
    assert_eq!(
        command(&menu, "Copy title"),
        Some(true),
        "the line's own facts"
    );
    assert_eq!(command(&menu, "Copy id"), Some(true));
    assert!(
        menu.outline()
            .contains(&MenuLine::Reason(SharedString::new_static(BLOCKED_REASON)))
    );
}

#[gpui_kit::test]
fn a_blocked_header_menu_shows_fold_disabled(cx: &mut TestAppContext) {
    let board = board(cx, true);
    let menu = menu_for(cx, &board, "group-b", true);
    assert_eq!(selected(cx, &board).as_deref(), Some("row-a"));
    assert_eq!(command(&menu, "Fold group"), Some(false));
}
