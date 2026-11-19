//! An issue reference's tooltip. The tooltip reads
//! the hub each time it draws, so a `Pending` reference's loading line turns
//! into its summary when the answer arrives. It is a plain popover surface
//! with no menu, so it never offers chat.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, ThemeStyled as _};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, WeakEntity, Window, div, rems,
};
use subspace_framework::contracts::issues::{ResolutionState, StateType};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone};

use crate::hub::Hub;

/// Names the tracker when the answering board gave no profile name.
const TRACKER: &str = "the issue tracker";
/// A note wraps rather than stretching the tooltip.
const NOTE_WIDTH_REM: f32 = 20.;

struct IssueTooltip {
    identifier: String,
    hub: WeakEntity<Hub>,
    _observation: Option<Subscription>,
}

/// The same passive view serves chips, inline references and keyboard focus.
pub(crate) fn tooltip(
    identifier: &str,
    hub: WeakEntity<Hub>,
    _: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| IssueTooltip {
        identifier: identifier.to_owned(),
        _observation: hub
            .upgrade()
            .map(|hub| cx.observe(&hub, |_, _, cx| cx.notify())),
        hub,
    })
    .into()
}

impl Render for IssueTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A tooltip is laid out as its own root: give it the theme's body font,
        // and Kit's tooltip margin so the card clears the pointer.
        div()
            .id("irc-issue-tooltip")
            .test_support()
            .font_family(cx.theme().font_family.clone())
            .m_3()
            .child(card(&self.identifier, &self.hub, cx))
    }
}

fn card(identifier: &str, hub: &WeakEntity<Hub>, cx: &mut App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let Some(hub) = hub.upgrade() else {
        return div().into_any_element();
    };
    let hub = hub.read(cx);
    let issues = &hub.issues;
    let profile = issues.profile_name().unwrap_or(TRACKER);
    match issues.state(identifier) {
        Some(ResolutionState::Available(summary)) => {
            super::tile::hover_card(summary, hub.now, &tokens, cx)
                .id("irc-issue-hover-card")
                .role(gpui_kit::Role::Tooltip)
                .test_support()
                .into_any_element()
        }
        state => {
            let note = note(identifier, state, profile);
            div()
                .id("irc-issue-note")
                .role(gpui_kit::Role::Tooltip)
                .popover_style(cx)
                .px_2()
                .py_1()
                .text_sm()
                .max_w(rems(NOTE_WIDTH_REM))
                .whitespace_normal()
                .child(note.clone())
                .test_text(note)
                .into_any_element()
        }
    }
}

/// What the tooltip says when there is no summary to show.
pub(crate) fn note(identifier: &str, state: Option<&ResolutionState>, profile: &str) -> String {
    match state {
        Some(ResolutionState::Unavailable) => {
            format!("{identifier} isn't available with {profile}.")
        }
        Some(ResolutionState::SignedOut) => format!("Sign in to {profile} to see this issue."),
        Some(ResolutionState::Failed) => format!("Couldn't reach {profile}."),
        _ => format!("Loading {identifier}…"),
    }
}

/// The glyph and tone of a workflow state, as the Issues board draws them.
pub(crate) fn state_glyph(kind: StateType) -> (IconName, Tone) {
    match kind {
        StateType::Triage => (IconName::CircleDotDashed, Tone::Orange),
        StateType::Backlog => (IconName::CircleDashed, Tone::Muted),
        StateType::Unstarted => (IconName::Circle, Tone::Fg),
        StateType::Started => (IconName::Contrast, Tone::Yellow),
        StateType::Completed => (IconName::CircleCheck, Tone::Accent),
        StateType::Canceled => (IconName::CircleX, Tone::Muted),
        StateType::Duplicate => (IconName::CircleSlash, Tone::Muted),
        _ => (IconName::Circle, Tone::Muted),
    }
}
