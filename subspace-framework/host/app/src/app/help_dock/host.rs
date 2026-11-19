//! Shell-owned native Dock state. Only this host writes the Help preference.

use std::time::Duration;

use gpui_kit::component::dock::{
    DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, Focusable as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Task, WeakEntity, Window, div, px,
};
use subspace_dashboard::prefs;

use super::super::SubspaceApp;
use super::geometry::shown_width;
use super::panel::{HelpContent, HelpPanel, HelpPanelEvent};
use super::state::{self, HelpDockState, Restored, Side};
use super::workspace::{WorkspacePanel, WorkspaceRenderer};

const SAVE_DELAY: Duration = Duration::from_millis(350);

pub(crate) enum HelpDockEvent {
    OpenChanged(bool),
}

pub(crate) struct HelpDock {
    area: Entity<DockArea>,
    help: Entity<HelpPanel>,
    owner: WeakEntity<SubspaceApp>,
    state: HelpDockState,
    protected: bool,
    applied_width: f32,
    geometry: Option<(gpui_kit::Pixels, gpui_kit::Pixels)>,
    clamp_queued: bool,
    focus_revision: u64,
    save_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl HelpDock {
    /// Create once and retain in the shell. `renderer` must render only the
    /// existing workspace, not the shell frame or this HelpDock again.
    pub(crate) fn create(
        owner: &Entity<SubspaceApp>,
        renderer: WorkspaceRenderer,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let restored = Restored::from_stored(
            prefs::get(cx, state::KEY).as_deref(),
            prefs::has_invalid_value(cx, state::KEY),
        );
        let state = restored.state();
        let protected = restored.protects_stored_value();
        let workspace = WorkspacePanel::create(owner, renderer, cx);
        let help = cx.new(|cx| HelpPanel::new(state.side, protected, window, cx));
        let (area, skin) = DockSkin::dock_area("app.help-dock", Some(1), window, cx);
        // A single Help tool supports explicit left/right moves, not arbitrary
        // nesting or drops into the board's own Dock. Resizing remains native.
        skin.set_toggle_button_visible(false, cx);
        let applied_width = dock_width(
            state.width,
            f32::from(window.viewport_size().width),
            f32::from(window.rem_size()),
        );
        install_center(&area, workspace, window, cx);
        let shown = HelpDockState {
            width: applied_width,
            ..state
        };
        install_help(&area, &help, shown, window, cx);
        cx.new(|cx| {
            let subscriptions = subscriptions(&area, &help, window, cx);
            Self {
                area,
                help,
                owner: owner.downgrade(),
                state,
                protected,
                applied_width,
                geometry: None,
                clamp_queued: false,
                focus_revision: 0,
                save_task: None,
                _subscriptions: subscriptions,
            }
        })
    }

    /// Pass a snapshot from the borrowed shell before rendering the host.
    /// Clamping never changes the preferred width or writes preferences.
    pub(crate) fn sync(
        &mut self,
        content: HelpContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.help
            .update(cx, |help, cx| help.set_content(content, cx));
        let geometry = (window.viewport_size().width, window.rem_size());
        if self.geometry != Some(geometry) {
            self.geometry = Some(geometry);
            self.queue_clamp(window, cx);
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.state.open
    }

    pub(crate) fn workspace_is_zoomed(&self, cx: &App) -> bool {
        self.area.read(cx).is_zoomed()
    }

    pub(crate) fn focus_if_zoomed(&self, window: &mut Window, cx: &mut App) -> bool {
        if !self.workspace_is_zoomed(cx) {
            return false;
        }
        let focus = self.help.focus_handle(cx);
        if !focus.contains_focused(window, cx) {
            focus.focus(window, cx);
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(!self.state.open, window, cx);
    }

    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(false, window, cx);
    }

    pub(crate) fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.open == open {
            return;
        }
        self.focus_revision = self.focus_revision.wrapping_add(1);
        self.area.update(cx, |area, cx| {
            if !open {
                area.set_zoomed_out(window, cx);
            }
            area.toggle_dock(placement(self.state.side), window, cx);
        });
        self.state.open = open;
        cx.emit(HelpDockEvent::OpenChanged(open));
        self.save(cx);
        if open {
            self.help.focus_handle(cx).focus(window, cx);
        } else {
            self.restore_workspace_focus(window, cx);
        }
        cx.notify();
    }

    fn move_side(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.side == side {
            return;
        }
        self.area.update(cx, |area, cx| {
            area.set_zoomed_out(window, cx);
            area.remove_dock(placement(self.state.side), window, cx);
        });
        self.state.side = side;
        self.help.update(cx, |help, cx| help.set_side(side, cx));
        let shown = HelpDockState {
            width: self.applied_width,
            ..self.state
        };
        install_help(&self.area, &self.help, shown, window, cx);
        self.save(cx);
        cx.notify();
    }

    fn layout_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let area = self.area.read(cx);
        let open = area.is_dock_open(placement(self.state.side));
        let width = area.dock_size(placement(self.state.side)).map(f32::from);
        let previous = self.state;
        if let Some(width) = width.filter(|width| *width != self.applied_width) {
            self.state.width = width;
            self.applied_width = width;
        }
        self.state.open = open;
        if previous == self.state {
            return;
        }
        if previous.open && !open {
            self.focus_revision = self.focus_revision.wrapping_add(1);
            self.area
                .update(cx, |area, cx| area.set_zoomed_out(window, cx));
            self.restore_workspace_focus(window, cx);
        }
        if previous.open != open {
            cx.emit(HelpDockEvent::OpenChanged(open));
        }
        self.clamp(window, cx);
        self.save(cx);
        cx.notify();
    }

