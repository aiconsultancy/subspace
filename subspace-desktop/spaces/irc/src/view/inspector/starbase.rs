//! The Starbase tab: the fleet's ships, one row each with its captain's
//! icon (the Icons setting), its name, its captain's state and its crew
//! meter. Only ships whose captain the roster knows as one of this
//! Starbase's fleet show, matched by the captain's API nick: another
//! fleet's captain on the same network never shows here.

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
    let own = crate::live::fleet_id(&front.roster, &front.channel);
    let ships = front
        .fleet
        .as_ref()
        .and_then(|fleet| fleet.ships(|id| captain(&front.roster, own, id)));
    match (ships, reason::reason(&state)) {
        (Some(ships), _) => ships_section(&ships, cx),
        (None, Some((title, sentence))) => reason::view(title, sentence, cx),
        (None, None) => {
            let (title, sentence) = reason::unshown(front.fleet.as_ref());
            reason::view(title, sentence.to_owned(), cx)
        }
    }
}

/// Whether `id` is the API nick of a captain the roster knows whose ship
/// channel the roster places in fleet `fleet`.
fn captain(roster: &Roster, fleet: Option<&str>, id: &str) -> bool {
    let Some(agent) = roster.agent(id).filter(|agent| agent.rank == Rank::Captain) else {
        return false;
    };
    let own = agent
        .channel
        .as_deref()
        .and_then(|channel| crate::live::fleet_id(roster, channel));
    own.is_some() && own == fleet
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_api_client::model::MemberInitiative;

    use super::captain;
    use crate::fleet::Roster;

    /// An initiative of fleet `slug` on the shared network, with one captain
    /// per ship, nicked `nick`.
    fn initiative(slug: &str, ships: &[(&str, &str)]) -> serde_json::Value {
        let agents: Vec<_> = ships
            .iter()
            .map(|(ship, nick)| {
                json!({"userId": format!("user-{nick}"), "name": nick, "rank": "captain",
                    "shipId": format!("ship-{ship}"), "ship": ship, "ircNick": nick,
                    "available": true})
            })
            .collect();
        json!({"id": format!("initiative-{slug}"), "name": slug, "channels": "",
            "fleetId": slug,
            "fleet": {"id": slug, "slug": slug, "name": slug, "ircHost": "irc.example.test",
                "ircPort": 6697, "ircTls": true, "agents": agents}})
    }

    #[test]
    fn a_starbase_lists_only_its_own_fleet_s_captains() {
        let initiatives: Vec<MemberInitiative> = serde_json::from_value(json!([
            initiative("aurora", &[("lyra", "aurora-lyra")]),
            initiative("borealis", &[("vega", "borealis-vega")]),
        ]))
        .unwrap();
        let held: Vec<&MemberInitiative> = initiatives.iter().collect();
        let roster = Roster::for_initiatives(&held);
        let aurora = crate::live::fleet_id(&roster, "#aurora-starbase");
        assert_eq!(aurora, Some("aurora"));
        assert!(captain(&roster, aurora, "aurora-lyra"));
        assert!(captain(&roster, aurora, "AURORA-LYRA"), "any case");
        // Aurora's Fleet API may list a ship whose id is Borealis's captain.
        assert!(
            !captain(&roster, aurora, "borealis-vega"),
            "another fleet's"
        );
        assert!(captain(&roster, Some("borealis"), "borealis-vega"));
        assert!(!captain(&roster, None, "aurora-lyra"), "no fleet, no ships");
        assert!(
            !captain(&roster, aurora, "aurora-ghost"),
            "not in the roster"
        );
    }
}
