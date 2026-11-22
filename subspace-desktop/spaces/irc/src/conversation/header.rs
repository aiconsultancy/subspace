//! Context shared by the dock title, the board's toolbar heading and the
//! heading of a standalone conversation: a fleet channel's glyph, the name,
//! the topic and, on a ship's channel, its captain's live status.

use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::{Member, Target};
use subspace_framework::dashboard::badge::{Pill, PillTone};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone, avatar, truncated};

use super::Conversation;
use crate::fleet::ChannelKind;
use crate::state::Statuses;

/// What the header shows, read from the hub.
pub(super) struct HeaderFacts {
    pub title: String,
    pub network: String,
    /// The topic without formatting codes.
    pub topic: Option<String>,
    /// What a fleet channel is for, when the Space knows it.
    pub kind: Option<ChannelKind>,
    /// A ship channel's captain's status, while it has one.
    pub live: Option<Live>,
}

/// A ship's captain's current status, for the header's live pill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Live {
    pub text: String,
    /// The accessible name: whose status it is, then the status.
    pub spoken: String,
}

/// The live status of a ship channel's captain from the network's nick
/// `statuses`; nothing for any other channel, while the captain has no
/// status, or while no channel member with the captain's nick is signed in
/// to the account of that nick, so a status shows only for the agent the
/// server vouches for.
pub(super) fn live(
    kind: Option<&ChannelKind>,
    statuses: &Statuses,
    members: &[Member],
) -> Option<Live> {
    let Some(ChannelKind::Ship { captain }) = kind else {
        return None;
    };
    let vouched = members.iter().any(|member| {
        member.nick.eq_ignore_ascii_case(captain)
            && member
                .account
                .as_deref()
                .is_some_and(|account| account.eq_ignore_ascii_case(captain))
    });
    if !vouched {
        return None;
    }
    statuses.get(captain).map(|text| Live {
        text: text.to_owned(),
        spoken: format!("Captain {captain}: {text}"),
    })
}

/// The glyph, its tone and its name for a fleet channel's `kind`.
fn kind_glyph(kind: &ChannelKind) -> (IconName, Tone, &'static str) {
    match kind {
        ChannelKind::Ship { .. } => (IconName::Navigation, Tone::Blue, "ship channel"),
        ChannelKind::Starbase => (IconName::Orbit, Tone::Accent, "Starbase"),
        ChannelKind::Command => (IconName::RadioTower, Tone::Accent, "fleet command"),
    }
}

impl HeaderFacts {
    /// The network and topic, for the dock tab's tooltip.
    pub(super) fn context(&self) -> String {
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => format!("{} · {topic}", self.network),
            None => self.network.clone(),
        }
    }

    /// The quiet text beside the name: the topic, or the network (for the
    /// server buffer, its status) while there is none.
    fn quiet(&self) -> String {
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => topic.to_owned(),
            None => self.network.clone(),
        }
    }

    fn spoken(&self) -> String {
        let kind = self
            .kind
            .as_ref()
            .map(|kind| format!(", {}", kind_glyph(kind).2))
            .unwrap_or_default();
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => format!("{}{kind}, {}, topic: {topic}", self.title, self.network),
            None => format!("{}{kind}, {}", self.title, self.network),
        }
    }
}

impl Conversation {
    pub(super) fn standalone_title(&self, cx: &App) -> Option<AnyElement> {
        if self.group.is_some() {
            return None;
        }
        self.heading(true, cx)
    }

    /// The heading for a host that shows it outside the conversation, as the
    /// board's toolbar does for the conversation in front. With `named`
    /// false the name is left to a control beside it, such as a menu that
    /// already says it.
    pub(crate) fn heading(&self, named: bool, cx: &App) -> Option<AnyElement> {
        let facts = self.header_facts(cx)?;
        Some(title(&self.buffer.target, &facts, named, cx))
    }
}

