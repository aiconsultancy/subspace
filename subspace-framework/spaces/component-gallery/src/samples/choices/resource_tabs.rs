//! Controlled selection, unread state and real close callbacks.

use crate::{
    ComponentGallery,
    samples::{self, Sample},
};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, ParentElement as _, Styled as _};
use subspace_dashboard::{
    action::{self, Intent},
    tabs::{self, TabSpec},
};
use subspace_lucide::IconName;

const SELECTED: &str = "resource-tabs-selected";
const CLOSED: &str = "resource-tabs-closed";
const LABELS: [&str; 3] = ["#general", "#ops", "#design"];

pub(super) fn sample(gallery: &ComponentGallery, cx: &mut Context<ComponentGallery>) -> Sample {
    let values = gallery.retained.values.clone();
    let selected = samples::number(&values, SELECTED, 0, cx);
    let closed = samples::number(&values, CLOSED, 0, cx);
    let open: Vec<_> = LABELS
        .into_iter()
        .enumerate()
        .filter(|(ix, _)| closed & (1 << ix) == 0)
        .collect();
    let remaining = open.len().saturating_sub(1);
    let tabs = open
        .into_iter()
        .enumerate()
        .map(|(shown, (ix, label))| {
            let values = values.clone();
            tabs::tab(TabSpec::new(label, IconName::Hash).unread(label == "#ops")).on_close(
                move |_, _, cx| {
                    samples::store_number(&values, CLOSED, closed | (1 << ix), cx);
                    let next = after_close(selected, shown, remaining);
                    samples::store_number(&values, SELECTED, next, cx);
                },
            )
        })
        .collect::<Vec<_>>();
    let select_values = values.clone();
    samples::sample(
        "Selected, unread, hover and keyboard focus",
        v_flex()
            .gap_2()
            .child(
                tabs::tab_bar("gallery-resource-tabs")
                    .selected_index(selected)
                    .children(tabs)
                    .on_click(move |ix, _, cx| {
                        samples::store_number(&select_values, SELECTED, *ix, cx)
                    }),
            )
            .when(closed != 0, |body| {
                body.child(
                    action::action_button(
                        "gallery-reset-tabs",
                        Intent::Neutral,
                        Some(IconName::RotateCcw),
                        "Reset tabs",
                    )
                    .on_click(move |_, _, cx| samples::store_number(&values, CLOSED, 0, cx)),
                )
            }),
    )
}

/// Closing a tab keeps the selected one selected; closing the selected tab
/// selects its neighbour.
fn after_close(selected: usize, closed: usize, remaining: usize) -> usize {
    let next = if selected > closed {
        selected - 1
    } else {
        selected
    };
    next.min(remaining.saturating_sub(1))
}

#[cfg(test)]
mod close_tests {
    use super::after_close;

    #[test]
    fn closing_a_tab_keeps_the_selection_on_the_same_tab() {
        assert_eq!(after_close(2, 0, 3), 1);
        assert_eq!(after_close(0, 2, 3), 0);
        assert_eq!(after_close(1, 1, 3), 1);
        assert_eq!(after_close(3, 3, 3), 2);
        assert_eq!(after_close(0, 0, 0), 0);
    }
}
