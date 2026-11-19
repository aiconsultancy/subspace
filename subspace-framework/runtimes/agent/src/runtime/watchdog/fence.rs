//! The shared turn state of every chat, fenced by turn generation so the
//! actor and the watchdog never both end one turn. Each turn a chat starts
//! has a new generation. The watchdog ends a turn only when the generation
//! it saw before its ping is still the running one, and it records that
//! generation as expired. The actor ends a turn only when the watchdog has
//! not expired it, and an update for an older generation changes nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ChatId;
use crate::failure::TurnContext;

/// What one chat last published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::runtime) struct Activity {
    pub(in crate::runtime) busy: bool,
    pub(in crate::runtime) last_event: Instant,
    pub(in crate::runtime) turn: TurnContext,
    /// The turn the chat is on.
    pub(in crate::runtime) generation: u64,
    /// The generation the watchdog ended, if it ended one.
    pub(in crate::runtime) expired: Option<u64>,
}

/// Every open chat's activity, shared by the chats and the watchdog.
pub(in crate::runtime) type Activities = Arc<Mutex<HashMap<ChatId, Activity>>>;

/// Whether a chat's running turn has been silent long enough to ping.
pub(in crate::runtime) fn stalled(
    now: Instant,
    activity: &Activity,
    stall_after: Duration,
) -> bool {
    activity.busy && now.saturating_duration_since(activity.last_event) >= stall_after
}

/// The stalled turns a ping is about to check, by chat and generation.
pub(in crate::runtime) fn observe(
    chats: &HashMap<ChatId, Activity>,
    now: Instant,
    stall_after: Duration,
) -> Vec<(ChatId, u64)> {
    chats
        .iter()
        .filter(|(_, activity)| stalled(now, activity, stall_after))
        .map(|(chat, activity)| (*chat, activity.generation))
        .collect()
}

/// The running activity of an observed turn, if it is still that turn.
fn running<'a>(
    chats: &'a mut HashMap<ChatId, Activity>,
    (chat, generation): &(ChatId, u64),
) -> Option<&'a mut Activity> {
    chats
        .get_mut(chat)
        .filter(|activity| activity.busy && activity.generation == *generation)
}

/// An answered ping restarts the wait of each observed turn still running.
pub(in crate::runtime) fn mark_alive(
    chats: &mut HashMap<ChatId, Activity>,
    observed: &[(ChatId, u64)],
    now: Instant,
) {
    for turn in observed {
        if let Some(activity) = running(chats, turn) {
            activity.last_event = now;
        }
    }
}

/// After a failed ping, ends each observed turn that is still the same
/// generation and still silent, and returns those turns. A turn that got an
/// event or was replaced during the ping is left alone.
pub(in crate::runtime) fn expire(
    chats: &mut HashMap<ChatId, Activity>,
    observed: &[(ChatId, u64)],
    now: Instant,
    stall_after: Duration,
) -> Vec<(ChatId, TurnContext)> {
    let mut ended = Vec::new();
    for turn in observed {
        let Some(activity) = running(chats, turn) else {
            continue;
        };
        if now.saturating_duration_since(activity.last_event) < stall_after {
            continue;
        }
        activity.busy = false;
        activity.expired = Some(turn.1);
        ended.push((turn.0, activity.turn.clone()));
    }
    ended
}

/// A chat's own update. An older generation changes nothing, and a
/// generation the watchdog expired stays ended.
pub(in crate::runtime) fn update(
    chats: &mut HashMap<ChatId, Activity>,
    chat: ChatId,
    mut activity: Activity,
) {
    if let Some(old) = chats.get(&chat) {
        if old.generation > activity.generation {
            return;
        }
        if old.expired == Some(activity.generation) {
            activity.busy = false;
            activity.expired = old.expired;
        }
    }
    chats.insert(chat, activity);
}

/// Whether the watchdog ended generation `generation` of `chat`.
pub(in crate::runtime) fn expired(
    chats: &HashMap<ChatId, Activity>,
    chat: ChatId,
    generation: u64,
) -> bool {
    chats
        .get(&chat)
        .is_some_and(|activity| activity.expired == Some(generation))
}

/// The actor ends generation `generation` of `chat`. Returns `false` when
/// the watchdog already ended it, so the actor reports nothing more.
pub(in crate::runtime) fn settle(
    chats: &mut HashMap<ChatId, Activity>,
    chat: ChatId,
    generation: u64,
) -> bool {
    let Some(activity) = chats.get_mut(&chat) else {
        return true;
    };
    if activity.expired == Some(generation) {
        return false;
    }
    if activity.generation == generation {
        activity.busy = false;
    }
    true
}

/// What the actor does with an SDK event after the watchdog ended the turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runtime) enum Late {
    /// Handle the event as usual.
    Normal,
    /// The expired turn's own end: the watchdog already reported it.
    Ignore,
    /// The runtime recovered and the turn goes on, as a new generation.
    Rearm,
}

/// What an SDK event of type `kind` means for a chat whose turn the
/// watchdog ended (`expired`).
pub(in crate::runtime) fn late(expired: bool, kind: &str) -> Late {
    if !expired {
        return Late::Normal;
    }
    match kind {
        "session.idle" | "session.error" => Late::Ignore,
        "assistant.message_delta"
        | "assistant.reasoning_delta"
        | "assistant.message"
        | "assistant.reasoning"
        | "assistant.turn_start"
        | "tool.execution_start"
        | "tool.execution_complete" => Late::Rearm,
        _ => Late::Normal,
    }
}

#[cfg(test)]
#[path = "fence_tests.rs"]
mod tests;
