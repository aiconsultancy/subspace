use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, Context, IntoElement as _, ParentElement as _, TestAppContext, Window, div,
};
use subspace_framework::dashboard::test_support::ToolbarTestAppContext as _;

use super::{Confirmation, render};
use crate::{Actions, load::mapping, writes::Operation};

fn dialog(board: &mut Actions, _window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
    div().children(render(board, cx)).into_any_element()
}

#[gpui_kit::test]
fn busy_confirmation_cannot_be_dismissed_by_its_back_button(cx: &mut TestAppContext) {
    let (board, cx) = cx.mount_toolbar(700., dialog);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            let record = mapping::notice("octo/app", "test", "Run 42");
            let mut pending = Confirmation::new(Operation::Cancel, record, window, cx);
            pending.busy = true;
            board.pending = Some(pending);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.click("actions-confirm-cancel", cx);
    });
    board.read_with(cx, |board, _| {
        assert!(board.pending.as_ref().is_some_and(|pending| pending.busy))
    });
    board.update(cx, |board, cx| {
        board.pending.as_mut().unwrap().busy = false;
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.click("actions-confirm-cancel", cx);
    });
    board.read_with(cx, |board, _| assert!(board.pending.is_none()));
}
