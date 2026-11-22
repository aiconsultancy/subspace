//! A fleet agent's avatar: a circle in its rank's tone with a ring and the
//! rank's glyph, beside the square initials of a person.

use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::{Icon, Sizable as _, Size};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};
use subspace_lucide::IconName;

use super::{MEMBER_SIZE_PX, SENDER_SIZE_PX};
use crate::{Tokens, Tone};

/// The fill inside the ring: the rank's tone, faint, so the glyph reads on
/// any surface.
const FILL: f32 = 0.14;
/// The glyph's share of the avatar's side.
const GLYPH_SHARE: f32 = 0.5;
/// The face's share of the side when a crew ring surrounds it, leaving room
/// for Kit's ring stroke.
const RINGED_SHARE: f32 = 0.62;
const PERCENT: f32 = 100.;
/// The progress of a crew of none.
const NO_CREW: f32 = 0.;

/// The rank a fleet agent's avatar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AgentRank {
    /// The agent that coordinates a fleet's captains: a diamond in the
    /// accent.
    RearAdmiral,
    /// An agent that commands a ship: a navigation arrow in blue.
    Captain,
}

impl AgentRank {
    /// The rank's theme tone, for its avatar and any mark beside its name.
    pub fn tone(self) -> Tone {
        match self {
            Self::RearAdmiral => Tone::Accent,
            Self::Captain => Tone::Blue,
        }
    }

    /// The rank's glyph inside the avatar.
    pub fn glyph(self) -> IconName {
        match self {
            Self::RearAdmiral => IconName::Diamond,
            Self::Captain => IconName::Navigation2,
        }
    }

    /// The rank in words, for the avatar's accessible name.
    pub fn title(self) -> &'static str {
        match self {
            Self::RearAdmiral => "rear admiral",
            Self::Captain => "captain",
        }
    }
}

/// A fleet agent's identity: [`agent`] builds it.
#[derive(IntoElement)]
pub struct AgentAvatar {
    id: ElementId,
    rank: AgentRank,
    name: SharedString,
    side: f32,
    crew: Option<Crew>,
}

/// How many of an agent's crew are busy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Crew {
    busy: usize,
    total: usize,
}

/// An agent's avatar, as large as [`super::sender`]: a circle with a ring
/// and its rank's glyph, in its rank's tone. `name` is its accessible name
/// with the rank, such as *aurora-lyra, captain*.
pub fn agent(
    id: impl Into<ElementId>,
    rank: AgentRank,
    name: impl Into<SharedString>,
) -> AgentAvatar {
    AgentAvatar {
        id: id.into(),
        rank,
        name: name.into(),
        side: SENDER_SIZE_PX,
        crew: None,
    }
}

impl AgentAvatar {
    /// The size of [`super::member`], for list rows and headings.
    pub fn small(mut self) -> Self {
        self.side = MEMBER_SIZE_PX;
        self
    }

    /// Replaces the plain ring with a crew ring: `busy` of `total` crew,
    /// drawn as Kit's progress circle around a smaller face, and said in the
    /// accessible name.
    pub fn crew(mut self, busy: usize, total: usize) -> Self {
        self.crew = Some(Crew {
            busy: busy.min(total),
            total,
        });
        self
    }

    /// The accessible name: the agent, its rank and any crew.
    pub fn spoken(&self) -> String {
        let rank = self.rank.title();
        match self.crew {
            Some(Crew { busy, total }) => {
                format!("{}, {rank}, {busy} of {total} crew busy", self.name)
            }
            None => format!("{}, {rank}", self.name),
        }
    }

    /// The avatar's side in pixels.
    pub fn side(&self) -> Pixels {
        px(self.side)
    }
}

/// The share of the crew that is busy, as Kit's progress value.
fn busy_percent(busy: usize, total: usize) -> f32 {
    match total {
        0 => NO_CREW,
        total => busy as f32 * PERCENT / total as f32,
    }
}

