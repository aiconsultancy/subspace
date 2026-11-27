use super::{OverflowAnchor, OverflowToolbar, OverflowUnit, PopupEntry};
use gpui_kit::component::{button::Button, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use std::{cell::RefCell, rc::Rc};

mod dynamic;
mod freshness;
mod interaction;
mod release;
mod sizing;

struct Harness {
    toolbar: Entity<OverflowToolbar>,
    width: f32,
    left: f32,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(
            div()
                .ml(px(self.left))
                .w(px(self.width))
                .flex()
                .child(self.toolbar.clone()),
        )
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn units(anchors: Rc<RefCell<Vec<OverflowAnchor>>>) -> Vec<OverflowUnit> {
    (0..3)
        .map(|index| {
            let anchors = anchors.clone();
            OverflowUnit::inline(
                format!("unit-{index}"),
                30.,
                move |_, _| Button::new(format!("inline-{index}")).w(px(30.)).label("X"),
                vec![PopupEntry::anchored(move |anchor| {
                    anchors.borrow_mut().push(anchor);
                    PopupMenuItem::new(format!("Action {index}"))
                })],
            )
        })
        .collect()
}

struct AnchorProbe {
    root: Entity<Harness>,
    anchors: Rc<RefCell<Vec<OverflowAnchor>>>,
}

fn anchor_window(cx: &mut TestAppContext) -> (AnchorProbe, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let anchors = Rc::new(RefCell::new(Vec::new()));
    let toolbar = cx.new(|_| OverflowToolbar::new(units(anchors.clone())));
    let (root, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 70.,
        left: 0.,
    });
    settle(cx);
    (AnchorProbe { root, anchors }, cx)
}

#[gpui_kit::test]
fn flex_viewport_shrinks_and_growth_restores_inline_controls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let toolbar = cx.new(|_| OverflowToolbar::new(units(Rc::default())));
    let observed = toolbar.clone();
    let (root, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 70.,
        left: 0.,
    });
    settle(cx);
    let row = cx.update(|window, _| window.find("overflow-toolbar-row").bounds());
    assert!(
        row.size.width <= px(70.),
        "viewport must shrink to allocation: {row:?}"
    );
    cx.update(|window, _| {
        assert!(window.try_find("overflow-toolbar-more").is_some());
        assert!(window.try_find("overflow-unit:unit-1").is_none());
        assert!(window.try_find("inline-1").is_none());
    });
    cx.update(|_, cx| assert_eq!(observed.read(cx).content_width(), 98.));
    root.update(cx, |root, cx| {
        root.width = 150.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("overflow-toolbar-more").is_none());
        assert!(window.try_find("overflow-unit:unit-2").is_some());
        assert_eq!(
            window.find("overflow-toolbar-row").bounds().size.width,
            px(98.)
        );
    });
    root.update(cx, |root, cx| {
        root.width = 70.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.find("overflow-toolbar-more").visible());
        assert!(window.try_find("overflow-unit:unit-2").is_none());
    });
}

