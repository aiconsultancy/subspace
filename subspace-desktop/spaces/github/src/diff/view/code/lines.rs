//! One rendered line of a diff: the numbers, the sign and the coloured code.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Div, Hsla, ParentElement as _, Styled as _, StyledText, div, px};
use subspace_framework::dashboard::Tokens;

use super::super::super::highlight::{Runs, overlay, shown};
use super::super::super::model::{DiffLine, LineKind};
use super::super::{
    EMPTY_HALF_OPACITY, LINE_BG_OPACITY, NUMBER_OPACITY, NUMBER_PAD_PX, NUMBER_W_PX, SIGN_W_PX,
    SPLIT_SIGN_W_PX, WORD_BG_OPACITY, code_row_height,
};

/// Line number cell.
pub(super) fn number(value: Option<u32>, tokens: &Tokens) -> Div {
    div()
        .w(tokens.mono_size(NUMBER_W_PX))
        .flex_shrink_0()
        .pr(tokens.mono_size(NUMBER_PAD_PX))
        .flex()
        .justify_end()
        .text_color(tokens.muted.opacity(NUMBER_OPACITY))
        .child(value.map(|n| n.to_string()).unwrap_or_default())
}

pub(super) fn line_colors(kind: LineKind, tokens: &Tokens) -> (Option<Hsla>, Hsla, &'static str) {
    match kind {
        LineKind::Add => (
            Some(tokens.green.opacity(LINE_BG_OPACITY)),
            tokens.green.opacity(WORD_BG_OPACITY),
            "+",
        ),
        LineKind::Del => (
            Some(tokens.red.opacity(LINE_BG_OPACITY)),
            tokens.red.opacity(WORD_BG_OPACITY),
            "-",
        ),
        LineKind::Context => (None, tokens.select, " "),
    }
}

/// The code of a line, in its syntax colours, with its changed words marked.
pub(super) fn code_text(line: &DiffLine, syntax: &Runs, word: Hsla) -> StyledText {
    let text = shown(&line.text);
    // Tabs widen the text, so the word ranges only apply without them.
    let words: &[std::ops::Range<usize>] = if line.text.contains('\t') {
        &[]
    } else {
        &line.words
    };
    let runs = overlay(syntax, words, word, text.len());
    StyledText::new(text).with_highlights(runs)
}

pub(super) fn unified_line(
    line: &DiffLine,
    syntax: &Runs,
    read: AnyElement,
    tokens: &Tokens,
) -> Div {
    let (background, word, sign) = line_colors(line.kind, tokens);
    h_flex()
        .h(px(code_row_height(tokens)))
        .w_full()
        .whitespace_nowrap()
        .overflow_hidden()
        .when_some(background, |el, bg| el.bg(bg))
        .child(read)
        .child(number(line.old, tokens))
        .child(number(line.new, tokens))
        .child(
            div()
                .w(tokens.mono_size(SIGN_W_PX))
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(sign),
        )
        .child(
            div()
                .text_color(tokens.fg)
                .child(code_text(line, syntax, word)),
        )
        .when(line.no_newline, |el| {
            el.child(div().pl_2().text_color(tokens.red).child("⏎̸"))
        })
}

pub(super) fn split_half(
    line: Option<(&DiffLine, &Runs, AnyElement)>,
    right: bool,
    tokens: &Tokens,
) -> Div {
    let base = h_flex()
        .h(px(code_row_height(tokens)))
        .flex_1()
        .min_w_0()
        .whitespace_nowrap()
        .overflow_hidden();
    let Some((line, syntax, read)) = line else {
        return base.bg(tokens.pane.opacity(EMPTY_HALF_OPACITY));
    };
    let (background, word, sign) = line_colors(line.kind, tokens);
    base.when_some(background, |el, bg| el.bg(bg))
        .child(read)
        .child(number(if right { line.new } else { line.old }, tokens))
        .child(
            div()
                .w(tokens.mono_size(SPLIT_SIGN_W_PX))
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(sign),
        )
        .child(
            div()
                .text_color(tokens.fg)
                .child(code_text(line, syntax, word)),
        )
}
