//! **In play here**: the issues and pull requests the conversation has
//! shown, from its tile owners (`crate::tiles`), in the order it first
//! showed them, each with its state as the chips and tiles draw it.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    Styled as _, TestSupportExt as _, div,
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
}

/// A reference's state, once answered.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Mark {
    Issue(Option<IssueState>),
    PullRequest(Option<PullRequestStatus>),
}

impl PlayLine {
    /// The state in words: the issue's state name, or the pull request's.
    pub(crate) fn state(&self) -> Option<String> {
        match &self.mark {
            Mark::Issue(state) => state.as_ref().map(|state| state.name.clone()),
            Mark::PullRequest(status) => status.map(|status| status_word(status).to_owned()),
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
        .in_play()
        .into_iter()
        .take(MAX_IN_PLAY)
        .map(|found| line(hub, found))
        .collect()
}

fn line(hub: &Hub, found: TileRef) -> PlayLine {
    let (key, title, mark) = match found {
        TileRef::Issue(identifier) => {
            let summary = match hub.issues.state(&identifier) {
                Some(IssueAnswer::Available(summary)) => Some(summary),
                _ => None,
            };
            let title = summary.map(|summary| summary.title.clone());
            let mark = Mark::Issue(summary.map(|summary| summary.state.clone()));
            (identifier, title, mark)
        }
        TileRef::PullRequest(key) => {
            let details = match hub.pull_requests.state(&key) {
                Some(PullRequestAnswer::Available(details)) => Some(details),
                _ => None,
            };
            let title = details.map(|details| details.title.clone());
            let mark = Mark::PullRequest(details.map(|details| details.status));
            (key, title, mark)
        }
    };
    PlayLine { key, title, mark }
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
                v_flex().w_full().gap_2().children(
                    lines
                        .iter()
                        .enumerate()
                        .map(|(index, line)| row(index, line, &tokens)),
                ),
            )
            .into_any_element(),
    )
}

fn row(index: usize, line: &PlayLine, tokens: &Tokens) -> AnyElement {
    let title = line.title.clone().unwrap_or_default();
    h_flex()
        .id(SharedString::from(format!("irc-inspector-play:{index}")))
        .test_support()
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
            truncated::text(format!("irc-inspector-play-title:{index}"), title)
                .flex_1()
                .text_color(tokens.fg),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(line.state().unwrap_or_default()),
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
