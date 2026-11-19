//! Production shell geometry for the global toolbar's independent size preference.

use gpui_kit::component::{Icon, Sizable as _, button::Button, h_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, AvailableSpace, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, TestAppContext, TestSupportExt as _, VisualTestContext, Window,
    div, px, size,
};

use super::{open_fixture, settle};

struct NativeSmallBaseline;

impl Render for NativeSmallBaseline {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .child(
                Button::new("native-small-baseline")
                    .small()
                    .icon(subspace_lucide::IconName::CircleQuestionMark),
            )
            .child(
                div()
                    .id("native-small-glyph")
                    .test_support()
                    .child(Icon::new(subspace_lucide::IconName::CircleQuestionMark).small()),
            )
    }
}

#[gpui_kit::test]
fn rooted_small_preserves_the_native_relative_baseline(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = super::status::rooted_fixture(cx, root.path(), 1400., false);
    let (control, glyph, gap, rem) = cx
        .update_window(window, |_, window, _| {
            let control = window.find("toggle-chat").bounds();
            let next = window.find("open-full-text-search").bounds();
            let glyph = window.within("toggle-chat").find("toolbar-icon").bounds();
            (
                control,
                glyph,
                next.left() - control.right(),
                window.rem_size(),
            )
        })
        .unwrap();
    let baseline = cx.new(|_| NativeSmallBaseline);
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.draw(Default::default(), size(px(100.), px(100.)), |_, _| {
        baseline.into_element()
    });
    visual.update(|window, _| {
        let native_glyph = window.find("native-small-glyph").bounds().size;
        let native_button = window.find("native-small-baseline").bounds().size;
        assert_eq!(window.rem_size(), rem);
        assert_eq!(glyph.size, native_glyph);
        assert_eq!(control.size, native_button);
        let relative_gap = gpui_kit::rems(0.25).to_pixels(window.rem_size());
        assert!((gap - relative_gap).abs() <= px(1. / window.scale_factor()));
    });
}

#[gpui_kit::test]
fn global_toolbar_sizes_scale_real_controls_in_both_tab_modes(cx: &mut TestAppContext) {
    for mode in ["horizontal", "vertical"] {
        for (saved, glyph, hit, gap, row) in [
            ("medium", 20., 36., 6., 45.),
            ("small", 14., 24., 4., 38.),
            ("large", 28., 48., 8., 60.),
            ("extra-large", 40., 72., 12., 90.),
        ] {
            let root = tempfile::tempdir().unwrap();
            let handle = open_fixture(cx, root.path(), mode, 5000., false);
            cx.update(|cx| {
                subspace_dashboard::icon_button::IconButtonSize::from_key(Some(saved)).apply(cx);
            });
            handle.update(cx, |_, _, cx| cx.notify()).unwrap();
            settle(&handle, cx);
            cx.update_window(*handle, |_, window, _| {
                let toolbar = window.find("app-toolbar").bounds();
                let first_row = window.find("app-toolbar-first-row").bounds();
                assert_eq!(first_row.size.height, px(row), "{mode}/{saved}");
                let second_row = window.try_find("board-toolbar-row").map_or(px(0.), |row| {
                    row.bounds().size.height + px(super::super::BAR_GAP)
                });
                assert_eq!(
                    toolbar.size.height,
                    first_row.size.height + second_row + px(1.)
                );
                let mut previous = None;
                for id in [
                    "toggle-chat",
                    "open-full-text-search",
                    "open-palette",
                    "reload",
                    "wrap",
                    "jig",
                    "settings",
                    "help",
                    "about",
                ] {
                    let mut globals = window.within("global-toolbar");
                    let control = globals.find(id).bounds();
                    assert_eq!(control.size, size(px(hit), px(hit)), "{mode}/{saved}/{id}");
                    let icon = globals.within(id).find("toolbar-icon").bounds();
                    assert_eq!(icon.size, size(px(glyph), px(glyph)), "{mode}/{saved}/{id}");
                    assert_eq!(icon.center(), control.center());
                    assert_eq!(control.center().y, first_row.center().y);
                    if let Some(right) = previous {
                        assert_eq!(control.left() - right, px(gap));
                    }
                    previous = Some(control.right());
                }
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn current_small_toolbar_geometry_is_measured_before_scaling(cx: &mut TestAppContext) {
    for mode in ["horizontal", "vertical"] {
        let root = tempfile::tempdir().unwrap();
        let handle = open_fixture(cx, root.path(), mode, 5000., false);
        cx.update(|cx| subspace_dashboard::icon_button::IconButtonSize::Small.apply(cx));
        settle(&handle, cx);
        cx.update_window(*handle, |_, window, _| {
            let help = window.within("global-toolbar").find("help").bounds();
            let settings = window.within("global-toolbar").find("settings").bounds();
            let row = window.find("app-toolbar").bounds();
            let gap = help.left() - settings.right();
            assert_eq!(help.size, size(px(24.), px(24.)));
            assert_eq!(gap, px(4.));
            assert_eq!(row.size.height, px(39.));
        })
        .unwrap();
        let mut visual = VisualTestContext::from_window(*handle, cx);
        visual.draw(
            Default::default(),
            size(px(100.), px(100.)),
            |window, cx| {
                // Button::small forwards Size::Small to its native Icon. Measure that
                // real component, not a second arithmetic copy of its size mapping.
                let mut glyph = Icon::new(subspace_lucide::IconName::CircleQuestionMark)
                    .small()
                    .into_any_element();
                let glyph = glyph.layout_as_root(
                    size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                    window,
                    cx,
                );
                assert_eq!(glyph, size(px(14.), px(14.)));
                div().into_element()
            },
        );
    }
}

#[gpui_kit::test]
fn extra_large_global_actions_overflow_without_clipping(cx: &mut TestAppContext) {
    for mode in ["horizontal", "vertical"] {
        let root = tempfile::tempdir().unwrap();
        let handle = open_fixture(cx, root.path(), mode, 260., false);
        cx.update(|cx| subspace_dashboard::icon_button::IconButtonSize::ExtraLarge.apply(cx));
        handle.update(cx, |_, _, cx| cx.notify()).unwrap();
        settle(&handle, cx);
        cx.update_window(*handle, |_, window, cx| {
            let globals = window.within("global-toolbar");
            let region = globals.find("overflow-toolbar-row").bounds();
            let trigger = globals.find("overflow-toolbar-button").bounds();
            assert!(trigger.left() >= region.left() && trigger.right() <= region.right());
            assert!(trigger.bottom() <= region.bottom());
            assert_eq!(trigger.size.height, px(72.));
            window
                .within("global-toolbar")
                .click("overflow-toolbar-button", cx);
            assert!(window.try_find("popup-menu").is_some());
        })
        .unwrap();
    }
}
