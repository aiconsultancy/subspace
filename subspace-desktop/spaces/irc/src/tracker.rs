//! Acting on the tracker from a channel: the composer's `/assign`, `/ready`
//! and `/decision`, and a message's **Make issue** and **Record decision**.
//! Each writes directly to the Subspace API as the signed-in person,
//! through the desktop's API client (an owner decision): nothing is sent
//! to IRC for an agent to act on, nothing posts a line to the channel, and
//! nothing here sends a private message.
//!
//! - **Where.** In a channel, while the Issues Space answers with a tracker
//!   scope (it holds the signed-in Subspace API session) and the tracker is
//!   Subspace API, whose decisions and Ready for agents flag these use.
//! - **Checks first** ([`plan`]): the issue is in the tracker's projects
//!   and not known to be missing, the assignee is a fleet agent the roster
//!   knows or the person (`to me`), and the issue's state allows the
//!   change. The write reads the issue again and repeats the checks.
//! - **Deliberate.** A command runs only on Return, after the completion
//!   row has said what it will do; a message action only from its dialog's
//!   button. Hovering, focus and the toolbar's buttons never write.
//! - **Honest results.** A brief notice in the conversation's notice area
//!   says what changed, or the tracker's refusal in plain words. A refused
//!   command keeps its draft.
//! - **Fresh tiles.** After a write the Issues Space is asked to read the
//!   issue again (`issues@1/refresh`); its `changed` brings the new state to
//!   the tiles and the inspector's **In play here**.
//!
//! Message text, titles and names are untrusted: they are bounded and shown
//! as text, and nothing in a message can make a command run.

mod check;
mod destination;
mod failed;
mod flow;
pub(crate) mod message;
mod parse;
mod plan;
mod quote;
mod suggest;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use gpui_kit::App;
use subspace_api_client::{Backend, Client};
use subspace_client_irc::{BufferKey, Target};
use subspace_framework::contracts::issues::{IssueSummary, ResolutionState, StateType};

use crate::conversation::Conversation;
use crate::fleet::{ChannelKind, Roster};
use crate::issues::IssueReferences;
use crate::tiles::{TileRef, Tiles};

#[cfg(test)]
pub(crate) use destination::WRONG_TRACKER;
#[cfg(test)]
pub(crate) use flow::SCOPE_CHANGED;
#[cfg(test)]
pub(crate) use flow::{BUSY, UNCONFIRMED_AGAIN};
pub(crate) use flow::{State, submit};
pub(crate) use parse::typed;
pub(crate) use plan::Facts;
use plan::{AgentChoice, Answer, Known};
pub(crate) use suggest::{arguments, commands};

/// Why tracker commands are not offered outside a channel.
pub(crate) const CHANNEL_ONLY: &str = "Tracker commands work in a channel.";
/// Why they are not offered without a tracker scope.
pub(crate) const NO_TRACKER: &str =
    "Tracker commands need the Issues Space signed in to Subspace API.";
/// Why they are not offered for another tracker service.
pub(crate) const NOT_SUBSPACE: &str = "Tracker commands work with Subspace API as the tracker.";

/// The API client tracker writes use, made when first needed and again
/// after the signed-in account changes.
#[derive(Default)]
pub(crate) struct Link {
    client: Option<Arc<Client>>,
}

impl Link {
    pub(crate) fn client(&mut self) -> Option<Arc<Client>> {
        let stale = self
            .client
            .as_ref()
            .is_none_or(|client| client.account_changed());
        if stale {
            self.client = Client::new().ok().map(Arc::new);
        }
        self.client.clone()
    }

    #[cfg(test)]
    pub(crate) fn test_connect(&mut self, client: Client) {
        self.client = Some(Arc::new(client));
    }
}

/// Whether tracker commands and message actions work in `buffer` now, or
/// why not.
pub(crate) fn available(buffer: &BufferKey, issues: &IssueReferences) -> Result<(), &'static str> {
    if !matches!(buffer.target, Target::Channel(_)) {
        return Err(CHANNEL_ONLY);
    }
    if subspace_api_client::backend() != Backend::Subspace {
        return Err(NOT_SUBSPACE);
    }
    issues.scope().map(|_| ()).ok_or(NO_TRACKER)
}

