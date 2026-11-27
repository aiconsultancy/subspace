//! The Framework log viewer, opened in place of the board like the Search
//! view (Cmd/Ctrl+Shift+L, the palette's "Show logs", Help › Show Logs). Its
//! header's Close, Escape, the palette's "Close logs" or choosing a board
//! return to the board.

use gpui_kit::{AppContext as _, Context, Focusable as _, Window};
use subspace_dashboard::log_view::{LogView, LogViewEvent};

use super::{CloseLogView, MainSurface, OpenLogView, SubspaceApp};

impl SubspaceApp {
    pub(super) fn open_log_view(
        &mut self,
        _: &OpenLogView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let MainSurface::Logs { view, .. } = &self.main {
            view.focus_handle(cx).focus(window, cx);
            return;
        }
        if !self.close_jig(window, cx) {
            return;
        }
        self.session = None;
        self.help_open = false;
        let view = cx.new(|cx| LogView::new(window, cx));
        let subscription =
            cx.subscribe_in(&view, window, |app, _, event, window, cx| match event {
                LogViewEvent::Close => app.close_log_view(&CloseLogView, window, cx),
            });
        let focus = view.focus_handle(cx);
        let next = MainSurface::Logs {
            view,
            _subscription: subscription,
        };
        self.set_main(next, window, cx);
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_log_view(
        &mut self,
        _: &CloseLogView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.main.is_logs() {
            return;
        }
        self.set_main(MainSurface::Board, window, cx);
        self.focus_active(window, cx);
        cx.notify();
    }
}
