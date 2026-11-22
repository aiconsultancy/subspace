//! The agent and external avatars and the crew meter, beside the sender
//! and member avatars.

use gpui_kit::{Context, Window};
use subspace_dashboard::avatar::AgentRank;
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
    _: &Tokens,
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
    .collect()
}
