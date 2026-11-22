//! The agent and external avatars and the crew meter, beside the sender
//! and member avatars.

use gpui_kit::{Context, Window};
use subspace_dashboard::avatar::{AgentIcons, AgentRank};
use subspace_dashboard::{Tokens, avatar};

use super::PEOPLE;
use crate::ComponentGallery;
use crate::samples::{Sample, sample};

pub(crate) fn agents(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Captain",
            avatar::agent("gallery-sample:agent-captain", AgentRank::Captain, "Lyra"),
        ),
        sample(
            "Rear admiral",
            avatar::agent(
                "gallery-sample:agent-admiral",
                AgentRank::RearAdmiral,
                "Vega",
            ),
        ),
        sample(
            "Small, for list rows",
            avatar::agent("gallery-sample:agent-small", AgentRank::Captain, "Lyra").small(),
        ),
        sample(
            "Crew ring: 3 of 4 busy",
            avatar::agent("gallery-sample:agent-crew", AgentRank::Captain, "Lyra").crew(3, 4),
        ),
    ]
    .into_iter()
    .chain(icons_small())
    .collect()
}

/// The small icon `avatar::agent_icon` gives under the Icons setting's
/// Small, the size of a row's leading glyph.
fn icons_small() -> Option<Sample> {
    let lyra = avatar::agent("gallery-sample:agent-mark", AgentRank::Captain, "Lyra").small();
    avatar::agent_icon(lyra, AgentIcons::Small)
        .map(|icon| sample("Icons: Small, where a row's # sits", icon))
}

pub(crate) fn externals(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Dashed initials",
        avatar::external("gallery-sample:external", PEOPLE[1]),
    )]
}

pub(crate) fn crew_meters(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    [
        ("3 of 4 busy", 3, 4),
        ("Idle", 0, 4),
        ("Too large for dots", 7, 12),
    ]
    .into_iter()
    .map(|(state, busy, total)| {
        sample(
            state,
            avatar::crew_meter(format!("gallery-sample:crew:{state}"), busy, total),
        )
    })
    .chain([true, false].map(|busy| activity_dot(busy, tokens)))
    .collect()
}

/// `avatar::activity_dot` alone: a ship working or a person here, filled;
/// otherwise a ring.
fn activity_dot(busy: bool, tokens: &Tokens) -> Sample {
    let state = if busy {
        "Activity dot: on"
    } else {
        "Activity dot: off"
    };
    sample(state, avatar::activity_dot(busy, tokens))
}
