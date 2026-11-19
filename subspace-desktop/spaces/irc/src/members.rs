//! A channel's member list, self-contained so any host can show it beside
//! the conversation: a header with the count, then operators, voiced and
//! other members, each with a small avatar and dimmed while away, in its own
//! scrolling region. Choosing a member asks the host to open a private
//! conversation ([`MemberListEvent`]).

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, TestSupportExt as _, WeakEntity, Window, div, px,
};
use subspace_client_irc::{BufferKey, Member};
use subspace_framework::dashboard::action::{Intent, icon_button, row_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, avatar};

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

/// What a member list asks its host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemberListEvent {
    /// Open a private conversation with this member.
    OpenQuery(String),
    /// Close the list.
    Close,
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
}

impl Render for MemberList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let members = self
            .hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .map(|held| held.members.clone())
            .unwrap_or_default();
        let owner = self.me.clone();
        // As wide as its host: the dock owns the width, which the user
        // resizes and the saved layout restores.
        v_flex()
            .id("irc-members")
            .test_support()
            .size_full()
            .min_h_0()
            .bg(tokens.surface)
            .border_l_1()
            .border_color(tokens.line)
            .child(header(owner, members.len(), &tokens))
            .child(
                v_flex()
                    .id("irc-members-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .gap_0p5()
                    .children(
                        groups(&members)
                            .into_iter()
                            .map(|(rank, list)| section(&self.me, rank, &list, &tokens)),
                    ),
            )
    }
}

/// `Members · N` and the button that hides the list.
fn header(owner: WeakEntity<MemberList>, count: usize, tokens: &Tokens) -> AnyElement {
    // The shared dock's medium tab row is 32 px tall.
    const TAB_HEIGHT: gpui_kit::Pixels = px(32.);
    h_flex()
        .id("irc-members-heading")
        .test_support()
        .flex_shrink_0()
        .min_h(TAB_HEIGHT)
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(tokens.line)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(tokens.secondary_size())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(format!("Members · {count}")),
        )
        .child(
            icon_button(
                "irc-members-close",
                Intent::Quiet,
                IconName::X,
                "Hide members",
            )
            .compact()
            .on_click(move |_, _, cx| {
                _ = owner.update(cx, |_, cx| cx.emit(MemberListEvent::Close));
            }),
        )
        .into_any_element()
}

fn section(
    owner: &WeakEntity<MemberList>,
    rank: Rank,
    list: &[&Member],
    tokens: &Tokens,
) -> AnyElement {
    v_flex()
        .w_full()
        .min_w_0()
        .child(
            div()
                .px_1()
                .pt_2()
                .pb_0p5()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(format!("{} · {}", rank.heading(), list.len())),
        )
        .children(list.iter().map(|member| entry(owner, member, tokens)))
        .into_any_element()
}

fn entry(owner: &WeakEntity<MemberList>, member: &Member, tokens: &Tokens) -> AnyElement {
    let owner = owner.clone();
    let nick = member.nick.clone();
    let prefix = member
        .prefixes
        .chars()
        .next()
        .map(String::from)
        .unwrap_or_default();
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
        .when(!prefix.is_empty(), |row| {
            row.child(div().flex_shrink_0().text_color(tokens.muted).child(prefix))
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
