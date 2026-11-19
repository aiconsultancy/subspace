//! Shared GPUI fixtures for dashboard consumer tests.

pub mod contracts;

use crate::Dashboard;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, TestAppContext, VisualTestContext, Window, div, px,
};

pub type ToolbarRender<T> = fn(&mut T, &mut Window, &mut Context<T>) -> AnyElement;

struct ToolbarHarness<T: Dashboard> {
    board: Entity<T>,
    width: f32,
    render_toolbar: ToolbarRender<T>,
    _subscription: Subscription,
}

impl<T: Dashboard> Render for ToolbarHarness<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_toolbar = self.render_toolbar;
        let content = self.board.update(cx, |board, board_cx| {
            render_toolbar(board, window, board_cx)
        });
        div()
            .w(px(self.width))
            .flex()
            .child(div().flex_1().min_w_0().max_w_full().child(content))
    }
}

/// Mounts real dashboard toolbars in GPUI consumer tests.
pub trait ToolbarTestAppContext {
    /// Mounts a toolbar and lets the board callback include its private menu overlay.
    fn mount_toolbar<T: Dashboard>(
        &mut self,
        width: f32,
        render_toolbar: ToolbarRender<T>,
    ) -> (Entity<T>, &mut VisualTestContext);
}

impl ToolbarTestAppContext for TestAppContext {
    fn mount_toolbar<T: Dashboard>(
        &mut self,
        width: f32,
        render_toolbar: ToolbarRender<T>,
    ) -> (Entity<T>, &mut VisualTestContext) {
        self.update(gpui_kit::init);
        let (root, cx) = self.add_window_view(move |window, cx| {
            let board = cx.new(|board_cx| T::new(window, board_cx));
            let subscription = cx.observe(&board, |_, _, cx| cx.notify());
            ToolbarHarness {
                board,
                width,
                render_toolbar,
                _subscription: subscription,
            }
        });
        let board = root.read_with(cx, |root, _| root.board.clone());
        settle(cx);
        (board, cx)
    }
}

/// Drives overflow controls in GPUI consumer tests.
pub trait ToolbarVisualTestContext {
    /// Activates an overflow entry with the same native pointer events as a user.
    fn activate_overflow_entry(&mut self, entry: usize);

    /// Activates an overflow entry and verifies its menu uses the visible trigger anchor.
    fn activate_anchored_overflow_entry(&mut self, entry: usize, message: &str);
}

impl ToolbarVisualTestContext for VisualTestContext {
    fn activate_overflow_entry(&mut self, entry: usize) {
        self.update(|window, cx| {
            window.click("overflow-toolbar-button", cx);
            window.click(entry, cx);
        });
        settle(self);
    }

    fn activate_anchored_overflow_entry(&mut self, entry: usize, message: &str) {
        let trigger = self.update(|window, _| window.find("overflow-toolbar-more").bounds());
        self.activate_overflow_entry(entry);
        self.update(|window, _| {
            let menu = window.find("menu").bounds();
            assert!(
                menu.top() >= trigger.bottom() || menu.bottom() <= trigger.top(),
                "{message}: chooser must not cover its trigger"
            );
            assert!(
                menu.left() <= trigger.right() && menu.right() >= trigger.left(),
                "{message}: chooser must remain aligned with the measured chevron"
            );
        });
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}
