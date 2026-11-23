//! The gates a draft meets before anything is sent: no private message to
//! a fleet agent, and a hand-typed TAGMSG held to the reactions gate, so a
//! stop goes only from its labelled item and nothing goes to an agent, or
//! to anyone while the initiatives are unknown, in private.

use gpui_kit::Context;
use subspace_client_irc::Target;

use super::{AgentNotice, agents};
use crate::conversation::Conversation;

/// Keeps a draft that would message a fleet agent privately, or send a
/// private TAGMSG the reactions gate refuses.
pub(super) fn refused(
    conversation: &mut Conversation,
    text: &str,
    cx: &mut Context<Conversation>,
) -> bool {
    refuse_agent(conversation, text, cx)
        || refuse_tag(conversation, text, cx)
        || refuse_raw_reaction(conversation, text, cx)
}

/// Keeps a draft whose hand-typed TAGMSG adds a reaction the reactions
/// gate would not send ([`crate::reactions::gate::gate`]): a stop
/// reaction, which goes only from its labelled Stop item, or any reaction
/// the gate refuses where it goes.
fn refuse_raw_reaction(
    conversation: &mut Conversation,
    text: &str,
    cx: &mut Context<Conversation>,
) -> bool {
    use crate::reactions::gate::{Gate, Via, gate};
    let roster = std::rc::Rc::clone(&conversation.roster);
    let refusal = subspace_client_irc::raw_reactions(text)
        .into_iter()
        .find_map(|(target, reaction)| {
            let target = if target.starts_with(['#', '&', '!', '+']) {
                Target::Channel(target)
            } else {
                Target::Query(target)
            };
            match gate((&target, &roster), &reaction, true, Via::Ordinary) {
                Gate::Send => None,
                Gate::Explain => Some(crate::reactions::stop::explanation(None)),
                Gate::Refuse(words) => Some(words),
            }
        });
    let refused = refusal.is_some();
    if let Some(words) = refusal {
        conversation
            .draft
            .entity
            .update(cx, |input, cx| input.set_error(words, cx));
    }
    refused
}

/// Keeps a draft whose hand-typed TAGMSG, which can carry a reaction,
/// goes to someone privately whom the reactions gate refuses
/// ([`crate::reactions::gate::refusal`]): a fleet agent, or anyone while
/// the initiatives are not known.
fn refuse_tag(conversation: &mut Conversation, text: &str, cx: &mut Context<Conversation>) -> bool {
    let refusal = subspace_client_irc::private_tag_recipients(text)
        .into_iter()
        .find_map(|nick| {
            crate::reactions::gate::refusal(&Target::Query(nick), &conversation.roster)
        });
    let refused = refusal.is_some();
    if let Some(words) = refusal {
        conversation
            .draft
            .entity
            .update(cx, |input, cx| input.set_error(words, cx));
    }
    refused
}

/// Keeps a draft that would message a fleet agent privately, and shows why.
/// True when it was kept.
fn refuse_agent(
    conversation: &mut Conversation,
    text: &str,
    cx: &mut Context<Conversation>,
) -> bool {
    let nicks = agents::direct_targets(text, &conversation.buffer.target);
    let state = &conversation.hub.read(cx).state;
    let network = &conversation.buffer.network;
    let notice = AgentNotice::refusing_any(&conversation.roster, &nicks, |channel| {
        agents::listed_channel(state, network, channel).is_some()
    });
    let refused = notice.is_some();
    if refused {
        conversation.draft.agent_notice = notice;
        cx.notify();
    }
    refused
}
