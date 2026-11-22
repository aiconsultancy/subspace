//! The Starbase tab: the fleet's ships, one row each with its captain's
//! icon (the Icons setting), its name, its captain's state and its crew
//! meter. Only ships whose captain the roster knows show, matched by the
//! captain's API nick.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::avatar::{self, AgentIcons, AgentRank};
use subspace_framework::dashboard::{Tokens, feedback, truncated, workspace};

use super::Front;
use super::reason;
use crate::fleet::{Rank, Roster};
use crate::live::ShipLive;
use crate::live::ship::state_word;

/// The Starbase tab's body.
pub(super) fn body(front: &Front, cx: &App) -> AnyElement {
    let state = super::state_of(front.fleet.as_ref());
    let ships = front
        .fleet
        .as_ref()
        .and_then(|fleet| fleet.ships(|id| captain(&front.roster, id)));
    match (ships, reason::reason(&state)) {
        (Some(ships), _) => ships_section(&ships, cx),
        (None, Some((title, sentence))) => reason::view(title, sentence, cx),
        (None, None) => reason::view("Waiting", reason::WAITING.to_owned(), cx),
    }
}

/// Whether `id` is the API nick of a captain the roster knows.
pub(crate) fn captain(roster: &Roster, id: &str) -> bool {
    roster.rank(id) == Some(Rank::Captain)
}

/// **Ships**: one row per ship.
fn ships_section(ships: &[ShipLive], cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let section = workspace::section(format!("Ships · {}", ships.len()), cx);
    if ships.is_empty() {
        let none = "The fleet's Fleet API lists none of the roster's ships.";
        return section
            .child(feedback::empty_line(
                &tokens,
                "irc-inspector-ships-none",
                none,
            ))
            .into_any_element();
    }
    section
        .child(
            v_flex()
                .w_full()
                .gap_2()
                .children(ships.iter().map(|ship| ship_row(ship, cx))),
        )
        .into_any_element()
}

fn ship_row(ship: &ShipLive, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let id = SharedString::from(format!("irc-inspector-ship-row:{}", ship.id));
    let avatar = avatar::agent(
        format!("irc-inspector-ship-icon:{}", ship.id),
        AgentRank::Captain,
        ship.name.clone(),
    )
    .small();
    let icon = avatar::agent_icon(avatar, AgentIcons::current(cx));
    let meter = ship.crew.map(|(busy, people)| {
        avatar::crew_meter(
            format!("irc-inspector-ship-meter:{}", ship.id),
            busy,
            people,
        )
    });
    h_flex()
        .id(id)
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .text_size(tokens.secondary_size())
        .children(icon)
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.fg_strong)
                .child(ship.name.clone()),
        )
        .child(
            truncated::text(
                format!("irc-inspector-ship-state:{}", ship.id),
                state_word(ship.activity).unwrap_or_default().to_lowercase(),
            )
            .flex_1()
            .text_color(tokens.muted),
        )
        .children(meter)
        .into_any_element()
}
