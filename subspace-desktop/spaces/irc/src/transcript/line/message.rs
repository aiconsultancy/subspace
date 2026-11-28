//! Message rows and their recognised issue/upload references.
use super::{
    Entry, Rendered, RowContext, RowFill, Tokens, body, filled, header, marks, markup,
    message_gutter,
};
use gpui_kit::component::message::{MessageAlignment, MessageContent};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_framework::dashboard::chat_bubble;
use subspace_framework::lucide::IconName;
pub(crate) fn message(
    original: &Entry,
    continues: bool,
    context: &RowContext,
    cx: &App,
) -> (AnyElement, Vec<String>) {
    let tokens = Tokens::current(cx);
    let shown = shown_now(original, context, cx);
    let entry = shown.as_ref().unwrap_or(original);
    let nick = entry.line.from.clone().unwrap_or_default();
    let id: SharedString = format!("irc-line:{}", entry.seq).into();
    let group: SharedString = format!("{id}:hover").into();
    let rendered = markup::rich(&entry.line.text, &context.mentions);
    let issues = markup::issues(&original.line.text, &context.mentions, rendered.as_ref());
    let refs = crate::tiles::candidates(markup::references(
        &original.line.text,
        &context.mentions,
        rendered.as_ref(),
    ));
    let toolbar = crate::transcript::actions::toolbar(entry, !issues.is_empty(), context, cx);
    let identifiers = identifiers(original, context, rendered.as_ref(), issues);
    let spoken = spoken(entry, rendered.as_ref());
    let rank = context
        .roster
        .verified(&nick, entry.line.account.as_deref());
    let leading = message_gutter(entry, (&nick, rank), continues, &group, cx);
    let column = message_column(entry, (&nick, rank), continues, &tokens, cx);
    let below = thread_facts(entry, context, &tokens, cx)
        .into_iter()
        .chain(crate::tiles::row(entry, &refs, context, cx))
        .chain(attachments(original, context, &tokens, cx))
        .chain(crate::transcript::reactions::row(entry, context))
        .chain(marks::focus_card(entry, context))
        .collect::<Vec<_>>();
    let row = row_frame((&id, group), spoken, &tokens).child(leading);
    let focus = marks::focus_ring(entry, context, &tokens, cx);
    let seq = entry.seq;
    let selected = context.selected_thread == Some(seq) || context.target == Some(seq);
    let row = message_click(hovered(row, seq, context), entry, context);
    let row = measure_focused(row, entry, context);
    let row = filled(row, RowFill::of(selected, &tokens));
    let content = body(entry, rendered.as_ref(), context, &tokens, cx)
        .when(entry.line.text.is_empty(), |body| body.h_0())
        .menu_host(move |body| {
            row.test_support()
                .child(column.child(body).children(below))
                .children(focus)
                .children(toolbar)
        });
    let content = if context.actions.reply && entry.line.id.is_some() {
        let conversation = context.conversation.clone();
        content
            .menu_action(move || crate::transcript::actions::reply_item(conversation.clone(), seq))
    } else {
        content
    };
    (message_frame(id, content, continues), identifiers)
}

fn message_column(
    entry: &Entry,
    (nick, rank): (&str, Option<crate::fleet::Rank>),
    continues: bool,
    tokens: &Tokens,
    cx: &App,
) -> gpui_kit::Div {
    let reply = entry.line.reply_to.is_some();
    v_flex()
        .relative()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .when(!continues, |column| {
            column.child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(header(entry, nick, rank, tokens, cx))
                    .children(reply.then(|| reply_mark(tokens))),
            )
        })
        .children((continues && reply).then(|| reply_mark(tokens)))
}

fn row_frame(
    (id, group): (&SharedString, SharedString),
    spoken: String,
    tokens: &Tokens,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    h_flex()
        .id(SharedString::from(format!("{id}:row")))
        .group(group)
        .role(Role::Group)
        .aria_label(spoken)
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .px_2()
        .py_1()
        .relative()
        .rounded(tokens.control_radius)
}

fn reply_mark(tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .gap_1()
        .flex_shrink_0()
        .text_color(tokens.muted)
        .text_size(tokens.caption_size())
        .child(div().child(Icon::from(IconName::Reply).xsmall()))
        .child("Thread reply")
}

/// `original` as the hub shows it now: with resolved uploads' text hidden,
/// when any is ([`displayed`]).
fn shown_now(original: &Entry, context: &RowContext, cx: &App) -> Option<Entry> {
    let hub = context.hub.upgrade();
    displayed(original, hub.as_ref().map(|hub| hub.read(cx)))
}

/// A message as the inspector's Thread tab shows it: its sender, rank and
/// time unless it `continues` their group, then its text with its
/// references, as the transcript draws them. No avatar, tiles, thread facts
/// or attachments: the transcript keeps those.
pub(crate) fn thread_message(
    original: &Entry,
    continues: bool,
    context: &RowContext,
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let shown = shown_now(original, context, cx);
    let entry = shown.as_ref().unwrap_or(original);
    let nick = entry.line.from.clone().unwrap_or_default();
    let rendered = markup::rich(&entry.line.text, &context.mentions);
    let rank = context
        .roster
        .verified(&nick, entry.line.account.as_deref());
    v_flex()
        .id(SharedString::from(format!("irc-thread-line:{}", entry.seq)))
        .role(Role::Group)
        .aria_label(spoken(entry, rendered.as_ref()))
        .test_support()
        .w_full()
        .min_w_0()
        .gap_0p5()
        .when(!continues, |column| {
            column.child(header(entry, &nick, rank, &tokens, cx))
        })
        .when(!entry.line.text.is_empty(), |column| {
            column.child(body(entry, rendered.as_ref(), context, &tokens, cx))
        })
        .children(crate::transcript::reactions::read_only(entry, context))
        .into_any_element()
}

