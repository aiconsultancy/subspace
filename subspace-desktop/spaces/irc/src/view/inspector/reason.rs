//! Why a fleet channel's Ship or Starbase tab has no live figures, in one
//! plain sentence: the fleet client's own words for each `FleetState`
//! where it has them, and where the person can act in Settings, where.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_fleet::FleetState;
use subspace_framework::dashboard::Tokens;

/// Where the person saves a fleet's token.
pub(crate) const SETTINGS: &str = "Open Settings › Fleets to do it.";
/// While the fleet's data is on its way.
pub(crate) const WAITING: &str = "Live figures show once the fleet's Fleet API answers.";
/// A live fleet whose Fleet API has no ship for the channel's captain.
pub(crate) const NOT_LISTED: (&str, &str) = (
    "Not listed",
    "The fleet's Fleet API does not list this ship.",
);

/// A live fleet whose Fleet API sends only an overview it kept after a
/// failed read.
pub(crate) const KEPT: (&str, &str) = (
    "Not current",
    "The fleet's Fleet API could not read the fleet just now and sent what it last read. Ships show again once it reads the fleet.",
);
/// A live fleet whose first overview has not arrived.
pub(crate) const FIRST: (&str, &str) = ("Waiting", WAITING);

/// Why a live fleet shows no ship of its own: its data is kept, not
/// current; its first overview has not arrived; or else its Fleet API does
/// not list the ship. Never "Not listed" before a current overview says so.
pub(crate) fn unshown(fleet: Option<&crate::live::FleetLive>) -> (&'static str, &'static str) {
    match fleet {
        Some(fleet) if fleet.kept() => KEPT,
        Some(fleet) if fleet.listed() => NOT_LISTED,
        _ => FIRST,
    }
}

/// The tab's title and sentence for a fleet in `state`; `None` while its
/// data arrives.
pub(crate) fn reason(state: &FleetState) -> Option<(&'static str, String)> {
    if state.is_live() {
        return None;
    }
    let sentence = match (state, state.help()) {
        (FleetState::NoToken | FleetState::AddressChanged { .. }, Some(help)) => {
            format!("{help} {SETTINGS}")
        }
        (FleetState::Checking, _) => "Reading the fleet's saved token.".to_owned(),
        (_, Some(help)) => help.to_owned(),
        (_, None) => WAITING.to_owned(),
    };
    Some((state.label(), sentence))
}

/// The reason: its title in strong text, then the sentence, wrapping.
pub(super) fn view(title: &'static str, sentence: String, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    v_flex()
        .id("irc-inspector-reason")
        .role(Role::Note)
        .aria_label(format!("{title}: {sentence}"))
        .test_support()
        .w_full()
        .min_w_0()
        .gap_1()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(title),
        )
        .child(div().text_color(tokens.muted).child(sentence))
        .into_any_element()
}
