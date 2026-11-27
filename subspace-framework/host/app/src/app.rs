mod about_dialog;
mod add_space;
mod background_refresh;
mod board_help;
mod chat;
mod configured;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod copy_details_fixture;
mod cover;
mod help;
mod help_dock;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use help_dock::test_support;
mod keys;
mod last_view;
mod logs;
mod menu_bar;
pub(crate) mod navigation;
#[cfg(test)]
mod notification_tests;
#[cfg(test)]
mod order_tests;
mod palette_lines;
mod palette_view;
mod quit_dialog;
#[cfg(feature = "report-issue")]
pub(crate) mod report_issue;
mod search;
mod session;
mod settings_dialog;
mod shell;
mod shortcuts;
mod sign_in_gate;
mod space_menu;
#[cfg(test)]
mod space_menu_tests;
mod state;
mod surface;
mod terminal_dock;
#[cfg(test)]
pub(crate) mod terminal_fixture;
#[cfg(test)]
mod terminal_tests;
mod theme_designer;
pub(crate) mod titlebar;
mod toasts;
mod toolbar;
mod window;

use std::collections::HashMap;

use gpui_kit::base::ResizableState;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::CommandState;
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, SharedString, Subscription, Window,
    actions,
};
use subspace_chat::{ChatPanel, ChatPanelEvent, OPEN_KEY as CHAT_OPEN_KEY};
use subspace_dashboard::overflow_toolbar::OverflowToolbar;
use subspace_dashboard::{DashboardEvent, DashboardHandle, Toast, Wrap, prefs};

use crate::jig::{Jig, JigEvent};
use crate::registry;
use crate::settings::Settings;

pub use keys::{bind_keys, palette_key};
pub(crate) use menu_bar::install_menus;
pub use subspace_dashboard::log_view::OpenLogView;
pub use window::{WINDOW_KEY, window_bounds};

pub(crate) use about_dialog::update_panel::available_update_version;
#[cfg(test)]
pub(crate) use about_dialog::update_panel::set_available_for_test;
pub(crate) use keys::save_prefs;
use navigation::NavigationMode;
use session::PaletteSession;
use state::{active_index, config_notice, recent_key, step_index, stored_flag, wrap_preference};
use surface::MainSurface;
use theme_designer::JigOpen;
use toasts::ShownToast;

actions!(
    subspace_app,
    [
        Quit,
        QuitAndRestart,
        TogglePalette,
        SearchItems,
        OpenSearchView,
        CloseSearchView,
        CloseLogView,
        Reload,
        NextDashboard,
        PreviousDashboard,
        ToggleHelp,
        ToggleShortcuts,
        CloseHelp,
        ToggleBoardHelp,
        CloseBoardHelp,
        ToggleWrap,
        ToggleJig,
        CloseJig,
        OpenSettings,
        OpenAbout,
        ToggleChat,
        /// Show or hide the docked terminal.
        ToggleTerminal,
        /// Fill the workspace with the terminal, or restore it.
        ToggleTerminalMaximized,
        /// Dock the terminal at the left edge.
        MoveTerminalLeft,
        /// Dock the terminal at the bottom edge.
        MoveTerminalToBottom,
        /// Dock the terminal at the right edge.
        MoveTerminalRight,
        /// Span a bottom terminal across the whole window, or put it back
        /// beside the chat.
        ToggleTerminalFullWidth,
        /// Show the terminal's guide in the Help panel.
        ShowTerminalHelp,
        ToggleNavigationMode,
        ToggleNavigationCollapsed,
        /// Show or hide the active Space's navigator, from its status bar
        /// switch.
        ToggleSpaceNavigator,
        /// Show or hide the active Space's inspector, from its status bar
        /// switch.
        ToggleSpaceInspector,
        PaletteEscape,
        PaletteBackspace
    ]
);

#[cfg(feature = "report-issue")]
actions!(subspace_app, [OpenReportIssue]);

