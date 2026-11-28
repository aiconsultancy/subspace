//! Fixed-height notification rows in the Comms inbox.

use std::ops::Range;
use std::rc::Rc;

use chrono::Local;
use gpui_kit::component::{Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, IntoElement as _, ParentElement as _, Styled as _, WeakEntity,
    uniform_list,
};
use subspace_framework::dashboard::action::row_button;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, truncated, workspace};

use super::{badge, open};
use crate::Irc;
use crate::comms::Summary;

/// Height as a multiple of the theme's one-line list row.
const ROW_HEIGHT: f32 = 3.;

pub(super) fn draw(rows: Vec<Rc<Summary>>, board: &Irc) -> AnyElement {
    let owner = board.me.clone();
    let selected = board.comms.selected.clone();
    uniform_list(
        "irc-comms-rows",
        rows.len(),
        move |range: Range<usize>, _, cx| {
            range
                .map(|index| {
                    let row = &rows[index];
                    summary(row, selected.as_ref() == Some(&row.key), &owner, cx)
                })
                .collect::<Vec<_>>()
        },
    )
    .track_scroll(&board.comms.scroll)
    .size_full()
    .into_any_element()
}

fn summary(row: &Summary, selected: bool, owner: &WeakEntity<Irc>, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let channel = crate::rows::title(&row.key.buffer);
    let unread = row.unread();
    let name = format!(
        "{channel}, {}, {}, {unread} unread",
        row.author, row.subject
    );
    let key = row.key.clone();
    let owner = owner.clone();
    let id = format!("irc-comms:{}:{}", row.key.buffer.network.0, row.key.root);
    row_button(id, name, content(row, channel, &tokens))
        .h(tokens.row_height * ROW_HEIGHT)
        .px(workspace::text_edge(&tokens))
        .selected(selected)
        .on_click(move |_, window, cx| {
            _ = owner.update(cx, |board, cx| open(board, &key, window, cx));
        })
        .into_any_element()
}

fn content(row: &Summary, channel: &str, tokens: &Tokens) -> impl gpui_kit::IntoElement {
    let unread = row.unread();
    let at = row.time.with_timezone(&Local);
    let when = if at.date_naive() == Local::now().date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%-d %b").to_string()
    };
    let preview = if row.author.is_empty() || row.subject == row.preview {
        row.author.clone()
    } else {
        format!("{} · {}", row.author, row.preview)
    };
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .gap_2()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(
                    Icon::from(if row.mentions {
                        IconName::AtSign
                    } else {
                        IconName::Reply
                    })
                    .xsmall(),
                )
                .child(truncated::text("irc-comms-channel", channel.to_owned()).flex_1())
                .child(when)
                .children((unread > 0).then(|| badge(unread, row.unread_mention(), tokens))),
        )
        .child(
            truncated::text("irc-comms-subject", row.subject.clone())
                .text_size(tokens.row_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong),
        )
        .child(
            truncated::text("irc-comms-excerpt", preview)
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted),
        )
}
