//! Group captions: none unless a board asks; drawn above their rows in
//! capitals; never selected, stepped onto or restored to; folded with their
//! group.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding, Render,
    Role, SharedString, TestAppContext, VisualTestContext, Window,
};

use super::{Detail, Glyph, Group, GroupCaption, ListDashboard, ListState, Row, render};
use crate::tokens::{Tokens, Tone};
use crate::{
    Activate, DASHBOARD_CONTEXT, Dashboard, DashboardEvent, SelectFirst, SelectLast, SelectNext,
    SelectPrevious,
};

/// A fleet group: two ships under "Ships", then one person under "People".
struct Fleet {
    focus: FocusHandle,
    list: ListState,
    captions: bool,
}

impl EventEmitter<DashboardEvent> for Fleet {}

impl Focusable for Fleet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Fleet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        render(self, window, cx)
    }
}

impl Dashboard for Fleet {
    const ID: &'static str = "list-caption-tests";
    const TITLE: &'static str = "Fleet";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            list: ListState::new(vec![fleet()]),
            captions: true,
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

impl ListDashboard for Fleet {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    fn detail(&self, _: &str) -> Option<Detail> {
        None
    }

    fn group_captions(&self, _: &Group, _: &App) -> Vec<GroupCaption> {
        if !self.captions {
            return Vec::new();
        }
        vec![
            GroupCaption::new("ship-a", "Ships"),
            GroupCaption::new("ada", "People"),
            GroupCaption::new("not-a-row", "Nowhere"),
        ]
    }
}

fn row(key: &'static str) -> Row {
    Row {
        key: key.into(),
        icon: Glyph::new("●", Tone::Green),
        id: None,
        title: key.into(),
        trailing: Vec::new(),
        done: false,
        indent: 0,
    }
}

fn fleet() -> Group {
    Group {
        key: "fleet".into(),
        icon: Glyph::new("◆", Tone::Blue),
        name: "Example fleet".into(),
        count: None,
        rows: vec![row("ship-a"), row("ship-b"), row("ada")],
    }
}

fn mount(cx: &mut TestAppContext) -> (Entity<Fleet>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let context = Some(DASHBOARD_CONTEXT);
        cx.bind_keys([
            KeyBinding::new("j", SelectNext, context),
            KeyBinding::new("down", SelectNext, context),
            KeyBinding::new("k", SelectPrevious, context),
            KeyBinding::new("up", SelectPrevious, context),
            KeyBinding::new("home", SelectFirst, context),
            KeyBinding::new("end", SelectLast, context),
            KeyBinding::new("enter", Activate, context),
        ]);
    });
    let (board, cx) = cx.add_window_view(Fleet::new);
    cx.update(|window, cx| {
        let focus = board.read(cx).focus.clone();
        focus.focus(window, cx);
        window.render_frame(cx);
    });
    (board, cx)
}

fn selected(board: &Entity<Fleet>, cx: &mut VisualTestContext) -> Option<SharedString> {
    cx.update(|_, cx| board.read(cx).list.selected().cloned())
}

/// Presses `key` and returns the selection after it.
fn press(board: &Entity<Fleet>, key: &str, cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        window.press(key, cx);
        window.render_frame(cx);
    });
    selected(board, cx)
        .map(|key| key.to_string())
        .unwrap_or_default()
}

#[gpui_kit::test]
fn a_board_without_the_hook_draws_no_caption(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    cx.update(|window, cx| {
        board.update(cx, |board, _| board.captions = false);
        window.render_frame(cx);
        assert!(window.try_find("list-caption:ship-a").is_none());
    });
}

#[gpui_kit::test]
fn a_caption_is_a_heading_above_its_row_in_capitals(cx: &mut TestAppContext) {
    let (_, cx) = mount(cx);
    cx.update(|window, cx| {
        let ships = window.find("list-caption:ship-a");
        assert_eq!(ships.role(), Some(Role::Heading));
        assert_eq!(ships.label(), Some("Ships"), "spoken as written");
        let people = window.find("list-caption:ada");
        let rows = Tokens::current(cx).row_height * 2.;
        assert!(
            people.bounds().top() - ships.bounds().bottom() >= rows,
            "the two ships sit between the captions"
        );
        assert!(
            window.try_find("list-caption:not-a-row").is_none(),
            "a caption for a row not in the group is not drawn"
        );
    });
}

#[gpui_kit::test]
fn the_list_keys_step_over_captions(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    let order: Vec<_> = ["j", "j", "j", "j", "j"]
        .into_iter()
        .map(|key| press(&board, key, cx))
        .collect();
    assert_eq!(order, ["fleet", "ship-a", "ship-b", "ada", "ada"]);
    let back: Vec<_> = ["k", "up", "up", "up"]
        .into_iter()
        .map(|key| press(&board, key, cx))
        .collect();
    assert_eq!(back, ["ship-b", "ship-a", "fleet", "fleet"]);
    assert_eq!(press(&board, "end", cx), "ada");
    assert_eq!(press(&board, "home", cx), "fleet");
}

#[gpui_kit::test]
fn a_caption_folds_with_its_group(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    assert_eq!(press(&board, "j", cx), "fleet");
    press(&board, "enter", cx);
    cx.update(|window, cx| {
        assert!(board.read(cx).list.is_folded("fleet"));
        assert!(window.try_find("list-caption:ship-a").is_none());
        assert!(window.try_find("list-caption:ada").is_none());
    });
}

#[gpui_kit::test]
fn new_groups_restore_only_rows_never_a_caption(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    cx.update(|_, cx| {
        board.update(cx, |board, _| {
            board.list.select("ship-b".into());
            let mut fewer = fleet();
            fewer.rows.retain(|row| row.key != "ship-b");
            board.list.set_groups(vec![fewer]);
            assert_eq!(board.list.selected(), None, "nothing takes its place");
            board.list.select("ada".into());
            board.list.set_groups(vec![fleet()]);
            assert_eq!(board.list.selected().map(AsRef::as_ref), Some("ada"));
            assert_eq!(
                board.list.visible_keys(),
                ["fleet", "ship-a", "ship-b", "ada"]
            );
        });
    });
}