/// State file keys. `board.wrap` is the stored name for the app-wide
/// wrap choice.
const WRAP_KEY: &str = "board.wrap";
const ACTIVE_KEY: &str = "app.dashboard";
const MAX_RECENT: usize = 8;
const REFRESH_INTERVAL_SECS: u64 = 60;
const REFRESH_JITTER_SLOTS: u64 = 11;
const DEFAULT_DOCK_BOTTOM: f32 = 240.;
const DEFAULT_DOCK_SIDE: f32 = 360.;
const MIN_DOCK_BOTTOM: f32 = 120.;
const MAX_DOCK_BOTTOM: f32 = 480.;
const MIN_DOCK_SIDE: f32 = 240.;
const MAX_DOCK_SIDE: f32 = 720.;

// qual:allow(srp, god_struct) reason: "One window shell retains dashboard handles, overlay/focus state and subscriptions under Context<Self>; show_notices delegates to its toast state rather than a separate responsibility."
pub struct SubspaceApp {
    dashboards: Vec<DashboardHandle>,
    /// Views of disabled boards that keep running processes; no tab shows them.
    parked: Vec<configured::Parked>,
    board_registrations: Vec<registry::BoardRegistration>,
    board_subscriptions: Vec<Subscription>,
    active: usize,
    palette: Entity<CommandState>,
    session: Option<PaletteSession>,
    help_open: bool,
    help_dock: Option<Entity<help_dock::HelpDock>>,
    shortcuts_anchor: Entity<subspace_dashboard::anchored::Anchor>,
    shortcuts_origin: Option<gpui_kit::FocusHandle>,
    jig: Entity<Jig>,
    jig_open: JigOpen,
    settings: Option<Entity<Settings>>,
    #[cfg(feature = "report-issue")]
    report_issue: Option<Entity<subspace_report_issue::ReportIssue>>,
    /// What covers the board area, as last told to the dashboards
    /// (`Dashboard::covered`).
    cover: cover::BoardCover,
    chat: Entity<ChatPanel>,
    chat_open: bool,
    /// The product's docked terminal, when it has one.
    terminal: Option<terminal_dock::TerminalDock>,
    /// Whether the Help panel shows the terminal's guide instead of the Space's.
    help_terminal: bool,
    quit_warnings: Option<Vec<SharedString>>,
    /// Whether the quit being confirmed restarts the app afterwards.
    quit_exit: quit_dialog::Exit,
    toasts: Vec<ShownToast>,
    next_toast: u64,
    /// Recently opened item ids, per dashboard id, newest first.
    recent: HashMap<&'static str, Vec<String>>,
    title: String,
    _subscriptions: Vec<Subscription>,
    notification_unavailable: bool,
    main: MainSurface,
    search_navigation: u64,
    /// The board view shown last, for a Back on another board
    /// (`last_view.rs`).
    last_view: Option<subspace_dashboard::ReturnPoint>,
    dock_bottom: f32,
    dock_side: f32,
    navigation_mode: NavigationMode,
    navigation_collapsed: bool,
    preferred_rail_width: Option<f32>,
    rail_state: Entity<ResizableState>,
    rail_synced_cap: Option<f32>,
    rail_synced_target: Option<f32>,
    rail_laid_out: bool,
    /// The horizontal dashboard tabs, as a shared responsive overflow row.
    tabs_toolbar: Entity<OverflowToolbar>,
    /// The shell-global controls share the same overflow behavior as tabs.
    globals_toolbar: Entity<OverflowToolbar>,
    /// The first row's measured available width, for the board-group
    /// beside/below placement decision (`toolbar::layout`).
    toolbar_row_width: f32,
    /// The active board's control group's measured natural width.
    toolbar_group_width: f32,
    /// The title-bar height the traffic lights were last centred on.
    traffic_lights_row: Option<gpui_kit::Pixels>,
}