#[gpui_kit::test]
fn popup_uses_current_trigger_bounds_after_row_moves(cx: &mut TestAppContext) {
    let (AnchorProbe { root, anchors }, cx) = anchor_window(cx);
    root.update(cx, |root, cx| {
        root.left = 90.;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let trigger = cx.update(|window, _| window.find("overflow-toolbar-more").bounds());
    cx.simulate_click(trigger.center(), Default::default());
    settle(cx);
    let anchors = anchors.borrow();
    assert_eq!(
        anchors.len(),
        2,
        "only the hidden suffix builds popup entries"
    );
    assert_eq!(anchors[0].bounds(), trigger);
    assert_eq!(anchors[0].bounds(), trigger);
}

#[gpui_kit::test]
fn open_popup_anchor_follows_a_moved_trigger(cx: &mut TestAppContext) {
    let (AnchorProbe { root, anchors }, cx) = anchor_window(cx);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    assert_eq!(anchors.borrow().len(), 2);
    root.update(cx, |root, cx| {
        root.left = 90.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, _| {
        let trigger = window.find("overflow-toolbar-more").bounds();
        assert!(trigger.left() >= px(90.));
        assert_eq!(anchors.borrow().last().unwrap().bounds(), trigger);
    });
}

#[gpui_kit::test]
fn measuring_a_new_viewport_requests_a_follow_up_native_frame(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let toolbar = cx.new(|_| OverflowToolbar::new(units(Rc::default())));
    let (root, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 70.,
        left: 0.,
    });
    for _ in 0..4 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    root.update(cx, |root, cx| {
        root.width = 80.;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let requested = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert!(
        requested > 0,
        "viewport measurement must wake the native frame source without another input event"
    );
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let unchanged = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(
        unchanged, 0,
        "an unchanged viewport must not keep scheduling frames"
    );
}

#[gpui_kit::test]
fn the_clip_leaves_a_focus_rings_room_round_the_row(cx: &mut TestAppContext) {
    let (_, cx) = anchor_window(cx);
    cx.update(|window, _| {
        let row = window.find("overflow-toolbar-row").bounds();
        let clip = window.find("overflow-toolbar-clip").bounds();
        let ring = px(crate::workspace::FOCUS_RING_PX);
        assert_eq!(clip.left(), row.left() - ring, "{clip:?} round {row:?}");
        assert_eq!(clip.right(), row.right() + ring);
        assert_eq!(clip.top(), row.top() - ring);
        assert_eq!(clip.bottom(), row.bottom() + ring);
    });
}

#[gpui_kit::test]
fn an_open_overflow_menu_closes_when_everything_fits_again(cx: &mut TestAppContext) {
    let (probe, cx) = anchor_window(cx);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("popup-menu").is_some(), "the menu opened"));
    probe.root.update(cx, |root, cx| {
        root.width = 150.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("overflow-toolbar-more").is_none());
        assert!(
            window.try_find("popup-menu").is_none(),
            "no menu left open unseen"
        );
        let toolbar = probe.root.read(cx).toolbar.clone();
        let anchor = toolbar.read(cx).anchor.clone();
        assert!(anchor.is_some_and(|anchor| !anchor.read(cx).is_open()));
    });
}

#[gpui_kit::test]
fn a_row_given_no_width_moves_every_unit_to_the_menu(cx: &mut TestAppContext) {
    let (probe, cx) = anchor_window(cx);
    probe.root.update(cx, |root, cx| {
        root.width = 0.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, _| {
        for index in 0..3 {
            assert!(
                window
                    .try_find(format!("overflow-unit:unit-{index}"))
                    .is_none(),
                "unit {index} is not drawn where it cannot be seen"
            );
        }
        assert!(window.try_find("overflow-toolbar-more").is_some());
    });
}

#[gpui_kit::test]
fn an_icon_button_runs_from_its_button_and_from_the_overflow_menu(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let runs = Rc::new(std::cell::Cell::new(0));
    let counted = runs.clone();
    let mut units = units(Rc::default());
    units.push(OverflowUnit::icon_button(
        "test-refresh",
        subspace_lucide::IconName::RefreshCw,
        "Refresh",
        move |_, _| counted.set(counted.get() + 1),
    ));
    let toolbar = cx.new(|_| OverflowToolbar::new(units));
    let (root, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 400.,
        left: 0.,
    });
    settle(cx);
    cx.update(|window, cx| window.click("test-refresh", cx));
    assert_eq!(runs.get(), 1, "the button runs it");
    root.update(cx, |root, cx| {
        root.width = 70.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("test-refresh").is_none(), "in the menu");
        window.click("overflow-toolbar-button", cx);
        let menu = window.within("popup-menu");
        let index = (0usize..8)
            .find(|index| {
                menu.try_find(*index)
                    .is_some_and(|item| item.label().is_some_and(|label| label == "Refresh"))
            })
            .expect("Refresh is in the menu");
        window.within("popup-menu").click(index, cx);
    });
    assert_eq!(runs.get(), 2, "the menu entry runs it");
}
