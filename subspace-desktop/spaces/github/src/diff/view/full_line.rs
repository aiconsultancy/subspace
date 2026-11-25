//! Read one line at full length without changing the fixed-height diff rows.

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::panel::IconName;

use super::super::highlight::shown;
use super::{CODE_FONT_PX, DiffView, Text};

#[derive(Clone, Copy)]
pub(super) enum LineSource {
    Patch(usize),
    Context(usize),
}

pub(super) struct FullLine {
    text: SharedString,
    scroll: ScrollHandle,
}

#[cfg(test)]
impl FullLine {
    pub(super) fn parts(&self) -> (&str, &ScrollHandle) {
        (&self.text, &self.scroll)
    }
}

impl FullLine {
    fn new(text: &str) -> Self {
        Self {
            text: shown(text).into(),
            scroll: ScrollHandle::new(),
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
                    .child("Full line")
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

pub(super) fn read_line_button(
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
    div()
        .id(("diff-read-file", file))
        .flex_shrink_0()
        .child(
            icon_button(id, Intent::Quiet, IconName::Maximize2, "Read full line").on_click(
                cx.listener(move |view, _, window, cx| {
                    view.open_line(file, source, cx);
                    view.focus.focus(window, cx);
                }),
            ),
        )
        .into_any_element()
}

impl DiffView {
    fn open_line(&mut self, file: usize, source: LineSource, cx: &mut Context<Self>) {
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
            self.full_line = Some(FullLine::new(text));
            cx.notify();
        }
    }
}
