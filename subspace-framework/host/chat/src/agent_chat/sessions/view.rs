//! The Sessions tab's layout: the search field, the grouped rows with their
//! badges and hover metadata, the row context menu and the delete
//! confirmation.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ClickEvent, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, px,
};
use subspace_agent::SessionSummary;
use subspace_lucide::IconName;

use super::super::input::CHAT_SESSIONS_CONTEXT;
use super::super::{ChatPanel, Context, h_flex, v_flex};
use super::menu::row_menu;
use super::{
    Group, badge_letter, created_label, grouped, id_prefix, listed, metadata_fit, selected_session,
    shows_metadata,
};
use crate::action::{Intent, action_button};
use crate::anchored::AnchoredContextMenuExt as _;
use crate::measure;
use crate::tokens::Tokens;
use crate::truncated;

/// The badge beside a row's name.
const BADGE: f32 = 22.;
/// The corner radius of the badge and of a hovered row.
const RADIUS: f32 = 6.;
/// The open-in-a-tab dot on the badge's corner, and how far it sits outside.
const DOT: f32 = 7.;
const DOT_OFFSET: f32 = -1.;
/// One row's height. It never changes, so showing the metadata cannot move a
/// row out from under a pointer that is pressing it.
const ROW_HEIGHT: f32 = 26.;
/// The hover group of one row, which reveals that row's metadata through a
/// style rather than a re-render.
const ROW_GROUP: &str = "chat-session-row";

/// What a row's context menu offers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::agent_chat) enum RowAction {
    Rename,
    Export,
    Delete,
}

impl ChatPanel {
    /// The sessions the list currently shows, after the search field.
    fn listed_sessions(&self) -> Vec<&SessionSummary> {
        listed(&self.sessions, &self.view.query)
    }

    /// The highlighted session, which the keys act on.
    pub(in crate::agent_chat) fn highlighted(&self) -> Option<SessionSummary> {
        selected_session(&self.listed_sessions(), self.view.selected).cloned()
    }

    fn render_search(&self) -> Div {
        div().px_2().pt_2().child(
            Input::new(&self.view.search)
                .small()
                .cleanable(true)
                .prefix(IconName::Search),
        )
    }

    /// The heading and row elements of every non-empty group, numbered as
    /// the highlight numbers them.
    fn render_groups(
        &self,
        shown: &[&SessionSummary],
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let mut rows = Vec::new();
        let mut index = 0;
        for (position, (group, sessions)) in grouped(shown, chrono::Utc::now()).iter().enumerate() {
            rows.push(render_heading(*group, position == 0, tokens, cx));
            for session in sessions {
                rows.push(
                    div()
                        .px_1()
                        .child(self.render_session_row(index, session, tokens, cx)),
                );
                index += 1;
            }
        }
        rows
    }

    /// One row's identity and whether the highlight is on it.
    fn row_state(&self, index: usize, session: &SessionSummary) -> (String, bool) {
        (session.id.clone(), index == self.view.selected)
    }

    /// The badge state of one row: whether its chat is open, and busy.
    fn badge_state(&self, session: &SessionSummary) -> (bool, bool) {
        let open = super::super::tabs::session_tab(&self.tabs, &session.id);
        let busy = open
            .and_then(|index| self.tabs.get(index))
            .is_some_and(|tab| tab.is_busy());
        (open.is_some(), busy)
    }
}

/// A group heading. The first one carries the New session action.
fn render_heading(group: Group, first: bool, tokens: &Tokens, cx: &mut Context<ChatPanel>) -> Div {
    h_flex()
        .px_2()
        .pt_2()
        .pb_1()
        .gap_2()
        .text_color(tokens.muted)
        .child(group.heading())
        .child(div().flex_1())
        .when(first, |row| {
            row.child(
                action_button(
                    "chat-new-session",
                    Intent::Quiet,
                    Some(IconName::SquarePen),
                    "New session",
                )
                .on_click(cx.listener(|chat, _, window, cx| chat.new_tab(window, cx))),
            )
        })
}

impl ChatPanel {
    /// The badge: the name's first letter, with an accent dot when the
    /// session is open in a tab.
    fn render_badge(&self, session: &SessionSummary, tokens: &Tokens) -> Div {
        let (open, busy) = self.badge_state(session);
        div()
            .relative()
            .flex_shrink_0()
            .size(px(BADGE))
            .rounded(px(RADIUS))
            .bg(tokens.select)
            .text_color(tokens.fg_strong)
            .flex()
            .items_center()
            .justify_center()
            .child(badge_letter(&session.title))
            .when(open, |badge| {
                badge.child(
                    div()
                        .absolute()
                        .top(px(DOT_OFFSET))
                        .right(px(DOT_OFFSET))
                        .size(px(DOT))
                        .rounded_full()
                        .bg(if busy { tokens.yellow } else { tokens.accent }),
                )
            })
    }