/// Turn wrapping on or off app-wide and store the choice.
pub(crate) fn toggle_wrap(cx: &mut Context<SubspaceApp>) {
    let wrap = !subspace_dashboard::wrap(cx);
    cx.set_global(Wrap(wrap));
    prefs::set(cx, WRAP_KEY, wrap_preference(wrap));
    cx.notify();
}

impl SubspaceApp {
    /// `notices` are config file problems to show at startup.
    pub fn new(notices: Vec<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        #[cfg(any(test, all(debug_assertions, feature = "capture")))]
        crate::capture::register(window, cx);
        let mut app = Self::build(window, cx);
        app.sync_board_order(cx);
        let subscriptions = app.shell_subscriptions(window, cx);
        app._subscriptions.extend(subscriptions);
        Self::register_notification_window(window, cx);
        app.install_chat_context(cx);
        Self::install_search_navigation(cx);
        app.show_notices(notices, cx);
        app.show_mcp_toasts(cx);
        app.open_initial_dashboard(window, cx);
        background_refresh::start(cx);
        app.install_update_panel(cx);
        app
    }

    fn register_notification_window(window: &Window, cx: &mut Context<Self>) {
        if cx.has_global::<crate::notifications::Runtime>() {
            let entity = cx.weak_entity();
            let runtime = cx.global_mut::<crate::notifications::Runtime>();
            runtime.window = Some((window.window_handle(), entity));
        }
    }

