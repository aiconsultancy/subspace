//! What a message row adds for its references: unavailable issue
//! references muted, the keyboard focus (a ring over the row, the focused
//! reference marked and an issue's card under the text), and the handlers
//! that open references and show issue tooltips.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Styled as _, div};
use subspace_framework::dashboard::{Tokens, anchored};

use super::RowContext;
use crate::issues;
use crate::markup::{Ref, Rendered};
use crate::pull_requests;
use crate::state::Entry;
use crate::style::{self, Styled};
use crate::text::RefHandlers;

/// `styled` with unavailable issue references and the focused reference
/// marked.
pub(super) fn marked(
    entry: &Entry,
    styled: Styled,
    rendered: Option<&Rendered>,
    context: &RowContext,
    cx: &App,
) -> Styled {
    let hub = context.hub.upgrade();
    let issues = hub.as_ref().map(|hub| &hub.read(cx).issues);
    let styled = style::with_unavailable(styled, |identifier| {
        issues.is_some_and(|issues| issues.is_unavailable(identifier))
    });
    let focused = focused_range(entry, &styled, rendered, context);
    style::with_focus(styled, focused)
}

/// The range of the message's focused link or reference, if it has one.
fn focused_range(
    entry: &Entry,
    styled: &Styled,
    rendered: Option<&Rendered>,
    context: &RowContext,
) -> Option<Range<usize>> {
    let at = context
        .focus
        .filter(|cursor| cursor.seq == entry.seq)?
        .reference?;
    super::super::targets::targets(styled, rendered)
        .get(at)
        .filter(|(range, _)| !range.is_empty())
        .map(|(range, _)| range.clone())
}

fn has_focus(entry: &Entry, context: &RowContext) -> bool {
    context.focus.is_some_and(|cursor| cursor.seq == entry.seq)
}

/// The focus ring over a focused message, drawn over the row so the text
/// does not move.
pub(super) fn focus_ring(
    entry: &Entry,
    context: &RowContext,
    tokens: &Tokens,
    cx: &App,
) -> Option<AnyElement> {
    has_focus(entry, context).then(|| {
        div()
            .absolute()
            .inset_0()
            .rounded(tokens.control_radius)
            .border_2()
            .border_color(cx.theme().ring)
            .into_any_element()
    })
}

/// The focused issue reference's card, under the message's text: an empty
/// line at the column's bottom anchors it there without moving the text.
pub(super) fn focus_card(entry: &Entry, context: &RowContext) -> Option<AnyElement> {
    let card = context.card.clone().filter(|_| has_focus(entry, context))?;
    let line = div().absolute().left_0().bottom_0().w_full().h_0();
    Some(line.child(anchored::floating(card)).into_any_element())
}

/// Reference handlers for styled text: opening through the thread-safe
/// claim, and an issue reference's tooltip.
pub(super) fn handlers(context: &RowContext, styled: Option<Styled>) -> RefHandlers {
    let claim = context.claim.clone();
    let hub = context.hub.clone();
    RefHandlers {
        open: claim,
        tooltip: Arc::new(move |found, window, cx| match found {
            Ref::Issue(identifier) => Some(issues::tooltip(identifier, hub.clone(), window, cx)),
            Ref::PullRequest(found) => Some(pull_requests::tooltip(found, hub.clone(), window, cx)),
            _ => None,
        }),
        styled,
    }
}