    /// The muted metadata a hovered row adds: the created time and the head
    /// of the session id. Values the SDK does not report are left out, and a
    /// narrow row drops them rather than squeezing the name.
    ///
    /// The pointer reveals it through the row's hover style, not through a
    /// re-render: GPUI reports a hover listener as "not hovered" as soon as a
    /// button goes down, so a row that re-rendered on hover changed under the
    /// press and lost the click.
    fn render_metadata(&self, session: &SessionSummary, selected: bool, cx: &App) -> Div {
        let fit = metadata_fit(*self.view.width.read(cx));
        let created = created_label(&session.created_at).filter(|_| fit.created);
        let id = fit
            .id
            .then(|| SharedString::from(id_prefix(&session.id).to_owned()));
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .whitespace_nowrap()
            .when(!selected, |meta| {
                // Nothing is reserved until the pointer is on the row, so a
                // name that fits keeps the whole line.
                meta.w_0()
                    .invisible()
                    .overflow_hidden()
                    .group_hover(ROW_GROUP, |style| style.w_auto().visible())
            })
            .children(created.map(|created| h_flex().gap_1().child(IconName::Clock).child(created)))
            .children(id.map(|id| h_flex().gap_1().child(IconName::Hash).child(id)))
    }

    /// The row's one line: the name, when it changed, and the extra
    /// metadata while the row is hovered or highlighted. The name can never
    /// make the row taller than one line, however long the chat's first
    /// message was.
    fn render_line(
        &self,
        index: usize,
        session: &SessionSummary,
        tokens: &Tokens,
        cx: &App,
    ) -> Div {
        self.render_session_rename(session)
            .unwrap_or_else(|| self.render_name_line(index, session, tokens, cx))
    }

    /// The name editor while `session` is being renamed. The editor keeps
    /// Enter and the click to itself, so saving a name neither opens the
    /// chat nor leaves the list.
    fn render_session_rename(&self, session: &SessionSummary) -> Option<Div> {
        (self.view.renaming.as_deref() == Some(session.id.as_str())).then(|| {
            div()
                .flex_1()
                .occlude()
                .child(Input::new(&self.view.rename).small())
        })
    }

    fn render_name_line(
        &self,
        index: usize,
        session: &SessionSummary,
        tokens: &Tokens,
        cx: &App,
    ) -> Div {
        let detailed = shows_metadata(index, self.view.selected);
        h_flex()
            .min_w_0()
            .flex_1()
            .gap_2()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(tokens.muted)
            .child(
                truncated::text(
                    SharedString::from(format!("session-name:{}", session.id)),
                    session.title.clone(),
                )
                .flex_1()
                .text_color(tokens.fg_strong),
            )
            .child(div().flex_shrink_0().child(session.modified.clone()))
            .child(self.render_metadata(session, detailed, cx))
    }

    fn render_session_row(
        &self,
        index: usize,
        session: &SessionSummary,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (id, selected) = self.row_state(index, session);
        let menu = row_menu(id.clone(), cx);
        h_flex()
            .id(("chat-session", index))
            .group(ROW_GROUP)
            .h(px(ROW_HEIGHT))
            .px_2()
            .gap_2()
            .rounded(px(RADIUS))
            .when(selected, |row| row.bg(tokens.select))
            .hover(|row| row.bg(tokens.select))
            .child(self.render_badge(session, tokens))
            .child(self.render_line(index, session, tokens, cx))
            .on_click(cx.listener(move |chat, _: &ClickEvent, window, cx| {
                chat.open_row(&id, window, cx);
            }))
            .anchored_context_menu(move |popup, _, _| menu(popup))
    }

    pub(in crate::agent_chat) fn render_sessions(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let shown = self.listed_sessions();
        let empty = shown.is_empty();
        let rows = self.render_groups(&shown, tokens, cx);
        session_keys(
            v_flex()
                .id("chat-sessions")
                .key_context(CHAT_SESSIONS_CONTEXT)
                .flex_1()
                .min_h_0()
                .child(self.render_search())
                .child(
                    div()
                        .id("chat-session-rows")
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(measure::width(self.view.width.clone()))
                        .children(rows)
                        .when(empty, |body| {
                            body.child(div().p_2().text_color(tokens.muted).child(EMPTY))
                        }),
                ),
            cx,
        )
    }
}

const EMPTY: &str = "No saved chats match.";

/// The session list's keys, which act on the highlighted row.
fn session_keys(list: Stateful<Div>, cx: &mut Context<ChatPanel>) -> Stateful<Div> {
    use super::super::input::{
        ChatSessionDelete, ChatSessionNext, ChatSessionOpen, ChatSessionPrevious, ChatSessionRename,
    };
    list.on_action(cx.listener(|chat, _: &ChatSessionNext, _, cx| chat.move_highlight(1, cx)))
        .on_action(cx.listener(|chat, _: &ChatSessionPrevious, _, cx| chat.move_highlight(-1, cx)))
        .on_action(
            cx.listener(|chat, _: &ChatSessionOpen, window, cx| chat.open_highlighted(window, cx)),
        )
        .on_action(cx.listener(|chat, _: &ChatSessionRename, window, cx| {
            chat.rename_highlighted(window, cx)
        }))
        .on_action(cx.listener(|chat, _: &ChatSessionDelete, _, cx| chat.delete_highlighted(cx)))
}