/// A fleet channel's glyph in its tone, before the name.
fn glyph(kind: &ChannelKind, tokens: &Tokens) -> AnyElement {
    let (icon, tone, _) = kind_glyph(kind);
    div()
        .id("irc-header-kind")
        .test_support()
        .flex_shrink_0()
        .text_color(tokens.tone(tone))
        .child(Icon::from(icon).small())
        .into_any_element()
}

/// The captain's status in the shared pill, after the topic.
fn live_pill(live: &Live) -> Pill {
    Pill::new("irc-header-live", live.text.clone())
        .tone(PillTone::Muted)
        .aria_label(live.spoken.clone())
}

/// The leading part of the header row: a fleet channel's glyph, the avatar
/// for a person and the name in strong text when `named`, then the topic in
/// quiet text on the same line, and a ship captain's live status at its
/// end. The accessible name still says the network.
fn title(target: &Target, facts: &HeaderFacts, named: bool, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let person = match target {
        Target::Query(nick) if named => Some(nick.clone()),
        _ => None,
    };
    h_flex()
        .id("irc-header")
        .role(Role::Heading)
        .aria_label(facts.spoken())
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .overflow_hidden()
        .children(facts.kind.as_ref().map(|kind| glyph(kind, &tokens)))
        .children(person.map(|nick| avatar::member(nick, &tokens)))
        .children(named.then(|| {
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_size(tokens.row_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(facts.title.clone())
        }))
        .child(
            truncated::text("irc-header-context", facts.quiet())
                .flex_1()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted),
        )
        .children(facts.live.as_ref().map(live_pill))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Member;

    use super::{HeaderFacts, Live, live};
    use crate::fleet::ChannelKind;
    use crate::state::Statuses;

    fn facts(topic: Option<&str>) -> HeaderFacts {
        HeaderFacts {
            title: "#subspace".to_owned(),
            network: "Local Ergo".to_owned(),
            topic: topic.map(str::to_owned),
            kind: None,
            live: None,
        }
    }

    #[test]
    fn a_ship_channel_s_live_pill_says_its_captain_s_status_and_nothing_without_one() {
        let ship = ChannelKind::Ship {
            captain: "aurora-lyra".into(),
        };
        let mut statuses = Statuses::default();
        let lyra = |account: Option<&str>| Member {
            nick: "Aurora-Lyra".into(),
            prefixes: String::new(),
            away: false,
            account: account.map(str::to_owned),
        };
        let signed = [lyra(Some("aurora-lyra"))];
        assert_eq!(
            live(Some(&ship), &statuses, &signed),
            None,
            "no invented idle state"
        );
        statuses.set("Aurora-Lyra", Some("Lyra is delegating NAV-2"));
        statuses.set("aurora-admiral", Some("Reviewing the plan"));
        assert_eq!(
            live(Some(&ship), &statuses, &signed),
            Some(Live {
                text: "Lyra is delegating NAV-2".into(),
                spoken: "Captain aurora-lyra: Lyra is delegating NAV-2".into(),
            })
        );
        for unvouched in [lyra(None), lyra(Some("mira"))] {
            assert_eq!(live(Some(&ship), &statuses, &[unvouched]), None);
        }
        assert_eq!(live(Some(&ship), &statuses, &[]), None, "not here");
        assert_eq!(live(Some(&ChannelKind::Starbase), &statuses, &signed), None);
        assert_eq!(live(None, &statuses, &signed), None);
    }

    #[test]
    fn beside_the_name_the_topic_shows_alone_and_the_network_stands_in_for_none() {
        assert_eq!(facts(Some("Release day")).quiet(), "Release day");
        assert_eq!(facts(Some("")).quiet(), "Local Ergo");
        assert_eq!(facts(None).quiet(), "Local Ergo");
        assert_eq!(
            facts(Some("Release day")).context(),
            "Local Ergo · Release day",
            "the tab's tooltip still names the network"
        );
    }
}
