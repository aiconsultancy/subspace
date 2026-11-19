//! A pull request reference's tooltip. It reads the hub each time it draws,
//! so a `Pending` reference's loading line turns into its card when the
//! answer arrives, and a `changed` clears it back. It is a passive popover
//! with no menu or button. Without readable details it says only "Details
//! unavailable": never whether the pull request exists.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, ThemeStyled as _};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, WeakEntity, Window, div, rems,
};
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::dashboard::Tokens;

use crate::hub::Hub;
use crate::markup::PrRef;

/// What a chip without readable details says.
pub(crate) const UNAVAILABLE: &str = "Details unavailable";
/// A note wraps rather than stretching the tooltip.
const NOTE_WIDTH_REM: f32 = 20.;

struct PullRequestTooltip {
    found: PrRef,
    hub: WeakEntity<Hub>,
    _observation: Option<Subscription>,
}

/// The same passive view serves pointer hover and keyboard focus.
pub(crate) fn tooltip(
    found: &PrRef,
    hub: WeakEntity<Hub>,
    _: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| PullRequestTooltip {
        found: found.clone(),
        _observation: hub
            .upgrade()
            .map(|hub| cx.observe(&hub, |_, _, cx| cx.notify())),
        hub,
    })
    .into()
}

impl Render for PullRequestTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Laid out as its own root: the theme's body font, and Kit's tooltip
        // margin so the card clears the pointer.
        div()
            .id("irc-pr-tooltip")
            .test_support()
            .font_family(cx.theme().font_family.clone())
            .m_3()
            .child(card(&self.found, &self.hub, cx))
    }
}

fn card(found: &PrRef, hub: &WeakEntity<Hub>, cx: &mut App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let Some(hub) = hub.upgrade() else {
        return div().into_any_element();
    };
    let hub = hub.read(cx);
    match hub.pull_requests.state(&found.key) {
        Some(ResolutionState::Available(details)) => {
            super::tile::hover_card(details, found.label(), hub.now, &tokens, cx)
                .id("irc-pr-hover-card")
                .role(gpui_kit::Role::Tooltip)
                .test_support()
                .into_any_element()
        }
        state => {
            let note = note(found.label(), state);
            div()
                .id("irc-pr-note")
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

/// What the tooltip says when there are no details to show.
pub(crate) fn note(label: &str, state: Option<&ResolutionState>) -> String {
    match state {
        None | Some(ResolutionState::Pending) => format!("Loading {label}…"),
        Some(_) => UNAVAILABLE.to_owned(),
    }
}
