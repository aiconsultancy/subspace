//! The Ship tab: the ship's card (its captain's icon with the crew ring,
//! its name, the captain's model and state, and the figures the Fleet API
//! gives), its crew and what each is on, what is in play in the
//! conversation, and who is here now. Crew never speak in the channel, so
//! this list is the only place they show.

use chrono::{DateTime, Utc};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _,
};
use subspace_client_fleet::{FleetApi, model::Activity};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::avatar::{self, AgentIcons, AgentRank};
use subspace_framework::dashboard::badge::Pill;
use subspace_framework::dashboard::{Tokens, Tone, dates, feedback, truncated, workspace};

use super::reason;
use super::{Front, crew, play};
use crate::Irc;
use crate::live::ship::state_word;
use crate::live::{CrewLine, ShipLive};

/// The most people the Here now line names before it counts the rest.
const HERE_NAMED: usize = 4;

/// The Ship tab's body for the ship of `captain`, the roster's captain of
/// the channel in front.
pub(super) fn body(board: &Irc, front: &Front, captain: &str, cx: &mut Context<Irc>) -> AnyElement {
    let ship = front.fleet.as_ref().and_then(|fleet| fleet.ship(captain));
    let fleet = crate::live::fleet_id(&front.roster, &front.channel);
    if let Some(detail) = crew::body(board, fleet, captain, ship.as_ref(), cx) {
        return detail;
    }
    let hub = board.hub.as_ref().map(|hub| hub.read(cx));
    let now = hub.map_or_else(Utc::now, |hub| hub.now);
    let here = hub.map(|hub| here_now(board, hub)).unwrap_or_default();
    let route = ShipRoute {
        fleet,
        captain,
        focus: &board.inspector_crew_focus,
    };
    v_flex()
        .w_full()
        .min_w_0()
        .gap_4()
        .child(fleet_part(ship.as_ref(), front, route, now, cx))
        .children(play::section(board, cx))
        .children(here_section(&here, cx))
        .into_any_element()
}

struct ShipRoute<'a> {
    fleet: Option<&'a str>,
    captain: &'a str,
    focus: &'a crew::CrewFocus,
}

/// The card and the crew while the ship is live, else why not.
fn fleet_part(
    ship: Option<&ShipLive>,
    front: &Front,
    ShipRoute {
        fleet,
        captain,
        focus,
    }: ShipRoute<'_>,
    now: DateTime<Utc>,
    cx: &mut Context<Irc>,
) -> AnyElement {
    let state = super::state_of(front.fleet.as_ref());
    match (ship, reason::reason(&state)) {
        (Some(ship), _) => v_flex()
            .w_full()
            .min_w_0()
            .gap_4()
            .child(card(ship, captain, cx))
            .child(crew_section(ship, fleet, now, focus, cx))
            .into_any_element(),
        (None, Some((title, sentence))) => reason::view(title, sentence, cx),
        (None, None) => {
            let (title, sentence) = reason::unshown(front.fleet.as_ref());
            reason::view(title, sentence.to_owned(), cx)
        }
    }
}

/// The ship's header card: icon, name, `captain · model`, state, then the
/// figures.
fn card(ship: &ShipLive, captain: &str, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let avatar = avatar::agent(
        "irc-inspector-captain",
        AgentRank::Captain,
        captain.to_owned(),
    );
    let avatar = match ship.crew {
        Some((busy, people)) => avatar.crew(busy, people),
        None => avatar,
    };
    let icon = avatar::agent_icon(avatar, AgentIcons::current(cx));
    let pill = state_word(ship.activity)
        .map(|word| Pill::new("irc-inspector-state", word).hue(state_tone(ship.activity)));
    let head = h_flex()
        .id("irc-inspector-ship")
        .test_support()
        .w_full()
        .min_w_0()
        .gap_3()
        .items_center()
        .children(icon)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    truncated::text("irc-inspector-ship-name", ship.name.clone())
                        .text_size(tokens.row_font)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.fg_strong),
                )
                .child(
                    truncated::text("irc-inspector-ship-model", subtitle(ship))
                        .text_size(tokens.secondary_size())
                        .text_color(tokens.muted),
                ),
        )
        .children(pill);
    let facts = figures(ship)
        .into_iter()
        .map(|(label, value)| workspace::fact(label, value, cx));
    let unavailable = ship
        .unavailable
        .clone()
        .map(|reason| feedback::empty_line(&tokens, "irc-inspector-unavailable", reason));
    v_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .child(head)
        .children(facts)
        .children(unavailable)
        .into_any_element()
}

/// `captain · claude-opus-5-5`, or `captain` when the API names no model.
pub(crate) fn subtitle(ship: &ShipLive) -> String {
    match &ship.model {
        Some(model) => format!("captain · {model}"),
        None => "captain".to_owned(),
    }
}

