//! One transcript row, each a native `Message` with a stable identity: a
//! message under its sender's avatar and name (or,
//! continuing their group, with its time shown on hover), a compact server
//! line, or a condensed run of membership lines. Every row but the day pill
//! is tinted under the pointer, as in Slack (`super::fill`).

use std::{rc::Rc, sync::Arc};

use chrono::{Local, NaiveDate};
use gpui_kit::component::message::{MessageAlignment, MessageContent};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, Window, div, px,
};
use subspace_framework::dashboard::badge::{Pill, PillTone};
use subspace_framework::dashboard::workspace::READING_WIDTH_PX;
use subspace_framework::dashboard::{Tokens, Tone, avatar, chat_bubble};

mod marks;
mod message;
pub(crate) use message::{displayed, message, thread_message, tile_refs};

use super::fill::{RowFill, filled};
use crate::fleet::Rank;
use crate::hub::Hub;
use crate::markup::{self, Mentions, Ref, Rendered};
use crate::state::Entry;
use crate::style::Styled;
use crate::transcript::focus::TranscriptFocus;
use crate::{lines, style, text};

/// The time format of every row, matching the rest of the app.
const TIME_FORMAT: &str = "%H:%M";
/// The monospace time's size before density scaling: small enough that
/// `00:00` fits the avatar-wide gutter.
const TIME_SIZE_PX: f32 = 10.5;

/// Opens a reference clicked in rich text; rich text needs a thread-safe
/// handler.
pub(crate) type Claim = Arc<dyn Fn(&Ref, &mut Window, &mut App) + Send + Sync>;

/// What every row of one transcript shares.
pub(crate) struct RowContext {
    pub conversation: WeakEntity<crate::conversation::Conversation>,
    /// Who the channel's messages may mention.
    pub mentions: Mentions,
    /// The local date that day pills call `Today`.
    pub today: NaiveDate,
    pub claim: Claim,
    pub upload: crate::uploads::viewer::Claim,
    /// The line a search result opened, shown selected.
    pub target: Option<i64>,
    /// The hub, whose answers give issue references their look and tooltip.
    pub hub: WeakEntity<Hub>,
    /// The message and reference with keyboard focus.
    pub focus: Option<TranscriptFocus>,
    /// The focused issue reference's tooltip card.
    pub card: Option<AnyView>,
    /// The upload thumbnail with keyboard focus, by its key.
    pub upload_focus: Option<String>,
    /// Counts from this buffer's last sync, shared by every virtual row.
    pub replies: Rc<super::replies::Counts>,
    /// Request status lines from the same sync, shared the same way.
    pub statuses: Rc<crate::thread_status::Statuses>,
    /// The time a running request's clock reads to.
    pub now: chrono::DateTime<chrono::Utc>,
    /// The fleet roles on the buffer's network, which mark agents' names.
    pub roster: Rc<crate::fleet::Roster>,
    /// Which message keeps each reference's tile, from the same sync.
    pub tiles: Rc<crate::tiles::Tiles>,
    /// The inspector has a Thread tab for this conversation, so reply
    /// counts and request status lines open their thread there.
    pub threads: bool,
}

fn time(entry: &Entry) -> String {
    entry
        .line
        .time
        .with_timezone(&Local)
        .format(TIME_FORMAT)
        .to_string()
}

/// The gutter beside every row, as wide as a sender's avatar, so message
/// text, compact lines and membership runs share one leading edge.
fn gutter() -> gpui_kit::Div {
    div().flex_shrink_0().w(px(avatar::SENDER_SIZE_PX))
}

/// A row's time in quiet monospace, so times line up as a column.
fn time_text(entry: &Entry, tokens: &Tokens, cx: &App) -> gpui_kit::Div {
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.mono_size(TIME_SIZE_PX))
        .text_color(tokens.muted)
        .child(time(entry))
}

/// The time in the gutter: always shown on compact lines, only on hover
/// for a message that continues its sender's group.
fn gutter_time(
    entry: &Entry,
    hover_group: Option<&SharedString>,
    tokens: &Tokens,
    cx: &App,
) -> AnyElement {
    gutter()
        .flex()
        .justify_end()
        .whitespace_nowrap()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.mono_size(TIME_SIZE_PX))
        .text_color(tokens.muted)
        .when_some(hover_group, |time, group| {
            time.invisible()
                .group_hover(group.clone(), |style| style.visible())
        })
        .child(time(entry))
        .into_any_element()
}

/// The sender's name in strong text in their tone, a fleet agent's rank
/// mark, a notice mark, and the time in quiet monospace. A captain's or the
/// rear admiral's name takes its rank's tone.
fn header(entry: &Entry, nick: &str, rank: Option<Rank>, tokens: &Tokens, cx: &App) -> AnyElement {
    let notice = entry.line.kind == subspace_client_irc::LineKind::Notice;
    let tone = rank
        .and_then(Rank::tone)
        .unwrap_or_else(|| style::sender_tone(nick, entry.line.own));
    h_flex()
        .id(SharedString::from(format!("irc-line:{}:sender", entry.seq)))
        .test_support()
        .gap_2()
        .items_baseline()
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(tokens.row_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.tone(tone))
                .child(nick.to_owned()),
        )
        .children(rank.map(|rank| rank_mark(entry, rank)))
        .when(notice, |header| {
            header.child(
                div()
                    .text_size(tokens.caption_size())
                    .text_color(tokens.muted)
                    .child("Notice"),
            )
        })
        .child(time_text(entry, tokens, cx))
        .into_any_element()
}

