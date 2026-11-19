//! The saved-preset gallery: dashboard layout mode selector, preset cards in
//! layout order, and the export/import round-trip demo.

use std::rc::Rc;

use gpui_kit::component::v_flex;
use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};
use subspace_framework::dashboard::typography;

use crate::contracts::Preset;

use super::super::Workbench;
use super::dims::{
    GALLERY_CARD_WIDTH_NARROW, GALLERY_CARD_WIDTH_WIDE, GALLERY_CHART_HEIGHT_NARROW,
    GALLERY_CHART_HEIGHT_WIDE,
};
use super::result::chart_model;

pub(super) fn gallery(
    this: &mut Workbench,
    window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let ordered = ordered_presets(this);
    let busy = this.jobs.busy;
    let mode = this.dashboard_layout.mode.clone();
    let mut cards = Vec::new();
    for preset in ordered {
        cards.push(gallery_card(preset, &mode, busy, t, cx));
    }
    let layout_entity = cx.entity();

    v_flex()
        .p(t.pad_x)
        .gap_3()
        .flex_1()
        .min_h_0()
        .child(h_flex().child(layout_mode_control(this, t, layout_entity)))
        .child(
            div()
                .text_size(t.secondary_size())
                .text_color(t.muted)
                .child(
                    "Saved presets survive a restart; each tile replays its last saved snapshot, \
                     not a live re-run.",
                ),
        )
        .child(
            h_flex()
                .id("workbench-gallery-cards")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .gap_3()
                .flex_wrap()
                .children(cards),
        )
        .child(export_import_demo(this, window, cx, t))
}

/// Presets in the saved dashboard's order, followed by any preset the
/// dashboard layout doesn't know about yet.
fn ordered_presets(this: &Workbench) -> Vec<Preset> {
    let mut ordered = Vec::new();
    for id in &this.dashboard_layout.order {
        if let Some(preset) = this.presets.iter().find(|preset| preset.id == *id) {
            ordered.push(preset.clone());
        }
    }
    for preset in &this.presets {
        if !ordered
            .iter()
            .any(|current: &Preset| current.id == preset.id)
        {
            ordered.push(preset.clone());
        }
    }
    ordered
}

fn layout_mode_control(
    this: &Workbench,
    t: &Tokens,
    layout_entity: gpui_kit::Entity<Workbench>,
) -> impl IntoElement {
    segmented::control(
        "workbench-dashboard-layout",
        [
            Segment::new("Grid").icon(IconName::LayoutDashboard),
            Segment::new("Wide").icon(IconName::Columns3),
            Segment::new("Rows").icon(IconName::Table),
        ],
        ["grid", "wide", "rows"]
            .iter()
            .position(|mode| *mode == this.dashboard_layout.mode.as_str()),
        t,
        move |index, _, _, cx| {
            let mode = ["grid", "wide", "rows"][index];
            layout_entity.update(cx, |this, cx| {
                this.change_layout(Some(mode.into()), None, cx);
            });
        },
    )
}

fn gallery_card(
    preset: Preset,
    mode: &str,
    busy: bool,
    t: &Tokens,
    cx: &mut Context<Workbench>,
) -> impl IntoElement + use<> {
    let heading_font = typography::heading_font(cx);
    let id = preset.id.clone();
    let model = preset
        .snapshot
        .as_ref()
        .map(|dataset| Rc::new(chart_model(dataset, &preset.view)));
    let card = v_flex()
        .w(if mode == "wide" {
            GALLERY_CARD_WIDTH_WIDE
        } else {
            GALLERY_CARD_WIDTH_NARROW
        })
        .p(t.pad_x)
        .gap_2()
        .border_1()
        .border_color(t.line)
        .rounded(t.surface_radius)
        .bg(t.pane)
        .child(
            div()
                .font_family(heading_font)
                .font_weight(FontWeight::MEDIUM)
                .text_size(t.heading_font)
                .text_color(t.fg_strong)
                .child(preset.title.clone()),
        )
        .child(
            subspace_framework::dashboard::truncated::text(
                SharedString::from(format!("workbench-gallery-meta-{id}")),
                format!(
                    "{} · rev {} · {}",
                    preset.environment, preset.revision, preset.updated_at
                ),
            )
            .w_full()
            .text_size(t.caption_size())
            .text_color(t.muted),
        )
        .children(model.map(|model| gallery_chart(model, &id, mode)))
        .child(gallery_card_actions(&id, busy, cx));
    if mode == "rows" { card.w_full() } else { card }
}

fn gallery_chart(
    model: Rc<subspace_framework::charts::ChartModel>,
    id: &str,
    mode: &str,
) -> impl IntoElement {
    subspace_framework::dashboard::chart::Chart::new(
        SharedString::from(format!("workbench-gallery-{id}")),
        model,
    )
    .height(if mode == "wide" {
        GALLERY_CHART_HEIGHT_WIDE
    } else {
        GALLERY_CHART_HEIGHT_NARROW
    })
}

fn gallery_card_actions(id: &str, busy: bool, cx: &mut Context<Workbench>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .flex_wrap()
        .child(
            icon_button(
                SharedString::from(format!("workbench-move-left-{id}")),
                Intent::Quiet,
                IconName::ChevronLeft,
                "Earlier",
            )
            .disabled(busy)
            .on_click(cx.listener({
                let id = id.to_owned();
                move |this, _, _, cx| this.change_layout(None, Some((id.clone(), -1)), cx)
            })),
        )
        .child(
            icon_button(
                SharedString::from(format!("workbench-move-right-{id}")),
                Intent::Quiet,
                IconName::ChevronRight,
                "Later",
            )
            .disabled(busy)
            .on_click(cx.listener({
                let id = id.to_owned();
                move |this, _, _, cx| this.change_layout(None, Some((id.clone(), 1)), cx)
            })),
        )
        .child(
            action_button(
                SharedString::from(format!("workbench-open-{id}")),
                Intent::Primary,
                Some(IconName::FolderOpen),
                "Open",
            )
            .on_click(cx.listener({
                let id = id.to_owned();
                move |this, _, window, cx| this.open_preset(&id, window, cx)
            })),
        )
        .child(
            action_button(
                SharedString::from(format!("workbench-gallery-delete-{id}")),
                Intent::Reject,
                Some(IconName::Trash),
                "Delete",
            )
            .disabled(busy)
            .on_click(cx.listener({
                let id = id.to_owned();
                move |this, _, _, cx| this.delete(id.clone(), cx)
            })),
        )
}

/// A local, explicitly-labelled demonstration of the canvas export/import
/// round trip. This is a tool action a person clicks, not a simulated
/// Copilot response.
fn export_import_demo(
    this: &mut Workbench,
    _window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let has_result = this.last_result.is_some();
    h_flex()
        .gap_2()
        .flex_wrap()
        .pt_2()
        .border_t_1()
        .border_color(t.hair)
        .child(
            div()
                .text_size(t.secondary_size())
                .text_color(t.muted)
                .child(
                    "Local tool action: export the open canvas, then import the export as a new \
                     snapshot preset.",
                ),
        )
        .child(
            action_button(
                "workbench-export-import",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Export → import copy",
            )
            .disabled(!has_result || this.jobs.busy)
            .on_click(cx.listener(|this, _, _, cx| this.export_import_round_trip(cx))),
        )
}
