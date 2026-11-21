//! The opened task or chat: opening it as a tab beside the list, which of
//! the two tabs shows, the one detail read behind the inspector and the
//! tab, and the page the pane shows for it. The title, properties and
//! sessions read in a column no wider than the reading width; the session
//! log, which is a log, runs the pane's full width.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_framework::client_github::copilot_agents::TaskDetail;
use subspace_framework::dashboard::list::{self, Detail, Section};
use subspace_framework::dashboard::workspace::{READING_WIDTH_PX, fact, section, text_edge};
use subspace_framework::dashboard::{Tokens, feedback, rich};

use crate::keys::RetrySessions;
use crate::model::Kind;
use crate::opened::LIST_TAB;
use crate::{CopilotAgents, inspector, logs, panels, peek};

/// The opened item's page, which tests find it by.
pub(crate) const ITEM_ID: &str = "github-copilot-agents-item";
/// What the page says when its row left the list.
const GONE: &str = "This task is no longer listed.";

// qual:allow(coupling, oi) reason: "One file per concern of the Copilot Agents dashboard; this one owns the opened item's tab and the detail read behind it."
impl CopilotAgents {
    /// The detail loaded for row `key`, when the one detail slot holds it.
    pub(crate) fn loaded_detail(&self, key: &str) -> Option<&TaskDetail> {
        self.detail
            .as_ref()
            .filter(|_| self.detail_key.as_deref() == Some(key))
    }

    /// Opens row `key` as the tab beside the list and shows it. Its
    /// sessions are read unless the inspector already read them; another
    /// task's open log closes.
    pub(crate) fn open_item(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.row(key).is_none() {
            return;
        }
        if self.log.as_ref().is_some_and(|log| log.task_key != key) {
            self.log = None;
        }
        self.item.open(key);
        self.sync_detail(cx);
        cx.notify();
    }

    /// Shows tab `index`: the list, or the opened item.
    pub(crate) fn show_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.item.show(index);
        self.sync_detail(cx);
        cx.notify();
    }

    /// Back to the list, leaving the opened item's tab open.
    pub(crate) fn show_list(&mut self, cx: &mut Context<Self>) {
        self.show_tab(LIST_TAB, cx);
    }

    /// Closes the opened item's tab, and its log with it, so the watcher
    /// stops reading that log.
    pub(crate) fn close_item(&mut self, cx: &mut Context<Self>) {
        let closed = self.item.key.take();
        if self.log.as_ref().map(|log| &log.task_key) == closed.as_ref() {
            self.log = None;
        }
        self.item.close();
        self.sync_detail(cx);
        cx.notify();
    }

    /// Reads the sessions of what the inspector or the opened tab shows,
    /// unless the one detail slot already holds them. Nothing is read while
    /// the list shows with the inspector hidden.
    pub(crate) fn sync_detail(&mut self, cx: &mut Context<Self>) {
        let wanted = self.inspector_open || self.item.showing().is_some();
        let key = self
            .target()
            .map(|row| row.key.clone())
            .filter(|key| wanted && self.detail_key.as_deref() != Some(key.as_str()));
        if let Some(key) = key {
            self.load_detail(&key, cx);
        }
    }

    /// What the opened tab shows of row `key`, as the old detail panel
    /// showed it, without the session log.
    pub(crate) fn item_detail(&self, key: &str) -> Option<Detail> {
        let row = self.row(key)?;
        if row.kind == Kind::Local {
            return Some(panels::local_detail(row));
        }
        let failed = self.detail_error.is_some() && self.detail_key.as_deref() == Some(key);
        let sections = match self.loaded_detail(key) {
            Some(detail) => panels::session_sections(detail),
            None if failed => Vec::new(),
            None => vec![panels::note_section(&row.state)],
        };
        Some(panels::cloud_detail(row, sections))
    }

    /// The session log section of row `key`, when its log is open.
    pub(crate) fn item_log(&self, key: &str) -> Option<Section> {
        self.log
            .as_ref()
            .and_then(|open| logs::log_section(open, key))
    }

    /// The banner with Retry when row `key`'s sessions failed to load.
    pub(crate) fn sessions_error(
        &self,
        key: &str,
        instance: &'static str,
        cx: &App,
    ) -> Option<AnyElement> {
        let message = self
            .detail_error
            .as_ref()
            .filter(|_| self.detail_key.as_deref() == Some(key))?;
        Some(
            feedback::error_banner(
                &Tokens::current(cx),
                instance,
                "Cannot load sessions",
                message.clone(),
                |_, window, cx| window.dispatch_action(Box::new(RetrySessions), cx),
            )
            .into_any_element(),
        )
    }

    /// The pane's content while the opened item's tab shows.
    pub(crate) fn item_page(&self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::current(cx);
        let body = match (self.row(key), self.item_detail(key)) {
            (Some(row), Some(detail)) => reading(&detail, &peek::status(&row.state), &tokens, cx),
            _ => vec![
                feedback::empty_line(&tokens, "github-copilot-agents-item-gone", GONE)
                    .into_any_element(),
            ],
        };
        let banner = self.sessions_error(key, "github-copilot-agents-banner", cx);
        let log = self.item_log(key).map(|log| log_block(key, &log, &tokens));
        v_flex()
            .id(ITEM_ID)
            .test_support()
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .id("github-copilot-agents-item-reading")
                    .test_support()
                    .w_full()
                    .max_w(px(READING_WIDTH_PX))
                    .px(text_edge(&tokens))
                    .py_4()
                    .gap_6()
                    .children(body)
                    .children(banner),
            )
            .children(log)
            .into_any_element()
    }
}

