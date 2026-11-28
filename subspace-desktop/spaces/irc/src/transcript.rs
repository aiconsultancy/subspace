//! A conversation's transcript: its rows in Kit's `MessageScroller` (the
//! older-history control, day dividers, messages grouped by sender, compact
//! server lines and membership runs) and the line above the composer
//! ([`activity`]: agents' status lines and who is typing). Hosted by
//! [`crate::conversation::Conversation`].

pub(crate) mod actions;
pub(crate) mod activity;
mod fill;
pub(crate) mod focus;
mod line;
pub(crate) mod reactions;
pub(crate) mod replies;
mod scroll;
pub(crate) mod status;
mod targets;
mod viewport;

use chrono::{NaiveDate, TimeZone};
use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, Entity, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    div, px,
};
use subspace_client_irc::{BufferKey, Target};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{ActionState, Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use crate::hub::Hub;
use crate::lines::{self, Item};
use crate::markup::{self, Mentions};
use crate::state::{Buffer, Entry, HistoryState};
use crate::style;
use crate::text;

pub(crate) use line::{Claim, RowContext, displayed, named_issues, thread_message, tile_refs};
pub(crate) use scroll::{Row, Scroller, identities, load_older, row_of};
pub(crate) use targets::message_targets;
pub(crate) use viewport::scroller;

/// Transcript separators use a hairline.
const RULE_PX: f32 = 1.;

/// The scroller's rows for `buffer`: the older-history control while the
/// server may have more, then the transcript items.
pub(crate) fn rows<Tz: TimeZone>(
    buffer: &Buffer,
    key: &BufferKey,
    zone: &Tz,
    threads: bool,
) -> Vec<Row> {
    let conversation = key.target != Target::Server;
    let (hidden, earlier, unread_thread) = replies::placement(buffer, key, threads);
    let history = conversation && buffer.history.state != HistoryState::Complete;
    let first_unread = buffer
        .entries
        .iter()
        .position(|entry| {
            !hidden.contains(&entry.seq) && buffer.shown_unread().contains(&entry.seq)
        })
        .or(unread_thread);
    history
        .then_some(Row::History)
        .into_iter()
        .chain(
            lines::items(&buffer.entries, &hidden, zone, conversation)
                .into_iter()
                .flat_map(|item| {
                    with_unread_divider(item, buffer, first_unread, &earlier)
                        .into_iter()
                        .flatten()
                }),
        )
        .collect()
}

fn with_unread_divider(
    item: Item,
    buffer: &Buffer,
    first: Option<usize>,
    earlier: &std::collections::HashSet<usize>,
) -> [Option<Row>; 2] {
    match item {
        Item::Message { at, .. } if earlier.contains(&at) => [
            (first == Some(at)).then(|| Row::UnreadDivider(buffer.entries[at].seq)),
            Some(Row::EarlierThread(at)),
        ],
        Item::Message { at, .. } if Some(at) == first => [
            Some(Row::UnreadDivider(buffer.entries[at].seq)),
            Some(Row::Item(Item::Message {
                at,
                continues: false,
            })),
        ],
        Item::Line(at) if Some(at) == first => [
            Some(Row::UnreadDivider(buffer.entries[at].seq)),
            Some(Row::Item(item)),
        ],
        _ => [None, Some(Row::Item(item))],
    }
}

/// The same displayed text as a loaded transcript row, even while virtualised.
pub(crate) fn find_document(
    row: Row,
    held: &Buffer,
    (mentions, hub): (&Mentions, Option<&Hub>),
    cx: &App,
) -> Option<subspace_framework::dashboard::find_in_page::Document> {
    let tokens = Tokens::current(cx);
    let (id, shown) = match row {
        Row::Item(Item::Message { at, .. }) => {
            let original = held.entries.get(at)?;
            let displayed = line::displayed(original, hub);
            let entry = displayed.as_ref().unwrap_or(original);
            let styled = style::message_view(entry, mentions).styled;
            let shown = markup::rich(&entry.line.text, mentions)
                .unwrap_or_else(|| text::literal(&styled, &tokens));
            (format!("irc-line:{}:text", entry.seq), shown)
        }
        Row::Item(Item::Line(at)) => {
            let entry = held.entries.get(at)?;
            (
                format!("irc-line:{}:text", entry.seq),
                text::literal(&style::line_view(entry).styled, &tokens),
            )
        }
        Row::Item(Item::Membership { first, last }) => {
            let run: Vec<_> = held
                .entries
                .iter()
                .skip(first)
                .take(last - first + 1)
                .collect();
            let first = *run.first()?;
            let styled = style::Styled {
                text: lines::membership_summary(&run),
                ..Default::default()
            };
            (
                format!("irc-members:{}:text", first.seq),
                text::literal(&styled, &tokens),
            )
        }
        _ => return None,
    };
    Some(
        subspace_framework::dashboard::find_in_page::Document::markdown(
            gpui_kit::SharedString::from(id),
            shown.markdown,
        ),
    )
}

/// Row `row` of `held`, the buffer `buffer` on `hub`.
pub(crate) fn row(
    row: Row,
    held: &Buffer,
    origin: (&Entity<Hub>, &BufferKey),
    context: &RowContext,
    cx: &App,
) -> (AnyElement, Vec<String>) {
    let tokens = Tokens::current(cx);
    let entry = |at: usize| held.entries.get(at);
    let element = match row {
        Row::History => history_control(origin, held.history.state == HistoryState::Loading),
        Row::EarlierThread(at) => replies::earlier_thread(&held.entries[at], held, context),
        Row::UnreadDivider(seq) => unread_divider(seq, held.shown_unread().len(), &tokens),
        Row::Item(Item::Day(date)) => day_separator(date, context.today, &tokens),
        Row::Item(Item::Message { at, continues }) => {
            return entry(at)
                .map(|entry| line::message(entry, continues, context, cx))
                .unwrap_or_else(|| (div().into_any_element(), Vec::new()));
        }
        Row::Item(Item::Line(at)) => entry(at)
            .map(|entry| line::line(entry, context.target == Some(entry.seq), &tokens, cx))
            .unwrap_or_else(|| div().into_any_element()),
        Row::Item(Item::Membership { first, last }) => {
            let run: Vec<&Entry> = held.entries.range(first..=last).collect();
            line::membership(&run, &tokens, cx)
        }
    };
    (element, Vec::new())
}

/// `Today`, `Yesterday` or the date, as quiet text between two hairlines.
fn day_separator(date: NaiveDate, today: NaiveDate, tokens: &Tokens) -> AnyElement {
    let label = lines::day_label(date, today);
    let rule = || div().flex_1().h(px(RULE_PX)).bg(tokens.line);
    h_flex()
        .id(gpui_kit::SharedString::from(format!("irc-day:{date}")))
        .role(Role::Label)
        .aria_label(label.clone())
        .test_support()
        .w_full()
        .gap_3()
        .px_2()
        .py_2()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .child(rule())
        .child(div().flex_shrink_0().child(label))
        .child(rule())
        .into_any_element()
}

/// The divider's words, in capitals as section labels are, with how many
/// messages arrived since the conversation was last read, and its
/// accessible name.
fn unread_label(count: usize) -> (String, String) {
    let noun = if count == 1 { "message" } else { "messages" };
    (
        format!("NEW SINCE YOU LEFT · {count}"),
        format!("{count} new {noun} since you left"),
    )
}

/// The rule and label above the first message that arrived since the
/// conversation was last read.
fn unread_divider(seq: i64, count: usize, tokens: &Tokens) -> AnyElement {
    let (label, spoken) = unread_label(count);
    h_flex()
        .id(gpui_kit::SharedString::from(format!("irc-unread:{seq}")))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .w_full()
        .gap_2()
        .px_2()
        .pt_2()
        .text_size(tokens.caption_size())
        .text_color(tokens.accent)
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .child(label.clone().test_text(label)),
        )
        .child(div().flex_1().h(px(RULE_PX)).bg(tokens.accent))
        .into_any_element()
}

