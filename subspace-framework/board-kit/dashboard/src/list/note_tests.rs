//! Group notes: none unless a board asks, and a spoken label per note.

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, TestAppContext,
    Window, div,
};

use super::{Detail, Glyph, Group, GroupNote, ListDashboard, ListState};
use crate::tokens::Tone;
use crate::{Dashboard, DashboardEvent};

/// A list board that overrides no optional hook.
struct Bare {
    focus: FocusHandle,
    list: ListState,
}

impl EventEmitter<DashboardEvent> for Bare {}

impl Focusable for Bare {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Bare {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Dashboard for Bare {
    const ID: &'static str = "list-note-tests";
    const TITLE: &'static str = "Bare list";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            list: ListState::default(),
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

impl ListDashboard for Bare {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    fn detail(&self, _: &str) -> Option<Detail> {
        None
    }
}

fn group() -> Group {
    Group {
        key: "acme/api".into(),
        icon: Glyph::new("◇", Tone::Muted),
        name: "acme/api".into(),
        count: None,
        rows: Vec::new(),
    }
}

#[gpui_kit::test]
fn a_board_without_the_hook_draws_no_group_note(cx: &mut TestAppContext) {
    let window = cx.add_window(Bare::new);
    window
        .update(cx, |board, _, cx| {
            assert_eq!(board.group_note(&group(), cx), None);
        })
        .unwrap();
}

#[test]
fn a_note_is_spoken_with_its_group_in_lower_case() {
    assert_eq!(
        GroupNote::Loading("Loading runs…".into()).label("acme/api"),
        "acme/api: loading runs"
    );
    assert_eq!(
        GroupNote::Empty("No failed runs in the latest runs.".into()).label("acme/api"),
        "acme/api: no failed runs in the latest runs"
    );
    assert_eq!(GroupNote::Empty("".into()).label("acme/api"), "acme/api: ");
}
