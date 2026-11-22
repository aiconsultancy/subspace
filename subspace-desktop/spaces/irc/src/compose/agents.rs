//! Direct messages are for people only: all communication with a fleet's
//! agents happens in channels, where everyone can see it (an owner
//! decision). Channels offers no private conversation with an agent the
//! roster knows; it shows a short notice that names the agent's channel and
//! offers to go there. A private conversation with an agent already in
//! history stays readable, but its composer sends nothing.
//!
//! This is a client-side rule: the server still accepts a private message
//! sent another way.

use gpui_kit::component::h_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::panel::IconName;

use crate::conversation::{Conversation, ConversationEvent};
use crate::fleet::Roster;
use crate::markup::Ref;

/// Why a private conversation was not opened: `nick` is a fleet agent that
/// talks in `channel`, when its fleet names one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentNotice {
    pub nick: String,
    pub channel: Option<String>,
}

impl AgentNotice {
    /// The notice for a private conversation with `nick`, when the roster
    /// knows it as an agent; `None` for a person.
    pub(crate) fn refusing(roster: &Roster, nick: &str) -> Option<Self> {
        let agent = roster.agent(nick)?;
        Some(Self {
            nick: nick.to_owned(),
            channel: agent.channel.clone(),
        })
    }

    /// The notice's one plain sentence.
    pub(crate) fn words(&self) -> String {
        let place = self.channel.as_deref().map_or_else(
            || "in its fleet's channels".to_owned(),
            |channel| format!("in {channel}"),
        );
        format!(
            "{} is a fleet agent. Agents talk in channels, where everyone can see: ask {place}.",
            self.nick
        )
    }
}

/// The nick a draft would message privately: the target of `/msg`,
/// `/query` or `/notice` when it is a nick, or the other side of the
/// private conversation `target` for text or `/me`. `None` for a channel or
/// any other command.
pub(crate) fn direct_target(text: &str, target: &Target) -> Option<String> {
    let Some(command) = text.strip_prefix('/').filter(|rest| !rest.starts_with('/')) else {
        return partner(target);
    };
    let mut words = command.split_whitespace();
    let name = words.next().unwrap_or_default().to_ascii_lowercase();
    match name.as_str() {
        "msg" | "query" | "notice" => words
            .next()
            .filter(|first| !first.starts_with(['#', '&', '!', '+']))
            .map(str::to_owned),
        "me" => partner(target),
        _ => None,
    }
}

/// The other side of a private conversation.
fn partner(target: &Target) -> Option<String> {
    match target {
        Target::Query(nick) => Some(nick.clone()),
        Target::Channel(_) | Target::Server => None,
    }
}

/// The notice above the composer, with a button that goes to the agent's
/// channel when its fleet names one, and one that dismisses it.
pub(super) fn notice(notice: &AgentNotice, cx: &mut Context<Conversation>) -> impl IntoElement {
    let tokens = Tokens::current(cx);
    let go = notice.channel.clone().map(|channel| {
        let label = format!("Go to {channel}");
        action_button(
            "irc-agent-notice-go",
            Intent::Quiet,
            Some(IconName::Hash),
            label,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.draft.agent_notice = None;
            cx.emit(ConversationEvent::Open(Ref::Channel(channel.clone())));
            cx.notify();
        }))
    });
    h_flex()
        .id("irc-agent-notice")
        .test_support()
        .role(Role::Status)
        .aria_label(SharedString::from(notice.words()))
        .w_full()
        .gap_2()
        .px_2()
        .py_1()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .text_color(tokens.fg)
                .child(notice.words()),
        )
        .children(go)
        .child(
            icon_button(
                "irc-agent-notice-dismiss",
                Intent::Quiet,
                IconName::X,
                "Dismiss",
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.draft.agent_notice = None;
                cx.notify();
            })),
        )
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Target;

    use super::{AgentNotice, direct_target};
    use crate::fleet::Roster;
    use crate::fleet::tests::{aurora, fleet};

    fn channel() -> Target {
        Target::Channel("#aurora-lyra".into())
    }

    #[test]
    fn msg_query_and_notice_name_a_nick_and_never_a_channel() {
        let room = channel();
        assert_eq!(
            direct_target("/msg aurora-lyra hello", &room).as_deref(),
            Some("aurora-lyra")
        );
        assert_eq!(
            direct_target("/QUERY Aurora-Lyra", &room).as_deref(),
            Some("Aurora-Lyra")
        );
        assert_eq!(
            direct_target("/notice aurora-lyra hi", &room).as_deref(),
            Some("aurora-lyra")
        );
        assert_eq!(direct_target("/msg #aurora-lyra hi", &room), None);
        assert_eq!(direct_target("hello", &room), None);
        assert_eq!(direct_target("/join #ops", &room), None);
        assert_eq!(direct_target("//msg aurora-lyra", &room), None, "literal");
    }

    #[test]
    fn in_a_private_conversation_text_and_actions_go_to_its_partner() {
        let query = Target::Query("aurora-lyra".into());
        assert_eq!(
            direct_target("hello", &query).as_deref(),
            Some("aurora-lyra")
        );
        assert_eq!(
            direct_target("/me waves", &query).as_deref(),
            Some("aurora-lyra")
        );
        assert_eq!(direct_target("/join #ops", &query), None);
        assert_eq!(direct_target("/part", &Target::Server), None);
    }

    #[test]
    fn only_an_agent_the_roster_knows_is_refused_and_the_notice_names_its_channel() {
        let aurora = aurora();
        let roster = Roster::new([&aurora]);
        let lyra = AgentNotice::refusing(&roster, "Aurora-Lyra");
        assert_eq!(
            lyra.as_ref().map(AgentNotice::words).as_deref(),
            Some(
                "Aurora-Lyra is a fleet agent. Agents talk in channels, where everyone can see: ask in #aurora-lyra."
            )
        );
        let admiral = AgentNotice::refusing(&roster, "aurora-admiral");
        assert_eq!(
            admiral.and_then(|notice| notice.channel).as_deref(),
            Some("#aurora-starbase"),
            "the rear admiral talks in the Starbase"
        );
        assert_eq!(AgentNotice::refusing(&roster, "mira"), None, "a person");
        assert_eq!(
            AgentNotice::refusing(&roster, "castor"),
            None,
            "an agent without a known nick is not guessed"
        );
        let unnamed = fleet(
            "Not A Slug",
            vec![crate::fleet::tests::agent(
                "Vega",
                "rearAdmiral",
                Some("odd-admiral"),
                None,
            )],
        );
        let roster = Roster::new([&unnamed]);
        assert_eq!(
            AgentNotice::refusing(&roster, "odd-admiral")
                .map(|notice| notice.words())
                .as_deref(),
            Some(
                "odd-admiral is a fleet agent. Agents talk in channels, where everyone can see: ask in its fleet's channels."
            )
        );
    }
}
