//! The search field: typing reads the list again after a short pause, as
//! the Issues Space's search does.

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use crate::Decisions;
use crate::model;

/// Typing waits this long before the list is read again.
const SEARCH_PAUSE: Duration = Duration::from_millis(300);

/// The field's state, and the subscription that follows its typing.
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
    if matches!(event, InputEvent::Change) {
        commit_after_a_pause(decisions, cx);
    }
}

fn commit_after_a_pause(decisions: &mut Decisions, cx: &mut Context<Decisions>) {
    decisions._search_task = Some(cx.spawn(async move |this, cx| {
        cx.background_executor().timer(SEARCH_PAUSE).await;
        _ = this.update(cx, commit);
    }));
}

/// Reads the list for the field's text, when it changed; the list shows
/// again in place of an open decision.
pub(crate) fn commit(decisions: &mut Decisions, cx: &mut Context<Decisions>) {
    let text = model::search_text(&decisions.search.read(cx).value());
    if text == decisions.query {
        return;
    }
    decisions.query = text;
    decisions.read(false, cx);
    decisions.show_list(cx);
}