/// The opened item in reading order: its title and status, its
/// properties, then its sections.
fn reading(detail: &Detail, status: &peek::Status, tokens: &Tokens, cx: &App) -> Vec<AnyElement> {
    let mut parts = vec![heading(detail, status, tokens), properties(detail, cx)];
    parts.extend(
        detail
            .sections
            .iter()
            .enumerate()
            .map(|(index, part)| prose(&detail.id, index, part, tokens)),
    );
    parts
}

fn heading(detail: &Detail, status: &peek::Status, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("github-copilot-agents-item-heading")
        .test_support()
        .aria_label(format!("{}, {}", detail.title, status.words))
        .gap_2()
        .min_w_0()
        .child(
            div()
                .text_size(tokens.heading_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(rich::plain_text(
                    "github-copilot-agents-item-title",
                    &detail.title,
                )),
        )
        .child(inspector::status_line(status, tokens))
        .into_any_element()
}

/// The old panel's properties, as labelled facts.
fn properties(detail: &Detail, cx: &App) -> AnyElement {
    section("Properties", cx)
        .children(detail.props.iter().map(|(label, value)| {
            let text = rich::plain_text(
                SharedString::from(format!("github-copilot-agents-item-fact:{label}")),
                value,
            );
            fact(label.clone(), text, cx).into_any_element()
        }))
        .into_any_element()
}

/// One headed section: a pull request, a session, or a note.
fn prose(detail_id: &SharedString, index: usize, part: &Section, tokens: &Tokens) -> AnyElement {
    v_flex()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(part.heading.clone()),
        )
        .child(list::render_body(
            SharedString::from(format!("github-copilot-agents-section:{detail_id}:{index}")),
            &part.body,
            tokens,
        ))
        .into_any_element()
}

/// The session log at the pane's full width.
fn log_block(key: &str, log: &Section, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("github-copilot-agents-log")
        .test_support()
        .w_full()
        .min_w_0()
        .px(text_edge(tokens))
        .pb_4()
        .gap_2()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(log.heading.clone()),
        )
        .child(list::render_body(
            SharedString::from(format!("github-copilot-agents-log-body:{key}")),
            &log.body,
            tokens,
        ))
        .into_any_element()
}
