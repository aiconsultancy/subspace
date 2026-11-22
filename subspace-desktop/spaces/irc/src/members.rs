//! A channel's member list, for a host that shows it beside the
//! conversation, as the workspace's inspector does: operators, voiced and
//! other members, each with a small avatar, the nick and a quiet note of
//! the roles the Space knows (`you`, a fleet's `captain` or `rear admiral`,
//! `owner`, `admin`, `half-operator`), the
//! member's status line, if any, quietly under the nick, and dimmed while
//! away. The host
//! supplies the heading, with the count ([`MemberList::count`]), and the
//! scrolling. Choosing a member asks the host to open a private conversation
//! ([`MemberListEvent`]).

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div,
};
use subspace_client_irc::{BufferKey, Member};
use subspace_framework::dashboard::action::row_button;
use subspace_framework::dashboard::{Tokens, avatar, truncated, workspace};

use crate::fleet::{self, Roster};
use crate::hub::Hub;
use crate::state::Statuses;

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

/// The quiet note after a member's nick: `you` for our own nick, the
/// fleet `rank` the Space knows for it, then the prefix's role where it
/// says more than the section heading does (an owner, admin or
/// half-operator among the operators). `None` when there is nothing to add.
pub(crate) fn note(
    member: &Member,
    own_nick: Option<&str>,
    rank: Option<fleet::Rank>,
) -> Option<String> {
    let you = own_nick.is_some_and(|nick| nick.eq_ignore_ascii_case(&member.nick));
    let role = match member.prefixes.chars().next() {
        Some('~') => Some("owner"),
        Some('&') => Some("admin"),
        Some('%') => Some("half-operator"),
        _ => None,
    };
    let parts: Vec<&str> = [you.then_some("you"), rank.map(fleet::Rank::note), role]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
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
        let network = state.network(&self.buffer.network);
        let own = network.map(|network| network.nick.clone());
        let statuses = network
            .map(|network| network.statuses.clone())
            .unwrap_or_default();
        let roster = self.hub.read(cx).roster(&self.buffer.network, cx);
        let ours = Ours {
            owner: &self.me,
            nick: own.as_deref(),
            statuses: &statuses,
            roster: &roster,
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
/// network, the network's nick statuses and fleet roles, and the tokens.
struct Ours<'a> {
    owner: &'a WeakEntity<MemberList>,
    nick: Option<&'a str>,
    statuses: &'a Statuses,
    roster: &'a Roster,
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

/// One member's row. With a status, the row grows to a second, quiet line
/// under the nick, so the status has the row's width.
fn entry(ours: &Ours<'_>, member: &Member) -> AnyElement {
    let owner = ours.owner.clone();
    let nick = member.nick.clone();
    let status = ours.statuses.get(&member.nick);
    row_button(
        SharedString::from(format!("irc-member:{}", member.nick)),
        spoken(member, status),
        content(ours, member, status),
    )
    .when(status.is_some(), |row| row.h_auto().py_1())
    .on_click(move |_, _, cx| {
        let nick = nick.clone();
        _ = owner.update(cx, |_, cx| cx.emit(MemberListEvent::OpenQuery(nick)));
    })
    .into_any_element()
}

/// A member row's accessible name: what choosing it does, the member's
/// status, then whether they are away.
fn spoken(member: &Member, status: Option<&str>) -> String {
    let status = status.map(|text| format!(", {text}")).unwrap_or_default();
    let away = if member.away { ", away" } else { "" };
    format!("Open private messages with {}{status}{away}", member.nick)
}

/// The avatar, then the nick with its quiet note and, under them, the
/// member's status as one quiet line cut with an ellipsis. Dimmed while
/// away.
fn content(ours: &Ours<'_>, member: &Member, status: Option<&str>) -> Div {
    let tokens = ours.tokens;
    let rank = ours
        .roster
        .verified(&member.nick, member.account.as_deref());
    let role = note(member, ours.nick, rank);
    let name = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(tokens.fg)
                .child(member.nick.clone().test_text(member.nick.clone())),
        )
        .when_some(role, |row, role| {
            row.child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_size(tokens.caption_size())
                    .text_color(tokens.muted)
                    .child(role.clone().test_text(role)),
            )
        });
    let status = status.map(|text| {
        truncated::styled(
            SharedString::from(format!("irc-member-status:{}", member.nick)),
            text.to_owned(),
            text.to_owned().test_text(text.to_owned()),
        )
        .w_full()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
    });
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .when(member.away, |row| row.opacity(AWAY_OPACITY))
        .child(face(member, rank, tokens))
        .child(v_flex().flex_1().min_w_0().child(name).children(status))
}

/// A member's avatar: a captain's or the rear admiral's agent avatar, or a
/// person's (and a crew member's) square initials.
fn face(member: &Member, rank: Option<fleet::Rank>, tokens: &Tokens) -> AnyElement {
    match rank.and_then(fleet::Rank::avatar) {
        Some(agent) => avatar::agent(
            SharedString::from(format!("irc-member-agent:{}", member.nick)),
            agent,
            member.nick.clone(),
        )
        .small()
        .into_any_element(),
        None => avatar::member(member.nick.clone(), tokens).into_any_element(),
    }
}

/// Away members are dimmed, and their accessible name says so.
const AWAY_OPACITY: f32 = 0.55;

#[cfg(test)]
mod tests {
    use subspace_client_irc::Member;

    use super::{note, spoken};
    use crate::fleet::Rank;

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
            note(&member("local-operator", "@"), own, None).as_deref(),
            Some("you"),
            "our own nick, matched ignoring case; the section says operator"
        );
        assert_eq!(
            note(&member("local-operator", "~@"), own, None).as_deref(),
            Some("you · owner")
        );
        assert_eq!(
            note(&member("alice", "&"), own, None).as_deref(),
            Some("admin")
        );
        assert_eq!(
            note(&member("bob", "%"), own, None).as_deref(),
            Some("half-operator")
        );
        assert_eq!(note(&member("carol", "@"), own, None), None);
        assert_eq!(note(&member("dave", "+"), own, None), None);
        assert_eq!(note(&member("erin", ""), None, None), None);
    }

    #[test]
    fn a_fleet_agent_s_note_says_its_rank() {
        let own = Some("mira");
        assert_eq!(
            note(&member("aurora-lyra", ""), own, Some(Rank::Captain)).as_deref(),
            Some("captain")
        );
        assert_eq!(
            note(
                &member("aurora-admiral", "~@"),
                own,
                Some(Rank::RearAdmiral)
            )
            .as_deref(),
            Some("rear admiral · owner")
        );
        assert_eq!(
            note(&member("Mira", ""), own, Some(Rank::Captain)).as_deref(),
            Some("you · captain")
        );
    }

    #[test]
    fn a_member_s_spoken_name_says_their_status_then_away() {
        let mut galactica = member("galactica", "");
        assert_eq!(
            spoken(&galactica, None),
            "Open private messages with galactica"
        );
        assert_eq!(
            spoken(&galactica, Some("Galactica is thinking…")),
            "Open private messages with galactica, Galactica is thinking…"
        );
        galactica.away = true;
        assert_eq!(
            spoken(&galactica, Some("Galactica is thinking…")),
            "Open private messages with galactica, Galactica is thinking…, away"
        );
        assert_eq!(
            spoken(&galactica, None),
            "Open private messages with galactica, away"
        );
    }
}
