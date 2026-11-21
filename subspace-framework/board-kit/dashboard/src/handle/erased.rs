//! The object-safe part of [`Dashboard`] that [`super::DashboardHandle`]
//! holds, and its one implementation for every dashboard entity.

use gpui_kit::{AnyElement, AnyView, App, Entity, FocusHandle, SharedString, Subscription, Window};

use super::{EventSink, FindView};
use crate::workspace::{SpaceDock, SpaceDocks};
use crate::{
    AgentContext, Dashboard, DashboardEvent, KeyHelp, Modal, PaletteCommand, ReturnLocation,
    ReturnPoint, Row, Search,
};

/// The object-safe part of [`Dashboard`], for [`super::DashboardHandle`].
// qual:allow(coupling, sit) reason: "The generic Entity<D> impl erases each concrete dashboard for the shell's heterogeneous handle list; Dashboard has associated constants and is not object-safe."
pub(super) trait AnyDashboard {
    fn view(&self) -> AnyView;
    fn find_view(&self, window: &mut Window, cx: &mut App) -> FindView;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn activate(&self, window: &mut Window, cx: &mut App);
    fn deactivate(&self, window: &mut Window, cx: &mut App);
    fn covered(&self, covered: bool, window: &mut Window, cx: &mut App);
    fn open_notification(&self, target: u64, window: &mut Window, cx: &mut App);
    fn window_title(&self, cx: &App) -> Option<SharedString>;
    fn diagnostic_config_path(&self, cx: &App) -> Option<std::path::PathBuf>;
    fn background_refresh(&self, cx: &mut App);
    fn render_toolbar(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement>;
    fn status(&self, cx: &App) -> Option<SharedString>;
    fn docks(&self, cx: &App) -> SpaceDocks;
    fn toggle_dock(&self, dock: SpaceDock, window: &mut Window, cx: &mut App);
    fn badge(&self, cx: &App) -> Option<SharedString>;
    fn busy(&self, cx: &App) -> bool;
    fn subject(&self, cx: &App) -> Option<Row>;
    fn agent_context(&self, cx: &App) -> Option<AgentContext>;
    fn commands(&self, cx: &mut App) -> Vec<PaletteCommand>;
    fn search(&self, cx: &App) -> Option<Search>;
    fn open_search_result(&self, id: &str, window: &mut Window, cx: &mut App);
    fn open_link(&self, url: &str, cx: &mut App) -> bool;
    fn return_location(&self, cx: &App) -> Option<ReturnLocation>;
    fn arrived_from(&self, from: Option<ReturnPoint>, cx: &mut App);
    fn return_to(&self, key: &str, window: &mut Window, cx: &mut App) -> bool;
    fn open_indexed_result_fenced(
        &self,
        hit: &subspace_search::Hit,
        permit: subspace_search::ReadPermit,
        window: &mut Window,
        cx: &mut App,
    );
    fn modal(&self, cx: &App) -> Option<Modal>;
    fn dismiss(&self, window: &mut Window, cx: &mut App);
    fn keys(&self, cx: &App) -> &'static [KeyHelp];
    fn help_foot(&self, cx: &App) -> Option<SharedString>;
    fn quit_warnings(&self, cx: &App) -> Vec<SharedString>;
    fn reload(&self, window: &mut Window, cx: &mut App);
    fn subscribe(&self, window: &mut Window, cx: &mut App, sink: EventSink) -> Subscription;
}

impl<D: Dashboard> AnyDashboard for Entity<D> {
    fn find_view(&self, window: &mut Window, cx: &mut App) -> FindView {
        super::find::create(self, window, cx)
    }
    fn open_notification(&self, target: u64, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| {
            dashboard.open_notification(target, window, cx)
        });
    }
    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn activate(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.activate(window, cx));
    }

    fn deactivate(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.deactivate(window, cx));
    }

    fn covered(&self, covered: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.covered(covered, window, cx));
    }

    fn window_title(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).window_title(cx)
    }

    fn diagnostic_config_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.read(cx).diagnostic_config_path(cx)
    }

    fn background_refresh(&self, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.background_refresh(cx));
    }

    fn render_toolbar(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.update(cx, |dashboard, cx| dashboard.render_toolbar(window, cx))
    }

    fn status(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).status(cx)
    }

    fn docks(&self, cx: &App) -> SpaceDocks {
        self.read(cx).docks(cx)
    }

    fn toggle_dock(&self, dock: SpaceDock, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.toggle_dock(dock, window, cx));
    }

    fn badge(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).badge(cx)
    }

    fn busy(&self, cx: &App) -> bool {
        self.read(cx).busy(cx)
    }

    fn subject(&self, cx: &App) -> Option<Row> {
        self.read(cx).subject(cx)
    }

    fn agent_context(&self, cx: &App) -> Option<AgentContext> {
        self.read(cx).agent_context(cx)
    }

    fn commands(&self, cx: &mut App) -> Vec<PaletteCommand> {
        self.update(cx, |dashboard, cx| dashboard.commands(cx))
    }

    fn search(&self, cx: &App) -> Option<Search> {
        self.read(cx).search(cx)
    }

    fn open_search_result(&self, id: &str, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| {
            dashboard.open_search_result(id, window, cx);
        });
    }

    fn open_link(&self, url: &str, cx: &mut App) -> bool {
        self.update(cx, |dashboard, cx| dashboard.open_link(url, cx))
    }

    fn return_location(&self, cx: &App) -> Option<ReturnLocation> {
        self.read(cx).return_location(cx)
    }

    fn arrived_from(&self, from: Option<ReturnPoint>, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.arrived_from(from, cx));
    }

    fn return_to(&self, key: &str, window: &mut Window, cx: &mut App) -> bool {
        self.update(cx, |dashboard, cx| dashboard.return_to(key, window, cx))
    }

    fn open_indexed_result_fenced(
        &self,
        hit: &subspace_search::Hit,
        permit: subspace_search::ReadPermit,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.update(cx, |dashboard, cx| {
            dashboard.open_indexed_result_fenced(hit, permit, window, cx)
        });
    }

    fn modal(&self, cx: &App) -> Option<Modal> {
        self.read(cx).modal(cx)
    }

    fn dismiss(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.dismiss(window, cx));
    }

    fn keys(&self, cx: &App) -> &'static [KeyHelp] {
        self.read(cx).keys()
    }

    fn help_foot(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).help_foot()
    }

    fn quit_warnings(&self, cx: &App) -> Vec<SharedString> {
        self.read(cx).quit_warnings(cx)
    }

    fn reload(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |dashboard, cx| dashboard.reload(window, cx));
    }

    fn subscribe(&self, window: &mut Window, cx: &mut App, sink: EventSink) -> Subscription {
        window.subscribe(self, cx, move |_, event: &DashboardEvent, window, cx| {
            sink(event, window, cx);
        })
    }
}
