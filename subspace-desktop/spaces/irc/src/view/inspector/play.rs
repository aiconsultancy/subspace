//! **In play here**: the issues and pull requests the conversation has
//! shown, from its tile owners (`crate::tiles`), in the order it first
//! showed them, each with its state as the chips and tiles draw it. A
//! reference awaiting its first answer says `Loading`. Unavailable issues
//! stay out of the list. Unavailable pull requests keep their row and say
//! `Unavailable`. The section asks for its references itself
//! ([`want`]): a tile's message may be out of view, where its row never
//! asked.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::BufferKey;
use subspace_framework::contracts::github::{
    PullRequestStatus, ResolutionState as PullRequestAnswer,
};
use subspace_framework::contracts::issues::{IssueState, ResolutionState as IssueAnswer};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, truncated, workspace};

use crate::Irc;
use crate::hub::Hub;
use crate::text::look;
use crate::tiles::TileRef;

/// The most references the section lists.
pub(crate) const MAX_IN_PLAY: usize = 8;

/// One reference in play.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PlayLine {
    /// The issue's identifier or the pull request's `owner/name#n`.
    pub key: String,
    /// Its title, once answered.
    pub title: Option<String>,
    pub mark: Mark,
    /// Whether it is answered, still to be, or cannot be shown.
    pub answer: Answer,
}

/// Where a reference's answer stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Known,
    /// Not answered yet: asked, or about to be.
    Loading,
    /// Cannot be shown: no access, signed out, or the read failed.
    Unavailable,
}

/// The words for an answer that is not known.
const LOADING: &str = "Loading";
const UNAVAILABLE: &str = "Unavailable";

/// A reference's state, once answered.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Mark {
    Issue(Option<IssueState>),
    PullRequest(Option<PullRequestStatus>),
}

impl PlayLine {
    /// The state in words: the issue's state name, or the pull request's;
    /// `Loading` or `Unavailable` while it has none.
    pub(crate) fn state(&self) -> Option<String> {
        let known = match &self.mark {
            Mark::Issue(state) => state.as_ref().map(|state| state.name.clone()),
            Mark::PullRequest(status) => status.map(|status| status_word(status).to_owned()),
        };
        match self.answer {
            Answer::Known => known,
            Answer::Loading => Some(LOADING.to_owned()),
            Answer::Unavailable => Some(UNAVAILABLE.to_owned()),
        }
    }
}

fn status_word(status: PullRequestStatus) -> &'static str {
    match status {
        PullRequestStatus::Open => "Open",
        PullRequestStatus::Draft => "Draft",
        PullRequestStatus::Merged => "Merged",
        _ => "Closed",
    }
}

/// What `buffer` has in play, as `hub` answers each reference now.
pub(crate) fn lines(hub: &Hub, buffer: &BufferKey) -> Vec<PlayLine> {
    hub.tiles
        .get(buffer)
        .in_play_first(MAX_IN_PLAY, |found| visible(hub, found))
        .into_iter()
        .map(|found| line(hub, found))
        .collect()
}

/// Asks for the answers of what the conversation in front has in play,
/// as a transcript row asks for its own (the message that keeps a tile
/// may be out of view), only while the section shows: the Space is in
/// front, the inspector shows, and it is on a ship's Ship tab. Nothing is
/// asked for an ordinary conversation, a Starbase or a hidden view, so no
/// GitHub or tracker quota goes on rows no one sees.
pub(crate) fn want(board: &Irc, cx: &mut gpui_kit::Context<Irc>) {
    let (Some(hub), Some(buffer)) = (board.hub.clone(), board.dock.focused.clone()) else {
        return;
    };
    if !shows(board, cx) {
        return;
    }
    let current = hub.read(cx);
    let tiles = current.tiles.get(&buffer);
    let shown = tiles.in_play_first(MAX_IN_PLAY, |found| visible(current, found));
    // Keep a bounded set of hidden issues eligible for the next resolution
    // round. They do not take a slot away from the visible references.
    let hidden = tiles.in_play_first(MAX_IN_PLAY, |found| !visible(current, found));
    let (issues, prs) = split(shown.into_iter().chain(hidden));
    hub.update(cx, |hub, cx| {
        crate::issues::want(hub, issues, cx);
        crate::pull_requests::want(hub, prs, cx);
    });
}

