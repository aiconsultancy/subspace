//! The virtual list of code rows: file headers, notes, hunk gaps and lines.

use std::sync::Arc;

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    div, px, uniform_list,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::feedback;

use super::super::highlight::{self, Runs};
use super::super::model::{DiffLine, DiffRow, Layout, LineKind, file_at};
use super::{CODE_FONT_PX, DiffView, NO_RUNS, SPLIT_DIVIDER_W_PX, Text, code_row_height};

mod lines;
mod rows;

use lines::{split_half, unified_line};

impl DiffView {
    pub(super) fn patch_runs(&self, file: usize, line: usize) -> &Runs {
        self.highlights
            .get(&file)
            .and_then(|runs| runs.get(line))
            .unwrap_or(NO_RUNS)
    }

    /// Colours the files of rows `range` the first time they show.
    pub(super) fn prepare_highlights(&mut self, range: std::ops::Range<usize>, cx: &Context<Self>) {
        let theme = &cx.theme().highlight_theme;
        if self.highlight_theme.as_ref() != Some(theme) {
            self.highlights.clear();
            self.context_highlights.clear();
            self.highlight_theme = Some(theme.clone());
        }
        let mut files: Vec<usize> = self.rows
            [range.start.min(self.rows.len())..range.end.min(self.rows.len())]
            .iter()
            .filter_map(|row| match *row {
                DiffRow::Line { file, .. }
                | DiffRow::Split { file, .. }
                | DiffRow::Context { file, .. } => Some(file),
                _ => None,
            })
            .collect();
        files.dedup();
        for file in files {
            let Some(view) = self.files.get(file) else {
                continue;
            };
            let Some(language) = highlight::language(&view.file.path) else {
                continue;
            };
            self.highlights.entry(file).or_insert_with(|| {
                Arc::new(highlight::highlight(language, &view.patch.lines, &**theme))
            });
            if let (false, Some(Text::Ready(lines))) = (
                self.context_highlights.contains_key(&file),
                self.texts.get(&file),
            ) {
                let shown: Vec<String> = lines.iter().map(|line| highlight::shown(line)).collect();
                let runs = highlight::side_runs(language, &shown, &**theme);
                self.context_highlights.insert(file, Arc::new(runs));
            }
        }
    }

    pub(super) fn code_row(
        &self,
        index: usize,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(&row) = self.rows.get(index) else {
            return div().h(px(code_row_height(tokens))).into_any_element();
        };
        match row {
            DiffRow::File(file) => self.file_header(file, ("diff-file", index), tokens, cx),
            DiffRow::Note(file) => self.note_row(file, index, tokens, cx),
            DiffRow::Gap { file, gap } => self.gap_row(file, gap, index, tokens, cx),
            DiffRow::Line { file, line } => {
                let runs = self.patch_runs(file, line);
                let line = &self.files[file].patch.lines[line];
                unified_line(line, runs, tokens)
                    .font_family(cx.theme().mono_font_family.clone())
                    .into_any_element()
            }
            DiffRow::Split { file, left, right } => {
                let lines = &self.files[file].patch.lines;
                let half =
                    |index: Option<usize>| index.map(|i| (&lines[i], self.patch_runs(file, i)));
                h_flex()
                    .font_family(cx.theme().mono_font_family.clone())
                    .h(px(code_row_height(tokens)))
                    .w_full()
                    .child(split_half(half(left), false, tokens))
                    .child(div().w(px(SPLIT_DIVIDER_W_PX)).h_full().bg(tokens.line))
                    .child(split_half(half(right), true, tokens))
                    .into_any_element()
            }
            DiffRow::Context { file, old, new } => self.context_row(file, (old, new), tokens, cx),
        }
    }

