//! The non-code rows of the diff list: file headers, notes and hunk gaps.

use gpui_kit::component::{Selectable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use subspace_framework::client_github::model::{GhFileStatus, image_type};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::truncated;
use subspace_framework::dashboard::{Tokens, links, typography};

use super::super::super::model::gaps;
use super::super::{
    BAR_PAD_PX, DiffView, FOLD_W_PX, GAP_BG_OPACITY, GAP_HOVER_OPACITY, NOTE_PAD_PX, Text,
    code_row_height,
};

impl DiffView {
    pub(super) fn file_header(
        &self,
        file: usize,
        id: impl Into<ElementId>,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = &self.files[file];
        let viewed = self.viewed.contains(&view.file.path);
        let collapsed = self.collapsed.contains(&file);
        let name = header_name(view.file.previous_path.as_deref(), &view.file.path);
        let github = self.item().is_some();
        let svg = github
            && view.file.patch.is_some()
            && image_type(&view.file.path) == Some("image/svg+xml");
        h_flex()
            .id(id)
            .h(px(code_row_height(tokens)))
            .w_full()
            .px(px(BAR_PAD_PX))
            .gap_2()
            .bg(tokens.pane)
            .border_t_1()
            .border_b_1()
            .border_color(tokens.line)
            .whitespace_nowrap()
            .text_size(tokens.row_font)
            .cursor_pointer()
            .child(
                div()
                    .w(px(FOLD_W_PX))
                    .text_color(tokens.muted)
                    .child(if collapsed { "▸" } else { "▾" }),
            )
            .child(
                typography::group_heading(
                    truncated::text(("diff-file-name", file), name.clone()).flex_1(),
                    cx,
                )
                .text_color(tokens.fg_strong),
            )
            .when(!view.binary, |row| {
                row.child(
                    div()
                        .text_color(tokens.green)
                        .child(format!("+{}", view.file.additions)),
                )
                .child(
                    div()
                        .text_color(tokens.red)
                        .child(format!("−{}", view.file.deletions)),
                )
            })
            .when(svg, |el| el.child(view_image_button(file, cx)))
            .when(github, |el| el.child(viewed_mark(file, viewed, tokens, cx)))
            .on_click(cx.listener(move |view, _, _, cx| view.toggle_collapsed(file, cx)))
            .into_any_element()
    }

    pub(super) fn note_row(
        &self,
        file: usize,
        index: usize,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = &self.files[file];
        let reason = view.reason.unwrap_or_default();
        let base = h_flex()
            .id(("diff-note", index))
            .h(px(code_row_height(tokens)))
            .w_full()
            .px(px(NOTE_PAD_PX))
            .gap_3()
            .text_size(tokens.row_font)
            .text_color(tokens.muted);
        if self.item().is_some() && view.image_only() {
            return base
                .cursor_pointer()
                .child("Image file")
                .child(div().text_color(tokens.accent).child("View image"))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.current = file;
                    view.open_image(file, cx);
                }))
                .into_any_element();
        }
        let too_large = reason == "Diff too large to show here";
        base.child(reason)
            .when(self.item().is_none() && view.binary, |el| {
                el.child(
                    action_button(
                        ("crew-diff-bytes", file),
                        Intent::Quiet,
                        Some(IconName::FileCode),
                        "View binary",
                    )
                    .on_click(
                        cx.listener(move |view, _, window, cx| view.open_binary(file, window, cx)),
                    ),
                )
            })
            .when_some(view.binary_size, |el, bytes| {
                el.child(format!("{bytes} bytes"))
            })
            .when_some(
                self.blob_url(&view.file).filter(|_| too_large),
                |el, url| {
                    el.child(
                        div()
                            .id(("diff-blob", index))
                            .cursor_pointer()
                            .text_color(tokens.accent)
                            .child("View on GitHub ↗")
                            .on_click(move |_, _, cx| {
                                links::open(&url, cx);
                            }),
                    )
                },
            )
            .into_any_element()
    }

    pub(super) fn more_row(
        &self,
        file: usize,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = h_flex().h(px(code_row_height(tokens))).px(px(BAR_PAD_PX));
        if self.fleet.changed() {
            return row
                .child(
                    action_button(
                        ("crew-diff-more-refresh", file),
                        Intent::Quiet,
                        Some(IconName::RefreshCw),
                        "Refresh diff",
                    )
                    .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
                )
                .into_any_element();
        }
        if self.fleet.loading.contains(&file) {
            return row
                .text_color(tokens.muted)
                .child("Loading…")
                .into_any_element();
        }
        row.child(
            action_button(
                ("crew-diff-more", file),
                Intent::Quiet,
                Some(IconName::ChevronDown),
                "Load next page",
            )
            .on_click(cx.listener(move |view, _, _, cx| view.load_more(file, cx))),
        )
        .into_any_element()
    }

    pub(super) fn gap_row(
        &self,
        file: usize,
        gap: usize,
        index: usize,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = &self.files[file];
        let lengths = match self.texts.get(&file) {
            Some(Text::Ready(lines)) => u32::try_from(lines.len()).ok(),
            _ => None,
        };
        let info = gaps(&view.patch.hunks, lengths).get(gap).copied();
        let hidden = info.and_then(|gap| gap.len);
        let header = view
            .patch
            .hunks
            .get(gap)
            .map(|hunk| hunk.header.clone())
            .unwrap_or_default();
        let expandable = self.item().is_some()
            && !self.expanded.contains(&(file, gap))
            && hidden != Some(0)
            && !matches!(
                view.file.status,
                GhFileStatus::Added | GhFileStatus::Removed
            );
        let loading = matches!(self.texts.get(&file), Some(Text::Loading));
        let label = match (hidden, loading) {
            (_, true) => "⋯ loading".to_string(),
            (Some(0), _) => String::new(),
            (Some(1), _) => "⋯ 1 hidden line".to_string(),
            (Some(n), _) => format!("⋯ {n} hidden lines"),
            (None, _) => "⋯ expand".to_string(),
        };
        h_flex()
            .id(("diff-gap", index))
            .h(px(code_row_height(tokens)))
            .w_full()
            .px(px(BAR_PAD_PX))
            .gap_3()
            .bg(tokens.blue.opacity(GAP_BG_OPACITY))
            .whitespace_nowrap()
            .text_color(tokens.muted)
            .when(expandable, |el| {
                el.cursor_pointer()
                    .hover(|style| style.bg(tokens.blue.opacity(GAP_HOVER_OPACITY)))
                    .child(div().text_color(tokens.blue).child(label))
                    .on_click(cx.listener(move |view, _, _, cx| view.expand(file, gap, cx)))
            })
            .child(truncated::text(("diff-hunk-header", gap), header).min_w_0())
            .into_any_element()
    }
}

