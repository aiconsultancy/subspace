//! The authoring canvas: profile/layout selector plus the composer/compact
//! arrangement of the controls panel and the result panel.

use gpui_kit::component::h_flex;
use gpui_kit::component::v_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};

use super::super::Workbench;
use super::super::state::{CanvasLayout, Profile};
use super::controls::controls_panel;
use super::dims::{COMPACT_RESULT_HEIGHT, COMPOSER_SIDEBAR_MIN_WIDTH, COMPOSER_SIDEBAR_WIDTH};
use super::environments::active_environment;
use super::result::result_panel;

pub(super) fn canvas(
    this: &mut Workbench,
    window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    v_flex()
        .flex_1()
        .min_h_0()
        .child(profile_and_layout_row(this, cx, t))
        .child(match this.view.layout {
            CanvasLayout::Composer => h_flex()
                .flex_1()
                .min_h_0()
                .child(
                    v_flex()
                        .w(COMPOSER_SIDEBAR_WIDTH)
                        .min_w(COMPOSER_SIDEBAR_MIN_WIDTH)
                        .h_full()
                        .min_h_0()
                        .flex_shrink_0()
                        .gap_3()
                        .p(t.pad_x)
                        .bg(t.pane)
                        .border_r_1()
                        .border_color(t.line)
                        .id("workbench-composer-controls")
                        .overflow_y_scroll()
                        .child(active_environment(this, cx, t))
                        .child(controls_panel(this, window, cx, t)),
                )
                .child(
                    result_panel(this, window, cx, t)
                        .h_full()
                        .flex_1()
                        .min_h_0(),
                )
                .into_any_element(),
            CanvasLayout::Compact => v_flex()
                .gap_3()
                .flex_1()
                .min_h_0()
                .id("workbench-compact-scroll")
                .overflow_y_scroll()
                .child(
                    v_flex()
                        .px(t.pad_x)
                        .pt(t.pad_x)
                        .gap_3()
                        .child(active_environment(this, cx, t))
                        .child(controls_panel(this, window, cx, t)),
                )
                .child(
                    result_panel(this, window, cx, t)
                        .h(COMPACT_RESULT_HEIGHT)
                        .flex_none(),
                )
                .into_any_element(),
        })
}

/// The layout's place among the layout toggles: Composer, then Compact.
fn layout_index(layout: CanvasLayout) -> usize {
    match layout {
        CanvasLayout::Composer => 0,
        CanvasLayout::Compact => 1,
    }
}

fn profile_and_layout_row(
    this: &mut Workbench,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let entity = cx.entity();
    let profile_entity = entity.clone();
    let profiles = segmented::control(
        "workbench-profiles",
        Profile::ALL
            .into_iter()
            .map(|profile| Segment::new(profile.title()).icon(IconName::Cpu)),
        Profile::ALL.iter().position(|p| *p == this.view.profile),
        t,
        move |index, _, window, cx| {
            if let Some(profile) = Profile::ALL.get(index) {
                profile_entity.update(cx, |this, cx| {
                    this.load_profile(*profile, window, cx);
                });
            }
        },
    );
    let layout_entity = entity;
    let layout = segmented::control(
        "workbench-layout",
        [
            Segment::new("Composer").icon(IconName::Columns3),
            Segment::new("Compact").icon(IconName::Table),
        ],
        Some(layout_index(this.view.layout)),
        t,
        move |index, _, _, cx| {
            layout_entity.update(cx, |this, cx| {
                this.view.layout = if index == 0 {
                    CanvasLayout::Composer
                } else {
                    CanvasLayout::Compact
                };
                cx.notify();
            });
        },
    );
    h_flex()
        .px(t.pad_x)
        .py_2()
        .gap_3()
        .justify_between()
        .child(profiles)
        .child(layout)
        .flex_wrap()
        .border_b_1()
        .border_color(t.line)
}
