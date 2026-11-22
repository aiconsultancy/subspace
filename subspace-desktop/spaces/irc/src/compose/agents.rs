//! Direct messages are for people only: all communication with a fleet's
//! agents happens in channels, where everyone can see it (an owner
//! decision). Channels offers no private conversation with an agent the
//! roster knows; it shows a short notice that names the agent's channel and
//! offers to go there. A private conversation with an agent already in
//! history stays readable, but its composer sends nothing.
//!
//! The rule reads what a draft would send, not how it was typed: every nick
//! of a target list, and the PRIVMSG or NOTICE a `/raw`, `/quote` or unknown
//! command carries, is checked, and a channel never is.
//!
//! This is a client-side rule: the server still accepts a private message
//! sent another way.

use gpui_kit::component::h_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::{BufferKey, NetworkId, Target};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::panel::IconName;

use crate::conversation::{Conversation, ConversationEvent};
use crate::fleet::Roster;
use crate::markup::Ref;
use crate::state::IrcState;

/// Why a private conversation was not opened: `nick` is a fleet agent that
/// talks in `channel`, when its fleet names one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentNotice {
    pub nick: String,
    pub channel: Option<String>,
    /// Whether **Go to** may open `channel`: the API records it, or the
    /// user's channel list holds it. A standard name standing in for an
    /// unrecorded channel is only named, as joining it would create it.
    pub go: bool,
}

impl AgentNotice {
    /// The notice for a private conversation with `nick`, when the roster
    /// knows it as an agent; `None` for a person. `listed` says whether the
    /// user's channel list holds a channel.
    pub(crate) fn refusing(
        roster: &Roster,
        nick: &str,
        listed: impl Fn(&str) -> bool,
    ) -> Option<Self> {
        let agent = roster.agent(nick)?;
        let go = agent
            .channel
            .as_deref()
            .is_some_and(|channel| agent.recorded || listed(channel));
        Some(Self {
            nick: nick.to_owned(),
            channel: agent.channel.clone(),
            go,
        })
    }

