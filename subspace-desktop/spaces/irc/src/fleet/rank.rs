//! A fleet agent's rank and how Channels marks it.

use subspace_api_client::model::AgentRank;
use subspace_framework::dashboard::Tone;
use subspace_framework::dashboard::avatar::AgentRank as AvatarRank;

/// A fleet agent's rank, as the API has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rank {
    RearAdmiral,
    Captain,
    Crew,
}

impl Rank {
    /// The rank of an API agent; `None` for a rank this build does not know.
    pub(super) fn of(rank: AgentRank) -> Option<Self> {
        match rank {
            AgentRank::RearAdmiral => Some(Self::RearAdmiral),
            AgentRank::Captain => Some(Self::Captain),
            AgentRank::Crew => Some(Self::Crew),
            _ => None,
        }
    }

    /// The quiet note beside a member, such as in the member list.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::RearAdmiral => "rear admiral",
            Self::Captain => "captain",
            Self::Crew => "crew",
        }
    }

    /// The mark after the agent's name in the transcript.
    pub(crate) fn mark(self) -> &'static str {
        match self {
            Self::RearAdmiral => "REAR ADMIRAL",
            Self::Captain => "CAPTAIN",
            Self::Crew => "CREW",
        }
    }

    /// The tone of the agent's name and mark: the rear admiral the accent,
    /// captains blue; crew keep a person's tone and a muted mark.
    pub(crate) fn tone(self) -> Option<Tone> {
        self.avatar().map(AvatarRank::tone)
    }

    /// The shared agent avatar's rank: the rear admiral and captains have
    /// one; crew keep a person's avatar.
    pub(crate) fn avatar(self) -> Option<AvatarRank> {
        match self {
            Self::RearAdmiral => Some(AvatarRank::RearAdmiral),
            Self::Captain => Some(AvatarRank::Captain),
            Self::Crew => None,
        }
    }
}
