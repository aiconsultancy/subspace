//! Typing notices: announced while a draft is typed, repeated at most every
//! few seconds, and never for a command.

use std::time::{Duration, Instant};

use gpui_kit::Context;
use subspace_client_irc::{Command, Typing};

use crate::conversation::Conversation;

/// While typing continues, an active notice is repeated at most this often.
const TYPING_REPEAT: Duration = Duration::from_secs(3);

/// What to tell the channel after the draft changed, if anything.
fn typing_notice(text: &str, since: Option<Instant>, now: Instant) -> Option<Typing> {
    let command = text.trim_start().starts_with('/');
    match since {
        _ if command => since.map(|_| Typing::Done),
        None if !text.is_empty() => Some(Typing::Active),
        Some(_) if text.is_empty() => Some(Typing::Done),
        Some(since) if now.duration_since(since) >= TYPING_REPEAT => Some(Typing::Active),
        _ => None,
    }
}

pub(super) fn typed(
    conversation: &mut Conversation,
    text: &str,
    now: Instant,
    cx: &mut Context<Conversation>,
) {
    let draft = &mut conversation.draft;
    let Some(state) = typing_notice(text, draft.typing_since, now) else {
        return;
    };
    draft.typing_since = (state == Typing::Active).then_some(now);
    conversation.send(
        Command::Typing {
            buffer: conversation.buffer.clone(),
            state,
        },
        cx,
    );
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use subspace_client_irc::Typing;

    use super::typing_notice;

    #[test]
    fn typing_is_announced_then_repeated_at_most_every_three_seconds() {
        let start = Instant::now();
        assert_eq!(typing_notice("h", None, start), Some(Typing::Active));
        let soon = start + Duration::from_secs(1);
        assert_eq!(typing_notice("he", Some(start), soon), None);
        let later = start + Duration::from_secs(3);
        assert_eq!(
            typing_notice("hel", Some(start), later),
            Some(Typing::Active)
        );
        assert_eq!(typing_notice("", Some(start), soon), Some(Typing::Done));
        assert_eq!(typing_notice("", None, soon), None);
    }

    #[test]
    fn commands_are_not_announced_as_typing() {
        let start = Instant::now();
        assert_eq!(typing_notice("/join", None, start), None);
        assert_eq!(
            typing_notice("/join", Some(start), start),
            Some(Typing::Done)
        );
    }
}
