//! The Comms navigator entry and notification inbox.

use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _, h_flex};
use gpui_kit::{
    App, Context, FontWeight, IntoElement as _, ParentElement as _, Styled as _,
    UniformListScrollHandle, Window, div,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};
use subspace_framework::dashboard::{Tokens, feedback, workspace};

use crate::comms::{Key, Summary};
use crate::{Irc, dock, reveal};

mod rows;

const BADGE_LIMIT: usize = 99;
const NAME: &str = "Comms";

#[derive(Default)]
pub(crate) struct Comms {
    pub active: bool,
    pub(crate) selected: Option<Key>,
    filter: usize,
    unread_only: bool,
    scroll: UniformListScrollHandle,
}

impl Comms {
    fn accepts(&self, row: &Summary) -> bool {
        let kind = match self.filter {
            1 => row.mentions,
            2 => row.replies,
            _ => true,
        };
        kind && (!self.unread_only || row.unread() != 0 || self.selected.as_ref() == Some(&row.key))
    }

    pub(crate) fn close(&mut self) -> bool {
        std::mem::take(&mut self.active)
    }
}

pub(super) fn nav(board: &Irc, cx: &App) -> workspace::NavRow {
    let (count, mentions) = board
        .hub
        .as_ref()
        .map_or((0, false), |hub| hub.read(cx).comms.unread());
    let owner = board.me.clone();
    let mut row = workspace::NavRow::new("irc-comms-nav", NAME)
        .icon(IconName::Inbox)
        .selected(board.comms.active)
        .strong(count != 0)
        .on_click(move |_, _, cx| {
            _ = owner.update(cx, |board, cx| {
                board.comms.active = true;
                super::publish_visible(board, cx);
                board.find_revision = board.find_revision.wrapping_add(1);
                cx.notify();
            });
        });
    if count != 0 {
        row = row.trailing(badge(count, mentions, &Tokens::current(cx)));
    }
    row
}

fn badge(count: usize, mention: bool, tokens: &Tokens) -> impl gpui_kit::IntoElement {
    let count = if count > BADGE_LIMIT {
        "99+".to_owned()
    } else {
        count.to_string()
    };
    h_flex()
        .gap_1()
        .text_size(tokens.caption_size())
        .text_color(tokens.accent)
        .children(mention.then(|| {
            div()
                .font_weight(FontWeight::BOLD)
                .text_color(tokens.red)
                .child("@")
        }))
        .child(count)
}

pub(super) fn pane(board: &Irc, cx: &mut Context<Irc>) -> workspace::Pane {
    let mut pane = workspace::Pane::new("irc-comms-pane").toolbar(heading(board, cx));
    let rows: Vec<_> = board
        .hub
        .as_ref()
        .into_iter()
        .flat_map(|hub| hub.read(cx).comms.summaries())
        .filter(|row| board.comms.accepts(row))
        .collect();
    pane = pane.child(filters(board, cx));
    pane.child(if rows.is_empty() {
        let tokens = Tokens::current(cx);
        let (title, hint) = if board.comms.unread_only {
            (
                "You’re all caught up",
                "Turn off Unread only to see earlier notifications.",
            )
        } else if board.comms.filter != 0 {
            (
                "No matching activity",
                "Choose All to see the rest of Comms.",
            )
        } else {
            (
                "No notifications yet",
                "Mentions and replies to threads you join appear here.",
            )
        };
        feedback::empty(&tokens, "irc-comms-empty", title, hint).into_any_element()
    } else {
        rows::draw(rows, board)
    })
}

fn heading(board: &Irc, cx: &Context<Irc>) -> workspace::Toolbar {
    let tokens = Tokens::current(cx);
    let count = board
        .hub
        .as_ref()
        .map_or(0, |hub| hub.read(cx).comms.unread().0);
    let label = h_flex()
        .gap_2()
        .text_color(tokens.fg_strong)
        .font_weight(FontWeight::SEMIBOLD)
        .child(Icon::from(IconName::Inbox).small())
        .child(NAME);
    workspace::Toolbar::new("irc-comms-heading")
        .strip()
        .leading(label)
        .trailing(
            action_button(
                "irc-comms-mark-all",
                Intent::Quiet,
                Some(IconName::Check),
                "Mark all read",
            )
            .disabled(count == 0)
            .on_click(cx.listener(|board, _, _, cx| {
                if let Some(hub) = board.hub.clone() {
                    hub.update(cx, |hub, cx| {
                        hub.comms.read(None);
                        cx.notify();
                    });
                }
            })),
        )
}

fn filters(board: &Irc, cx: &Context<Irc>) -> workspace::Toolbar {
    let tokens = Tokens::current(cx);
    let choices = [
        Segment::new("All"),
        Segment::new("Mentions").icon(IconName::AtSign),
        Segment::new("Replies").icon(IconName::Reply),
    ];
    let owner = board.me.clone();
    let filters = segmented::control(
        "irc-comms-kinds",
        choices,
        Some(board.comms.filter),
        &tokens,
        move |index, _, _, cx| {
            _ = owner.update(cx, |board, cx| {
                board.comms.filter = index;
                cx.notify();
            });
        },
    );
    let unread = action_button(
        "irc-comms-unread",
        Intent::Quiet,
        Some(IconName::Circle),
        "Unread only",
    )
    .selected(board.comms.unread_only)
    .toggled(board.comms.unread_only)
    .on_click(cx.listener(|board, _, _, cx| {
        board.comms.unread_only = !board.comms.unread_only;
        cx.notify();
    }));
    workspace::Toolbar::new("irc-comms-filters")
        .leading(filters)
        .leading(unread)
}

fn open(board: &mut Irc, key: &Key, window: &mut Window, cx: &mut Context<Irc>) {
    let Target::Channel(channel) = &key.buffer.target else {
        return;
    };
    if let Some(hub) = board.hub.clone() {
        hub.update(cx, |hub, cx| {
            hub.comms.read(Some(key));
            cx.notify();
        });
    }
    reveal::open_thread(board, (&key.buffer.network, channel), &key.root, window, cx);
    cx.notify();
}

pub(super) fn inspector(board: &Irc, cx: &mut Context<Irc>) -> workspace::Inspector {
    let ready = board.comms.selected.as_ref().is_some_and(|selected| {
        board.dock.focused.as_ref() == Some(&selected.buffer)
            && board
                .hub
                .as_ref()
                .is_some_and(|hub| hub.read(cx).state.buffer(&selected.buffer).is_some())
            && board.threads.shown(&selected.buffer).is_some_and(|shown| {
                shown.root_id == selected.root
                    || shown.root.line.id.as_ref() == Some(&selected.root)
            })
    });
    let inspector = workspace::Inspector::new("irc-comms-inspector", "Thread").on_close(
        "Close inspector",
        cx.listener(|board, _, window, cx| {
            dock::toggle_members(board, cx);
            board.focus.focus(window, cx);
        }),
    );
    if !ready {
        let tokens = Tokens::current(cx);
        return inspector.child(feedback::empty_line(
            &tokens,
            "irc-comms-select-thread",
            "Select a notification to open its thread.",
        ));
    }
    let inspector = inspector.child(super::inspector::thread::body(board, cx));
    match super::inspector::thread::footer(board, cx) {
        Some(footer) => inspector.footer(footer),
        None => inspector,
    }
}
