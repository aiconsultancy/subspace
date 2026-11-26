//! Controls for selecting a side and paging or previewing its bytes.

use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, Div, IntoElement, ParentElement as _, Styled as _, div};
use subspace_client_fleet::diff::DiffSide;
use subspace_framework::client_github::model::GhFileStatus;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};

use super::{BinaryView, DiffView, Move, Retry, preview};
use crate::diff::view::pages::busy_message;

impl DiffView {
    pub(super) fn binary_controls(
        &self,
        binary: &BinaryView,
        tokens: &Tokens,
        cx: &Context<Self>,
    ) -> Div {
        let changed = self.fleet.changed();
        let status = self.files[binary.file].file.status;
        h_flex()
            .px_3()
            .gap_2()
            .flex_wrap()
            .child(side_control(binary, status, changed, tokens, cx))
            .child(paging_controls(self, binary, tokens, cx))
    }
}

fn side_control(
    binary: &BinaryView,
    status: GhFileStatus,
    changed: bool,
    tokens: &Tokens,
    cx: &Context<DiffView>,
) -> AnyElement {
    side_label(binary.side, status, changed).map_or_else(
        || side_selector(binary, tokens, cx),
        IntoElement::into_any_element,
    )
}

fn side_label(side: DiffSide, status: GhFileStatus, changed: bool) -> Option<&'static str> {
    (changed || matches!(status, GhFileStatus::Added | GhFileStatus::Removed)).then_some(match side
    {
        DiffSide::Old => "Old bytes",
        DiffSide::New => "New bytes",
    })
}

fn side_selector(binary: &BinaryView, tokens: &Tokens, cx: &Context<DiffView>) -> AnyElement {
    let file = binary.file;
    let weak = cx.entity().downgrade();
    segmented::control(
        "crew-binary-side",
        [Segment::new("Old"), Segment::new("New")],
        Some(binary.side_index()),
        tokens,
        move |index, _, window, cx| {
            _ = weak.update(cx, |view, cx| {
                view.select_binary_side(file, index, window, cx)
            });
        },
    )
    .into_any_element()
}

fn paging_controls(
    view: &DiffView,
    binary: &BinaryView,
    tokens: &Tokens,
    cx: &Context<DiffView>,
) -> Div {
    let row = h_flex().gap_2().flex_wrap();
    if view.fleet.changed() {
        return row
            .child(
                action_button(
                    "crew-binary-refresh",
                    Intent::Quiet,
                    Some(IconName::RefreshCw),
                    "Refresh diff",
                )
                .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
            )
            .when(binary.preview.is_some(), |row| {
                row.child(preview::bytes_button(cx))
            });
    }
    if binary.read.is_some() {
        return row
            .when(binary.page.is_some(), |row| {
                row.children(binary_actions(binary, true, cx))
            })
            .child(div().text_color(tokens.muted).child("Loading…"));
    }
    if let Some(retry) = &binary.retry {
        let remaining = view.binary_retry_remaining(cx);
        let bytes_busy = matches!(retry, Retry::Bytes(_));
        return row
            .when(!bytes_busy || binary.page.is_some(), |row| {
                row.children(binary_actions(binary, bytes_busy || remaining > 0, cx))
            })
            .child(
                action_button(
                    "crew-binary-retry-page",
                    Intent::Quiet,
                    Some(IconName::RefreshCw),
                    "Retry page",
                )
                .disabled(remaining > 0)
                .on_click(cx.listener(|view, _, _, cx| view.retry_binary(cx))),
            )
            .text_color(tokens.muted)
            .child(busy_message(remaining));
    }
    if binary.preview.is_some() {
        return row.child(preview::bytes_button(cx));
    }
    row.children(binary_actions(binary, false, cx))
        .when_some(preview::format(view.binary_path(binary)), |row, format| {
            row.child(preview::open_button(format, cx))
        })
}

fn binary_actions(binary: &BinaryView, disabled: bool, cx: &Context<DiffView>) -> Vec<AnyElement> {
    [
        (
            Move::Start,
            binary.page.is_none(),
            "crew-binary-load",
            IconName::RefreshCw,
            "Load bytes",
        ),
        (
            Move::Previous,
            !binary.history.is_empty(),
            "crew-binary-previous",
            IconName::ChevronUp,
            "Previous page",
        ),
        (
            Move::Next,
            binary
                .page
                .as_ref()
                .is_some_and(|page| page.next_page.is_some()),
            "crew-binary-next",
            IconName::ChevronDown,
            "Next page",
        ),
    ]
    .into_iter()
    .filter(|(_, show, _, _, _)| *show)
    .map(|(direction, _, id, icon, label)| {
        action_button(id, Intent::Quiet, Some(icon), label)
            .disabled(disabled)
            .on_click(cx.listener(move |view, _, _, cx| view.read_binary(direction, cx)))
            .into_any_element()
    })
    .collect()
}
