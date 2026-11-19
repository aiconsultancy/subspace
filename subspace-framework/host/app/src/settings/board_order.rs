//! Domain-ID reordering shared by native drag/drop and keyboard move buttons.

use gpui_kit::{
    Context, EntityId, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window,
    div,
};
use subspace_dashboard::Tokens;

use super::Settings;

#[derive(Clone)]
pub(crate) struct BoardDrag {
    pub(crate) owner: EntityId,
    pub(crate) id: &'static str,
    pub(crate) title: SharedString,
}

pub(crate) struct DragPreview(pub(crate) SharedString);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        div()
            .px_3()
            .py_2()
            .bg(tokens.pane)
            .border_1()
            .border_color(tokens.line)
            .text_color(tokens.fg)
            .child(self.0.clone())
    }
}

impl Settings {
    pub(crate) fn drop_board(&mut self, drag: &BoardDrag, target: &str, cx: &mut Context<Self>) {
        if drag.owner != cx.entity_id() || drag.id == target {
            return;
        }
        let source = self.boards.iter().position(|board| board.id == drag.id);
        let target = self.boards.iter().position(|board| board.id == target);
        if let (Some(source), Some(target)) = (source, target) {
            move_to(&mut self.boards, source, target);
            cx.notify();
        }
    }
}

/// Move to the target's position, shifting the intervening items by one.
pub(crate) fn move_to<T>(order: &mut [T], source: usize, target: usize) {
    if source < target {
        order[source..=target].rotate_left(1);
    } else if source > target {
        order[target..=source].rotate_right(1);
    }
}
