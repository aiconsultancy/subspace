//! Read one line at full length without changing the fixed-height diff rows.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::panel::IconName;

use super::{CODE_FONT_PX, DiffView, Text};

#[derive(Clone, Copy)]
pub(super) enum LineSource {
    Patch(usize),
    Context(usize),
}

pub(super) struct FullLine {
    text: SharedString,
    scroll: ScrollHandle,
    partial: Option<(usize, usize)>,
}

#[cfg(test)]
impl FullLine {
    pub(super) fn parts(&self) -> (&str, &ScrollHandle) {
        (&self.text, &self.scroll)
    }
}

fn reader_text(text: &str) -> SharedString {
    text.chars()
        .map(|ch| match ch {
            '\t' => '⇥',
            '\r' => '␍',
            ch => ch,
        })
        .collect::<String>()
        .into()
}

impl FullLine {
    fn new(text: &str, partial: Option<(usize, usize)>) -> Self {
        Self {
            text: reader_text(text),
            scroll: ScrollHandle::new(),
            partial,
        }
    }

    pub(super) fn render(&self, tokens: &Tokens, cx: &Context<DiffView>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .justify_between()
                    .child(self.title())
                    .child(
                        action_button(
                            "diff-full-line-close",
                            Intent::Quiet,
                            Some(IconName::X),
                            "Close full line",
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.full_line = None;
                            view.focus.focus(window, cx);
                            cx.notify();
                        })),
                    ),
            )
            .child(
                self.text_view(tokens)
                    .font_family(cx.theme().mono_font_family.clone()),
            )
    }

    pub(super) fn title(&self) -> &'static str {
        if self.partial.is_some() {
            "Partial line"
        } else {
            "Full line"
        }
    }
    fn text_view(&self, tokens: &Tokens) -> impl Styled + IntoElement {
        h_flex()
            .id("diff-full-line-text")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .items_start()
            .overflow_x_scroll()
            .track_scroll(&self.scroll)
            .px_3()
            .py_2()
            .whitespace_nowrap()
            .text_size(tokens.mono_size(CODE_FONT_PX))
            .child(div().flex_shrink_0().child(self.text.clone()))
            .horizontal_scrollbar(&self.scroll)
    }
}

impl DiffView {
    pub(super) fn read_line_button(
        &self,
        file: usize,
        source: LineSource,
        right: bool,
        cx: &Context<DiffView>,
    ) -> AnyElement {
        let id = match (source, right) {
            (LineSource::Patch(index), false) => ("diff-line-left", index),
            (LineSource::Patch(index), true) => ("diff-line-right", index),
            (LineSource::Context(index), false) => ("diff-context-left", index),
            (LineSource::Context(index), true) => ("diff-context-right", index),
        };
        let partial = self.is_partial_line(file, source);
        let label = if partial {
            "Read partial line"
        } else {
            "Read full line"
        };
        div()
            .id(("diff-read-file", file))
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(
                icon_button(id, Intent::Quiet, IconName::Maximize2, label).on_click(cx.listener(
                    move |view, _, window, cx| {
                        view.open_line(file, source, cx);
                        view.focus.focus(window, cx);
                    },
                )),
            )
            .when(partial, |row| {
                row.child(div().px_1().child("Partial line").test_text("Partial line"))
            })
            .into_any_element()
    }

    fn is_partial_line(&self, file: usize, source: LineSource) -> bool {
        let LineSource::Patch(index) = source else {
            return false;
        };
        self.files.get(file).is_some_and(|file| {
            file.patch.lines.len().checked_sub(1) == Some(index)
                && file.partial_line() == Some(index)
        })
    }

    pub(super) fn update_partial_line(&mut self, file: usize) {
        let Some(reader) = &mut self.full_line else {
            return;
        };
        let Some((open_file, line)) = reader.partial.filter(|(open, _)| *open == file) else {
            return;
        };
        let patch = &self.files[file];
        if let Some(text) = patch.patch.lines.get(line) {
            reader.text = reader_text(&text.text);
            reader.partial = (patch.partial_line() == Some(line)).then_some((open_file, line));
        }
    }

    fn open_line(&mut self, file: usize, source: LineSource, cx: &mut Context<Self>) {
        let partial = match source {
            LineSource::Patch(line) if self.is_partial_line(file, source) => Some((file, line)),
            _ => None,
        };
        let text = match source {
            LineSource::Patch(index) => self
                .files
                .get(file)
                .and_then(|file| file.patch.lines.get(index))
                .map(|line| &line.text),
            LineSource::Context(index) => match self.texts.get(&file) {
                Some(Text::Ready(lines)) => lines.get(index),
                _ => None,
            },
        };
        if let Some(text) = text {
            self.full_line = Some(FullLine::new(text, partial));
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FullLine;

    #[gpui_kit::test]
    fn full_line_renders_each_tab_and_carriage_return() {
        let line = FullLine::new("\tfn main() {\t}\r", None);
        assert_eq!(line.parts().0, "⇥fn main() {⇥}␍");
    }
}
