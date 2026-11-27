//! A strip toolbar too narrow for both its sides keeps each side's overflow
//! menu button in view: the leading side keeps room for its button and the
//! trailing side yields the rest of the line.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, SharedString, Styled as _, TestAppContext, TestSupportExt as _,
    VisualTestContext, Window, div, px,
};

use super::{FOCUS_RING_PX, Toolbar, text_edge};
use crate::icon_button::Metrics;
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};

/// A strip toolbar whose two sides are overflow toolbars, in a pane only
/// `width` wide; with `heading`, a heading that takes what the leading
/// overflow toolbar leaves follows it, as in Channels.
struct Crowded {
    leading: Entity<OverflowToolbar>,
    trailing: Entity<OverflowToolbar>,
    width: f32,
    heading: bool,
}

/// `count` 60 px buttons named after `side`.
fn crowd(side: &'static str, count: usize) -> Vec<OverflowUnit> {
    (0..count)
        .map(|index| {
            OverflowUnit::inline(
                format!("{side}-{index}"),
                0.,
                move |_, _| {
                    div()
                        .id(SharedString::from(format!("{side}-button-{index}")))
                        .test_support()
                        .w(px(60.))
                        .h(px(20.))
                },
                vec![PopupEntry::new(move || {
                    PopupMenuItem::new(format!("{side} {index}"))
                })],
            )
        })
        .collect()
}

impl Render for Crowded {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut toolbar = Toolbar::new("crowded-toolbar")
            .strip()
            .leading(div().min_w_0().max_w_full().child(self.leading.clone()));
        if self.heading {
            toolbar = toolbar.leading(div().flex_1().min_w_0().child("A heading that truncates"));
        }
        let toolbar = toolbar.trailing(self.trailing.clone());
        div().w(px(self.width)).h(px(200.)).child(toolbar)
    }
}

/// A crowded strip `width` wide, its trailing side wider than the strip on
/// its own.
fn crowded(
    width: f32,
    heading: bool,
    cx: &mut TestAppContext,
) -> (Entity<Crowded>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (root, cx) = cx.add_window_view(|_, cx| Crowded {
        leading: cx.new(|_| OverflowToolbar::new(crowd("leading", 3))),
        trailing: cx.new(|_| OverflowToolbar::new(crowd("trailing", 5))),
        width,
        heading,
    });
    settle(cx);
    (root, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..6 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
}

/// The width of an overflow menu's button, as a strip reserves it.
fn reserve(window: &Window, cx: &gpui_kit::App) -> Pixels {
    Metrics::current(cx)
        .overflow_width()
        .to_pixels(window.rem_size())
}

#[gpui_kit::test]
fn a_crowded_strip_keeps_each_sides_menu_button_in_view(cx: &mut TestAppContext) {
    let (_, cx) = crowded(240., false, cx);
    cx.update(|window, _| {
        let strip = window.find("crowded-toolbar").bounds();
        let leading = window.find("workspace-toolbar-leading").bounds();
        let trailing = window.find("workspace-toolbar-trailing").bounds();
        // Each side's clip reaches a focus ring's width past its controls.
        let ring = px(FOCUS_RING_PX);
        assert!(
            leading.right() - ring <= trailing.left() + ring,
            "{leading:?} then {trailing:?}"
        );
        assert!(
            trailing.right() - ring <= strip.right(),
            "{trailing:?} in {strip:?}"
        );
        let buttons = window.find_all("overflow-toolbar-button");
        assert_eq!(buttons.len(), 2, "each side has its menu button");
        for button in buttons {
            let bounds = button.bounds();
            assert!(
                bounds.left() >= strip.left() && bounds.right() <= strip.right(),
                "{bounds:?} in {strip:?}"
            );
        }
        for index in 0..3 {
            if let Some(unit) = window.try_find(format!("leading-button-{index}")) {
                let bounds = unit.bounds();
                assert!(
                    bounds.left() >= leading.left() && bounds.right() <= leading.right(),
                    "leading {index} at {bounds:?} in {leading:?}"
                );
            }
        }
    });
}

#[gpui_kit::test]
fn a_strip_narrower_than_two_menu_buttons_keeps_each_sides_reserve(cx: &mut TestAppContext) {
    let (root, cx) = crowded(240., false, cx);
    // Room for one menu button and half of another, inside the text edges.
    cx.update(|window, cx| {
        let edge = text_edge(&crate::Tokens::current(cx));
        let width = edge + edge + reserve(window, cx) * 1.5;
        root.update(cx, |root, cx| {
            root.width = f32::from(width);
            cx.notify();
        });
    });
    settle(cx);
    cx.update(|window, cx| {
        let reserve = reserve(window, cx);
        let ring = px(FOCUS_RING_PX);
        let leading = window.find("workspace-toolbar-leading").bounds();
        let trailing = window.find("workspace-toolbar-trailing").bounds();
        for (side, bounds) in [("leading", leading), ("trailing", trailing)] {
            assert!(
                bounds.size.width >= reserve + ring + ring,
                "the {side} side keeps its menu button's room: {bounds:?}"
            );
        }
        assert!(
            leading.right() - ring <= trailing.left() + ring,
            "the sides do not overlap: {leading:?} then {trailing:?}"
        );
        let strip = window.find("crowded-toolbar").bounds();
        let button = window
            .within("workspace-toolbar-leading")
            .find("overflow-toolbar-button")
            .bounds();
        assert!(
            button.left() >= strip.left() && button.right() <= strip.right(),
            "the leading menu button {button:?} in {strip:?}"
        );
    });
}

#[gpui_kit::test]
fn a_collapsed_heading_and_its_gap_leave_the_leading_menu_button_its_room(cx: &mut TestAppContext) {
    // Narrow enough that the leading side keeps only its reserve.
    let (_, cx) = crowded(200., true, cx);
    cx.update(|window, _| {
        let leading = window.within("workspace-toolbar-leading");
        let row = leading.find("overflow-toolbar-row").bounds();
        let button = leading.find("overflow-toolbar-button").bounds();
        assert!(
            button.left() >= row.left() && button.right() <= row.right(),
            "the menu button {button:?} inside its own toolbar's row {row:?}"
        );
    });
}

#[gpui_kit::test]
fn a_strip_with_room_for_one_menu_button_clips_the_trailing_side_at_its_edge(
    cx: &mut TestAppContext,
) {
    let (root, cx) = crowded(240., false, cx);
    // Room for the leading text edge and menu button alone, so the trailing
    // side starts past the strip.
    cx.update(|window, cx| {
        let edge = text_edge(&crate::Tokens::current(cx));
        let width = edge + reserve(window, cx);
        root.update(cx, |root, cx| {
            root.width = f32::from(width);
            cx.notify();
        });
    });
    settle(cx);
    cx.update(|window, _| {
        let strip = window.find("crowded-toolbar").bounds();
        let leading = window
            .within("workspace-toolbar-leading")
            .find("overflow-toolbar-button");
        assert!(leading.visible(), "the leading menu button shows");
        let trailing = window
            .within("workspace-toolbar-trailing")
            .find("overflow-toolbar-button");
        assert!(
            trailing.bounds().left() >= strip.right(),
            "the trailing menu button {:?} lies past the strip {strip:?}",
            trailing.bounds()
        );
        assert!(
            !trailing.visible(),
            "the strip clips the trailing side at its edge"
        );
    });
}
