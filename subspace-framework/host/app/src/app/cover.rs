//! Whether something covers the board area, for boards that draw native
//! views above GPUI.

use gpui_kit::component::WindowExt as _;
use gpui_kit::{AnyElement, App, Window};

use super::SubspaceApp;

impl SubspaceApp {
    /// Tell the dashboards when one of the shell's `overlays` or a GPUI Kit
    /// dialog or sheet, such as Settings, starts or stops covering the
    /// board area.
    pub(super) fn sync_board_cover(
        &mut self,
        overlays: [&Option<AnyElement>; 5],
        window: &mut Window,
        cx: &mut App,
    ) {
        // A window without Kit's root, such as a test window, has no Kit overlays.
        let kit = window.root::<gpui_kit::base::Root>().flatten().is_some();
        let covered = overlays.iter().any(|overlay| overlay.is_some())
            || (kit && (window.has_active_dialog(cx) || window.has_active_sheet(cx)));
        if covered == self.board_covered {
            return;
        }
        self.board_covered = covered;
        for dashboard in &self.dashboards {
            dashboard.covered(covered, window, cx);
        }
    }
}