    /// The notice for the first fleet agent among `nicks`; `None` when all
    /// of them are people.
    pub(crate) fn refusing_any(
        roster: &Roster,
        nicks: &[String],
        listed: impl Fn(&str) -> bool,
    ) -> Option<Self> {
        nicks
            .iter()
            .find_map(|nick| Self::refusing(roster, nick, &listed))
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

/// The nicks a draft would message privately, read from the PRIVMSG and
/// NOTICE lines the session would send for it: each nick of a `/msg`,
/// `/notice` or `/query` target list, of a raw PRIVMSG or NOTICE line, or
/// the other side of the private conversation `target` for text or `/me`.
/// Channels are never named.
pub(crate) fn direct_targets(text: &str, target: &Target) -> Vec<String> {
    subspace_client_irc::private_recipients(text, target)
}

/// The channel `name` on `network` that the user's channel list holds,
/// ignoring ASCII case.
pub(crate) fn listed_channel(
    state: &IrcState,
    network: &NetworkId,
    name: &str,
) -> Option<BufferKey> {
    state
        .buffers()
        .map(|(key, _)| key)
        .find(|key| {
            &key.network == network
                && matches!(&key.target, Target::Channel(held) if held.eq_ignore_ascii_case(name))
        })
        .cloned()
}

/// The notice above the composer, with a button that goes to the agent's
/// channel when the API records it or the user's channel list holds it, and
/// one that dismisses it.
pub(super) fn notice(notice: &AgentNotice, cx: &mut Context<Conversation>) -> impl IntoElement {
    let tokens = Tokens::current(cx);
    let go = notice
        .go
        .then(|| notice.channel.clone())
        .flatten()
        .map(|channel| {
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
    use serde_json::json;
    use subspace_client_irc::Target;

    use super::{AgentNotice, direct_targets};
    use crate::fleet::Roster;
    use crate::fleet::tests::{agent, aurora, fleet, fleet_with};

    fn channel() -> Target {
        Target::Channel("#aurora-lyra".into())
    }

    fn unlisted(_: &str) -> bool {
        false
    }

    #[test]
    fn msg_query_and_notice_name_each_nick_and_never_a_channel() {
        let room = channel();
        assert_eq!(
            direct_targets("/msg aurora-lyra hello", &room),
            ["aurora-lyra"]
        );
        assert_eq!(direct_targets("/QUERY Aurora-Lyra", &room), ["Aurora-Lyra"]);
        assert_eq!(
            direct_targets("/notice mira,aurora-lyra hi", &room),
            ["mira", "aurora-lyra"]
        );
        assert!(direct_targets("/msg #aurora-lyra hi", &room).is_empty());
        assert!(direct_targets("hello", &room).is_empty());
        assert!(direct_targets("/join #ops", &room).is_empty());
        assert!(
            direct_targets("//msg aurora-lyra", &room).is_empty(),
            "literal"
        );
    }

    #[test]
    fn raw_quote_and_unknown_commands_are_read_for_their_privmsg_or_notice() {
        let room = channel();
        assert_eq!(
            direct_targets("/raw PRIVMSG aurora-lyra :hi", &room),
            ["aurora-lyra"]
        );
        assert_eq!(
            direct_targets("/quote NOTICE #ops,aurora-lyra :hi", &room),
            ["aurora-lyra"]
        );
        assert_eq!(
            direct_targets("/privmsg aurora-lyra hi", &room),
            ["aurora-lyra"]
        );
        assert!(direct_targets("/raw PRIVMSG #aurora-lyra :hi", &room).is_empty());
    }

    #[test]
    fn in_a_private_conversation_text_and_actions_go_to_its_partner() {
        let query = Target::Query("aurora-lyra".into());
        assert_eq!(direct_targets("hello", &query), ["aurora-lyra"]);
        assert_eq!(direct_targets("/me waves", &query), ["aurora-lyra"]);
        assert!(direct_targets("/join #ops", &query).is_empty());
        assert!(direct_targets("/part", &Target::Server).is_empty());
    }

    #[test]
    fn any_agent_in_a_target_list_is_refused() {
        let aurora = aurora();
        let roster = Roster::new([&aurora]);
        let nicks = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            AgentNotice::refusing_any(&roster, &nicks(&["mira", "Aurora-Lyra"]), unlisted)
                .map(|notice| notice.nick)
                .as_deref(),
            Some("Aurora-Lyra")
        );
        assert_eq!(
            AgentNotice::refusing_any(&roster, &nicks(&["mira", "dave"]), unlisted),
            None
        );
    }

    #[test]
    fn only_an_agent_the_roster_knows_is_refused_and_the_notice_names_its_channel() {
        let aurora = aurora();
        let roster = Roster::new([&aurora]);
        let lyra = AgentNotice::refusing(&roster, "Aurora-Lyra", unlisted);
        assert_eq!(
            lyra.as_ref().map(AgentNotice::words).as_deref(),
            Some(
                "Aurora-Lyra is a fleet agent. Agents talk in channels, where everyone can see: ask in #aurora-lyra."
            )
        );
        let admiral = AgentNotice::refusing(&roster, "aurora-admiral", unlisted);
        assert_eq!(
            admiral.and_then(|notice| notice.channel).as_deref(),
            Some("#aurora-starbase"),
            "the rear admiral talks in the Starbase"
        );
        assert_eq!(
            AgentNotice::refusing(&roster, "mira", unlisted),
            None,
            "a person"
        );
        assert_eq!(
            AgentNotice::refusing(&roster, "castor", unlisted),
            None,
            "an agent without a known nick is not guessed"
        );
        let unnamed = fleet(
            "Not A Slug",
            vec![agent("Vega", "rearAdmiral", Some("odd-admiral"), None)],
        );
        let roster = Roster::new([&unnamed]);
        let odd = AgentNotice::refusing(&roster, "odd-admiral", |_| true);
        assert_eq!(
            odd.as_ref().map(AgentNotice::words).as_deref(),
            Some(
                "odd-admiral is a fleet agent. Agents talk in channels, where everyone can see: ask in its fleet's channels."
            )
        );
        assert!(odd.is_some_and(|notice| !notice.go), "nowhere to go");
    }

    #[test]
    fn go_to_is_offered_only_for_a_recorded_or_listed_channel() {
        let aurora = aurora();
        let roster = Roster::new([&aurora]);
        let go = |nick: &str, listed: &dyn Fn(&str) -> bool| {
            AgentNotice::refusing(&roster, nick, listed).map(|notice| notice.go)
        };
        assert_eq!(go("aurora-lyra", &unlisted), Some(false), "a guessed name");
        assert_eq!(
            go("aurora-lyra", &|channel| channel == "#aurora-lyra"),
            Some(true),
            "a channel the user's list holds"
        );
        let recorded = fleet_with(
            "aurora",
            vec![agent(
                "admiral-vance",
                "rearAdmiral",
                Some("aurora-admiral"),
                None,
            )],
            json!({"starbaseChannel": "#aurora-base"}),
        );
        let roster = Roster::new([&recorded]);
        let notice = AgentNotice::refusing(&roster, "aurora-admiral", unlisted);
        assert_eq!(
            notice.map(|notice| (notice.channel, notice.go)),
            Some((Some("#aurora-base".to_owned()), true)),
            "a channel the API records"
        );
    }
}
