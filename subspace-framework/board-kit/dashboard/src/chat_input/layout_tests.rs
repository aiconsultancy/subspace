//! The editor's measured viewport must finish before the toolbar starts.

use super::ChatInput;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Role, ScrollDelta, TestAppContext, VisualTestContext, point, px, size};

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
}

fn draft(input: &Entity<ChatInput>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.activate_window();
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.replace_range(0..input.text(cx).len(), text, window, cx);
        });
    });
    draw(cx);
}

fn viewport_height(input: &Entity<ChatInput>, cx: &mut VisualTestContext) -> gpui_kit::Pixels {
    input.read_with(cx, |input, cx| {
        input.state.read(cx).input_bounds().size.height
    })
}

#[gpui_kit::test]
fn a_long_draft_never_lays_out_under_the_toolbar(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text(&["A fixture line"; 10].join("\n"), window, cx);
            input.focus(window, cx);
        });
    });
    draw(cx);
    cx.update(|window, cx| {
        let editor = input.read(cx).state.read(cx).input_bounds();
        let toolbar = window.find("chat-input-toolbar").bounds();
        assert!(editor.bottom() <= toolbar.top(), "{editor:?}, {toolbar:?}");
    });
}

#[gpui_kit::test]
fn automatic_growth_adds_whole_rows_then_scrolls_and_follows_the_board(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_available_height(1000.0, cx)));
    draft(&input, "one\ntwo", cx);
    let two = viewport_height(&input, cx);
    draft(&input, "one\ntwo\nthree", cx);
    let three = viewport_height(&input, cx);
    let line = input.read_with(cx, |input, cx| input.state.read(cx).line_height().unwrap());
    assert_eq!(three - two, line);
    draft(&input, &["fixture line"; 12].join("\n"), cx);
    let cap = viewport_height(&input, cx);
    assert_eq!(cap, line * 12.0);
    draft(&input, &["fixture line"; 20].join("\n"), cx);
    assert_eq!(viewport_height(&input, cx), cap);
    cx.update(|window, cx| {
        window.scroll(
            "input-state",
            ScrollDelta::Pixels(point(px(0.0), px(-100.0))),
            cx,
        );
    });
    draw(cx);
    assert!(
        input.read_with(cx, |input, cx| input.state.read(cx).scroll_offset().y
            < px(0.0))
    );
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_available_height(400.0, cx)));
    draw(cx);
    assert!(viewport_height(&input, cx) < cap);
    cx.update(|window, _| assert!(window.find("chat-input").bounds().size.height <= px(200.0)));
}

#[gpui_kit::test]
fn counter_is_opt_in_and_is_a_status_above_the_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    draft(&input, "one\ntwo", cx);
    cx.update(|window, _| assert!(window.try_find("chat-input-counter").is_none()));
    cx.update(|_, cx| {
        input.update(cx, |input, cx| {
            input.set_limits(Some(super::DraftLimits::new(10, 4096)), cx);
        })
    });
    draw(cx);
    cx.update(|window, cx| {
        let pill = window.find("chat-input-counter");
        let editor = input.read(cx).state.read(cx).input_bounds();
        assert_eq!(pill.role(), Some(Role::Status));
        assert!(pill.label().unwrap().contains("2 / 10 lines"));
        assert!(pill.bounds().bottom() <= editor.top());
    });
}

#[gpui_kit::test]
fn an_agent_input_keeps_its_original_row_bounds_in_short_windows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(|window, cx| ChatInput::new("Fixture", window, cx));
    for height in [400.0, 160.0, 100.0, 900.0] {
        cx.simulate_resize(size(px(600.0), px(height)));
        for (text, rows) in [("short".to_owned(), 2.0), (["line"; 20].join("\n"), 12.0)] {
            draft(&input, &text, cx);
            input.read_with(cx, |input, cx| {
                assert!(input.message.is_none());
                assert_eq!(input.row_limits, (2, 12));
                let state = input.state.read(cx);
                assert_eq!(
                    state.input_bounds().size.height,
                    state.line_height().unwrap() * rows
                );
            });
        }
    }
}

#[gpui_kit::test]
fn a_counter_does_not_move_or_shrink_a_manually_sized_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    cx.update(|_, cx| {
        input.update(cx, |input, cx| {
            input.set_limits(Some(super::DraftLimits::new(100, 4096)), cx);
            input.set_available_height(400.0, cx);
            input.message.as_mut().unwrap().height = Some(200.0);
        })
    });
    draft(&input, "one", cx);
    let before = input.read_with(cx, |input, cx| {
        (input.row_limits, input.state.read(cx).input_bounds().size)
    });
    draft(&input, "one\ntwo", cx);
    let after = input.read_with(cx, |input, cx| {
        (input.row_limits, input.state.read(cx).input_bounds().size)
    });
    assert_eq!(before, after);
    cx.update(|window, cx| {
        let pill = window.find("chat-input-counter");
        assert!(pill.visible());
        assert!(pill.bounds().bottom() <= input.read(cx).state.read(cx).input_bounds().top());
    });
}

#[gpui_kit::test]
fn draft_boundary_arrows_scroll_the_caret_into_view(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::bind_keys);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_recall_keys(true, cx)));
    let text = ["Fixture café"; 20].join("\n");
    draft(&input, &text, cx);

    cx.update(|window, cx| window.press("down", cx));
    draw(cx);
    input.read_with(cx, |input, cx| {
        assert_eq!(input.cursor(cx), text.len());
        assert!(input.state.read(cx).scroll_offset().y < px(0.0));
    });

    cx.update(|window, cx| window.press("up", cx));
    draw(cx);
    input.read_with(cx, |input, cx| {
        assert_eq!(input.cursor(cx), 0);
        assert_eq!(input.state.read(cx).scroll_offset().y, px(0.0));
    });
}
