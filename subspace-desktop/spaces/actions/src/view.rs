//! Text views use the shared list; Flow owns its interactive graph.

use gpui_kit::component::v_flex;
use gpui_kit::{
    App, Context, InteractiveElement as _, IntoElement, KeyContext, ParentElement as _, Render,
    Styled as _, Window, div,
};
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, Dashboard as _, Tokens, list};

use crate::{Actions, dialogs, keys, model::View, toolbar};

impl Render for Actions {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let content = if self.view == View::Flow {
            crate::panel::flow(self, window, cx)
        } else {
            list::render(self, window, cx).into_any_element()
        };
        // An element declares context names; `view_keys` is a binding
        // predicate and must not be parsed as one.
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(DASHBOARD_CONTEXT);
        context.add(Self::ID);
        keys::handlers(
            v_flex()
                .size_full()
                .relative()
                .key_context(context)
                .track_focus(&self.focus),
            cx,
        )
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(toolbar::render(self, cx))
        .child(div().flex_1().min_h_0().child(content))
        .children(self.menu_anchors.view())
        .children(dialogs::render(self, cx))
    }
}

pub(crate) fn open_selected(this: &Actions, cx: &mut App, copy: bool) {
    if let Some(record) = this.selected() {
        if copy {
            subspace_framework::dashboard::links::copy(record.link.clone(), cx);
        } else {
            subspace_framework::dashboard::links::open(&record.link, cx);
        }
    }
}

#[cfg(test)]
#[path = "view/notes_tests.rs"]
mod notes_tests;

#[cfg(test)]
mod tests {
    use gpui_kit::{AnyElement, Context, IntoElement as _, Render as _, TestAppContext, Window};
    use subspace_framework::dashboard::test_support::ToolbarTestAppContext as _;

    use crate::{Actions, model::View};

    fn root(board: &mut Actions, window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
        board.render(window, cx).into_any_element()
    }

    // The root once parsed a binding predicate as its key context, which
    // overflowed the stack on the first frame.
    #[gpui_kit::test]
    fn every_view_renders_its_root(cx: &mut TestAppContext) {
        let (board, cx) = cx.mount_toolbar(900., root);
        for view in [View::Runs, View::Deployments, View::Releases, View::Flow] {
            board.update(cx, |board, cx| {
                board.view = view;
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            board.read_with(cx, |board, _| assert_eq!(board.view, view));
        }
    }
}
