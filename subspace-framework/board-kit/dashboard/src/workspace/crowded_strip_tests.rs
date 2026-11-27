//! A strip toolbar too narrow for both its sides keeps each side's overflow
//! menu button in view: the leading side keeps room for its button and the
//! trailing side yields the rest of the line.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, TestAppContext, TestSupportExt as _, Window, div, px,
};

use super::{FOCUS_RING_PX, Toolbar};
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};

/// A strip toolbar whose two sides are overflow toolbars, in a pane only
/// `width` wide.
struct Crowded {
    leading: Entity<OverflowToolbar>,
    trailing: Entity<OverflowToolbar>,
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
        let toolbar = Toolbar::new("crowded-toolbar")
            .strip()
            .leading(div().min_w_0().max_w_full().child(self.leading.clone()))
            .trailing(self.trailing.clone());
        div().w(px(240.)).h(px(200.)).child(toolbar)
    }
}

#[gpui_kit::test]
fn a_crowded_strip_keeps_each_sides_menu_button_in_view(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, cx| Crowded {
        leading: cx.new(|_| OverflowToolbar::new(crowd("leading", 3))),
        // Wider than the strip on its own.
        trailing: cx.new(|_| OverflowToolbar::new(crowd("trailing", 5))),
    });
    for _ in 0..6 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
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
