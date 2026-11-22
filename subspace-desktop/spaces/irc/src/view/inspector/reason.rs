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