/// Pointer movement below this threshold still counts as selecting a message.
const CLICK_SLOP_PX: f32 = 4.;

fn message_click(
    row: gpui_kit::Stateful<gpui_kit::Div>,
    entry: &Entry,
    context: &RowContext,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let Some(id) = entry.line.id.as_ref().filter(|_| context.threads).cloned() else {
        return row;
    };
    let conversation = context.conversation.clone();
    row.on_click(move |event, _, cx| {
        let ClickEvent::Mouse(event) = event else {
            return;
        };
        // A text drag or a multi-click selection is not a thread selection.
        let moved = event.up.position - event.down.position;
        if event.up.button == MouseButton::Left
            && event.down.click_count == 1
            && moved.x.abs() < px(CLICK_SLOP_PX)
            && moved.y.abs() < px(CLICK_SLOP_PX)
        {
            _ = conversation.update(cx, |_, cx| {
                cx.emit(crate::conversation::ConversationEvent::PreviewThread(
                    id.clone(),
                ));
            });
        }
    })
}

/// The row tells its conversation when the pointer is over it, which
/// shows its toolbar.
fn hovered<E: gpui_kit::StatefulInteractiveElement>(row: E, seq: i64, context: &RowContext) -> E {
    let conversation = context.conversation.clone();
    row.on_hover(move |hovered, _, cx| {
        let hovered = *hovered;
        _ = conversation.update(cx, |this, cx| {
            crate::conversation::hover(this, seq, hovered, cx)
        });
    })
}

/// The issues message `entry`'s text names, as its row reads them.
pub(crate) fn named_issues(entry: &Entry, mentions: &crate::markup::Mentions) -> Vec<String> {
    let rendered = markup::rich(&entry.line.text, mentions);
    markup::issues(&entry.line.text, mentions, rendered.as_ref())
}

/// The keyboard-focused row anchors the `r` shortcut. The toolbar measures
/// its own reaction button independently.
fn measure_focused<E: gpui_kit::base::ElementExt>(
    row: E,
    entry: &Entry,
    context: &RowContext,
) -> E {
    if !context.focus.is_some_and(|focus| focus.seq == entry.seq) {
        return row;
    }
    let anchor = context.react_anchors.keyboard.clone();
    row.on_prepaint(move |bounds, _, cx| {
        anchor.update(cx, |anchor, cx| anchor.set_bounds(bounds, cx));
    })
}

/// What a message's thread says under it: the reply count, then the status
/// line of a request in progress or its settled line once it ended. Where
/// the inspector has a Thread tab, either opens the thread, and a request's
/// line carries the count itself.
fn thread_facts(entry: &Entry, context: &RowContext, tokens: &Tokens, cx: &App) -> Vec<AnyElement> {
    let link = crate::transcript::replies::link(entry, context);
    let on_status = context.statuses.contains_key(&entry.seq);
    crate::transcript::replies::metadata(
        entry,
        &context.replies,
        (link.as_ref(), on_status),
        tokens,
    )
    .into_iter()
    .chain(crate::transcript::status::line(
        entry,
        (&context.statuses, link.as_ref()),
        context.now,
        tokens,
        cx,
    ))
    .collect()
}

/// The message's upload thumbnails, from its text as sent.
fn attachments(
    original: &Entry,
    context: &RowContext,
    tokens: &Tokens,
    cx: &App,
) -> Option<AnyElement> {
    let focused = context.upload_focus.as_deref();
    crate::uploads::tiles(&original.line.text, context, focused, tokens, cx)
}

/// The issues and pull requests message `original` names that could show
/// a tile, read exactly as its row reads them.
pub(crate) fn tile_refs(
    original: &Entry,
    hub: Option<&crate::hub::Hub>,
    mentions: &crate::markup::Mentions,
) -> Vec<crate::markup::Ref> {
    let shown = displayed(original, hub);
    let entry = shown.as_ref().unwrap_or(original);
    let rendered = markup::rich(&entry.line.text, mentions);
    crate::tiles::candidates(markup::references(
        &original.line.text,
        mentions,
        rendered.as_ref(),
    ))
}

/// `entry` with the sent text of every resolved upload hidden, when any is:
/// its thumbnail under the text says the same.
pub(crate) fn displayed(entry: &Entry, hub: Option<&crate::hub::Hub>) -> Option<Entry> {
    let text = crate::uploads::shown_text(hub, &entry.line.text);
    (text != entry.line.text).then(|| {
        let mut shown = entry.clone();
        shown.line.text = text;
        shown
    })
}

/// Every key the row asks the hub about: its `issues`, then its pull
/// requests and uploads.
fn identifiers(
    entry: &Entry,
    context: &RowContext,
    rendered: Option<&Rendered>,
    mut keys: Vec<String>,
) -> Vec<String> {
    keys.extend(markup::pull_requests(
        &entry.line.text,
        &context.mentions,
        rendered,
    ));
    keys.extend(subspace_api_client::chat_uploads::references(
        &entry.line.text,
    ));
    keys
}

fn message_frame(id: SharedString, row: impl gpui_kit::IntoElement, continues: bool) -> AnyElement {
    chat_bubble::message(
        id,
        MessageAlignment::Start,
        MessageContent::new().child(row),
    )
    .when(!continues, |message| message.pt_2())
    .into_any_element()
}

fn spoken(entry: &Entry, rendered: Option<&Rendered>) -> String {
    let nick = entry.line.from.as_deref().unwrap_or_default();
    let shown = rendered.map_or_else(
        || subspace_client_irc::format::strip(&entry.line.text),
        |r| r.plain.clone(),
    );
    format!("{nick}: {shown}")
}