/// The face's side: the whole avatar, or less inside a crew ring.
fn face_side(side: f32, ringed: bool) -> f32 {
    if ringed { side * RINGED_SHARE } else { side }
}

/// The circle with the rank's glyph; `ring` draws its own outline.
fn face(rank: AgentRank, side: f32, ring: bool, tokens: &Tokens) -> gpui_kit::Div {
    let tone = tokens.tone(rank.tone());
    let face = div()
        .size(px(side))
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(tone.opacity(FILL))
        .text_color(tone)
        .child(Icon::from(rank.glyph()).with_size(Size::Size(px(side * GLYPH_SHARE))));
    if ring {
        face.border_1().border_color(tone)
    } else {
        face
    }
}

/// The crew ring around `face`.
fn ringed(id: ElementId, crew: Crew, rank: AgentRank, side: f32, tokens: &Tokens) -> AnyElement {
    ProgressCircle::new(id)
        .value(busy_percent(crew.busy, crew.total))
        .color(tokens.tone(rank.tone()))
        .size(px(side))
        .flex_shrink_0()
        .child(face(rank, face_side(side, true), false, tokens))
        .into_any_element()
}

impl RenderOnce for AgentAvatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let spoken = self.spoken();
        let ring_id = ElementId::Name(format!("{}:crew", self.id).into());
        let body = match self.crew {
            Some(crew) => ringed(ring_id, crew, self.rank, self.side, &tokens),
            None => face(self.rank, self.side, true, &tokens).into_any_element(),
        };
        div()
            .id(self.id)
            .test_support()
            .role(Role::Image)
            .aria_label(spoken)
            .flex_shrink_0()
            .size(px(self.side))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentRank, Crew, RINGED_SHARE, agent, busy_percent, face_side};
    use crate::Tone;
    use crate::avatar::{MEMBER_SIZE_PX, SENDER_SIZE_PX};
    use gpui_kit::px;
    use subspace_lucide::IconName;

    #[test]
    fn each_rank_has_its_tone_glyph_and_words() {
        assert_eq!(AgentRank::Captain.tone(), Tone::Blue);
        assert_eq!(AgentRank::RearAdmiral.tone(), Tone::Accent);
        assert_eq!(AgentRank::Captain.glyph(), IconName::Navigation2);
        assert_eq!(AgentRank::RearAdmiral.glyph(), IconName::Diamond);
        assert_eq!(AgentRank::RearAdmiral.title(), "rear admiral");
    }

    #[test]
    fn an_agent_is_as_large_as_a_sender_or_a_member_and_says_its_rank() {
        let lyra = agent("lyra", AgentRank::Captain, "aurora-lyra");
        assert_eq!(lyra.side(), px(SENDER_SIZE_PX));
        assert_eq!(lyra.spoken(), "aurora-lyra, captain");
        let small = agent("vega", AgentRank::RearAdmiral, "aurora-admiral").small();
        assert_eq!(small.side(), px(MEMBER_SIZE_PX));
        assert_eq!(small.spoken(), "aurora-admiral, rear admiral");
    }

    #[test]
    fn a_crew_ring_says_how_many_are_busy_and_never_more_than_all() {
        let ringed = agent("lyra", AgentRank::Captain, "aurora-lyra").crew(3, 4);
        assert_eq!(ringed.crew, Some(Crew { busy: 3, total: 4 }));
        assert_eq!(ringed.spoken(), "aurora-lyra, captain, 3 of 4 crew busy");
        let over = agent("lyra", AgentRank::Captain, "aurora-lyra").crew(9, 4);
        assert_eq!(over.crew, Some(Crew { busy: 4, total: 4 }));
        assert!((busy_percent(3, 4) - 75.).abs() < f32::EPSILON);
        assert!(busy_percent(0, 0).abs() < f32::EPSILON, "no crew, no arc");
        assert!((face_side(32., true) - 32. * RINGED_SHARE).abs() < f32::EPSILON);
        assert!((face_side(32., false) - 32.).abs() < f32::EPSILON);
    }
}