    fn context_row(
        &self,
        file: usize,
        (old, new): (u32, u32),
        tokens: &Tokens,
        cx: &Context<Self>,
    ) -> AnyElement {
        let at = new.saturating_sub(1) as usize;
        let text = match self.texts.get(&file) {
            Some(Text::Ready(lines)) => lines.get(at).cloned().unwrap_or_default(),
            _ => String::new(),
        };
        let runs = self
            .context_highlights
            .get(&file)
            .and_then(|runs| runs.get(at))
            .unwrap_or(NO_RUNS);
        let line = DiffLine {
            kind: LineKind::Context,
            old: Some(old),
            new: Some(new),
            text,
            words: Vec::new(),
            no_newline: false,
        };
        match self.layout {
            Layout::Unified => unified_line(&line, runs, tokens)
                .font_family(cx.theme().mono_font_family.clone())
                .into_any_element(),
            Layout::Split => h_flex()
                .font_family(cx.theme().mono_font_family.clone())
                .h(px(code_row_height(tokens)))
                .w_full()
                .child(split_half(Some((&line, runs)), false, tokens))
                .child(div().w(px(SPLIT_DIVIDER_W_PX)).h_full().bg(tokens.line))
                .child(split_half(Some((&line, runs)), true, tokens))
                .into_any_element(),
        }
    }

    pub(super) fn code(&mut self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if let Some(image) = &self.image {
            return self.image_view(image, tokens, cx);
        }
        match (self.loaded, self.files.is_empty()) {
            (false, _) => self.code_placeholder(tokens, cx),
            (true, true) => feedback::empty(
                tokens,
                "github-diff-empty",
                "No changed files",
                "This pull request changes no files.",
            )
            .into_any_element(),
            (true, false) => self.code_list(tokens, cx),
        }
    }

    /// The skeleton while the diff loads, or the error that stopped it.
    fn code_placeholder(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        match &self.error {
            Some(error) => feedback::error_box(
                tokens,
                "github-diff-error",
                "Cannot load the diff",
                error.clone(),
                cx.listener(|view, _, _, cx| view.load(cx)),
            )
            .into_any_element(),
            None => feedback::skeleton(tokens).into_any_element(),
        }
    }

    /// The virtual list of rows, its sticky file header and the reload error.
    fn code_list(&mut self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let top = self.follow_scroll(code_row_height(tokens));
        let sticky = self.sticky_header(top);
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .text_size(tokens.mono_size(CODE_FONT_PX))
            .on_scroll(cx.listener(|view, _, _, cx| {
                view.pending = None;
                cx.notify();
            }))
            .child(self.row_list(cx))
            .when_some(sticky, |el, file| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .shadow_md()
                        .child(self.file_header(file, "diff-sticky", tokens, cx)),
                )
            })
            .when(self.error.is_some(), |el| {
                let error = self.error.clone().unwrap_or_default();
                el.child(div().absolute().bottom_0().left_0().right_0().child(
                    feedback::error_banner(
                        tokens,
                        "github-diff-banner",
                        "Reload failed",
                        error,
                        cx.listener(|view, _, _, cx| view.load(cx)),
                    ),
                ))
            })
            .into_any_element()
    }

    /// The tree marks the file at the top of the code view. After `n`, `p` or
    /// a tree click the list scrolls on its next layout, so the old offset
    /// must not move the mark back. Gives the top row.
    fn follow_scroll(&mut self, row_height: f32) -> usize {
        let top = self.top_row(row_height);
        if top != self.last_top {
            self.last_top = top;
            match self.pending {
                Some(target) if target == top => self.pending = None,
                Some(_) => {}
                None => self.current = file_at(&self.starts, top),
            }
        }
        top
    }

    /// The file whose header hangs above the top row, if its own header
    /// has already scrolled away.
    fn sticky_header(&self, top: usize) -> Option<usize> {
        let file = file_at(&self.starts, top);
        self.starts
            .get(file)
            .is_some_and(|&start| start < top)
            .then_some(file)
    }

    fn row_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        uniform_list(
            "diff-code",
            self.rows.len(),
            cx.processor(|view: &mut Self, range: std::ops::Range<usize>, _, cx| {
                let tokens = Tokens::current(cx);
                view.prepare_highlights(range.clone(), cx);
                range
                    .map(|index| view.code_row(index, &tokens, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full()
    }
}
