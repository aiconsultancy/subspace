//! Content-sized controls avoid the SDK action wrappers' full-size containers.

use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{ParentElement as _, Styled as _, div};
use subspace_dashboard::action::{Intent, dialog_button};
use subspace_lucide::IconName;

use super::lifecycle::Lifecycle;

pub(super) fn settings(lifecycle: Lifecycle) -> DialogFooter {
    let cancel = lifecycle.clone();
    DialogFooter::new().flex_wrap().child(
        div()
            .flex()
            .flex_wrap()
            .w_full()
            .min_w_0()
            .justify_end()
            .items_center()
            .gap_2()
            .child(
                dialog_button(
                    "settings-cancel",
                    Intent::Neutral,
                    Some(IconName::X),
                    "Cancel",
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    cancel.dismiss(window, cx);
                }),
            )
            .child(
                dialog_button(
                    "settings-save",
                    Intent::Primary,
                    Some(IconName::Save),
                    "Save",
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    lifecycle.save_and_close(window, cx);
                }),
            ),
    )
}

pub(super) fn confirmation(lifecycle: Lifecycle) -> DialogFooter {
    let discard = lifecycle.clone();
    DialogFooter::new()
        .flex_wrap()
        .child(
            dialog_button(
                "settings-discard",
                Intent::Neutral,
                Some(IconName::X),
                "Exit without saving",
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                discard.discard(window, cx);
            }),
        )
        .child(
            dialog_button(
                "settings-save-and-exit",
                Intent::Primary,
                Some(IconName::Save),
                "Save and exit",
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                lifecycle.save_and_exit(window, cx);
            }),
        )
}
