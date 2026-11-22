//! The composer's completion list: after each edit it offers what the word
//! at the cursor could become ([`super::suggest`]), and the chat input's
//! keys or a click choose from it.

use gpui_kit::{App, Context, Entity, Window};
use subspace_framework::dashboard::chat_input::completion::{CompletionEvent, CompletionList};

use super::suggest::{Facts, Offer, suggest};
use crate::conversation::Conversation;

/// Offers the choices for the text as it is now, or closes the list.
pub(super) fn refresh(conversation: &mut Conversation, cx: &mut Context<Conversation>) {
    let offer = current(conversation, cx);
    show(conversation, offer, cx);
}

fn current(conversation: &Conversation, cx: &App) -> Option<Offer> {
    let input = conversation.draft.entity.read(cx);
    let (text, cursor) = (input.text(cx), input.cursor(cx));
    let facts = Facts::gather(&conversation.hub.read(cx).state, &conversation.buffer)
        .ranked(&conversation.roster)
        .asking(super::ask::agent(
            &conversation.roster,
            &conversation.buffer,
        ))
        .tracking(crate::tracker::facts(conversation, typed_issue(&text), cx));
    suggest(&text, cursor, &facts)
}

/// The issue a tracker command in `text` names, so its answer is gathered.
fn typed_issue(text: &str) -> Option<String> {
    crate::tracker::typed(text).and_then(|typed| typed.issue)
}

fn show(conversation: &mut Conversation, offer: Option<Offer>, cx: &mut Context<Conversation>) {
    let items = offer.as_ref().map_or_else(Vec::new, |offer| {
        offer
            .choices
            .iter()
            .map(|choice| choice.item.clone())
            .collect()
    });
    let open = !items.is_empty();
    let draft = &mut conversation.draft;
    draft.offer = offer;
    draft.list.update(cx, |list, cx| list.show(items, cx));
    draft
        .entity
        .update(cx, |input, cx| input.set_completing(open, cx));
}

pub(super) fn is_open(conversation: &Conversation) -> bool {
    conversation.draft.offer.is_some()
}

pub(super) fn step(conversation: &Conversation, step: isize, cx: &mut App) {
    conversation
        .draft
        .list
        .update(cx, |list, cx| list.step(step, cx));
}

pub(super) fn close(conversation: &mut Conversation, cx: &mut Context<Conversation>) {
    show(conversation, None, cx);
}

/// Replaces the word with the highlighted choice, or the one at `index`.
pub(super) fn accept(
    conversation: &mut Conversation,
    index: Option<usize>,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let draft = &conversation.draft;
    let Some(index) = index.or_else(|| draft.list.read(cx).active()) else {
        return;
    };
    let Some((range, insert, run)) = draft.offer.as_ref().and_then(|offer| {
        let choice = offer.choices.get(index)?;
        Some((offer.range.clone(), choice.insert.clone(), choice.run))
    }) else {
        return;
    };
    let input = draft.entity.clone();
    show(conversation, None, cx);
    if run {
        let text = input.read(cx).text(cx);
        super::send::submit(conversation, text, window, cx);
        return;
    }
    input.update(cx, |input, cx| {
        input.replace_range(range, &insert, window, cx);
        input.focus(window, cx);
    });
}

/// A click on a row of the list.
pub(super) fn clicked(
    conversation: &mut Conversation,
    _: &Entity<CompletionList>,
    event: &CompletionEvent,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    match event {
        CompletionEvent::Accept(index) => accept(conversation, Some(*index), window, cx),
    }
}
