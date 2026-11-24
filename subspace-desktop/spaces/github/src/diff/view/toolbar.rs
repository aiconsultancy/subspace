//! The shared diff toolbar and its GitHub controls.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, Div, ParentElement as _, Styled as _, div, px};
use subspace_framework::client_github::model::GhItem;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};
use subspace_framework::dashboard::{Tokens, links, truncated};

use super::{BAR_PAD_PX, DiffEvent, DiffSource, DiffView, Layout};
use crate::rows::state_glyph;

impl DiffView {
    pub(super) fn title_bar(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        match &self.source {
            DiffSource::GitHub(source) => self.github_bar(&source.item, tokens, cx),
            DiffSource::Fleet(source) => self.fleet_bar(source, tokens, cx),
        }
    }

    pub(super) fn bar(tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        h_flex()
            .flex_shrink_0()
            .h(tokens.toolbar_height)
            .gap_2()
            .px(px(BAR_PAD_PX))
            .border_b_1()
            .border_color(tokens.line)
            .text_size(tokens.row_font)
            .whitespace_nowrap()
            .child(
                action_button(
                    "diff-back",
                    Intent::Quiet,
                    Some(IconName::ChevronLeft),
                    "Back",
                )
                .on_click(cx.listener(|_, _, _, cx| cx.emit(DiffEvent::Close))),
            )
    }

    fn github_bar(&self, item: &GhItem, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let icon = state_glyph(item);
        Self::bar(tokens, cx)
            .child(div().text_color(tokens.glyph(&icon)).child(icon.text))
            .child(
                div()
                    .text_color(tokens.muted)
                    .child(format!("{}#{}", item.repo, item.number)),
            )
            .child(
                truncated::text("diff-pr-title", item.title.clone())
                    .flex_1()
                    .text_color(tokens.fg_strong),
            )
            .child(div().text_color(tokens.muted).child(branch_text(item)))
            .when_some(
                item.additions.zip(item.deletions),
                |bar, (additions, deletions)| {
                    bar.child(
                        div()
                            .text_color(tokens.green)
                            .child(format!("+{additions}")),
                    )
                    .child(div().text_color(tokens.red).child(format!("−{deletions}")))
                },
            )
            .child(self.layout_toggles(tokens, cx))
            .child(
                action_button(
                    "diff-github",
                    Intent::Neutral,
                    Some(IconName::ExternalLink),
                    "GitHub",
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    if let Some(url) = view.files_url() {
                        links::open(&url, cx);
                    }
                })),
            )
    }

    /// The unified and split choices of the title bar.
    pub(super) fn layout_toggles(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let entity = cx.entity().downgrade();
        segmented::control(
            "diff-layout",
            [Segment::new("Unified"), Segment::new("Split")],
            Some(layout_index(self.layout)),
            tokens,
            move |index, _, _, cx| {
                _ = entity.update(cx, |view, cx| view.set_layout(selected_layout(index), cx));
            },
        )
    }
}

fn layout_index(layout: Layout) -> usize {
    usize::from(layout == Layout::Split)
}

fn selected_layout(index: usize) -> Layout {
    if index == 0 {
        Layout::Unified
    } else {
        Layout::Split
    }
}

fn branch_text(item: &GhItem) -> String {
    match (&item.head_ref, &item.base_ref) {
        (Some(head), Some(base)) => format!("{head} → {base}"),
        _ => String::new(),
    }
}
