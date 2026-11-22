//! Message rows and their recognised issue/upload references.
use super::{
    Entry, Rendered, RowContext, RowFill, Tokens, body, filled, header, marks, markup,
    message_gutter,
};
use gpui_kit::component::message::{MessageAlignment, MessageContent};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};
use subspace_framework::dashboard::chat_bubble;
pub(crate) fn message(
    original: &Entry,
    continues: bool,
    unread: bool,
    context: &RowContext,
    cx: &App,
) -> (AnyElement, Vec<String>) {
    let tokens = Tokens::current(cx);
    let hub = context.hub.upgrade();
    let shown = displayed(original, hub.as_ref().map(|hub| hub.read(cx)));
    let entry = shown.as_ref().unwrap_or(original);
    let nick = entry.line.from.clone().unwrap_or_default();
    let id: SharedString = format!("irc-line:{}", entry.seq).into();
    let group: SharedString = format!("{id}:hover").into();
    let rendered = markup::rich(&entry.line.text, &context.mentions);
    let identifiers = identifiers(original, context, rendered.as_ref());
    let spoken = spoken(entry, rendered.as_ref());
    let leading = message_gutter(entry, &nick, continues, &group, cx);
    let column = v_flex()
        .relative()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .when(!continues, |column| {
            column.child(header(entry, &nick, &tokens, cx))
        })
        .when(!entry.line.text.is_empty(), |column| {
            column.child(body(entry, rendered.as_ref(), context, &tokens, cx))
        })
        .children(thread_facts(entry, context, &tokens))
        .children(crate::issues::tiles(
            entry,
            &identifiers,
            context,
            &tokens,
            cx,
        ))
        .children(attachments(original, context, &tokens, cx))
        .children(marks::focus_card(entry, context));
    let row = h_flex()
        .id(SharedString::from(format!("{id}:row")))
        .group(group)
        .role(Role::Group)
        .aria_label(spoken)
        .test_support()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .px_2()
        .py_0p5()
        .relative()
        .rounded(tokens.control_radius)
        .child(leading)
        .child(column)
        .children(marks::focus_ring(entry, context, &tokens, cx));
    let selected = entry.highlight || context.target == Some(entry.seq);
    let row = filled(row, RowFill::of(selected, unread, &tokens));
    let row = message_frame(id, row, continues);
    (row, identifiers)
}

/// What a message's thread says under it: the reply count, then the live
/// status line of a request still in progress.
fn thread_facts(entry: &Entry, context: &RowContext, tokens: &Tokens) -> Vec<AnyElement> {
    crate::transcript::replies::metadata(entry, &context.replies, tokens)
        .into_iter()
        .chain(crate::transcript::status::line(
            entry,
            &context.statuses,
            tokens,
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

fn identifiers(entry: &Entry, context: &RowContext, rendered: Option<&Rendered>) -> Vec<String> {
    let mut keys = markup::issues(&entry.line.text, &context.mentions, rendered);
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
