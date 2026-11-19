use super::fixture::{mount, settle};
use gpui_kit::TestAppContext;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};

#[gpui_kit::test]
fn dragging_draws_a_title_preview_and_accepts_the_trailing_slot(cx: &mut TestAppContext) {
    let (_area, _, cx) = mount(cx, false);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bar = window.within("dock-resource-tabs");
        let start = bar.find(0usize).bounds().center();
        let insertion = bar.find(1usize).bounds().center();
        let target = bar.find("dock-trailing-drop").bounds();
        assert!(target.size.width >= gpui_kit::rems(4.).to_pixels(window.rem_size()));
        window.pointer_down(start, gpui_kit::MouseButton::Left, cx);
        window.pointer_move(
            start + gpui_kit::point(gpui_kit::px(24.), gpui_kit::px(0.)),
            Some(gpui_kit::MouseButton::Left),
            cx,
        );
        let preview = window.find("dock-tab-preview");
        assert!(preview.visible());
        assert_eq!(
            preview.bounds().size,
            gpui_kit::size(gpui_kit::px(96.), gpui_kit::px(30.))
        );
        window.pointer_move(insertion, Some(gpui_kit::MouseButton::Left), cx);
        assert!(
            window
                .painted_quads()
                .iter()
                .any(|quad| quad.border_widths.left
                    == gpui_kit::px(2.).scale(window.scale_factor())
                    && quad.border_color == crate::Tokens::current(cx).accent)
        );
        window.pointer_move(target.center(), Some(gpui_kit::MouseButton::Left), cx);
        assert!(
            window.painted_quads().iter().any(|quad| quad.background
                == gpui_kit::Background::from(crate::Tokens::current(cx).select))
        );
        window.pointer_up(target.center(), gpui_kit::MouseButton::Left, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        let bar = window.within("dock-resource-tabs");
        assert_eq!(bar.find(0usize).label(), Some("Operations"));
        assert_eq!(bar.find(2usize).label(), Some("General"));
    });
}

#[gpui_kit::test]
fn host_items_get_insertion_and_trailing_feedback_and_reach_kit(cx: &mut TestAppContext) {
    let (area, _, cx) = mount(cx, false);
    let drops = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = drops.clone();
    let _events = cx.update(|_, cx| {
        cx.subscribe(&area, move |_, event, _| {
            if matches!(event, gpui_kit::component::dock::DockEvent::DragDrop { .. }) {
                observed.set(observed.get() + 1);
            }
        })
    });
    for trailing in [false, true] {
        cx.update(|window, cx| {
            let start = window.find("external-drag").bounds().center();
            let bar = window.within("dock-resource-tabs");
            let target = if trailing {
                bar.find("dock-trailing-drop").bounds().center()
            } else {
                bar.find(1usize).bounds().center()
            };
            window.pointer_down(start, gpui_kit::MouseButton::Left, cx);
            window.pointer_move(
                start + gpui_kit::point(gpui_kit::px(24.), gpui_kit::px(0.)),
                Some(gpui_kit::MouseButton::Left),
                cx,
            );
            window.pointer_move(target, Some(gpui_kit::MouseButton::Left), cx);
            let tokens = crate::Tokens::current(cx);
            assert!(window.painted_quads().iter().any(|quad| if trailing {
                quad.background == gpui_kit::Background::from(tokens.select)
            } else {
                quad.border_widths.left == gpui_kit::px(2.).scale(window.scale_factor())
                    && quad.border_color == tokens.accent
            }));
            window.pointer_up(target, gpui_kit::MouseButton::Left, cx);
        });
        settle(cx);
    }
    assert_eq!(drops.get(), 2);
}
