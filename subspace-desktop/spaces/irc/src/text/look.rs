//! What an inline reference shows of its answer: an issue's state category
//! as a dot (a hollow ring before work starts), and a pull request's status
//! as its glyph's tint. A reference without an answer, still loading or
//! unresolved, shows neither, as before.

use std::sync::Arc;

use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    Styled as _, TestSupportExt as _, div,
};
use subspace_framework::contracts::github::PullRequestStatus;
use subspace_framework::contracts::issues::{IssueState, StateType};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone};

use super::Opens;
use crate::markup::Ref;

/// The dot's width as a share of the caption size, so it scales with the
/// text.
const DOT_SCALE: f32 = 0.55;

/// A reference's answered state, as its chip shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// An issue's state category: a filled dot in its tone.
    Dot(Tone),
    /// An issue whose work has not started: a hollow ring in its tone.
    Ring(Tone),
    /// A pull request's status: its glyph in this tone.
    Tint(Tone),
}

/// Reads a reference's look from the answers held now; `None` without one.
pub(crate) type RefLook = Arc<dyn Fn(&Ref, &App) -> Option<Look> + Send + Sync>;

/// An issue state's dot: started orange, in review blue, done green, not
/// started a hollow ring, cancelled or duplicate muted. The tracker's
/// started states share one category, so review is told by the state's
/// name.
pub(crate) fn issue(state: &IssueState) -> Look {
    match state.kind {
        StateType::Started if state.name.to_lowercase().contains("review") => Look::Dot(Tone::Blue),
        StateType::Started => Look::Dot(Tone::Orange),
        StateType::Completed => Look::Dot(Tone::Green),
        StateType::Canceled | StateType::Duplicate => Look::Dot(Tone::Muted),
        StateType::Unstarted => Look::Ring(Tone::Fg),
        _ => Look::Ring(Tone::Muted),
    }
}

/// A pull request's glyph tint: open blue, merged magenta, closed or draft
/// muted.
pub(crate) fn pull_request(status: PullRequestStatus) -> Look {
    match status {
        PullRequestStatus::Open => Look::Tint(Tone::Blue),
        PullRequestStatus::Merged => Look::Tint(Tone::Magenta),
        _ => Look::Tint(Tone::Muted),
    }
}

/// What `found` shows of its answer, read through `look`.
pub(super) fn of(look: &RefLook, found: &Opens, cx: &App) -> Option<Look> {
    match found {
        Opens::Ref(found) => look(found, cx),
        _ => None,
    }
}

/// A pull request chip's leading glyph, in its status's tone once known.
pub(super) fn glyph(found: &Opens, look: Option<Look>, tokens: &Tokens) -> Option<AnyElement> {
    let tint = tint(look).map(|tone| tokens.tone(tone));
    matches!(found, Opens::Ref(Ref::PullRequest(_))).then(|| {
        div()
            .flex()
            .when_some(tint, |glyph, tint| glyph.text_color(tint))
            .child(Icon::from(IconName::GitPullRequest).xsmall())
            .into_any_element()
    })
}

/// An answered issue reference's dot, identified by the reference's source
/// `range` for tests.
pub(super) fn state(
    look: Option<Look>,
    range: Option<std::ops::Range<usize>>,
    tokens: &Tokens,
) -> Option<AnyElement> {
    let dot = look.and_then(|look| dot(look, tokens))?;
    let id = SharedString::from(format!("irc-reference-state:{range:?}"));
    Some(
        div()
            .id(id)
            .test_support()
            .flex()
            .child(dot)
            .into_any_element(),
    )
}

/// The dot or ring an issue reference leads with.
fn dot(look: Look, tokens: &Tokens) -> Option<AnyElement> {
    let (tone, filled) = match look {
        Look::Dot(tone) => (tone, true),
        Look::Ring(tone) => (tone, false),
        Look::Tint(_) => return None,
    };
    let colour = tokens.tone(tone);
    Some(
        div()
            .flex_shrink_0()
            .size(tokens.caption_size() * DOT_SCALE)
            .rounded_full()
            .when(filled, |dot| dot.bg(colour))
            .when(!filled, |ring| ring.border_1().border_color(colour))
            .into_any_element(),
    )
}

/// The tone a pull request's glyph takes, when its status is known.
fn tint(look: Option<Look>) -> Option<Tone> {
    match look {
        Some(Look::Tint(tone)) => Some(tone),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::contracts::github::PullRequestStatus;
    use subspace_framework::contracts::issues::{IssueState, StateType};
    use subspace_framework::dashboard::{Tokens, Tone};

    use super::{Look, dot, glyph, issue, pull_request, state, tint};
    use crate::markup::{PrRef, Ref};
    use crate::text::Opens;

    #[test]
    fn an_issue_s_state_category_picks_its_dot() {
        let look = |name: &str, kind| issue(&IssueState::new(name, kind));
        assert_eq!(
            look("In progress", StateType::Started),
            Look::Dot(Tone::Orange)
        );
        assert_eq!(look("In Review", StateType::Started), Look::Dot(Tone::Blue));
        assert_eq!(look("Done", StateType::Completed), Look::Dot(Tone::Green));
        assert_eq!(look("Todo", StateType::Unstarted), Look::Ring(Tone::Fg));
        assert_eq!(look("Backlog", StateType::Backlog), Look::Ring(Tone::Muted));
        assert_eq!(
            look("Canceled", StateType::Canceled),
            Look::Dot(Tone::Muted)
        );
        assert_eq!(
            look("Duplicate", StateType::Duplicate),
            Look::Dot(Tone::Muted)
        );
    }

    #[test]
    fn a_pull_request_s_status_tints_its_glyph_and_draws_no_dot() {
        assert_eq!(
            pull_request(PullRequestStatus::Open),
            Look::Tint(Tone::Blue)
        );
        assert_eq!(
            pull_request(PullRequestStatus::Merged),
            Look::Tint(Tone::Magenta)
        );
        assert_eq!(
            pull_request(PullRequestStatus::Closed),
            Look::Tint(Tone::Muted)
        );
        let tokens = Tokens::one_dark();
        assert!(dot(Look::Tint(Tone::Blue), &tokens).is_none());
        assert!(dot(Look::Ring(Tone::Fg), &tokens).is_some());
        assert_eq!(tint(Some(Look::Tint(Tone::Blue))), Some(Tone::Blue));
        assert_eq!(tint(Some(Look::Dot(Tone::Green))), None);
        assert_eq!(
            tint(None),
            None,
            "an unanswered pull request keeps its look"
        );
    }

    #[test]
    fn only_a_pull_request_chip_leads_with_the_pull_request_glyph() {
        let tokens = Tokens::one_dark();
        let pull_request = Opens::Ref(Ref::PullRequest(
            PrRef::from_url("https://github.com/octo-org/api/pull/421").unwrap(),
        ));
        assert!(glyph(&pull_request, None, &tokens).is_some());
        let issue = Opens::Ref(Ref::Issue("TST-12".into()));
        assert!(glyph(&issue, Some(Look::Dot(Tone::Green)), &tokens).is_none());
        assert!(glyph(&Opens::Url("https://example.org".into()), None, &tokens).is_none());
        assert!(state(Some(Look::Dot(Tone::Green)), Some(0..6), &tokens).is_some());
        assert!(
            state(None, Some(0..6), &tokens).is_none(),
            "loading: no dot"
        );
    }
}
