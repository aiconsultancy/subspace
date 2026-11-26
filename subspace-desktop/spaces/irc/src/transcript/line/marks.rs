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
use crate::hub::Hub;
use crate::issues;
use crate::markup::{Ref, Rendered};
use crate::pull_requests;
use crate::state::Entry;
use crate::style::{self, Styled};
use crate::text::{Look, RefHandlers, look};
use subspace_framework::contracts::github::ResolutionState as PullRequestAnswer;
use subspace_framework::contracts::issues::ResolutionState as IssueAnswer;

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
/// claim, an issue reference's tooltip, and the state a chip shows.
pub(super) fn handlers(context: &RowContext, styled: Option<Styled>) -> RefHandlers {
    let claim = context.claim.clone();
    let hub = context.hub.clone();
    let answers = context.hub.clone();
    RefHandlers {
        look: Arc::new(move |found, cx| {
            let hub = answers.upgrade()?;
            look_of(hub.read(cx), found)
        }),
        open: claim,
        tooltip: Arc::new(move |found, window, cx| match found {
            Ref::Decision(identifier) => {
                Some(crate::decision_references::tooltip(identifier, window, cx))
            }
            Ref::Issue(identifier) => Some(issues::tooltip(identifier, hub.clone(), window, cx)),
            Ref::PullRequest(found) => Some(pull_requests::tooltip(found, hub.clone(), window, cx)),
            _ => None,
        }),
        styled,
    }
}

/// The look of `found` from the answers `hub` holds: an available issue's
/// state or pull request's status; nothing while it loads or is unresolved.
fn look_of(hub: &Hub, found: &Ref) -> Option<Look> {
    match found {
        Ref::Issue(identifier) => match hub.issues.state(identifier)? {
            IssueAnswer::Available(summary) => Some(look::issue(&summary.state)),
            _ => None,
        },
        Ref::PullRequest(found) => match hub.pull_requests.state(&found.key)? {
            PullRequestAnswer::Available(details) => Some(look::pull_request(details.status)),
            _ => None,
        },
        _ => None,
    }
}
