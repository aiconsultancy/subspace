//! A channel's member list, for a host that shows it beside the
//! conversation, as the workspace's inspector does: operators, voiced and
//! other members, each with a small avatar, the nick and a quiet note of
//! the role the Space knows (`you`, `owner`, `admin`, `half-operator`), and
//! dimmed while away. The host
//! supplies the heading, with the count ([`MemberList::count`]), and the
//! scrolling. Choosing a member asks the host to open a private conversation
//! ([`MemberListEvent`]).

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div,
};
use subspace_client_irc::{BufferKey, Member};
use subspace_framework::dashboard::action::row_button;
use subspace_framework::dashboard::{Tokens, avatar, workspace};

use crate::hub::Hub;

/// Prefix ranks: operators of any level, then voiced, then everyone else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rank {
    Operators,
    Voiced,
    Members,
}

impl Rank {
    fn of(member: &Member) -> Self {
        match member.prefixes.chars().next() {
            Some('~' | '&' | '@' | '%') => Self::Operators,
            Some('+') => Self::Voiced,
            _ => Self::Members,
        }
    }

    // qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
    fn heading(self) -> &'static str {
        match self {
            Self::Operators => "Operators",
            Self::Voiced => "Voiced",
            Self::Members => "Members",
        }
    }
}

/// Members in their groups, each sorted by nick ignoring case.
pub(crate) fn groups(members: &[Member]) -> Vec<(Rank, Vec<&Member>)> {
    let mut sorted: Vec<&Member> = members.iter().collect();
    sorted.sort_by_cached_key(|member| (Rank::of(member), member.nick.to_lowercase()));
    let mut groups: Vec<(Rank, Vec<&Member>)> = Vec::new();
    for member in sorted {
        let rank = Rank::of(member);
        match groups.last_mut() {
            Some((last, list)) if *last == rank => list.push(member),
            _ => groups.push((rank, vec![member])),
        }
    }
    groups
}

/// The quiet note after a member's nick: `you` for our own nick, then the
/// prefix's role where it says more than the section heading does (an
/// owner, admin or half-operator among the operators). `None` when there
/// is nothing to add.
pub(crate) fn note(member: &Member, own_nick: Option<&str>) -> Option<String> {
    let you = own_nick.is_some_and(|nick| nick.eq_ignore_ascii_case(&member.nick));
    let role = match member.prefixes.chars().next() {
        Some('~') => Some("owner"),
        Some('&') => Some("admin"),
        Some('%') => Some("half-operator"),
        _ => None,
    };
    match (you, role) {
        (true, Some(role)) => Some(format!("you · {role}")),
        (true, None) => Some("you".to_owned()),
        (false, role) => role.map(str::to_owned),
    }
}

/// What a member list asks its host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemberListEvent {
    /// Open a private conversation with this member.
    OpenQuery(String),
}

/// One channel's member list.
pub(crate) struct MemberList {
    me: WeakEntity<Self>,
    buffer: BufferKey,
    hub: Entity<Hub>,
    _subscription: Subscription,
}

impl EventEmitter<MemberListEvent> for MemberList {}

impl MemberList {
    pub fn buffer(&self) -> &BufferKey {
        &self.buffer
    }

    pub fn new(buffer: BufferKey, hub: Entity<Hub>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&hub, |_, _, cx| cx.notify());
        Self {
            me: cx.entity().downgrade(),
            buffer,
            hub,
            _subscription: subscription,
        }
    }

    /// How many members the channel has, for the host's heading.
    pub fn count(&self, cx: &App) -> usize {
        self.hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .map_or(0, |held| held.members.len())
    }
}

impl Render for MemberList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let state = &self.hub.read(cx).state;
        let members = state
            .buffer(&self.buffer)
            .map(|held| held.members.clone())
            .unwrap_or_default();
        let own = state
            .network(&self.buffer.network)
            .map(|network| network.nick.clone());
        let ours = Ours {
            owner: &self.me,
            nick: own.as_deref(),
            tokens: &tokens,
        };
        let sections: Vec<AnyElement> = groups(&members)
            .into_iter()
            .map(|(rank, list)| section(&ours, rank, &list, cx))
            .collect();
        // As wide as its host, which owns the width, the heading and the
        // scrolling. The rank groups are spaced as the inspector spaces its
        // own sections.
        v_flex()
            .id("irc-members")
            .test_support()
            .w_full()
            .min_w_0()
            .gap_6()
            .children(sections)
    }
}

/// What every row of one list shares: the list, our own nick on its
/// network, and the tokens.
struct Ours<'a> {
    owner: &'a WeakEntity<MemberList>,
    nick: Option<&'a str>,
    tokens: &'a Tokens,
}

/// One rank's members under the inspector's section heading, such as
/// `Operators · 2`.
fn section(ours: &Ours<'_>, rank: Rank, list: &[&Member], cx: &App) -> AnyElement {
    workspace::section(format!("{} · {}", rank.heading(), list.len()), cx)
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_0p5()
                .children(list.iter().map(|member| entry(ours, member))),
        )
        .into_any_element()
}

fn entry(ours: &Ours<'_>, member: &Member) -> AnyElement {
    let tokens = ours.tokens;
    let owner = ours.owner.clone();
    let nick = member.nick.clone();
    let role = note(member, ours.nick);
    let spoken = if member.away {
        format!("Open private messages with {}, away", member.nick)
    } else {
        format!("Open private messages with {}", member.nick)
    };
    let content = h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .when(member.away, |row| row.opacity(AWAY_OPACITY))
        .child(avatar::member(member.nick.clone(), tokens))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(tokens.fg)
                .child(member.nick.clone()),
        )
        .when_some(role, |row, role| {
            row.child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_size(tokens.caption_size())
                    .text_color(tokens.muted)
                    .child(role),
            )
        });
    row_button(
        SharedString::from(format!("irc-member:{}", member.nick)),
        spoken,
        content,
    )
    .on_click(move |_, _, cx| {
        let nick = nick.clone();
        _ = owner.update(cx, |_, cx| cx.emit(MemberListEvent::OpenQuery(nick)));
    })
    .into_any_element()
}

/// Away members are dimmed, and their accessible name says so.
const AWAY_OPACITY: f32 = 0.55;

#[cfg(test)]
mod tests {
    use subspace_client_irc::Member;

    use super::note;

    fn member(nick: &str, prefixes: &str) -> Member {
        Member {
            nick: nick.into(),
            prefixes: prefixes.into(),
            away: false,
            account: None,
        }
    }

    #[test]
    fn a_member_s_note_says_you_and_a_role_the_section_does_not() {
        let own = Some("Local-Operator");
        assert_eq!(
            note(&member("local-operator", "@"), own).as_deref(),
            Some("you"),
            "our own nick, matched ignoring case; the section says operator"
        );
        assert_eq!(
            note(&member("local-operator", "~@"), own).as_deref(),
            Some("you · owner")
        );
        assert_eq!(note(&member("alice", "&"), own).as_deref(), Some("admin"));
        assert_eq!(
            note(&member("bob", "%"), own).as_deref(),
            Some("half-operator")
        );
        assert_eq!(note(&member("carol", "@"), own), None);
        assert_eq!(note(&member("dave", "+"), own), None);
        assert_eq!(note(&member("erin", ""), None), None);
    }
}