/// The shared pill after a fleet agent's name, such as `CAPTAIN`, in its
/// rank's tone: captains blue, the rear admiral the accent, crew muted.
fn rank_mark(entry: &Entry, rank: Rank) -> Pill {
    let pill = Pill::new(
        SharedString::from(format!("irc-line:{}:rank", entry.seq)),
        rank.mark(),
    )
    .aria_label(rank.note());
    match rank.tone() {
        Some(tone) => pill.hue(tone),
        None => pill.tone(PillTone::Muted),
    }
}

/// The message text as one selectable text view: formatted text rewritten
/// as safe Markdown, or plain and mIRC-coded text shown exactly as sent,
/// with references. mIRC codes keep the IRC reading, so Markdown characters
/// beside them stay as typed.
fn body(
    entry: &Entry,
    rendered: Option<&Rendered>,
    context: &RowContext,
    tokens: &Tokens,
    cx: &App,
) -> AnyElement {
    let view = style::message_view(entry, &context.mentions);
    let styled = marks::marked(entry, view.styled, rendered, context, cx);
    let id = SharedString::from(format!("irc-line:{}:text", entry.seq));
    let shown = rendered
        .cloned()
        .unwrap_or_else(|| text::literal(&styled, tokens));
    let content = text::selectable(
        id,
        shown,
        view.base,
        Some(marks::handlers(
            context,
            rendered.is_none().then_some(styled),
        )),
        cx,
    );
    // Message text keeps a readable line length in a wide pane; the row's
    // fill still runs the pane's width.
    div()
        .w_full()
        .max_w(px(READING_WIDTH_PX))
        .min_w_0()
        .text_size(tokens.row_font)
        .text_color(tokens.tone(view.base))
        .when(view.italic, |body| body.italic())
        .child(content)
        .into_any_element()
}

/// The gutter of a message: the time on hover when it continues its
/// sender's group, else the sender's avatar, a captain's or the rear
/// admiral's agent avatar by `rank`.
fn message_gutter(
    entry: &Entry,
    (nick, rank): (&str, Option<Rank>),
    continues: bool,
    group: &SharedString,
    cx: &App,
) -> AnyElement {
    let tokens = &Tokens::current(cx);
    if continues {
        return gutter_time(entry, Some(group), tokens, cx);
    }
    match rank.and_then(Rank::avatar) {
        Some(agent) => avatar::agent(
            SharedString::from(format!("irc-line:{}:agent", entry.seq)),
            agent,
            nick.to_owned(),
        )
        .into_any_element(),
        None => avatar::sender(nick.to_owned(), tokens)
            .flex_shrink_0()
            .into_any_element(),
    }
}

/// A compact muted row: the time in the gutter, then `content`.
fn compact(
    id: SharedString,
    first: &Entry,
    spoken: String,
    content: AnyElement,
    (tokens, fill, cx): (&Tokens, RowFill, &App),
) -> AnyElement {
    let row = h_flex()
        .id(SharedString::from(format!("{id}:row")))
        .role(Role::Group)
        .aria_label(spoken)
        .test_support()
        .w_full()
        .min_w_0()
        .items_baseline()
        .gap_2()
        .px_2()
        .py_0p5()
        .rounded(tokens.control_radius)
        .child(gutter_time(first, None, tokens, cx))
        .child(
            div()
                .flex_1()
                .max_w(px(READING_WIDTH_PX))
                .min_w_0()
                .text_size(tokens.secondary_size())
                .child(content),
        );
    chat_bubble::message(
        id,
        MessageAlignment::Start,
        MessageContent::new().child(filled(row, fill)),
    )
    .pt_0p5()
    .into_any_element()
}

/// A server, topic, mode or error line, as compact as before.
pub(super) fn line(entry: &Entry, unread: bool, tokens: &Tokens, cx: &App) -> AnyElement {
    let view = style::line_view(entry);
    let id: SharedString = format!("irc-line:{}", entry.seq).into();
    let spoken = subspace_client_irc::format::strip(&view.styled.text);
    let text = div()
        .when(view.italic, |text| text.italic())
        .child(text::selectable(
            SharedString::from(format!("{id}:text")),
            text::literal(&view.styled, tokens),
            view.base,
            None,
            cx,
        ))
        .into_any_element();
    compact(
        id,
        entry,
        spoken,
        text,
        (tokens, RowFill::of(entry.highlight, unread, tokens), cx),
    )
}

/// A run of joins, parts, quits and nick changes as one muted line.
pub(super) fn membership(run: &[&Entry], tokens: &Tokens, cx: &App) -> AnyElement {
    let Some(first) = run.first() else {
        return div().into_any_element();
    };
    let summary = lines::membership_summary(run);
    let id: SharedString = format!("irc-members:{}", first.seq).into();
    let styled = Styled {
        text: summary.clone(),
        ..Styled::default()
    };
    let text = text::selectable(
        SharedString::from(format!("{id}:text")),
        text::literal(&styled, tokens),
        Tone::Muted,
        None,
        cx,
    );
    compact(
        id,
        first,
        summary,
        text,
        (tokens, RowFill::of(false, false, tokens), cx),
    )
}