    fn open_initial_dashboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = cx
            .try_global::<crate::notifications::Runtime>()
            .and_then(|runtime| runtime.pending);
        if let Some(route) = pending {
            cx.global_mut::<crate::notifications::Runtime>().pending = None;
            self.open_notification(route, window, cx);
        } else {
            self.focus_active(window, cx);
            self.activate_dashboard(window, cx);
        }
    }

    /// Construct shell state before attaching its panel subscriptions.
    fn build(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::restore_wrap(cx);
        // BoardFactories follows normalized config order; state.json must not override it.
        let dashboards = registry::dashboards(window, cx);
        let active = active_index(&dashboards, prefs::get(cx, ACTIVE_KEY).as_deref());
        let recent = Self::recent_lists(&dashboards, cx);
        let subscriptions = Self::dashboard_subscriptions(&dashboards, window, cx);
        let jig = cx.new(|cx| Jig::new(window, cx));
        let chat = cx.new(|cx| ChatPanel::new(window, cx));
        let chat_open = Self::restore_chat(&chat, window, cx);
        let layout = stored_shell(cx);
        Self {
            dashboards,
            parked: Vec::new(),
            active,
            palette: cx.new(|cx| CommandState::new(window, cx)),
            session: None,
            help_open: false,
            help_dock: None,
            shortcuts_anchor: cx.new(subspace_dashboard::anchored::Anchor::new),
            shortcuts_origin: None,
            jig,
            jig_open: JigOpen::default(),
            settings: None,
            #[cfg(feature = "report-issue")]
            report_issue: None,
            cover: cover::BoardCover::default(),
            chat,
            chat_open,
            terminal: terminal_dock::TerminalDock::create(window, cx),
            help_terminal: false,
            quit_warnings: None,
            quit_exit: quit_dialog::Exit::Quit,
            toasts: Vec::new(),
            next_toast: 0,
            recent,
            title: String::new(),
            _subscriptions: Vec::new(),
            board_subscriptions: subscriptions,
            board_registrations: cx.global::<registry::BoardFactories>().0.clone(),
            notification_unavailable: false,
            main: MainSurface::Board,
            search_navigation: 0,
            last_view: None,
            dock_bottom: layout.dock_bottom,
            dock_side: layout.dock_side,
            navigation_mode: layout.navigation_mode,
            navigation_collapsed: layout.navigation_collapsed,
            preferred_rail_width: layout.preferred_rail_width,
            rail_state: cx.new(|_| ResizableState::default()),
            rail_synced_cap: None,
            rail_synced_target: None,
            rail_laid_out: false,
            tabs_toolbar: cx.new(|_| OverflowToolbar::new(Vec::new())),
            globals_toolbar: cx.new(|_| OverflowToolbar::new(Vec::new())),
            toolbar_row_width: subspace_dashboard::measure::UNMEASURED,
            toolbar_group_width: subspace_dashboard::measure::UNMEASURED,
            traffic_lights_row: None,
        }
    }

    pub(super) fn toggle_navigation_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigation_mode = self.navigation_mode.toggle();
        navigation::save_mode(cx, self.navigation_mode);
        self.sync_rail(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_navigation_collapsed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_collapsed = !self.navigation_collapsed;
        navigation::save_collapsed(cx, self.navigation_collapsed);
        self.sync_rail(window, cx);
        cx.notify();
    }

    pub(super) fn sync_navigation_preferences(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_board_order(cx);
        self.navigation_mode = navigation::read_mode(cx);
        self.navigation_collapsed = navigation::read_collapsed_preference(cx);
        self.preferred_rail_width = navigation::read_width_preference(cx);
        self.sync_rail(window, cx);
        cx.notify();
    }

    /// Reorder retained dashboards without reopening them or changing the active board.
    fn sync_board_order(&mut self, cx: &App) {
        let Some(services) = cx.try_global::<subspace_dashboard::Services>() else {
            return;
        };
        let order = &services.config.board_order;
        let active = self.dashboards.get(self.active).map(|board| board.id);
        self.dashboards.sort_by_key(|board| {
            order
                .iter()
                .position(|id| id == board.id)
                .unwrap_or(order.len())
        });
        self.active = self
            .dashboards
            .iter()
            .position(|board| Some(board.id) == active)
            .unwrap_or_default();
    }

    pub(super) fn set_dock_dimension(&mut self, bottom: bool, value: f32, cx: &mut Context<Self>) {
        let value = if bottom {
            clamp_dock_dimension(value, MIN_DOCK_BOTTOM, MAX_DOCK_BOTTOM)
        } else {
            clamp_dock_dimension(value, MIN_DOCK_SIDE, MAX_DOCK_SIDE)
        };
        if bottom {
            self.dock_bottom = value;
            prefs::set(cx, "app.dock-bottom", value.to_string());
        } else {
            self.dock_side = value;
            prefs::set(cx, "app.dock-side", value.to_string());
        }
        cx.notify();
    }

    /// The stored wrap choice, as the app-wide `Wrap` global.
    fn restore_wrap(cx: &mut Context<Self>) {
        let wrap = stored_flag(prefs::get(cx, WRAP_KEY).as_deref());
        cx.set_global(Wrap(wrap));
    }

    /// Each dashboard's recently opened item ids.
    fn recent_lists(
        dashboards: &[DashboardHandle],
        cx: &mut Context<Self>,
    ) -> HashMap<&'static str, Vec<String>> {
        dashboards
            .iter()
            .map(|d| (d.id, prefs::get_list(cx, &recent_key(d.id))))
            .collect()
    }

    /// Open the chat pane when it was open last time; returns whether it is.
    fn restore_chat(chat: &Entity<ChatPanel>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if cx
            .try_global::<crate::notifications::Runtime>()
            .is_some_and(|runtime| runtime.pending.is_some())
        {
            return false;
        }
        let open = stored_flag(prefs::get(cx, CHAT_OPEN_KEY).as_deref());
        if open {
            chat.update(cx, |chat, cx| chat.on_open(window, cx));
        }
        open
    }

    /// Shell panel events, help filtering and window-bound persistence.
    fn shell_subscriptions(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        vec![
            cx.observe_global_in::<registry::BoardFactories>(window, |app, window, cx| {
                app.reconcile_boards(window, cx);
            }),
            cx.observe(&self.rail_state, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &self.shortcuts_anchor,
                window,
                |app, anchor, _: &gpui_kit::DismissEvent, window, cx| {
                    if anchor.read(cx).focus_handle(cx).is_focused(window) {
                        if let Some(origin) = app.shortcuts_origin.take() {
                            origin.focus(window, cx);
                        }
                    } else {
                        app.shortcuts_origin = None;
                    }
                    cx.notify();
                },
            ),
            cx.subscribe_in(&self.jig, window, |app, _, event, window, cx| {
                let JigEvent::Close = event;
                app.set_jig(false, window, cx);
            }),
            cx.observe(&self.chat, |_, _, cx| cx.notify()),
            cx.observe_global::<crate::mcp::McpService>(|app, cx| app.show_mcp_toasts(cx)),
            cx.subscribe_in(
                &self.chat,
                window,
                |app, _, event, window, cx| match event {
                    ChatPanelEvent::Close if app.chat_open => {
                        app.toggle_chat(&ToggleChat, window, cx);
                    }
                    ChatPanelEvent::Attention { kind, chat } => {
                        app.chat_attention(*kind, *chat, cx);
                    }
                    ChatPanelEvent::Close => {}
                },
            ),
            cx.observe_window_bounds(window, |_, window, cx| {
                let bounds = window.window_bounds().get_bounds();
                prefs::set(cx, WINDOW_KEY, window::format_bounds(&bounds));
            }),
        ]
    }

    /// Takes a route that opens nothing now: the test notification, or one
    /// held until sign-in.
    fn intercepted(&mut self, route: crate::notifications::Route, cx: &mut Context<Self>) -> bool {
        if route == crate::notifications::Route::Test {
            self.push_toast(Toast::info("Test notification activated."), cx);
            return true;
        }
        sign_in_gate::hold(route, cx)
    }

    pub(crate) fn open_notification(
        &mut self,
        route: crate::notifications::Route,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.intercepted(route, cx) {
            return;
        }
        self.dismiss_shortcuts(window, cx);
        if let crate::notifications::Route::Board(id, _) = route {
            self.note_arrival_at(id, cx);
        }
        if self.leave_main()
            && let Some(previous) = self.dashboard()
        {
            previous.deactivate(window, cx);
        }
        self.chat
            .update(cx, |chat, cx| chat.dismiss_popups(window, cx));
        self.chat_open = false;
        if let Some(settings) = &self.settings {
            let _ = settings.update(cx, |settings, cx| settings.restore_preview(window, cx));
        }
        self.settings = None;
        // Initial construction runs before Kit attaches its root/plugin state.
        if window.root::<gpui_kit::base::Root>().flatten().is_some() {
            window.close_all_dialogs(cx);
        }
        self.help_open = false;
        self.close_jig(window, cx);
        self.session = None;
        self.search_navigation = self.search_navigation.wrapping_add(1);
        self.notification_unavailable = false;
        match route {
            crate::notifications::Route::Board(id, target) => {
                if let Some(index) = self.dashboards.iter().position(|board| board.id == id) {
                    self.active = index;
                    // The board covers old details and validates before ordinary activation/restoration.
                    self.dashboards[index].open_notification(target, window, cx);
                    self.focus_active(window, cx);
                    prefs::set(cx, ACTIVE_KEY, id);
                } else {
                    self.notification_unavailable = true;
                }
            }
            crate::notifications::Route::Unavailable => self.notification_unavailable = true,
            crate::notifications::Route::Test => {}
        }
        cx.notify();
    }

    fn show_notices(&mut self, notices: Vec<String>, cx: &mut Context<Self>) {
        for notice in notices {
            self.push_toast(Toast::error(config_notice(&notice)), cx);
        }
    }

    fn activate_dashboard(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dashboard) = self.dashboards.get(self.active) {
            dashboard.activate(window, cx);
        }
    }

    /// Forward every dashboard's events to the shell.
    fn dashboard_subscriptions(
        dashboards: &[DashboardHandle],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let me = cx.weak_entity();
        dashboards
            .iter()
            .map(|dashboard| {
                let me = me.clone();
                let id = dashboard.id;
                dashboard.subscribe(
                    window,
                    cx,
                    Box::new(move |event, window, cx| {
                        _ = me.update(cx, |app, cx| app.on_dashboard_event(id, event, window, cx));
                    }),
                )
            })
            .collect()
    }

    fn dashboard(&self) -> Option<&DashboardHandle> {
        if self.notification_unavailable {
            return None;
        }
        self.dashboards.get(self.active)
    }

    #[cfg(any(test, all(debug_assertions, feature = "capture")))]
    pub(crate) fn capture_space_focus(&self, cx: &App) -> Option<gpui_kit::FocusHandle> {
        self.dashboard().map(|space| space.focus_handle(cx))
    }

    fn focus_active(&self, window: &mut Window, cx: &mut App) {
        if let Some(host) = &self.help_dock
            && host.update(cx, |host, cx| host.focus_if_zoomed(window, cx))
        {
            return;
        }
        if let Some(view) = self.main.focus_handle(cx) {
            view.focus(window, cx);
        } else if let Some(dashboard) = self.dashboard() {
            dashboard.focus_handle(cx).focus(window, cx);
        }
    }

    fn switch_to(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if sign_in_gate::shown(cx) {
            return;
        }
        self.dismiss_shortcuts(window, cx);
        self.note_arrival(index, cx);
        let board_showing = self.leave_main();
        self.search_navigation = self.search_navigation.wrapping_add(1);
        self.notification_unavailable = false;
        let Some(dashboard) = self.dashboards.get(index) else {
            return;
        };
        let id = dashboard.id;
        if board_showing
            && index != self.active
            && let Some(previous) = self.dashboards.get(self.active)
        {
            previous.deactivate(window, cx);
        }
        self.active = index;
        prefs::set(cx, ACTIVE_KEY, id);
        subspace_core::log::Logger::new("app").debug(format!("switched to {id}"));
        if self.terminal_maximized() {
            self.change_terminal(terminal_dock::Change::Restore, window, cx);
        }
        self.focus_active(window, cx);
        self.dashboards[index].activate(window, cx);
        cx.notify();
    }

    fn step(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            return;
        }
        if let Some(next) = step_index(self.active, step, self.dashboards.len()) {
            self.switch_to(next, window, cx);
        }
    }

    /// F1 to F12: the dashboard at `index`, in toolbar order.
    fn show(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            self.switch_to(index, window, cx);
        }
    }

    fn on_dashboard_event(
        &mut self,
        id: &'static str,
        event: &DashboardEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DashboardEvent::Toast(toast) => self.push_toast(toast.clone(), cx),
            DashboardEvent::OpenPalette(mode) => {
                if self.dashboard().is_some_and(|d| d.id == id) {
                    self.open_palette(*mode, window, cx);
                }
            }
            DashboardEvent::Opened(item) => self.push_recent(id, item.to_string(), cx),
            DashboardEvent::Navigate(link) => self.open_board_link(id, link, window, cx),
            DashboardEvent::Return(point) => self.return_to_view(id, point, window, cx),
            DashboardEvent::Chat(request) => self.chat_request(request, window, cx),
            DashboardEvent::ConfigChanged => {
                if let Err(error) = registry::apply_running_config(window, cx) {
                    let message = format!("Agent tools keep their scope until a restart: {error}");
                    self.push_toast(Toast::error(message), cx);
                }
            }
        }
    }

    fn open_board_link(
        &mut self,
        requester: &'static str,
        link: &subspace_dashboard::BoardNavigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.shows_current(self.active)
            || self.dashboard().is_none_or(|board| board.id != requester)
        {
            return;
        }
        let Some(target) =
            subspace_dashboard::contracts::navigation_target(requester, link.responder, cx)
        else {
            return;
        };
        let Some(index) = self.dashboards.iter().position(|board| board.id == target) else {
            return;
        };
        // The link came from the requester's view: a Back there returns to it.
        self.note_arrival_from(index, link.origin.clone(), cx);
        if self.dashboards[index].open_link(&link.url, cx) {
            self.switch_to(index, window, cx);
        }
    }

    // ---- Toasts (REQ-12) ------------------------------------------------------------

    // ---- Palette (REQ-04, REQ-05) ---------------------------------------------------

    // ---- Help, wrap, jig, reload ----------------------------------------------------

    fn reload(&mut self, _: &Reload, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            return;
        }
        if let Some(view) = self.main.search() {
            view.update(cx, |view, cx| view.refresh_sources(window, cx));
            return;
        }
        if self.reload_terminal(window, cx) {
            return;
        }
        if let Some(dashboard) = self.dashboard() {
            dashboard.reload(window, cx);
        }
    }

    // ---- Rendering ------------------------------------------------------------------
}

