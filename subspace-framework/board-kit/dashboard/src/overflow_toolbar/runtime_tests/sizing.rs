use super::{Harness, settle};
use crate::{
    icon_button::{IconButton, IconButtonSize},
    overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry},
};
use gpui_kit::component::{button::ButtonVariants as _, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Pixels, TestAppContext, Window, px};
use std::{cell::Cell, rc::Rc};

fn units(calls: Rc<Cell<usize>>) -> Vec<OverflowUnit> {
    (0..3)
        .map(|index| {
            let calls = calls.clone();
            OverflowUnit::inline(
                format!("sized-{index}"),
                1.,
                move |_, _| {
                    IconButton::new(format!("sized-button-{index}"))
                        .ghost()
                        .icon(subspace_lucide::IconName::Search)
                        .accessibility_label("Search")
                },
                vec![PopupEntry::new(move || {
                    let calls = calls.clone();
                    PopupMenuItem::new(format!("Action {index}"))
                        .on_click(move |_, _, _| calls.set(calls.get() + 1))
                })],
            )
        })
        .collect()
}

#[gpui_kit::test]
fn live_global_size_remeasures_units_gap_and_overflow_without_replacing_toolbar(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let calls = Rc::new(Cell::new(0));
    let toolbar = cx.new(|_| OverflowToolbar::new(units(calls.clone())));
    let observed = toolbar.clone();
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 140.,
        left: 0.,
    });
    for (choice, width, visible, trigger_width, target) in [
        (IconButtonSize::Small, 80., 3, 28., 24.),
        (IconButtonSize::Medium, 120., 3, 42., 36.),
        (IconButtonSize::Large, 160., 1, 56., 48.),
        (IconButtonSize::ExtraLarge, 240., 0, 84., 72.),
        (IconButtonSize::Small, 80., 3, 28., 24.),
    ] {
        cx.update(|_, cx| choice.apply(cx));
        settle(cx);
        cx.update(|window, cx| {
            assert_eq!(observed.read(cx).content_width(), width);
            for index in 0..3 {
                assert_eq!(
                    window.try_find(format!("sized-button-{index}")).is_some(),
                    index < visible
                );
            }
            if visible < 3 {
                let trigger = window.find("overflow-toolbar-button").bounds();
                assert_eq!(trigger.size.width, px(trigger_width));
                assert_eq!(trigger.size.height, px(target));
                assert!(trigger.right() <= window.find("overflow-toolbar-row").bounds().right());
                window.click("overflow-toolbar-button", cx);
                window.within("popup-menu").click(0usize, cx);
            }
        });
    }
    assert_eq!(calls.get(), 2);
}

#[gpui_kit::test]
fn fractional_rem_and_display_scale_keep_fitting_actions_inline_and_overflow_contained(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let calls = Rc::new(Cell::new(0));
    let toolbar = cx.new(|_| OverflowToolbar::new(units(calls.clone())));
    let (root, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 300.,
        left: 0.,
    });
    for (scale, rem) in [
        (1., 15.076923),
        (2., 15.076923),
        (1., 18.3),
        (2., 18.3),
        (1.25, 15.076923),
        (1.5, 18.3),
    ] {
        cx.simulate_scale_factor_change(scale);
        cx.update(|window, _| window.set_rem_size(px(rem)));
        root.update(cx, |root, cx| {
            root.width = 300.;
            cx.notify();
        });
        settle(cx);
        cx.update(|window, _| assert_inline_grid(window, scale));
        root.update(cx, |root, cx| {
            root.width = 50.;
            cx.notify();
        });
        settle(cx);
        cx.update(|window, cx| {
            let row = window.find("overflow-toolbar-row").bounds();
            let trigger = window.find("overflow-toolbar-button").bounds();
            assert!(trigger.left() >= row.left() && trigger.right() <= row.right());
            window.click("overflow-toolbar-button", cx);
            window.within("popup-menu").click(0usize, cx);
        });
    }
    assert_eq!(calls.get(), 6);
}

fn assert_inline_grid(window: &Window, scale: f32) {
    assert!(
        window.try_find("overflow-toolbar-more").is_none(),
        "scale={scale}, rem={:?}",
        window.rem_size()
    );
    let row = window.find("overflow-toolbar-row").bounds();
    let mut previous: Option<Pixels> = None;
    for index in 0..3 {
        let control = window.find(format!("sized-button-{index}")).bounds();
        assert!(control.left() >= row.left() && control.right() <= row.right());
        if let Some(right) = previous {
            let physical_gap = control.left().scale(scale).round() - right.scale(scale).round();
            let expected_gap = gpui_kit::rems(0.25)
                .to_pixels(window.rem_size())
                .scale(scale)
                .round();
            assert_eq!(physical_gap, expected_gap);
        }
        previous = Some(control.right());
    }
}