fn view_image_button(file: usize, cx: &Context<DiffView>) -> impl IntoElement {
    action_button(
        ("diff-view-image", file),
        Intent::Neutral,
        Some(IconName::Eye),
        "View image",
    )
    .on_click(cx.listener(move |view, _, _, cx| {
        cx.stop_propagation();
        view.open_image(file, cx);
    }))
}

/// The "☑ Viewed" box of a file header.
fn viewed_mark(
    file: usize,
    viewed: bool,
    tokens: &Tokens,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    action_button(
        ("diff-viewed", file),
        Intent::Quiet,
        Some(if viewed {
            IconName::Check
        } else {
            IconName::Square
        }),
        "Viewed",
    )
    .compact()
    .selected(viewed)
    .toggled(viewed)
    .text_color(if viewed { tokens.green } else { tokens.muted })
    .on_click(cx.listener(move |view, _, _, cx| {
        cx.stop_propagation();
        view.toggle_viewed_file(file, false, cx);
    }))
}

/// A file header's name: "old → new" when the file moved.
fn header_name(previous_path: Option<&str>, path: &str) -> String {
    match previous_path {
        Some(previous) if previous != path => format!("{previous} → {path}"),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::header_name;

    #[test]
    fn a_moved_file_header_names_both_paths() {
        assert_eq!(header_name(Some("old.rs"), "new.rs"), "old.rs → new.rs");
        assert_eq!(header_name(Some("same.rs"), "same.rs"), "same.rs");
        assert_eq!(header_name(None, "new.rs"), "new.rs");
    }
}