fn refresh_jitter(index: usize) -> u64 {
    1 + (index as u64 % REFRESH_JITTER_SLOTS)
}

struct StoredShell {
    dock_bottom: f32,
    dock_side: f32,
    navigation_mode: NavigationMode,
    navigation_collapsed: bool,
    preferred_rail_width: Option<f32>,
}

fn stored_shell(cx: &App) -> StoredShell {
    StoredShell {
        dock_bottom: stored_dimension(
            prefs::get(cx, "app.dock-bottom").as_deref(),
            DEFAULT_DOCK_BOTTOM,
            MIN_DOCK_BOTTOM,
            MAX_DOCK_BOTTOM,
        ),
        dock_side: stored_dimension(
            prefs::get(cx, "app.dock-side").as_deref(),
            DEFAULT_DOCK_SIDE,
            MIN_DOCK_SIDE,
            MAX_DOCK_SIDE,
        ),
        navigation_mode: navigation::read_mode(cx),
        navigation_collapsed: navigation::read_collapsed_preference(cx),
        preferred_rail_width: navigation::read_width_preference(cx),
    }
}

fn stored_dimension(value: Option<&str>, default: f32, minimum: f32, maximum: f32) -> f32 {
    value
        .and_then(|value| value.parse().ok())
        .map_or(default, |value| {
            clamp_dock_dimension(value, minimum, maximum)
        })
}

