use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div,
};
use subspace_lucide::IconName;

use super::super::SearchView;
use subspace_dashboard::{
    Tokens,
    action::{Intent, action_button},
    fields::{self, FieldLayout, LabelledInput},
};

impl SearchView {
    pub(super) fn source_panel(&self, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        v_flex().flex_1().min_h_0().child(v_flex().id("search-source-settings").flex_1().min_h_0().overflow_y_scroll().px(t.pad_x).py_4().gap_4()
            .child(v_flex().gap_1().child(div().text_size(t.heading_font).child("Cached sources"))
                .child(div().text_color(t.muted).child("Choose per board. Content stays local and belongs to the verified account. Disabling removes it from search immediately.")))
            .children(self.editors.iter().enumerate().map(|(index, _)| self.source_row(index, cx)))
            .children(self.editors.is_empty().then(|| div().child("Loading source settings…")))
            .children(self.confirm_clear.as_ref().map(|board| self.clear_confirmation(board, cx)))
            .child(self.model_panel(cx)))
    }

    fn source_row(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        let editor = &self.editors[index];
        let board = editor.board.clone();
        let status = self.sources.iter().find(|source| source.board == board);
        let coverage = status.map_or_else(
            || "No account or cached content available.".into(),
            |source| {
                format!(
                    "{} records · {:.2} GB · {} · {}",
                    source.records,
                    source.bytes as f64 / 1_000_000_000.,
                    source.account.as_deref().unwrap_or("No active account"),
                    source.message
                )
            },
        );
        v_flex()
            .gap_2()
            .pb_4()
            .border_b_1()
            .border_color(t.line)
            .child(self.source_heading(index, cx))
            .child(div().text_color(t.muted).child(coverage))
            .child(self.source_limits(index, cx))
    }

    fn source_heading(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let editor = &self.editors[index];
        let enabled = editor.policy.enabled;
        h_flex().w_full().gap_2().child(IconName::Database).child(
            subspace_dashboard::toggle::toggle_row(
                format!("Cache {}", self.board_name(&editor.board)),
                subspace_dashboard::toggle::switch(("source-toggle", index), cx)
                    .checked(enabled)
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        this.save_source(index, *checked, window, cx)
                    })),
            )
            .flex_1()
            .min_w_0(),
        )
    }

    fn source_limits(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        let editor = &self.editors[index];
        let enabled = editor.policy.enabled;
        let board = editor.board.clone();
        h_flex()
            .items_end()
            .gap_3()
            .child(fields::labelled_input(
                LabelledInput::new(&editor.days, "Retention (days)", FieldLayout::Stacked),
                &t,
            ))
            .child(fields::labelled_input(
                LabelledInput::new(
                    &editor.gigabytes,
                    "Maximum storage (GB)",
                    FieldLayout::Stacked,
                ),
                &t,
            ))
            .child(
                action_button(
                    ("source-save", index),
                    Intent::Primary,
                    Some(IconName::Check),
                    "Save limits",
                )
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.save_source(index, enabled, window, cx)
                })),
            )
            .child(
                action_button(
                    ("source-clear", index),
                    Intent::Reject,
                    Some(IconName::Trash),
                    "Clear cache…",
                )
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm_clear = Some(board.clone());
                    cx.notify();
                })),
            )
    }

    fn clear_confirmation(&self, board: &str, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        let board = board.to_owned();
        v_flex().gap_2().p_3().bg(t.pane)
            .child(div().text_color(t.fg_strong).child(format!("Clear cached content for {}?", self.board_name(&board))))
            .child("This removes local indexed content, not service data. New successful reads can be cached again if caching remains enabled.")
            .child(h_flex().gap_2()
                .child(action_button("search-cancel-clear", Intent::Neutral, Some(IconName::X), "Cancel")
                    .on_click(cx.listener(|this, _, _, cx| { this.confirm_clear = None; cx.notify(); })))
                .child(action_button("search-confirm-clear", Intent::Reject, Some(IconName::Trash), "Clear local cache")
                    .disabled(self.busy).on_click(cx.listener(move |this, _, window, cx| this.clear_source(board.clone(), window, cx)))))
    }

    fn model_panel(&self, cx: &mut Context<Self>) -> Div {
        let t = Tokens::current(cx);
        let mut panel = v_flex().gap_2()
            .child(div().text_size(t.heading_font).child("Optional local ranking"))
            .child(div().text_color(t.muted).child("BM25 works without a model. Local reranking reorders matching candidates; queries and cached text stay on this device."));
        if let Some(model) = &self.model {
            panel = panel
                .child(format!(
                    "{} · {:.0} MB download · {}",
                    model.name,
                    model.download_bytes as f64 / 1_000_000.,
                    model.message
                ))
                .child(
                    action_button(
                        "search-install-model",
                        Intent::Neutral,
                        Some(IconName::Download),
                        if self.installing {
                            "Preparing local model…"
                        } else if model.installed {
                            "Verify / repair model"
                        } else {
                            "Download local model"
                        },
                    )
                    .disabled(self.installing)
                    .loading(self.installing)
                    .on_click(cx.listener(|this, _, window, cx| this.install_model(window, cx))),
                );
        } else {
            panel = panel.child(
                div()
                    .text_color(t.muted)
                    .child("Local reranking is unavailable in this app. Search uses BM25."),
            );
        }
        panel
    }
}
