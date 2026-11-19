use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, TestAppContext, TouchDragEvent, TouchPhase, Window, div, point, px,
};

use super::{NumericSliderEvent, NumericSliderSpec, NumericSliderState, control};
use crate::Tokens;

struct Harness {
    state: Entity<NumericSliderState>,
    _subscription: Subscription,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w_64().child(control(
            "volume",
            "Volume",
            &self.state,
            &Tokens::one_dark(),
        ))
    }
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    Entity<Harness>,
    Rc<RefCell<Vec<f64>>>,
    &mut gpui_kit::VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let received = events.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let spec = NumericSliderSpec::new(5., 29., 10.).unwrap();
        let state = cx.new(|cx| NumericSliderState::new(spec, 5., window, cx));
        let subscription = cx.subscribe(&state, move |_, _, event, _| {
            let NumericSliderEvent::Commit(value) = event;
            received.borrow_mut().push(*value);
        });
        Harness {
            state,
            _subscription: subscription,
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, events, cx)
}

#[gpui_kit::test]
fn keyboard_pointer_and_external_sync_use_domain_values(cx: &mut TestAppContext) {
    let (view, events, cx) = mount(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find("volume").label(), Some("Volume"));
        window.focus_next(cx);
        window.press("right", cx);
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[15.]);
    cx.update(|window, cx| {
        let bounds = window.find("volume").bounds();
        window.click_at(
            "volume",
            point(bounds.size.width - px(1.), bounds.size.height / 2.),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[15., 29.]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.state
                .update(cx, |state, cx| state.set_value(25., window, cx))
        })
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[15., 29.]);
    assert_eq!(
        view.read_with(cx, |view, cx| view.state.read(cx).value()),
        25.
    );
}

#[gpui_kit::test]
fn disabled_control_rejects_pointer_and_keyboard(cx: &mut TestAppContext) {
    let (view, events, cx) = mount(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.state
                .update(cx, |state, cx| state.set_disabled(true, window, cx))
        })
    });
    cx.update(|window, cx| {
        window.click("volume", cx);
        window.press("right", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, cx| view.state.read(cx).value()),
        5.
    );
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn native_touch_release_commits_mapped_endpoint(cx: &mut TestAppContext) {
    let (_view, events, cx) = mount(cx);
    let (start, end) = cx.update(|window, _| {
        let bounds = window.find("volume").bounds();
        (
            point(bounds.left() + px(2.), bounds.center().y),
            point(bounds.right() - px(2.), bounds.center().y),
        )
    });
    cx.simulate_mouse_move(start, None, gpui_kit::Modifiers::default());
    let touch_start = TouchDragEvent {
        phase: TouchPhase::Started,
        start_position: start,
        position: start,
    };
    cx.simulate_event(touch_start.clone());
    cx.simulate_event(TouchDragEvent {
        phase: TouchPhase::Moved,
        position: end,
        ..touch_start.clone()
    });
    cx.simulate_event(TouchDragEvent {
        phase: TouchPhase::Ended,
        position: end,
        ..touch_start
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[29.]);
}

#[test]
fn rejects_unrepresentable_or_invalid_tick_ranges() {
    assert!(NumericSliderSpec::new(0., 1., 0.).is_err());
    assert!(NumericSliderSpec::new(0., f64::INFINITY, 1.).is_err());
    assert!(NumericSliderSpec::new(0., 16_777_217., 1.).is_err());
}

#[test]
fn domain_values_use_the_nearest_tick_and_exact_final_endpoint() {
    let spec = NumericSliderSpec::new(5., 29., 10.).unwrap();
    assert_eq!(
        (spec.value(0), spec.value(1), spec.value(2), spec.value(3)),
        (5., 15., 25., 29.)
    );
    assert_eq!(spec.tick(27.5), 3);
    assert_eq!(spec.tick(24.), 2);
}
