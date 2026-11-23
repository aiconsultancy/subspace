//! The reactions that stop an agent's turn, described as data so a UI can
//! mark them. Nothing here sends or stops anything.
//!
//! Source: Stargate `main` at commit `da242f8` (read on 22 November
//! 2026; the behaviour matches the v0.43.0 release the fleet installer
//! pins, and is unchanged at `d4842b8`): `docs/irc.md`, "Stop by reaction", `routeIrcStop` and
//! `reactionKey` in `src/irc/routing.ts`, the `stopReactions` setting in
//! `src/irc/config.ts` and its caller in `src/irc/ship.ts`. There, a person
//! who reacts with one of the gateway's `irc[].stopReactions` to an agent's
//! request message, or to an agent message answering it, stops the active
//! (queued or running) turn as Slack's Stop does. The defaults are ✋ and
//! 🛑; skin-tone modifiers (U+1F3FB to U+1F3FF) and the emoji variation
//! selector (U+FE0F) are ignored when matching. Stargate also requires:
//!
//! - the gateway's `reactions` feature is on and `stopReactions` is not
//!   empty;
//! - the reaction is a `TAGMSG` with `+draft/react` and a valid
//!   `+draft/reply` (or, failing that, `+reply`) naming the message;
//! - the reacting user has an account (`account` tag), is a person, not
//!   the ship's own account or a peer captain, and passes its access rules;
//! - a reaction to an agent message counts only for message IDs the
//!   gateway has seen since it started; one on the request always counts;
//! - the turn is active: a reaction to finished work stops nothing, and
//!   replayed (history) reactions never stop a turn;
//! - `+draft/unreact` is ignored: a Stop cannot be undone.
//!
//! A gateway can configure other stop reactions in `irc[].stopReactions`
//! (up to 10 of 1 to 32 code points each, or `[]` for none). The client
//! cannot see that setting, so [`AGENT_STOP_REACTIONS`] describes the
//! defaults only.

/// Stargate's default `irc[].stopReactions`: raised hand and stop sign.
pub const AGENT_STOP_REACTIONS: &[&str] = &["\u{270B}", "\u{1F6D1}"];

/// What Stargate compares: the reaction without the emoji variation
/// selector (U+FE0F) or skin-tone modifiers (U+1F3FB to U+1F3FF).
pub fn reaction_key(reaction: &str) -> String {
    reaction
        .chars()
        .filter(|c| *c != '\u{FE0F}' && !('\u{1F3FB}'..='\u{1F3FF}').contains(c))
        .collect()
}

/// Whether Stargate, with its default settings, treats `reaction` as a stop
/// reaction. A UI may use it to mark the reaction; whether a stop happens
/// depends on the conditions in this module's documentation.
pub fn is_agent_stop_reaction(reaction: &str) -> bool {
    let key = reaction_key(reaction);
    AGENT_STOP_REACTIONS.iter().any(|stop| key == *stop)
}