/// Whether **In play here** is on screen: the Space active, its inspector
/// shown on the subject tab, and the conversation in front a ship's
/// channel, the only subject whose tab lists it.
fn shows(board: &Irc, cx: &App) -> bool {
    board.active
        && board.dock.members_open()
        && board.inspector_tab == super::InspectorTab::Subject
        && super::front(board, cx)
            .is_some_and(|front| matches!(front.subject, super::Subject::Ship { .. }))
}

/// `found` as issue identifiers and pull request keys.
fn split(found: impl IntoIterator<Item = TileRef>) -> (Vec<String>, Vec<String>) {
    let mut issues = Vec::new();
    let mut prs = Vec::new();
    for found in found {
        match found {
            TileRef::Issue(identifier) => issues.push(identifier),
            TileRef::PullRequest(key) => prs.push(key),
        }
    }
    (issues, prs)
}

/// Where an answer stands: known, still to come, or not to be shown.
pub(super) fn standing<T>(answer: Option<&T>, known: bool, pending: bool) -> Answer {
    match (answer, known, pending) {
        (_, true, _) => Answer::Known,
        (None, _, _) | (Some(_), _, true) => Answer::Loading,
        _ => Answer::Unavailable,
    }
}

fn visible(hub: &Hub, found: &TileRef) -> bool {
    match found {
        TileRef::Issue(identifier) => !hub.issues.is_unavailable(identifier),
        TileRef::PullRequest(_) => true,
    }
}

fn line(hub: &Hub, found: TileRef) -> PlayLine {
    let (key, title, mark, answer) = match found {
        TileRef::Issue(identifier) => {
            let state = hub.issues.state(&identifier);
            let summary = match state {
                Some(IssueAnswer::Available(summary)) => Some(summary),
                _ => None,
            };
            let pending = matches!(state, Some(IssueAnswer::Pending));
            let answer = standing(state, summary.is_some(), pending);
            let title = summary.map(|summary| summary.title.clone());
            let mark = Mark::Issue(summary.map(|summary| summary.state.clone()));
            (identifier, title, mark, answer)
        }
        TileRef::PullRequest(key) => {
            let state = hub.pull_requests.state(&key);
            let details = match state {
                Some(PullRequestAnswer::Available(details)) => Some(details),
                _ => None,
            };
            let pending = matches!(state, Some(PullRequestAnswer::Pending));
            let answer = standing(state, details.is_some(), pending);
            let title = details.map(|details| details.title.clone());
            let mark = Mark::PullRequest(details.map(|details| details.status));
            (key, title, mark, answer)
        }
    };
    PlayLine {
        key,
        title,
        mark,
        answer,
    }
}

/// The section, when the conversation in front has anything in play.
pub(super) fn section(board: &Irc, cx: &App) -> Option<AnyElement> {
    let buffer = board.dock.focused.as_ref()?;
    let lines = lines(board.hub.as_ref()?.read(cx), buffer);
    if lines.is_empty() {
        return None;
    }
    let tokens = Tokens::current(cx);
    Some(
        workspace::section("In play here", cx)
            .child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .children(lines.iter().map(|line| row(line, &tokens))),
            )
            .into_any_element(),
    )
}

fn row(line: &PlayLine, tokens: &Tokens) -> AnyElement {
    let title = line.title.clone().unwrap_or_default();
    let state = line.state().unwrap_or_default();
    h_flex()
        .id(SharedString::from(format!(
            "irc-inspector-play:{}",
            line.key
        )))
        .test_support()
        .aria_label(format!("{} {title} {state}", line.key))
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .text_size(tokens.secondary_size())
        .children(mark(&line.mark, tokens))
        .child(
            div()
                .flex_shrink_0()
                .text_size(tokens.caption_size())
                .text_color(tokens.fg_strong)
                .child(line.key.clone()),
        )
        .child(
            truncated::text(format!("irc-inspector-play-title:{}", line.key), title)
                .flex_1()
                .text_color(tokens.fg),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(state),
        )
        .into_any_element()
}

/// An issue's state dot or a pull request's tinted glyph, as its chip
/// draws it.
fn mark(mark: &Mark, tokens: &Tokens) -> Option<AnyElement> {
    match mark {
        Mark::Issue(state) => look::issue_mark(state.as_ref()?, tokens),
        Mark::PullRequest(status) => {
            let tone = match status.map(look::pull_request) {
                Some(look::Look::Tint(tone)) => tokens.tone(tone),
                _ => tokens.muted,
            };
            Some(
                div()
                    .flex()
                    .text_color(tone)
                    .child(Icon::from(IconName::GitPullRequest).xsmall())
                    .into_any_element(),
            )
        }
    }
}
