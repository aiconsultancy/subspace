//! The tab strip above the transcript: one tab per open chat, a busy or
//! attention marker, a close button, and the `+` that opens a new chat.

mod actions;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::Input;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
    px, relative,
};
use subspace_dashboard::{icon_button::Metrics, overflow_toolbar::OverflowToolbar};
use subspace_lucide::IconName;

use super::tabs::{Tab, tab_widths};
use super::{
    ChatPanel, Context, HEADER_ACTION_MAX_SHARE, Tokens, h_flex, input::CHAT_RENAME_CONTEXT,
};
use crate::action::{Intent, icon_button};
use crate::measure;
use crate::truncated;

/// The gap between the tabs and before the `+`, which the shares leave room
/// for: one for each tab.
const GAP: f32 = 4.;
/// The widest the rename editor runs.
const RENAME_WIDTH: f32 = 160.;

/// The marker after a tab's title: a running turn, or a waiting question.
fn tab_marker(busy: bool, attention: bool) -> Option<&'static str> {
    if attention {
        return Some("!");
    }
    busy.then_some("…")
}

/// The accessible name of one tab, which says what it is waiting for.
fn tab_name(title: &str, busy: bool, attention: bool) -> String {
    match tab_marker(busy, attention) {
        Some("!") => format!("Chat {title}, waiting for an answer"),
        Some(_) => format!("Chat {title}, working"),
        _ => format!("Chat {title}"),
    }
}

/// The close button's own name. It must differ from the tab's, or choosing
/// the tab through the accessibility tree would close it instead.
fn close_name(title: &str) -> String {
    format!("Close {title}")
}

impl ChatPanel {
    /// The width this tab takes of the strip the panel measured.
    fn tab_width(&self, index: usize, cx: &App) -> f32 {
        let gaps = GAP * self.tabs.len().saturating_sub(1) as f32;
        let available = self.strip_width.read(cx) - gaps;
        tab_widths(available, self.tabs.len(), self.active)
            .get(index)
            .copied()
            .unwrap_or_default()
    }

    fn render_tab(
        &self,
        index: usize,
        tab: &Tab,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (id, title) = (tab.id, tab.title.clone());
        let width = self.tab_width(index, cx);
        let selected = index == self.active;
        let marker = tab_marker(tab.is_busy(), tab.needs_attention());
        let name = tab_name(&title, tab.is_busy(), tab.needs_attention());
        let close = close_name(&title);
        h_flex()
            .id(("chat-tab", index))
            .aria_label(SharedString::from(name))
            .w(px(width))
            .flex_shrink_0()
            .px_1()
            .gap_1()
            .border_b_2()
            .border_color(if selected { tokens.accent } else { tokens.line })
            .text_color(if selected {
                tokens.fg_strong
            } else {
                tokens.muted
            })
            .child(truncated::text(("chat-tab-title", index), title).flex_1())
            .children(marker.map(|marker| div().text_color(tokens.accent).child(marker)))
            .on_click(cx.listener(move |chat, event: &ClickEvent, window, cx| {
                if event.click_count() >= 2 {
                    chat.start_rename(id, window, cx);
                } else {
                    chat.select_tab(index, window, cx);
                }
            }))
            .child(
                icon_button(("chat-tab-close", index), Intent::Quiet, IconName::X, close)
                    .compact()
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        chat.close_tab(id, window, cx);
                    })),
            )
    }

    /// The editor that replaces a tab's title while it is renamed.
    fn render_rename(&self, cx: &mut Context<Self>) -> Div {
        div()
            .key_context(CHAT_RENAME_CONTEXT)
            .w(px(RENAME_WIDTH))
            .on_action(
                cx.listener(|chat, _: &super::input::ChatRenameCancel, window, cx| {
                    chat.cancel_rename(window, cx);
                }),
            )
            .child(Input::new(&self.rename_input).small())
    }

    pub(super) fn render_header(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let renaming = self.renaming;
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .filter(|_| !self.is_disabled())
            .map(|(index, tab)| match renaming {
                Some(id) if id == tab.id => self.render_rename(cx).into_any_element(),
                _ => self.render_tab(index, tab, tokens, cx).into_any_element(),
            })
            .collect::<Vec<_>>();
        let units = actions::units(self, cx);
        let toolbar = OverflowToolbar::sync(&mut self.header_toolbar.borrow_mut(), units, cx);
        let size = Metrics::current(cx);
        h_flex()
            .id("chat-header")
            .test_support()
            .flex_shrink_0()
            .flex_nowrap()
            .min_w_0()
            .min_h(size.row_height())
            .p_1()
            .gap(size.gap())
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .text_color(tokens.muted)
                    .child(self.engine_icon())
                    .child(self.engine_label()),
            )
            // Tabs keep their own identity and scrolling. Header commands share
            // the remaining lane and move to More rather than squeezing tabs.
            .child(
                h_flex()
                    .id("chat-tabs")
                    .relative()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .overflow_x_scroll()
                    .child(measure::width(self.strip_width.clone()))
                    .children(tabs),
            )
            .child(
                div()
                    .flex_shrink_1()
                    .min_w(size.overflow_width())
                    .max_w(relative(HEADER_ACTION_MAX_SHARE))
                    .child(toolbar),
            )
            .into_any_element()
    }
}

/// The tab-strip key actions, bound while the chat panel has focus.
pub(super) fn bind_tab_actions(pane: Stateful<Div>, cx: &mut Context<ChatPanel>) -> Stateful<Div> {
    pane.on_action(
        cx.listener(|chat, _: &super::input::ChatNewTab, window, cx| chat.new_tab(window, cx)),
    )
    .on_action(
        cx.listener(|chat, _: &super::input::ChatCloseTab, window, cx| {
            let id = chat.tab().id;
            chat.close_tab(id, window, cx);
        }),
    )
    .on_action(
        cx.listener(|chat, _: &super::input::ChatNextTab, window, cx| chat.step_tab(1, window, cx)),
    )
    .on_action(
        cx.listener(|chat, _: &super::input::ChatPreviousTab, window, cx| {
            chat.step_tab(-1, window, cx)
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{close_name, tab_marker, tab_name};

    #[test]
    fn a_waiting_tab_outranks_a_working_one() {
        assert_eq!(tab_marker(false, false), None);
        assert_eq!(tab_marker(true, false), Some("…"));
        assert_eq!(tab_marker(true, true), Some("!"));
        assert_eq!(tab_marker(false, true), Some("!"));
    }

    /// Pressing the tab through the accessibility tree must select it, so
    /// its close button cannot answer to the same name.
    #[test]
    fn the_close_button_is_named_apart_from_its_tab() {
        assert_eq!(close_name("Release notes"), "Close Release notes");
        assert_ne!(
            close_name("Release notes"),
            tab_name("Release notes", false, false)
        );
    }

    #[test]
    fn a_tab_says_what_it_is_waiting_for() {
        assert_eq!(
            tab_name("Release notes", false, false),
            "Chat Release notes"
        );
        assert_eq!(
            tab_name("Release notes", true, false),
            "Chat Release notes, working"
        );
        assert_eq!(
            tab_name("Release notes", true, true),
            "Chat Release notes, waiting for an answer"
        );
    }
}
