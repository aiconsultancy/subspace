//! Page a project's task text without shaping or copying the other pages.

use std::collections::HashMap;
use std::fmt::Write as _;

use gpui_kit::component::{Sizable as _, Size, pagination::Pagination, v_flex};
use gpui_kit::{AnyElement, Context, IntoElement as _, ParentElement as _, Styled as _};
use subspace_client_fleet::model::{Task, TaskStatus};
use subspace_framework::dashboard::workspace;

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

#[derive(Clone, Copy)]
enum Field {
    Summary,
    Phase,
    Progress,
}

impl Field {
    const ALL: [Self; 3] = [Self::Summary, Self::Phase, Self::Progress];

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
                Pagination::new(format!("irc-crew-task-pages:{project}:{key}"))
                    .with_size(Size::XSmall)
                    .current_page(page.current)
                    .total_pages(page.total)
                    .on_click(cx.listener(move |board, page, _, cx| {
                        if let Some(selection) = &mut board.inspector_crew {
                            selection.pages.set(&project, &task, field, *page);
                            cx.notify();
                        }
                    }))
            });
            Some(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .children(pager)
                    .child(workspace::fact(label, fact_text(page.visible()), cx))
                    .into_any_element(),
            )
        })
        .collect()
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
