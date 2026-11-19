//! The type-erased dashboard handle the app shell holds, and the
//! object-safe part of [`Dashboard`] behind it.

mod find;

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Entity, FocusHandle, SharedString, Subscription, Window,
};

use crate::{
    AgentContext, Dashboard, DashboardEvent, KeyHelp, Modal, PaletteCommand, ReturnLocation,
    ReturnPoint, Row, Search,
};

type FindView = Entity<crate::find_in_page::FindInPage>;

/// Receives a dashboard's events in the app.
pub type EventSink = Box<dyn Fn(&DashboardEvent, &mut Window, &mut App)>;

/// The object-safe part of [`Dashboard`], for [`DashboardHandle`].
// qual:allow(coupling, sit) reason: "The generic Entity<D> impl erases each concrete dashboard for the shell's heterogeneous handle list; Dashboard has associated constants and is not object-safe."
trait AnyDashboard {
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
        find::create(self, window, cx)
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

/// A created dashboard, with its type erased so the app can hold a list.
pub struct DashboardHandle {
    pub id: &'static str,
    pub title: SharedString,
    pub description: SharedString,
    pub icon: Option<&'static str>,
    /// [`Dashboard::HELP_MARKDOWN`], empty when the board has no help.
    pub help: &'static str,
    pub help_images: &'static [crate::HelpImage],
    pub searchable: bool,
    inner: Box<dyn AnyDashboard>,
    find: std::cell::OnceCell<FindView>,
}

/// The dashboard types whose keys are bound.
#[derive(Default)]
struct BoundKeys(std::collections::HashSet<std::any::TypeId>);

impl gpui_kit::Global for BoundKeys {}

/// Records `D` as bound and reports whether it was not bound before.
fn first_creation<D: Dashboard>(cx: &mut App) -> bool {
    cx.default_global::<BoundKeys>()
        .0
        .insert(std::any::TypeId::of::<D>())
}

impl DashboardHandle {
    // qual:api - shell notification navigation adapter.
    pub fn open_notification(&self, target: u64, window: &mut Window, cx: &mut App) {
        self.inner.open_notification(target, window, cx);
    }
    /// Create dashboard `D`. Settings can create a board again after it
    /// disables it, so `D`'s keys are bound once per process.
    pub fn new<D: Dashboard>(window: &mut Window, cx: &mut App) -> Self {
        first_creation::<D>(cx).then(|| D::bind_keys(cx));
        let entity = cx.new(|cx| D::new(window, cx));
        Self::from_entity(entity)
    }

    /// Wrap a dashboard the caller created itself, such as one of several
    /// configured instances of the same type. The registry replaces the
    /// type's identity with the instance's.
    pub fn from_entity<D: Dashboard>(entity: Entity<D>) -> Self {
        Self {
            id: D::ID,
            title: D::TITLE.into(),
            description: D::DESCRIPTION.into(),
            icon: D::ICON,
            help: D::HELP_MARKDOWN,
            help_images: D::HELP_IMAGES,
            searchable: D::SEARCHABLE,
            inner: Box::new(entity),
            find: std::cell::OnceCell::new(),
        }
    }

    pub fn view(&self) -> AnyView {
        self.inner.view()
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.inner.focus_handle(cx)
    }

    pub fn activate(&self, window: &mut Window, cx: &mut App) {
        self.inner.activate(window, cx);
    }

    pub fn deactivate(&self, window: &mut Window, cx: &mut App) {
        self.inner.deactivate(window, cx);
    }

    /// See [`Dashboard::covered`].
    pub fn covered(&self, covered: bool, window: &mut Window, cx: &mut App) {
        self.inner.covered(covered, window, cx);
    }

    // qual:api - the app shell titles the window from it.
    pub fn window_title(&self, cx: &App) -> Option<SharedString> {
        self.inner.window_title(cx)
    }

    /// Active source for the issue form's explicit Include paths consent.
    pub fn diagnostic_config_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.inner.diagnostic_config_path(cx)
    }

    pub fn background_refresh(&self, cx: &mut App) {
        self.inner.background_refresh(cx);
    }

    // qual:api - the app shell draws the dashboard toolbar with it.
    pub fn render_toolbar(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.inner.render_toolbar(window, cx)
    }

    pub fn status(&self, cx: &App) -> Option<SharedString> {
        self.inner.status(cx)
    }

    /// The mark the switcher tab shows, if the dashboard offers one.
    // qual:api - the app shell draws it on the switcher tab.
    pub fn badge(&self, cx: &App) -> Option<SharedString> {
        self.inner.badge(cx)
    }

    pub fn busy(&self, cx: &App) -> bool {
        self.inner.busy(cx)
    }

    pub fn subject(&self, cx: &App) -> Option<Row> {
        self.inner.subject(cx)
    }

