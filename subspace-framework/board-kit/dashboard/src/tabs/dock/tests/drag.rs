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
        let accent = crate::Tokens::current(cx).accent;
        assert!(
            crate::painted_border::painted_borders(window, accent)
                .iter()
                .any(|(_, edges)| edges.left == gpui_kit::px(2.).scale(window.scale_factor()))
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
            let insertion = crate::painted_border::painted_borders(window, tokens.accent)
                .iter()
                .any(|(_, edges)| edges.left == gpui_kit::px(2.).scale(window.scale_factor()));
            let trailing_fill = window
                .painted_quads()
                .iter()
                .any(|quad| quad.background == gpui_kit::Background::from(tokens.select));
            assert!(if trailing { trailing_fill } else { insertion });
            window.pointer_up(target, gpui_kit::MouseButton::Left, cx);
        });
        settle(cx);
    }
    assert_eq!(drops.get(), 2);
}

#[gpui_kit::test]
fn a_tab_dropped_near_the_far_end_of_the_strip_moves_last(cx: &mut TestAppContext) {
    let (_area, _, cx) = mount(cx, false);
    cx.update(|window, cx| {
        // The free width is measured from the frame before.
        window.render_frame(cx);
        window.render_frame(cx);
        let bar = window.within("dock-resource-tabs");
        let start = bar.find(0usize).bounds().center();
        let last = bar.find(2usize).bounds();
        let target = bar.find("dock-trailing-drop").bounds();
        assert!(
            target.size.width > gpui_kit::rems(4.).to_pixels(window.rem_size()),
            "the drop target covers more than its minimum width"
        );
        assert!(target.left() <= last.right() + gpui_kit::px(1.));
        let far = gpui_kit::point(target.right() - gpui_kit::px(4.), target.center().y);
        window.pointer_down(start, gpui_kit::MouseButton::Left, cx);
        window.pointer_move(
            start + gpui_kit::point(gpui_kit::px(24.), gpui_kit::px(0.)),
            Some(gpui_kit::MouseButton::Left),
            cx,
        );
        window.pointer_move(far, Some(gpui_kit::MouseButton::Left), cx);
        window.pointer_up(far, gpui_kit::MouseButton::Left, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        let bar = window.within("dock-resource-tabs");
        assert_eq!(bar.find(2usize).label(), Some("General"));
    });
}

#[gpui_kit::test]
fn after_a_resize_the_drop_target_ends_at_the_new_end_of_the_strip(cx: &mut TestAppContext) {
    let (_area, _, cx) = mount(cx, false);
    let before = cx.update(|window, cx| {
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        // Settled: the strip asks for no further frame.
        assert_eq!(window.simulate_next_frame(cx), 0);
        window
            .within("dock-resource-tabs")
            .find("dock-trailing-drop")
            .bounds()
    });
    let size = cx.update(|window, _| window.viewport_size());
    cx.simulate_resize(gpui_kit::size(size.width - gpui_kit::px(600.), size.height));
    cx.run_until_parked();
    cx.update(|window, cx| {
        // The frame the resize lays out measures the moved end and asks
        // for the next frame itself; that frame draws the new width.
        window.render_frame(cx);
        assert_eq!(
            window.simulate_next_frame(cx),
            1,
            "the measurement asks for a frame"
        );
        window.render_frame(cx);
        let bar = window.within("dock-resource-tabs");
        let target = bar.find("dock-trailing-drop").bounds();
        assert!(target.size.width <= before.size.width - gpui_kit::px(500.));
        let start = bar.find(0usize).bounds().center();
        let far = gpui_kit::point(target.right() - gpui_kit::px(4.), target.center().y);
        window.pointer_down(start, gpui_kit::MouseButton::Left, cx);
        window.pointer_move(
            start + gpui_kit::point(gpui_kit::px(24.), gpui_kit::px(0.)),
            Some(gpui_kit::MouseButton::Left),
            cx,
        );
        window.pointer_move(far, Some(gpui_kit::MouseButton::Left), cx);
        window.pointer_up(far, gpui_kit::MouseButton::Left, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        let bar = window.within("dock-resource-tabs");
        assert_eq!(bar.find(2usize).label(), Some("General"));
    });
}
