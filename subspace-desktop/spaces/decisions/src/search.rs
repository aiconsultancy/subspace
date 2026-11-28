//! Debounced decision search. Keep the visible rows while the latest
//! request is running, and discard any response superseded by a keystroke.

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use crate::Decisions;

const SEARCH_PAUSE: Duration = Duration::from_millis(200);

pub(crate) fn input(
    window: &mut Window,
    cx: &mut Context<Decisions>,
) -> (Entity<InputState>, Subscription) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search decisions"));
    let typing = cx.subscribe(&input, changed);
    (input, typing)
}

fn changed(
    decisions: &mut Decisions,
    _: Entity<InputState>,
    event: &InputEvent,
    cx: &mut Context<Decisions>,
) {
    if matches!(event, InputEvent::Change) && decisions.capture_query(cx) {
        decisions._search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_PAUSE).await;
            _ = this.update(cx, commit);
        }));
        cx.notify();
    }
}

pub(crate) fn commit(decisions: &mut Decisions, cx: &mut Context<Decisions>) {
    if decisions.capture_query(cx) || decisions.search_pending() {
        decisions.read(false, cx);
        decisions.show_list(cx);
    }
}