/// The state pill's tones: green while the captain works, yellow while
/// its outcome is uncertain; muted otherwise.
const STATE_TONES: [(Activity, Tone); 2] = [
    (Activity::Running, Tone::Green),
    (Activity::Uncertain, Tone::Yellow),
];

pub(super) fn state_tone(activity: Activity) -> Tone {
    STATE_TONES
        .iter()
        .find(|(state, _)| *state == activity)
        .map_or(Tone::Muted, |(_, tone)| *tone)
}

/// The card's figures, each only when the API gives it: crew busy,
/// queued and done.
pub(crate) fn figures(ship: &ShipLive) -> Vec<(&'static str, String)> {
    let busy = ship
        .crew
        .map(|(busy, people)| ("Crew busy", format!("{busy} of {people}")));
    let queued = ship.queued.map(|queued| ("Queued", queued.to_string()));
    let done = ship.done.map(|done| ("Done", done.to_string()));
    [busy, queued, done].into_iter().flatten().collect()
}

/// **Crew**: each member, what they are on and since when.
fn crew_section(
    ship: &ShipLive,
    fleet: Option<&str>,
    now: DateTime<Utc>,
    focus: &crew::CrewFocus,
    cx: &mut Context<Irc>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let section = workspace::section("Crew", cx);
    if ship.members.is_empty() {
        let note = crew_note(ship);
        return section
            .child(feedback::empty_line(
                &tokens,
                "irc-inspector-crew-none",
                note,
            ))
            .into_any_element();
    }
    let fleet = fleet.filter(|_| FleetApi::current(cx).is_some());
    let members = crew::visible_members(ship, focus.show_all);
    let toggle = (ship.members.len() > crate::live::ship::MAX_CREW).then(|| {
        let label = if focus.show_all {
            "Show fewer members".to_owned()
        } else {
            format!("Show all {} members", ship.members.len())
        };
        action_button("irc-crew-toggle-roster", Intent::Quiet, None, label).on_click(cx.listener(
            |board, _, _, cx| {
                board.inspector_crew_focus.show_all = !board.inspector_crew_focus.show_all;
                cx.notify();
            },
        ))
    });
    section
        .children(toggle)
        .children(members.iter().map(|member| {
            let since = member.since.map(|at| age(at, now)).unwrap_or_default();
            crew::list::row(focus, member, (ship, fleet), since, cx)
        }))
        .into_any_element()
}

/// Why a ship shows no crew lines: its crew's detail is kept, not
/// current; it has not arrived; or the ship has no crew.
pub(crate) fn crew_note(ship: &ShipLive) -> &'static str {
    match ship.crew {
        Some((_, people)) if people > 0 && ship.crew_kept => {
            "The crew's detail is kept from an earlier read and not current, so it is not shown."
        }
        Some((_, people)) if people > 0 => "The crew's detail has not arrived yet.",
        _ => "This ship has no crew.",
    }
}

/// What a crew member is on: the task's phase and summary while working,
/// else `idle`.
pub(crate) fn work(member: &CrewLine) -> String {
    let parts: Vec<&str> = [member.phase.as_deref(), member.task.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    match (member.working, parts.is_empty()) {
        (true, false) => parts.join(" · "),
        (true, true) => "working".to_owned(),
        (false, _) => "idle".to_owned(),
    }
}

/// How long since `at` (epoch milliseconds), as rows say it: `35m`, `3h`.
pub(crate) fn age(at: i64, now: DateTime<Utc>) -> String {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|at| dates::age(&at.to_rfc3339(), now))
        .unwrap_or_default()
}

/// Who is in the channel in front, by nick, in the member list's order.
fn here_now(board: &Irc, hub: &crate::hub::Hub) -> Vec<String> {
    board
        .dock
        .focused
        .as_ref()
        .and_then(|buffer| hub.state.buffer(buffer))
        .map(|held| {
            held.members
                .iter()
                .map(|member| member.nick.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// **Here now**: who is in the channel, named up to [`HERE_NAMED`].
fn here_section(here: &[String], cx: &App) -> Option<AnyElement> {
    if here.is_empty() {
        return None;
    }
    let tokens = Tokens::current(cx);
    let faces = avatar::group(
        here.iter()
            .map(|nick| avatar::member(nick.clone(), &tokens)),
    );
    Some(
        workspace::section(format!("Here now · {}", here.len()), cx)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(faces)
                    .child(
                        truncated::text("irc-inspector-here", here_line(here))
                            .flex_1()
                            .text_size(tokens.secondary_size())
                            .text_color(tokens.muted),
                    ),
            )
            .into_any_element(),
    )
}

/// `aurora-lyra, mira, tavi and 2 others`.
pub(crate) fn here_line(here: &[String]) -> String {
    let named = here
        .iter()
        .take(HERE_NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    match here.len().saturating_sub(HERE_NAMED) {
        0 => named,
        1 => format!("{named} and 1 other"),
        rest => format!("{named} and {rest} others"),
    }
}
