//! The diff view's file tree.

use std::sync::Arc;

use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Hsla, InteractiveElement as _, IntoElement, KeyContext,
    OperatingSystem, ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px, uniform_list,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::truncated;

use super::super::model::{TreeRow, TreeStatus, tree_rows, tree_status};
use super::{
    BAR_PAD_PX, DiffView, SEARCH_CONTEXT, SEARCH_PAD_X_PX, SEARCH_PAD_Y_PX, TREE_HEADER_H_PX,
    TREE_INDENT_PX, TREE_LETTER_W_PX, TREE_MAX_SHARE, TREE_MIN_W_PX, TREE_PAD_PX, TREE_ROW_H,
    VIEWED_OPACITY,
};

impl DiffView {
    pub(super) fn tree(&self, tokens: &Tokens, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let query = self.search.read(cx).value().to_string();
        v_flex()
            .flex_shrink_0()
            .w(px(tree_width(
                f32::from(tokens.tree_width),
                f32::from(window.viewport_size().width),
            )))
            .h_full()
            .border_r_1()
            .border_color(tokens.line)
            .bg(tokens.pane)
            .text_size(tokens.row_font)
            .child(self.tree_header(tokens))
            .when(self.total as usize > self.files.len(), |el| {
                el.child(self.tree_overflow_note(tokens))
            })
            .when(self.searching || !query.is_empty(), |el| {
                el.child(self.tree_search_box(tokens, cx.platform_info().os))
            })
            .child(self.tree_list(&query, cx))
    }

    /// "Files", the file count and how many of them are viewed.
    fn tree_header(&self, tokens: &Tokens) -> Div {
        let viewed_count = self
            .files
            .iter()
            .filter(|file| self.viewed.contains(&file.file.path))
            .count();
        let all_viewed = viewed_count == self.files.len() && !self.files.is_empty();
        h_flex()
            .h(px(TREE_HEADER_H_PX))
            .px(px(TREE_PAD_PX))
            .gap_2()
            .border_b_1()
            .border_color(tokens.line)
            .child(div().text_color(tokens.fg_strong).child("Files"))
            .child(
                div()
                    .text_color(tokens.muted)
                    .child(self.files.len().to_string()),
            )
            .child(div().flex_1())
            .when(self.item().is_some(), |header| {
                header.child(
                    div()
                        .text_color(if all_viewed {
                            tokens.green
                        } else {
                            tokens.muted
                        })
                        .child(format!("{viewed_count} / {} viewed", self.files.len())),
                )
            })
    }

    /// GitHub lists at most 300 files; say so when it cut the list.
    fn tree_overflow_note(&self, tokens: &Tokens) -> Div {
        div()
            .px(px(TREE_PAD_PX))
            .py_1()
            .text_xs()
            .text_color(tokens.yellow)
            .child(format!(
                "GitHub lists the first {} of {} files.",
                self.files.len(),
                self.total
            ))
    }

    fn tree_search_box(&self, tokens: &Tokens, os: OperatingSystem) -> Div {
        let mut search_context = KeyContext::new_with_defaults(os);
        search_context.add(SEARCH_CONTEXT);
        div()
            .key_context(search_context)
            .px(px(SEARCH_PAD_X_PX))
            .py(px(SEARCH_PAD_Y_PX))
            .border_b_1()
            .border_color(tokens.line)
            .child(Input::new(&self.search).appearance(false))
    }

    /// The virtual list of tree rows that the search leaves.
    fn tree_list(&self, query: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let paths = self.paths();
        let rows = Arc::new(tree_rows(&paths, query));
        uniform_list(
            "diff-tree",
            rows.len(),
            cx.processor(
                move |view: &mut Self, range: std::ops::Range<usize>, _, cx| {
                    let tokens = Tokens::current(cx);
                    range
                        .filter_map(|index| {
                            let row = rows.get(index)?;
                            Some(view.tree_row(index, row, &tokens, cx))
                        })
                        .collect::<Vec<_>>()
                },
            ),
        )
        .track_scroll(&self.tree_scroll)
        .flex_1()
        .min_h_0()
    }