    // qual:api - GPUI adapter; pure context conversion is tested in dashboard crates.
    pub fn agent_context(&self, cx: &App) -> Option<AgentContext> {
        self.inner.agent_context(cx)
    }

    pub fn commands(&self, cx: &mut App) -> Vec<PaletteCommand> {
        self.inner.commands(cx)
    }

    pub fn search(&self, cx: &App) -> Option<Search> {
        self.inner.search(cx)
    }

    // qual:api - the app shell opens a palette hit with it.
    pub fn open_search_result(&self, id: &str, window: &mut Window, cx: &mut App) {
        self.inner.open_search_result(id, window, cx);
    }

    pub fn open_link(&self, url: &str, cx: &mut App) -> bool {
        self.inner.open_link(url, cx)
    }

    /// See [`Dashboard::return_location`].
    pub fn return_location(&self, cx: &App) -> Option<ReturnLocation> {
        self.inner.return_location(cx)
    }

    /// See [`Dashboard::arrived_from`].
    pub fn arrived_from(&self, from: Option<ReturnPoint>, cx: &mut App) {
        self.inner.arrived_from(from, cx);
    }

    /// See [`Dashboard::return_to`].
    pub fn return_to(&self, key: &str, window: &mut Window, cx: &mut App) -> bool {
        self.inner.return_to(key, window, cx)
    }

    pub fn open_indexed_result_fenced(
        &self,
        hit: &subspace_search::Hit,
        permit: subspace_search::ReadPermit,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .open_indexed_result_fenced(hit, permit, window, cx);
    }

    pub fn modal(&self, cx: &App) -> Option<Modal> {
        self.inner.modal(cx)
    }

    pub fn dismiss(&self, window: &mut Window, cx: &mut App) {
        self.inner.dismiss(window, cx);
    }

    pub fn keys(&self, cx: &App) -> &'static [KeyHelp] {
        self.inner.keys(cx)
    }

    // qual:api - the app shell adds a footnote to the help sheet.
    pub fn help_foot(&self, cx: &App) -> Option<SharedString> {
        self.inner.help_foot(cx)
    }

    pub fn quit_warnings(&self, cx: &App) -> Vec<SharedString> {
        self.inner.quit_warnings(cx)
    }

    // qual:api - the app shell reloads the dashboard with it.
    pub fn reload(&self, window: &mut Window, cx: &mut App) {
        self.inner.reload(window, cx);
    }

    /// Call `sink` with each event the dashboard emits, while the returned
    /// subscription lives.
    pub fn subscribe(&self, window: &mut Window, cx: &mut App, sink: EventSink) -> Subscription {
        self.inner.subscribe(window, cx, sink)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{
        App, Context, EventEmitter, FocusHandle, Focusable, Global, IntoElement, Render,
        TestAppContext, Window, div,
    };

    use super::DashboardHandle;
    use crate::{Dashboard, DashboardEvent};

    #[derive(Default)]
    struct Binds(usize);
    impl Global for Binds {}

    struct Board {
        focus: FocusHandle,
        source: Option<std::path::PathBuf>,
    }
    impl EventEmitter<DashboardEvent> for Board {}
    impl Focusable for Board {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }
    impl Render for Board {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    impl Dashboard for Board {
        const ID: &'static str = "board";
        const TITLE: &'static str = "Board";
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                focus: cx.focus_handle(),
                source: None,
            }
        }
        fn bind_keys(cx: &mut App) {
            cx.default_global::<Binds>().0 += 1;
        }
        fn diagnostic_config_path(&self, _: &App) -> Option<std::path::PathBuf> {
            self.source.clone()
        }
        fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    }

    #[gpui_kit::test]
    fn creating_a_board_again_binds_its_keys_once(cx: &mut TestAppContext) {
        let window = cx.add_window(Board::new);
        window
            .update(cx, |_, window, cx| {
                let first = DashboardHandle::new::<Board>(window, cx);
                let second = DashboardHandle::new::<Board>(window, cx);
                assert_ne!(first.focus_handle(cx), second.focus_handle(cx));
                assert_eq!(cx.global::<Binds>().0, 1);
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn diagnostic_path_capability_reads_the_current_board_without_loading_a_file(
        cx: &mut TestAppContext,
    ) {
        let (board, cx) = cx.add_window_view(Board::new);
        let handle = DashboardHandle::from_entity(board.clone());
        cx.update(|_, cx| {
            assert_eq!(handle.diagnostic_config_path(cx), None);
            board.update(cx, |board, _| {
                board.source = Some("/fixture/config/active.json".into());
            });
            assert_eq!(
                handle.diagnostic_config_path(cx),
                Some(std::path::PathBuf::from("/fixture/config/active.json")),
            );
        });
    }
}
