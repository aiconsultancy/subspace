use super::{AnimatedCollapsible, DISCLOSURE_DURATION, Disclosure};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, TestAppContext, VisualTestContext, div, px, rems,
};
use std::{cell::Cell, rc::Rc, time::Duration};

struct Harness {
    disclosure: Disclosure,
    height: f32,
    shown: bool,
    width: f32,
    wrap: bool,
    remeasures: Rc<Cell<usize>>,
}
impl Render for Harness {
    fn render(&mut self, _: &mut gpui_kit::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(rems(self.width))
            .children(self.shown.then(|| {
                AnimatedCollapsible::new(
                    "fixture-message:1:SUB-2",
                    &self.disclosure,
                    div()
                        .id("summary-content")
                        .debug_selector(|| "summary-content".into())
                        .when(!self.wrap, |content| content.h(rems(self.height)))
                        .when(self.wrap, |content| {
                            content.child(
                                "A summary that wraps as the window changes width. ".repeat(8),
                            )
                        }),
                )
                .on_resize({
                    let remeasures = self.remeasures.clone();
                    Rc::new(move |_| remeasures.set(remeasures.get() + 1))
                })
            }))
            .child(div().debug_selector(|| "following-content".into()).h_4())
    }
}
fn mount(cx: &mut TestAppContext) -> (Entity<Harness>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.add_window_view(|_, _| Harness {
        disclosure: Disclosure::default(),
        height: 4.,
        shown: true,
        width: 16.,
        wrap: false,
        remeasures: Rc::default(),
    })
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

#[gpui_kit::test]
fn animation_remeasures_at_most_once_per_frame_with_time_between_callback_and_render(
    cx: &mut TestAppContext,
) {
    const FRAME_TIME: Duration = Duration::from_millis(8);
    const RENDER_DELAY: Duration = Duration::from_millis(1);
    let (view, cx) = mount(cx);
    let remeasures = view.read_with(cx, |view, _| view.remeasures.clone());
    for expanded in [true, false] {
        open(&view, cx, expanded);
        let mut total = 0;
        for frame in 0..=(DISCLOSURE_DURATION.as_millis() / FRAME_TIME.as_millis()) {
            remeasures.set(0);
            cx.executor().advance_clock(FRAME_TIME - RENDER_DELAY);
            cx.update(|window, cx| window.simulate_next_frame(cx));
            // A real clock keeps moving between the callback and layout.
            cx.executor().advance_clock(RENDER_DELAY);
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
            let count = remeasures.get();
            assert!(
                count <= 1,
                "{count} remeasures in frame {frame} (open: {expanded})"
            );
            total += count;
        }
        assert!(
            total > 1,
            "the next-frame path must report intermediate heights"
        );
    }
}
fn advance(cx: &mut VisualTestContext, elapsed: Duration) -> Pixels {
    cx.executor().advance_clock(elapsed);
    draw(cx);
    cx.debug_bounds("following-content").unwrap().origin.y
}
fn open(view: &Entity<Harness>, cx: &mut VisualTestContext, open: bool) {
    view.update(cx, |view, cx| {
        view.disclosure.set_open(open, cx);
        cx.notify();
    });
    draw(cx);
    draw(cx);
}

#[gpui_kit::test]
fn disclosure_has_intermediate_height_and_finishes_at_the_measured_height(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    open(&view, cx, true);
    let halfway = advance(cx, DISCLOSURE_DURATION / 2);
    let full = advance(cx, DISCLOSURE_DURATION);
    assert!(halfway > px(0.) && halfway < full);
    assert_eq!(
        full,
        cx.debug_bounds("summary-content").unwrap().size.height
    );
    open(&view, cx, false);
    assert_eq!(advance(cx, Duration::ZERO), full);
    let closing = advance(cx, DISCLOSURE_DURATION / 2);
    assert!(closing > px(0.) && closing < full);
    assert_eq!(advance(cx, DISCLOSURE_DURATION), px(0.));
    assert!(cx.debug_bounds("summary-content").is_none());
}

#[gpui_kit::test]
fn reduced_motion_changes_instantly(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    open(&view, cx, true);
    assert_eq!(
        advance(cx, Duration::ZERO),
        cx.debug_bounds("summary-content").unwrap().size.height
    );
    open(&view, cx, false);
    assert!(cx.debug_bounds("summary-content").is_none());
}

#[gpui_kit::test]
fn an_interrupted_transition_reverses_from_its_current_height(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    open(&view, cx, true);
    let middle = advance(cx, DISCLOSURE_DURATION / 2);
    open(&view, cx, false);
    assert_eq!(advance(cx, Duration::ZERO), middle);
    let closing = advance(cx, DISCLOSURE_DURATION / 2);
    assert!(closing > px(0.) && closing < middle);
    open(&view, cx, true);
    assert_eq!(advance(cx, Duration::ZERO), closing);
}

#[gpui_kit::test]
fn settled_open_content_snaps_to_new_measurements(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    open(&view, cx, true);
    advance(cx, DISCLOSURE_DURATION);
    for height in [8., 2.] {
        view.update(cx, |view, cx| {
            view.height = height;
            cx.notify();
        });
        draw(cx);
        let full = advance(cx, Duration::ZERO);
        assert_eq!(
            full,
            cx.debug_bounds("summary-content").unwrap().size.height
        );
        assert_eq!(advance(cx, DISCLOSURE_DURATION / 2), full);
    }
}

fn show(view: &Entity<Harness>, cx: &mut VisualTestContext, shown: bool) {
    view.update(cx, |view, cx| {
        view.shown = shown;
        cx.notify();
    });
    draw(cx);
}

#[gpui_kit::test]
fn settled_open_and_closed_do_not_replay_after_frames_without_the_element(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    for expanded in [true, false] {
        open(&view, cx, expanded);
        let settled = advance(cx, DISCLOSURE_DURATION);
        for elapsed in [Duration::ZERO, DISCLOSURE_DURATION * 2] {
            show(&view, cx, false);
            advance(cx, elapsed);
            show(&view, cx, true);
            assert_eq!(advance(cx, Duration::ZERO), settled);
            assert_eq!(advance(cx, DISCLOSURE_DURATION / 2), settled);
        }
    }
}

#[gpui_kit::test]
fn redraw_resumes_at_elapsed_fraction_and_settles_even_while_absent(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    open(&view, cx, true);
    let quarter = advance(cx, DISCLOSURE_DURATION / 4);
    show(&view, cx, false);
    advance(cx, DISCLOSURE_DURATION / 4);
    show(&view, cx, true);
    let middle = advance(cx, Duration::ZERO);
    let full = advance(cx, DISCLOSURE_DURATION / 2);
    assert!(quarter < middle);
    assert!((middle - full * 0.5).abs() < px(0.01));
    for expanded in [false, true] {
        open(&view, cx, expanded);
        advance(cx, DISCLOSURE_DURATION / 4);
        show(&view, cx, false);
        advance(cx, DISCLOSURE_DURATION);
        show(&view, cx, true);
        assert_eq!(
            advance(cx, Duration::ZERO),
            if expanded { full } else { px(0.) }
        );
        assert_eq!(
            advance(cx, DISCLOSURE_DURATION / 2),
            if expanded { full } else { px(0.) }
        );
    }
}

#[gpui_kit::test]
fn live_width_changes_do_not_start_new_transitions(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    view.update(cx, |view, _| view.wrap = true);
    open(&view, cx, true);
    let original = advance(cx, DISCLOSURE_DURATION);
    for width in [12., 20., 10.] {
        view.update(cx, |view, cx| {
            view.width = width;
            cx.notify();
        });
        draw(cx);
        let resized = cx.debug_bounds("following-content").unwrap().origin.y;
        assert_eq!(
            resized,
            cx.debug_bounds("summary-content").unwrap().size.height
        );
        assert_ne!(resized, original);
        assert_eq!(advance(cx, DISCLOSURE_DURATION / 4), resized);
    }
}
