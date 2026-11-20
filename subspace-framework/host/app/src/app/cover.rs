//! Whether something covers the board area, for boards that draw native
//! views above GPUI.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{AnyElement, App, Context, DismissEvent, WeakEntity, Window};

use super::SubspaceApp;

/// What covers the board area, for boards that draw native views above GPUI.
#[derive(Default)]
pub(super) struct BoardCover {
    /// Whether a shell overlay covers the board area, as last told to the
    /// dashboards.
    pub covered: bool,
    /// While a shell context menu is open, such as a Space tab's: it covers
    /// the board area, where a native view would draw over it.
    pub menu: Option<gpui_kit::Subscription>,
}

impl SubspaceApp {
    /// Tell the dashboards when one of the shell's `overlays`, such as an
    /// open context menu, or a GPUI Kit dialog or sheet, such as Settings,
    /// starts or stops covering the board area.
    pub(super) fn sync_board_cover(
        &mut self,
        overlays: [&Option<AnyElement>; 6],
        window: &mut Window,
        cx: &mut App,
    ) {
        // A window without Kit's root, such as a test window, has no Kit overlays.
        let kit = window.root::<gpui_kit::base::Root>().flatten().is_some();
        let covered = overlays.iter().any(|overlay| overlay.is_some())
            || (kit && (window.has_active_dialog(cx) || window.has_active_sheet(cx)));
        if covered == self.cover.covered {
            return;
        }
        self.cover.covered = covered;
        for dashboard in &self.dashboards {
            dashboard.covered(covered, window, cx);
        }
    }
}

/// Counts the context menu being built as covering the board area until it
/// is dismissed, so a native view, such as a web Space's, does not draw over
/// it. Call it from the menu's builder.
pub(super) fn cover_while_open(
    app: &WeakEntity<SubspaceApp>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) {
    let menu = cx.weak_entity();
    let app = app.clone();
    // The menu is still being built; it exists once this turn ends.
    window.defer(cx, move |_, cx| {
        let Some(menu) = menu.upgrade() else {
            return;
        };
        _ = app.update(cx, |app, cx| {
            let dismissed = cx.subscribe(&menu, |app, _, _: &DismissEvent, cx| {
                app.cover.menu = None;
                cx.notify();
            });
            app.cover.menu = Some(dismissed);
            cx.notify();
        });
    });
}
