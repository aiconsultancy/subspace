//! Real Chat and Canvas panes must fit the workspace row left by a side dock,
//! rather than consuming the window's width and reducing the board to zero.

use std::sync::Arc;

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, TestAppContext, Window, WindowHandle, div, px,
    relative, size,
};
use subspace_agent::canvas::{Canvas, CanvasContent, CanvasKind};

use super::ChatPanel;
use crate::prefs;

struct Workspace {
    chat: Entity<ChatPanel>,
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let gallery = self.chat.read(cx).canvas_gallery();
        let canvas = self
            .chat
            .update(cx, |chat, cx| chat.render_canvas(window, cx));
        h_flex()
            .size_full()
            .items_stretch()
            .child(
                h_flex()
                    .id("workspace-row")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .items_stretch()
                    .when(!gallery, |row| {
                        row.child(div().id("board").test_support().flex_1().min_w_0())
                    })
                    .children(canvas)
                    .child(self.chat.clone()),
            )
            .child(div().w(px(500.)).max_w(relative(0.6)).flex_shrink_0())
    }
}

struct Harness {
    window: WindowHandle<Workspace>,
    chat: Entity<ChatPanel>,
}

impl Harness {
    fn open(cx: &mut TestAppContext, canvas: bool) -> Self {
        cx.update(|cx| {
            cx.set_reduce_motion(true);
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
            prefs::set(cx, "chat.width", "420");
        });
        let window = cx.open_window(size(px(2_000.), px(900.)), |window, cx| Workspace {
            chat: cx.new(|cx| ChatPanel::new_copilot(window, cx)),
        });
        let chat = window.update(cx, |view, _, _| view.chat.clone()).unwrap();
        if canvas {
            chat.update(cx, |chat, _| {
                chat.tab_mut().canvases.items.push(Arc::new(Canvas {
                    id: "test-canvas".into(),
                    kind: CanvasKind::Diagram,
                    title: "Test diagram".into(),
                    caption: "Test canvas".into(),
                    opened_at: "test".into(),
                    source: String::new(),
                    content: CanvasContent::Error("Test canvas content".into()),
                }));
            });
        }
        Self { window, chat }
    }

    fn resize(&self, cx: &mut TestAppContext, width: f32) {
        cx.simulate_window_resize(self.window.into(), size(px(width), px(900.)));
        for _ in 0..3 {
            cx.update_window(self.window.into(), |_, window, cx| window.render_frame(cx))
                .unwrap();
            cx.run_until_parked();
        }
    }

    fn bounds(&self, cx: &mut TestAppContext, id: &'static str) -> Bounds<Pixels> {
        cx.update_window(self.window.into(), |_, window, _| window.find(id).bounds())
            .unwrap()
    }

    fn assert_preferences_unchanged(&self, cx: &TestAppContext) {
        self.chat.read_with(cx, |chat, cx| {
            assert_eq!(chat.width, 420.);
            assert_eq!(chat.canvas_width, 380.);
            assert_eq!(prefs::get(cx, "chat.width").as_deref(), Some("420"));
            assert!(
                chat.handle.is_none(),
                "layout must not start an agent runtime"
            );
        });
    }
}

fn assert_contained(parent: Bounds<Pixels>, child: Bounds<Pixels>) {
    assert!(child.size.width > px(0.));
    assert!(child.left() >= parent.left() - px(1.));
    assert!(child.right() <= parent.right() + px(1.));
}

fn exercise_narrow_workspace(cx: &mut TestAppContext, canvas: bool) -> Harness {
    let shell = Harness::open(cx, canvas);
    shell.resize(cx, 2_000.);
    assert_eq!(shell.bounds(cx, "agent-chat").size.width, px(420.));
    if canvas {
        assert_eq!(shell.bounds(cx, "chat-canvas-area").size.width, px(380.));
    }
    shell.resize(cx, 600.);
    let row = shell.bounds(cx, "workspace-row");
    let board = shell.bounds(cx, "board");
    let chat = shell.bounds(cx, "agent-chat");
    assert!(
        board.size.width >= row.size.width * 0.4 - px(1.),
        "board lost its share: {board:?} in {row:?}"
    );
    assert_contained(row, board);
    assert_contained(row, chat);
    if canvas {
        let canvas = shell.bounds(cx, "chat-canvas-area");
        assert_contained(row, canvas);
        assert!(board.right() <= canvas.left() + px(1.));
        assert!(canvas.right() <= chat.left() + px(1.));
    } else {
        assert!(board.right() <= chat.left() + px(1.));
    }
    shell.assert_preferences_unchanged(cx);
    shell.resize(cx, 2_000.);
    if canvas {
        assert_eq!(shell.bounds(cx, "chat-canvas-area").size.width, px(380.));
    }
    shell.assert_preferences_unchanged(cx);
    shell
}

#[gpui_kit::test]
fn chat_leaves_board_space_in_a_narrow_parent_and_regains_its_width(cx: &mut TestAppContext) {
    let shell = exercise_narrow_workspace(cx, false);
    assert_eq!(shell.bounds(cx, "agent-chat").size.width, px(420.));
}

#[gpui_kit::test]
fn chat_and_canvas_leave_board_space_in_a_narrow_parent_and_regain_their_widths(
    cx: &mut TestAppContext,
) {
    let shell = exercise_narrow_workspace(cx, true);
    assert_eq!(shell.bounds(cx, "agent-chat").size.width, px(420.));
}

#[gpui_kit::test]
fn gallery_keeps_its_space_and_escape_restores_the_board(cx: &mut TestAppContext) {
    let shell = Harness::open(cx, true);
    shell.resize(cx, 2_000.);
    cx.update_window(shell.window.into(), |_, window, cx| {
        window.click("canvas-zoom", cx);
    })
    .unwrap();
    shell.resize(cx, 600.);
    let row = shell.bounds(cx, "workspace-row");
    let canvas = shell.bounds(cx, "chat-canvas-area");
    let chat = shell.bounds(cx, "agent-chat");
    assert_contained(row, canvas);
    assert_contained(row, chat);
    assert_eq!(canvas.size.width + chat.size.width, row.size.width);
    cx.update_window(shell.window.into(), |_, window, cx| {
        assert!(window.try_find("board").is_none());
        window.press("escape", cx);
    })
    .unwrap();
    assert!(shell.bounds(cx, "board").size.width > px(0.));
    shell.resize(cx, 2_000.);
    assert_eq!(shell.bounds(cx, "agent-chat").size.width, px(420.));
    assert_eq!(shell.bounds(cx, "chat-canvas-area").size.width, px(380.));
    shell.assert_preferences_unchanged(cx);
}
