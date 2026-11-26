//! Page a project's task text without shaping or copying the other pages.

use std::collections::HashMap;
use std::fmt::Write as _;

use gpui_kit::base::TestTextExt as _;
use gpui_kit::base::{Pagination, PaginationState};
use gpui_kit::component::{
    Disableable as _, Sizable as _, Size,
    button::{Button, ButtonVariants as _},
    menu::{PopupMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div, px};
use subspace_client_fleet::model::{Task, TaskStatus};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, icon_button},
    anchored, feedback,
    panel::IconName,
    workspace,
};

use super::super::state::{Field, PartialFields};
use super::{Irc, fact_text, nonempty};
use crate::live::ship::is_bidi_control;

pub(super) fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "Queued",
        TaskStatus::Dispatching => "Dispatching",
        TaskStatus::Running => "Running",
        TaskStatus::Cancelling => "Cancelling",
        TaskStatus::Completed => "Completed",
        TaskStatus::Cancelled => "Cancelled",
        TaskStatus::Uncertain => "Uncertain",
        TaskStatus::Abandoned => "Abandoned",
        TaskStatus::Unknown => "Unknown",
    }
}

/// Show control codes without giving them control over the surrounding text.
pub(super) fn visible(text: &str) -> String {
    visible_codes(text, false)
}

fn visible_codes(text: &str, mark_whitespace: bool) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        if (ch.is_control() && (mark_whitespace || !matches!(ch, '\n' | '\r' | '\t')))
            || is_bidi_control(ch)
        {
            let _ = write!(result, "⟨U+{:04X}⟩", u32::from(ch));
            if ch == '\n' {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

const PAGE_CODE_POINTS: usize = 512;

impl Field {
    fn key(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Phase => "phase",
            Self::Progress => "progress",
        }
    }

    fn label_and_text(self, task: &Task) -> (&'static str, &str) {
        match self {
            Self::Summary => ("Current task", nonempty(&task.summary).unwrap_or(&task.id)),
            Self::Phase => ("Phase", &task.phase),
            Self::Progress => ("Progress", &task.progress),
        }
    }
}

#[derive(Default)]
pub(in super::super) struct TaskPages(HashMap<String, TaskPosition>);

struct TaskPosition {
    id: String,
    pages: [usize; Field::ALL.len()],
}

impl TaskPages {
    fn page(&self, project: &str, id: &str, field: Field) -> usize {
        self.0
            .get(project)
            .filter(|task| task.id == id)
            .map_or(1, |task| task.pages[field as usize])
    }

    fn set(&mut self, project: &str, id: &str, field: Field, page: usize) {
        let start = || TaskPosition {
            id: id.to_owned(),
            pages: [1; Field::ALL.len()],
        };
        let task = self.0.entry(project.to_owned()).or_insert_with(start);
        if task.id != id {
            *task = start();
        }
        task.pages[field as usize] = page;
    }
}

struct TextPage<'a> {
    current: usize,
    total: usize,
    text: &'a str,
}

impl<'a> TextPage<'a> {
    fn new(text: &'a str, requested: usize) -> Self {
        let total = text.chars().count().div_ceil(PAGE_CODE_POINTS).max(1);
        let current = requested.clamp(1, total);
        let start = (current - 1) * PAGE_CODE_POINTS;
        let mut indices = text.char_indices().skip(start);
        let first = indices.next().map_or(text.len(), |(byte, _)| byte);
        let end = indices
            .nth(PAGE_CODE_POINTS - 1)
            .map_or(text.len(), |(byte, _)| byte);
        Self {
            current,
            total,
            text: &text[first..end],
        }
    }

    fn visible(&self) -> String {
        visible_codes(self.text, true)
    }
}

pub(super) fn rows(
    task: Option<&Task>,
    project: &str,
    partial: PartialFields,
    pages: &TaskPages,
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    let Some(task) = task else {
        return vec![workspace::fact("Current task", "None", cx).into_any_element()];
    };
    Field::ALL
        .into_iter()
        .filter_map(|field| {
            let (label, text) = field.label_and_text(task);
            if text.is_empty() {
                return None;
            }
            let page = TextPage::new(text, pages.page(project, &task.id, field));
            let key = field.key();
            let pager = (page.total > 1).then(|| {
                let project = project.to_owned();
                let task = task.id.clone();
                let id = format!("irc-crew-task-pages:{project}:{key}");
                let change = cx.listener(move |board, page, _, cx| {
                    if let Some(selection) = &mut board.inspector_crew {
                        selection.pages.set(&project, &task, field, *page);
                        cx.notify();
                    }
                });
                let state = PaginationState::new(page.current, page.total)
                    .on_change(move |page, window, cx| change(&page, window, cx));
                pager(id, label, state)
            });
            Some(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .children(pager)
                    .child(workspace::fact(
                        label,
                        field_value(field, page.visible(), partial, project, cx),
                        cx,
                    ))
                    .into_any_element(),
            )
        })
        .collect()
}