/// What the tracker commands know in `conversation`, where they work;
/// `typed` is an issue a draft names, whose answer is gathered too.
pub(crate) fn facts(conversation: &Conversation, typed: Option<String>, cx: &App) -> Option<Facts> {
    let hub = conversation.hub.read(cx);
    available(&conversation.buffer, &hub.issues).ok()?;
    Some(gather(
        &hub.issues,
        &conversation.tiles,
        &conversation.roster,
        &conversation.buffer,
        typed,
    ))
}

/// The facts from the hub's answers, the conversation's tiles and the
/// fleet roles.
pub(crate) fn gather(
    issues: &IssueReferences,
    tiles: &Tiles,
    roster: &Roster,
    buffer: &BufferKey,
    typed: Option<String>,
) -> Facts {
    let in_play: Vec<String> = in_play(tiles);
    let answers = in_play
        .iter()
        .cloned()
        .chain(typed)
        .map(|identifier| {
            let answer = answer(issues.state(&identifier));
            (identifier, answer)
        })
        .collect();
    let (captain, ship) = captain(roster, buffer).unzip();
    let captain = captain.flatten();
    Facts {
        tracker: issues
            .scope()
            .map(|scope| (scope.profile_id.clone(), scope.api_origin.clone())),
        keys: issues.keys().clone(),
        projects: issues
            .scope()
            .map(|scope| scope.project_keys.clone())
            .unwrap_or_default(),
        in_play,
        answers,
        captain,
        ship: ship.flatten(),
        agents: agents(roster),
    }
}

/// The issues `tiles` shows, newest first.
pub(crate) fn in_play(tiles: &Tiles) -> Vec<String> {
    let mut found: Vec<String> = tiles
        .in_play_first(usize::MAX, |_| true)
        .into_iter()
        .filter_map(|found| match found {
            TileRef::Issue(identifier) => Some(identifier),
            TileRef::PullRequest(_) => None,
        })
        .collect();
    found.reverse();
    found
}

fn answer(state: Option<&ResolutionState>) -> Answer {
    match state {
        Some(ResolutionState::Available(summary)) => Answer::Known(known(summary)),
        Some(ResolutionState::Unavailable) => Answer::Unavailable,
        _ => Answer::Unknown,
    }
}

fn known(summary: &IssueSummary) -> Known {
    Known {
        title: summary.title.clone(),
        assignee: summary.assignee.clone(),
        assignee_id: summary.assignee_id().map(str::to_owned),
        ready: summary.ready_for_agents(),
        tracker: summary.tracker,
        closed: matches!(
            summary.state.kind,
            StateType::Completed | StateType::Canceled | StateType::Duplicate
        ),
    }
}

/// The captain of a ship's channel and its ship's name.
fn captain(roster: &Roster, buffer: &BufferKey) -> Option<(Option<AgentChoice>, Option<String>)> {
    let Target::Channel(name) = &buffer.target else {
        return None;
    };
    let Some(ChannelKind::Ship { captain }) = roster.channel(name) else {
        return None;
    };
    let agent = roster.agent(captain)?;
    // A captain the API records no user for cannot be assigned: no
    // default, so the plan asks whom.
    let choice = (!agent.user_id.is_empty()).then(|| AgentChoice {
        nick: captain.clone(),
        user_id: agent.user_id.clone(),
    });
    Some((choice, agent.ship.clone()))
}

/// Every fleet agent with an API user, by nick.
fn agents(roster: &Roster) -> Vec<AgentChoice> {
    let mut agents: Vec<AgentChoice> = roster
        .agents()
        .filter(|(_, agent)| !agent.user_id.is_empty())
        .map(|(nick, agent)| AgentChoice {
            nick: nick.to_owned(),
            user_id: agent.user_id.clone(),
        })
        .collect();
    agents.sort_by(|left, right| left.nick.cmp(&right.nick));
    agents
}