    pub(super) fn tree_row(
        &self,
        index: usize,
        row: &TreeRow,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let indent = px(TREE_PAD_PX + row.depth as f32 * TREE_INDENT_PX);
        match row.file {
            None => directory_row(index, row, indent, tokens),
            Some(file) => self.file_row(index, row, file, tokens, cx),
        }
    }

    /// One file of the tree: its status letter, name and diff stat.
    fn file_row(
        &self,
        index: usize,
        row: &TreeRow,
        file: usize,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = &self.files[file];
        let viewed = self.viewed.contains(&view.file.path);
        let (letter, color) = status_mark(tree_status(view.file.status), tokens);
        h_flex()
            .id(("tree-file", index))
            .test_support()
            .w_full()
            .h(px(TREE_ROW_H))
            .pl(px(TREE_PAD_PX + row.depth as f32 * TREE_INDENT_PX))
            .pr(px(BAR_PAD_PX))
            .gap_2()
            .whitespace_nowrap()
            .cursor_pointer()
            .when(file == self.current, |el| el.bg(tokens.select))
            .when(viewed, |el| el.opacity(VIEWED_OPACITY))
            .hover(|style| style.bg(tokens.select))
            .child(
                div()
                    .w(px(TREE_LETTER_W_PX))
                    .text_color(color)
                    .child(letter),
            )
            .child(
                truncated::text(("tree-file-name", index), row.label.clone())
                    .flex_1()
                    .text_color(tokens.fg),
            )
            .when(!view.binary, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(tokens.green)
                        .child(format!("+{}", view.file.additions)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(tokens.red)
                        .child(format!("−{}", view.file.deletions)),
                )
            })
            .on_click(cx.listener(move |view, _, window, cx| view.go_to(file, Some(window), cx)))
            .into_any_element()
    }
}

/// The tree takes its token width, at most half the window and at least
/// `TREE_MIN_W_PX`.
fn tree_width(token_width: f32, viewport_width: f32) -> f32 {
    token_width
        .min(viewport_width * TREE_MAX_SHARE)
        .max(TREE_MIN_W_PX)
}

/// One directory of the tree.
fn directory_row(index: usize, row: &TreeRow, indent: Pixels, tokens: &Tokens) -> AnyElement {
    h_flex()
        .id(("tree-dir", index))
        .w_full()
        .h(px(TREE_ROW_H))
        .pl(indent)
        .gap_1()
        .whitespace_nowrap()
        .text_color(tokens.muted)
        .child("▾")
        .child(truncated::text(
            ("tree-folder-name", index),
            row.label.clone(),
        ))
        .into_any_element()
}

/// The letter and colour of a file's status in the tree.
fn status_mark(status: Option<TreeStatus>, tokens: &Tokens) -> (&'static str, Hsla) {
    match status {
        Some(TreeStatus::Added) => ("A", tokens.green),
        Some(TreeStatus::Deleted) => ("D", tokens.red),
        Some(TreeStatus::Renamed) => ("R", tokens.blue),
        Some(TreeStatus::Modified) => ("M", tokens.yellow),
        None => (" ", tokens.muted),
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::dashboard::Tokens;

    use super::super::super::model::TreeStatus;
    use super::{TREE_MIN_W_PX, status_mark, tree_width};

    #[test]
    fn the_tree_keeps_its_width_between_half_the_window_and_the_minimum() {
        assert_eq!(tree_width(240.0, 1600.0), 240.0, "the token width fits");
        assert_eq!(tree_width(900.0, 1000.0), 500.0, "half the window at most");
        assert_eq!(tree_width(80.0, 1600.0), TREE_MIN_W_PX, "never narrower");
    }

    #[test]
    fn each_file_status_has_its_letter_and_colour() {
        let tokens = Tokens::one_dark();
        let letters: Vec<&str> = [
            Some(TreeStatus::Added),
            Some(TreeStatus::Deleted),
            Some(TreeStatus::Renamed),
            Some(TreeStatus::Modified),
            None,
        ]
        .into_iter()
        .map(|status| status_mark(status, &tokens).0)
        .collect();

        assert_eq!(letters, ["A", "D", "R", "M", " "]);
        assert_eq!(
            status_mark(Some(TreeStatus::Added), &tokens).1,
            tokens.green
        );
        assert_eq!(status_mark(None, &tokens).1, tokens.muted);
    }
}