fn field_value(
    field: Field,
    text: String,
    partial: PartialFields,
    project: &str,
    cx: &Context<Irc>,
) -> AnyElement {
    let text = fact_text(text);
    if !partial.contains(field) {
        return text.into_any_element();
    }
    let tail = field.is_tail();
    let edge = if tail { "latest" } else { "first" };
    // Keep the ellipsis a separate element on the cut edge, including when
    // the preview wraps or contains multiple lines.
    let marker = || div().child("…").test_text("…");
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .children(tail.then(marker))
        .child(text)
        .children((!tail).then(marker))
        .child(feedback::empty_line(
            &Tokens::current(cx),
            format!("irc-crew-preview:{project}:{}", field.key()),
            format!(
                "Showing the {edge} {} characters until the full detail loads.",
                field.limit()
            ),
        ))
        .into_any_element()
}

fn pager(id: String, label: &str, state: PaginationState) -> impl IntoElement {
    Pagination::new(id, state.clone())
        .accessibility_label(format!("{label} pages"))
        .flex()
        .items_center()
        .gap_1()
        .child(page_step(&state, true))
        .child(page_jump(state.clone()))
        .child(page_step(&state, false))
}

fn page_step(state: &PaginationState, previous: bool) -> impl IntoElement {
    let (id, icon, label, target) = if previous {
        (
            "prev",
            IconName::ChevronLeft,
            "Previous page",
            state.previous_page(),
        )
    } else {
        (
            "next",
            IconName::ChevronRight,
            "Next page",
            state.next_page(),
        )
    };
    let state = state.clone();
    icon_button(id, Intent::Quiet, icon, label)
        .compact()
        .disabled(target.is_none())
        .on_click(move |_, window, cx| {
            if let Some(page) = target {
                state.request_page(page, window, cx);
            }
        })
}

fn page_jump(state: PaginationState) -> impl IntoElement {
    let (current, total) = (state.current_page(), state.total_pages());
    let button = Button::new("jump")
        .ghost()
        .compact()
        .with_size(Size::XSmall)
        .accessibility_label(format!("Jump to page, current page {current} of {total}"))
        .label(format!("Page {current} of {total}"))
        .tooltip("Jump to page")
        .dropdown_caret(true);
    anchored::dropdown_menu(button, move |menu, _, _| jump_menu(menu, &state))
}

fn jump_menu(menu: PopupMenu, state: &PaginationState) -> PopupMenu {
    // Normal 64,000-code-point fields have at most 125 pages. Keep even a
    // larger response cheap to open, while still offering both ends.
    const MAX_MENU_PAGES: usize = 125;
    const MENU_MAX_HEIGHT: f32 = 240.;
    let total = state.total_pages();
    let start = state
        .current_page()
        .saturating_sub(MAX_MENU_PAGES / 2)
        .clamp(2, total.saturating_sub(MAX_MENU_PAGES).max(2));
    let nearby = (start..total)
        .take(MAX_MENU_PAGES)
        .map(|page| (page, format!("Page {page}")));
    [
        (1, "First page (1)".into()),
        (total, format!("Last page ({total})")),
    ]
    .into_iter()
    .chain(nearby)
    .fold(menu, |menu, (page, label)| {
        let state = state.clone();
        menu.item(
            PopupMenuItem::new(label)
                // A checked item would scroll the end jumps out of view on open.
                .disabled(state.current_page() == page)
                .on_click(move |_, window, cx| state.request_page(page, window, cx)),
        )
    })
    .max_h(px(MENU_MAX_HEIGHT))
    .scrollable(true)
}

#[cfg(test)]
mod tests {
    use super::{Field, PAGE_CODE_POINTS, TaskPages, TextPage};

    #[test]
    fn the_last_page_keeps_unicode_text_and_control_markers() {
        let tail = "🌊\t\r\n\u{202e}\x1bfinish";
        let text = "λ".repeat(64_000 - tail.chars().count()) + tail;
        let mut pages = TaskPages::default();
        let page = |pages: &TaskPages| {
            TextPage::new(&text, pages.page("chart", "task-1", Field::Progress))
        };
        let first = page(&pages);
        assert_eq!(first.text, "λ".repeat(PAGE_CODE_POINTS));
        assert_eq!(first.total, 125);
        pages.set("chart", "task-1", Field::Progress, first.total);
        let last = page(&pages);
        assert_eq!(last.current, last.total);
        assert_eq!(last.text.chars().count(), PAGE_CODE_POINTS);
        assert!(
            last.visible()
                .ends_with("🌊⟨U+0009⟩⟨U+000D⟩⟨U+000A⟩\n⟨U+202E⟩⟨U+001B⟩finish")
        );
        assert_eq!(pages.page("chart", "task-2", Field::Progress), 1);
        assert_eq!(pages.page("chart", "task-1", Field::Summary), 1);
    }
}