fn history_control(origin: (&Entity<Hub>, &BufferKey), loading: bool) -> AnyElement {
    let (hub, buffer) = (origin.0.downgrade(), origin.1.clone());
    let state = if loading {
        ActionState::Busy
    } else {
        ActionState::Idle
    };
    let label = state.label([
        "Load older messages",
        "Loading older messages…",
        "Load older messages",
        "Load older messages",
    ]);
    h_flex()
        .w_full()
        .justify_center()
        .py_1()
        .child(
            state
                .apply(action_button(
                    "irc-history",
                    Intent::Quiet,
                    Some(IconName::Clock),
                    label.to_owned(),
                ))
                .on_click(move |_, _, cx| {
                    if let Some(hub) = hub.upgrade() {
                        load_older(&hub, &buffer, cx);
                    }
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_unread_divider_says_how_many_arrived_since_you_left() {
        assert_eq!(
            unread_label(3),
            (
                "NEW SINCE YOU LEFT · 3".to_owned(),
                "3 new messages since you left".to_owned()
            )
        );
        assert_eq!(unread_label(1).1, "1 new message since you left");
    }

    use chrono::Utc;
    use subspace_client_irc::{Event, LineKind};

    use super::{Row, rows, unread_label};
    use crate::fixtures::{at, channel, config, line, message};
    use crate::lines::Item;
    use crate::state::IrcState;

    #[test]
    fn divider_follows_the_remote_read_position_and_breaks_a_sender_group() {
        let room = channel("#subspace");
        let mut state = IrcState::new(&[config()], Vec::new());
        state.restore(&room, vec![message(&room, "alice", "stored", 0)]);
        state.apply(
            Event::Line(message(&room, "alice", "read elsewhere", 1)),
            at(1),
        );
        state.apply(Event::Line(message(&room, "alice", "still new", 2)), at(2));
        state.apply(
            Event::Line(message(&room, "subspace", "my reply", 3)),
            at(3),
        );
        state.apply(
            Event::Line(line(&room, LineKind::Join, "bob", "joined", 4)),
            at(4),
        );
        state.apply(
            Event::ReadMarker {
                buffer: room.clone(),
                time: at(1),
            },
            at(4),
        );

        let held = state.buffer(&room).unwrap();
        let seq = held.entries[2].seq;
        assert_eq!(held.unread_entries.len(), 1);
        assert!(held.unread_entries.contains(&seq));
        let rows = rows(held, &room, &Utc, false);
        let boundary = rows.iter().position(|row| *row == Row::UnreadDivider(seq));
        assert_eq!(
            boundary.map(|at| rows[at + 1]),
            Some(Row::Item(Item::Message {
                at: 2,
                continues: false
            }))
        );

        state.switch_unread(None, Some(&room));
        state.mark_read(&room);
        let held = state.buffer(&room).unwrap();
        assert_eq!(held.shown_unread().len(), 1);
        assert_eq!(held.unread, 0);
        state.switch_unread(Some(&room), None);
        let held = state.buffer(&room).unwrap();
        assert!(held.shown_unread().is_empty());
    }
}