    fn clamp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let measured = self.area.read(cx).bounds().size.width;
        let available = if measured > gpui_kit::Pixels::ZERO {
            measured
        } else {
            window.viewport_size().width
        };
        let width = dock_width(
            self.state.width,
            f32::from(available),
            f32::from(window.rem_size()),
        );
        if width == self.applied_width {
            return;
        }
        self.applied_width = width;
        self.area.update(cx, |area, cx| {
            // These pixels are resolved window geometry, not styling constants.
            area.set_dock_size(placement(self.state.side), px(width), window, cx);
        });
    }

    fn queue_clamp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clamp_queued {
            return;
        }
        self.clamp_queued = true;
        let owner = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = owner.update(cx, |host, cx| {
                host.clamp_queued = false;
                host.clamp(window, cx);
            });
        });
    }

    fn restore_workspace_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = self.owner.clone();
        let host = cx.weak_entity();
        let revision = self.focus_revision;
        // The shell may be the caller of close/toggle. Wait until that borrow
        // ends before restoring the active board, search or log view's focus.
        window.defer(cx, move |window, cx| {
            let current = host.upgrade().is_some_and(|host| {
                let host = host.read(cx);
                !host.state.open && host.focus_revision == revision
            });
            if !current {
                return;
            }
            let _ = owner.update(cx, |app, cx| {
                if app.settings.is_none()
                    && app.session.is_none()
                    && !app.jig_open.get()
                    && app.quit_warnings.is_none()
                {
                    app.focus_active(window, cx);
                }
            });
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.protected {
            return;
        }
        let Ok(text) = self.state.encode() else {
            return;
        };
        if prefs::get(cx, state::KEY).as_deref() == Some(text.as_str()) {
            return;
        }
        // Quit can flush this immediately, before the debounce completes.
        prefs::set(cx, state::KEY, text);
        self.save_task = Some(cx.spawn(async |_, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            cx.update(super::super::save_prefs);
        }));
    }
}

impl EventEmitter<HelpDockEvent> for HelpDock {}

fn dock_width(preferred: f32, available: f32, rem: f32) -> f32 {
    shown_width(preferred, available, rem).max(f32::from(gpui_kit::base::PANEL_MIN_SIZE))
}

fn subscriptions(
    area: &Entity<DockArea>,
    help: &Entity<HelpPanel>,
    window: &mut Window,
    cx: &mut Context<HelpDock>,
) -> Vec<Subscription> {
    vec![
        cx.subscribe_in(area, window, |host, _, event, window, cx| {
            if matches!(event, DockEvent::LayoutChanged) {
                host.layout_changed(window, cx);
            }
        }),
        cx.subscribe_in(help, window, |host, _, event, window, cx| match event {
            HelpPanelEvent::Close => host.close(window, cx),
            HelpPanelEvent::Move(side) => host.move_side(*side, window, cx),
        }),
        cx.observe_window_bounds(window, |host, window, cx| host.queue_clamp(window, cx)),
    ]
}

fn install_center(
    area: &Entity<DockArea>,
    workspace: Entity<WorkspacePanel>,
    window: &mut Window,
    cx: &mut App,
) {
    let center = DockLayout::tabs().panel_view(panel_handle(workspace), cx);
    area.update(cx, |area, cx| {
        area.set_center(center, window, cx);
        area.set_locked(true, window, cx);
    });
}

fn install_help(
    area: &Entity<DockArea>,
    help: &Entity<HelpPanel>,
    state: HelpDockState,
    window: &mut Window,
    cx: &mut App,
) {
    let layout = DockLayout::tabs().panel_view(panel_handle(help.clone()), cx);
    area.update(cx, |area, cx| {
        let placement = placement(state.side);
        area.set_dock(placement, layout, window, cx);
        area.set_dock_size(placement, px(state.width), window, cx);
        if !state.open {
            area.toggle_dock(placement, window, cx);
        }
    });
}

fn placement(side: Side) -> DockPlacement {
    match side {
        Side::Left => DockPlacement::Left,
        Side::Right => DockPlacement::Right,
    }
}

impl Render for HelpDock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.area.clone())
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