fn clamp_dock_dimension(value: f32, minimum: f32, maximum: f32) -> f32 {
    value.clamp(minimum, maximum)
}

/// Capture evidence: the updater as if an update to `version` had installed.
#[cfg(feature = "capture")]
pub(crate) fn seed_installed_update(version: &str, cx: &mut App) {
    about_dialog::update_panel::seed_installed(version, cx);
}

#[cfg(test)]
mod tests {
    use super::{clamp_dock_dimension, refresh_jitter};
    use std::time::Duration;
    use subspace_dashboard::refresh::Schedule;

    #[test]
    fn refresh_jitter_staggers_startup_and_periodic_work() {
        let startup: Vec<_> = (0..12).map(refresh_jitter).collect();
        assert_eq!(startup[0], 1);
        assert_eq!(startup[11], 1);
        assert!(startup.iter().all(|delay| (1..=11).contains(delay)));
    }

    #[test]
    fn each_board_keeps_its_own_sixty_second_deadline() {
        let mut first = Schedule::new(Duration::from_secs(refresh_jitter(0)));
        let mut eighth = Schedule::new(Duration::from_secs(refresh_jitter(7)));
        let minute = Duration::from_secs(60);
        assert!(first.ready(Duration::from_secs(1), true, minute));
        assert!(!eighth.ready(Duration::from_secs(1), true, minute));
        assert!(eighth.ready(Duration::from_secs(8), true, minute));
        assert!(first.ready(Duration::from_secs(61), true, minute));
        assert!(!eighth.ready(Duration::from_secs(61), true, minute));
        assert!(eighth.ready(Duration::from_secs(68), true, minute));
    }

    #[test]
    fn dock_dimensions_are_clamped_at_the_persistence_boundary() {
        assert_eq!(clamp_dock_dimension(50., 120., 480.), 120.);
        assert_eq!(clamp_dock_dimension(600., 120., 480.), 480.);
        assert_eq!(clamp_dock_dimension(240., 120., 480.), 240.);
    }
}
