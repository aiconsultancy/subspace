//! The dashboard interface. Every dashboard is a crate that implements
//! [`Dashboard`] for one GPUI view; the app lists each dashboard once, in
//! `framework/host/app/src/registry.rs`, and drives it through [`DashboardHandle`].
//!
//! The app shell owns the window, the dashboard switcher, the ⌘K palette,
//! help, toasts, wrap, the density jig and reload; a dashboard owns its view,
//! its toolbar controls, its commands, its item search and its keys. A
//! dashboard talks to the shell with [`DashboardEvent`]s and reads the shell's
//! choices from globals ([`Wrap`], [`Density`], [`Services`]).
//!
//! Dashboards that show grouped rows with a detail panel implement
//! [`ListDashboard`] too and render with [`list::render`].

pub mod action;
pub mod alerts;
pub mod anchored;
pub mod animated_collapsible;
pub mod artwork;
pub mod avatar;
pub mod badge;
pub mod chart;
pub mod chat_bubble;
pub mod chat_input;
pub mod chat_sessions;
pub mod code;
pub mod composer;
pub mod context_text;
pub mod contracts;
pub mod contrast;
pub mod copy_details;
pub mod dates;
pub mod density;
pub mod dialog;
pub mod dock_panel;
pub mod feedback;
pub mod fields;
pub mod find_in_page;
pub mod github_sign_in;
pub mod glass;
pub mod help;
pub mod html;
pub mod icon_button;
pub mod images;
pub mod links;
pub mod list;
pub mod markdown_editor;
// Keep the bundled editor's upstream API intact for its own tests and updates.
#[allow(dead_code, unused_imports)]
#[path = "../vendor/markdown-editor/src/lib.rs"]
mod markdown_engine;
extern crate gpui_kit as gpui;
pub mod measure;
pub mod media;
pub mod menu;
pub mod native_grid;
pub mod notifications;
pub mod overflow_toolbar;
pub mod panel;
pub mod prefetch;
pub mod prefs;
mod product;
pub mod reference_card;
pub mod refresh;
pub mod repositories;
pub mod retained;
pub mod reveal;
pub mod rich;
pub mod row_menu;
pub mod search_adapter;
pub mod secret;
pub mod settings;
pub mod slider;
pub mod surface_header;
pub mod tabs;
#[cfg(feature = "test-support")]
pub mod test_support;
pub mod toggle;
pub mod tokens;
pub mod toolbar_label;
pub mod truncated;
pub mod typography;
pub mod workspace;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    AnyElement, App, Context, EventEmitter, Focusable, Global, KeyBinding, Render, SharedString,
    Window, actions,
};
use subspace_core::config::Config;
use subspace_core::credentials::Credentials;
use subspace_core::search::SearchIndex;

/// How long a toast stays: longer for an error or one with a link, which
/// the user may want to act on.
const SHORT_TOAST: Duration = Duration::from_secs(4);
const LONG_TOAST: Duration = Duration::from_secs(8);

pub use artwork::BoardArtwork;
pub use chat_input::{ChatInput, ChatInputEvent};
pub use density::{Density, DensityPreview};
pub use list::{Body, Detail, Glyph, Group, ListDashboard, ListState, Row, Section, ServiceLink};
pub use product::CurrentProduct;
pub use row_menu::RowMenu;
pub use subspace_agent_model::ContextRef as AgentContext;
pub use subspace_agent_tools::{AgentTool, Catalog, HelpImage, ToolEnvironment};
pub use subspace_core::search::SearchDoc;
pub use tokens::{ThemeId, ThemeTones, Tokens, Tone};
pub use typography::{Typography, heading_font};

actions!(
    subspace,
    [
        /// Open connection settings for the visible dashboard.
        ConfigureBoard,
        /// Select the next visible row or group header.
        SelectNext,
        /// Select the previous visible row or group header.
        SelectPrevious,
        /// Select the first line.
        SelectFirst,
        /// Select the last line.
        SelectLast,
        /// Open the selected row's details, or fold or unfold the selected group.
        Activate,
        /// Close the detail panel.
        CloseDetail,
        /// Add the selected row and its open details to the Copilot chat.
        AddToChat,
        /// Add the text selected in a detail pane to the Copilot chat.
        AddSelectionToChat,
    ]
);

/// Key context of a dashboard's view.
pub const DASHBOARD_CONTEXT: &str = "Dashboard";
/// Key context of menus and dialogs: shell and dashboard keys stay off while
/// one has focus (REQ-06).
pub const MODAL_CONTEXT: &str = "SubspaceModal";
/// Context owned by a dashboard that routes raw keys to its focused view.
pub const RAW_KEYS_CONTEXT: &str = "SubspaceRawKeys";

/// The key context predicate for keys without `secondary`, `ctrl` or `alt`
/// in the view named `context`: they do not act while a text field, a menu or
/// a dialog has focus (REQ-06), nor while a view that owns raw keys has it,
/// because a plain key belongs to that view instead. Dashboards bind their
/// own plain keys with `Some(&view_keys(Self::ID))`.
///
/// A dashboard key that does carry `secondary`, `ctrl` or `alt` keeps acting
/// inside a raw-key view, because the shell reserves those combinations: bind
/// it against `"{context} && !SubspaceModal"` instead.
pub fn view_keys(context: &str) -> String {
    format!("{context} && !Input && !{MODAL_CONTEXT} && !{RAW_KEYS_CONTEXT}")
}

/// App keys remain available unless a dashboard owns raw-key input.
pub fn app_keys() -> String {
    format!("!{RAW_KEYS_CONTEXT}")
}

/// The keystrokes `keys` with the secondary modifier: `cmd` on macOS and
/// `ctrl` elsewhere. `KeyBinding::new` takes only explicit modifiers, so
/// `secondary!("shift-c")` stands for the `secondary-shift-c` keymap form.
#[macro_export]
macro_rules! secondary {
    ($keys:literal) => {
        if cfg!(target_os = "macos") {
            concat!("cmd-", $keys)
        } else {
            concat!("ctrl-", $keys)
        }
    };
}

#[cfg(test)]
mod key_context_tests {
    use super::{RAW_KEYS_CONTEXT, app_keys, view_keys};

    #[test]
    fn app_keys_excludes_raw_key_context() {
        assert_eq!(RAW_KEYS_CONTEXT, "SubspaceRawKeys");
        assert_eq!(app_keys(), "!SubspaceRawKeys");
    }

    #[test]
    fn plain_view_keys_stand_down_in_a_raw_key_view() {
        assert_eq!(
            view_keys("Subspace"),
            "Subspace && !Input && !SubspaceModal && !SubspaceRawKeys"
        );
    }
}

/// The list keys every [`ListDashboard`] shares and the keys of the shared
/// components. The app binds them once.
pub fn bind_keys(cx: &mut App) {
    let context = view_keys(DASHBOARD_CONTEXT);
    let context = Some(context.as_str());
    cx.bind_keys([
        KeyBinding::new("j", SelectNext, context),
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("k", SelectPrevious, context),
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("home", SelectFirst, context),
        KeyBinding::new("end", SelectLast, context),
        KeyBinding::new("enter", Activate, context),
        KeyBinding::new("escape", CloseDetail, context),
        KeyBinding::new("escape", find_in_page::Close, context),
    ]);
    menu::bind_keys(cx);
    composer::bind_keys(cx);
    chat_input::bind_keys(cx);
    panel::bind_keys(cx);
}

/// Help rows for the list keys, before a dashboard's own keys.
pub const LIST_KEYS: &[KeyHelp] = &[
    KeyHelp::new("j / ↓", "Next row"),
    KeyHelp::new("k / ↑", "Previous row"),
    KeyHelp::new("Enter", "Open details, or fold or unfold a group"),
    KeyHelp::new("Esc", "Close details"),
];

/// One row of the help overlay: a key and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHelp {
    pub key: &'static str,
    pub meaning: &'static str,
}

impl KeyHelp {
    pub const fn new(key: &'static str, meaning: &'static str) -> Self {
        Self { key, meaning }
    }
}

// ---- Events: dashboard to shell -------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastTone {
    #[default]
    Info,
    Error,
}

/// A link in a toast. Only `https` links show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastLink {
    pub href: SharedString,
    pub label: SharedString,
}

/// A short message at the bottom left (REQ-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub text: SharedString,
    pub tone: ToastTone,
    pub link: Option<ToastLink>,
}

impl Toast {
    pub fn info(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            tone: ToastTone::Info,
            link: None,
        }
    }

    pub fn error(text: impl Into<SharedString>) -> Self {
        Self {
            tone: ToastTone::Error,
            ..Self::info(text)
        }
    }

    pub fn link(mut self, href: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        self.link = Some(ToastLink {
            href: href.into(),
            label: label.into(),
        });
        self
    }

    /// 4 s for information, 8 s for an error or a toast with a link.
    pub fn duration(&self) -> Duration {
        match self.tone == ToastTone::Error || self.link.is_some() {
            true => LONG_TOAST,
            false => SHORT_TOAST,
        }
    }
}

/// Which palette to open: commands then items (⌘K), or items only (`/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Commands,
    Items,
}

/// A cloud agent task to replay in a chat tab. The dashboard that lists the
/// tasks knows the id, the name and the page; the chat pane connects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudSession {
    /// The GitHub cloud agent task id.
    pub task_id: SharedString,
    /// The task's name, shown until the session reports its own.
    pub title: SharedString,
    /// The task's session page on GitHub, empty when the row has none.
    pub url: SharedString,
}

/// What a dashboard asks of the Copilot chat pane the shell owns. The chat
/// pane holds the one agent runtime, so a dashboard never touches it
/// directly; it reads [`chat_sessions::sessions`] and asks with this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRequest {
    /// Ask the runtime for a fresh saved-session list.
    ListSessions,
    /// Show the saved session in a chat tab, opening the pane when closed.
    OpenSession(SharedString),
    /// Replay a cloud agent task's last session in a read-only chat tab.
    OpenCloudSession(CloudSession),
    /// Open a validated local search citation in the visible Subspace board.
    OpenInSubspace(SharedString),
    /// Refresh the board currently visible beside chat.
    RefreshVisibleBoard,
}

/// What a dashboard asks of the shell. Emit with `cx.emit(...)`.
#[derive(Debug, Clone, PartialEq)]
pub enum DashboardEvent {
    Toast(Toast),
    OpenPalette(PaletteMode),
    /// An item's details opened; the shell adds it to the recent items of
    /// the item search. The value is the item's [`SearchDoc::id`].
    Opened(SharedString),
    /// Show the board that answers a contract and open a link it owns.
    Navigate(BoardNavigation),
    /// Return to a view the person came from, as an issue page's Back does.
    /// The shell shows `board` and asks it to restore `location`. When
    /// either no longer exists, the shell stays on the requesting board and
    /// says so, so the requester shows its own fallback first.
    Return(ReturnPoint),
    /// Something the Copilot chat pane should do.
    Chat(ChatRequest),
    /// The dashboard wrote the config file through a checked immediate edit
    /// and updated [`Services::config`], as [`repositories::apply`] does. The
    /// shell rebuilds the agent tools and the MCP catalog from it.
    ConfigChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardNavigation {
    pub responder: contracts::MessageKey,
    pub url: SharedString,
    /// The place in the requesting board the link was followed from, such
    /// as the channel whose issue chip was clicked. A Back on the
    /// destination returns there. `None` uses the requester's
    /// [`Dashboard::return_location`] at the time of the request.
    pub origin: Option<ReturnLocation>,
}

/// A view the person was on: a board and, optionally, a place in it that
/// the board restores itself. The shell records one each time it changes the
/// current board, the *last view*, and gives it to the board it shows with
/// [`Dashboard::arrived_from`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnPoint {
    /// The board's id.
    pub board: &'static str,
    /// What Back names, such as `#discovery` or `PRs`.
    pub label: SharedString,
    /// The board's own key for the place, from
    /// [`Dashboard::return_location`]; `None` returns to the board as it is.
    pub location: Option<SharedString>,
}

impl ReturnPoint {
    /// The text of a Back control that returns here, such as
    /// *Back to #discovery*.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn back_label(&self) -> SharedString {
        back_label(&self.label)
    }
}

/// The longest destination a Back label shows before it is shortened.
const BACK_LABEL_CHARS: usize = 32;

/// *Back to* `destination`, shortened with an ellipsis when it is long.
// qual:api - Consumed by app crates through the Framework facade.
pub fn back_label(destination: &str) -> SharedString {
    let mut chars = destination.chars();
    let short: String = chars.by_ref().take(BACK_LABEL_CHARS).collect();
    let ellipsis = if chars.next().is_some() { "…" } else { "" };
    format!("Back to {short}{ellipsis}").into()
}

/// A place in a board that a later Back can return to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnLocation {
    /// What Back names, such as `#discovery`.
    pub label: SharedString,
    /// The board's own key, given back to [`Dashboard::return_to`]. It is
    /// opaque to the shell and other boards.
    pub key: SharedString,
}

/// While a dashboard shows a menu, keys go to the menu and ⌘K closes it
/// first; while it shows a dialog, keys go to the dialog only (REQ-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    Menu,
    Dialog,
}

// ---- Palette commands -------------------------------------------------------------

mod palette;

pub use palette::{
    Callback, CommandRun, ItemSearch, PaletteCommand, PaletteOption, PickCallback, Search, SubList,
};

pub use subspace_search as full_text;
pub mod log_view;
pub mod segmented;

/// The shared persistent search service, without owning a board's data model.
pub fn full_text_service(cx: &App) -> Option<full_text::Service> {
    services(cx).and_then(|services| services.search.clone())
}

/// Internal search citations are routed in-app and never sent to the OS URL opener.
pub struct SearchNavigation(pub std::rc::Rc<dyn Fn(full_text::Target, &mut Window, &mut App)>);
impl Global for SearchNavigation {}

// ---- Globals the shell sets -------------------------------------------------------

/// The app's wrap toggle (REQ-09): wrapped titles when true, else one line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wrap(pub bool);

impl Global for Wrap {}

pub fn wrap(cx: &App) -> bool {
    cx.try_global::<Wrap>().is_some_and(|wrap| wrap.0)
}

/// Text selected in a detail pane, held between the pane's context menu and
/// the shell's [`AddSelectionToChat`] handler, which takes it.
#[derive(Debug, Clone, Default)]
pub struct Selection(pub String);

impl Global for Selection {}

pub fn set_selection(cx: &mut App, text: String) {
    cx.set_global(Selection(text));
}

/// The held selection, leaving none behind.
pub fn take_selection(cx: &mut App) -> String {
    let text = cx.try_global::<Selection>().map(|held| held.0.clone());
    cx.set_global(Selection::default());
    text.unwrap_or_default()
}

/// The config file and the credential sources, loaded once at startup.
/// Clients take `Arc` clones into background tasks. `tools` is the catalog
/// of the selected dashboards, which Copilot chat registers.
pub struct Services {
    pub config: Arc<Config>,
    pub credentials: Arc<Credentials>,
    pub tools: Catalog,
    pub search: Option<full_text::Service>,
    /// The config file the settings were read from.
    pub config_file: std::path::PathBuf,
}

impl Global for Services {}

impl Services {
    /// Native board `id`'s own settings file, beside the config file.
    // qual:api
    pub fn board_config_file(&self, id: &str) -> std::path::PathBuf {
        subspace_core::config::board_file(&self.config_file, id)
    }
}

pub fn services(cx: &App) -> Option<&Services> {
    cx.try_global::<Services>()
}

// ---- The dashboard trait ----------------------------------------------------------

/// A dashboard: one view in the app's switcher.
///
/// The app creates each enabled dashboard at startup, or when Settings
/// enables it, and keeps it while it stays enabled and the window is open,
/// so a dashboard keeps its state across switches. View work
/// starts in [`Dashboard::activate`]; process-owned monitoring starts in
/// [`Dashboard::start_background`]. Run blocking calls with `cx.background_spawn`.
pub trait Dashboard: Render + Focusable + EventEmitter<DashboardEvent> {
    /// Stable identifier, such as `board`. Lower case, no spaces. Also the
    /// view's key context and the prefix of its stored choices.
    const ID: &'static str;
    /// The name in the dashboard switcher and the window title.
    const TITLE: &'static str;
    /// A concise capability description used by navigation tooltips.
    const DESCRIPTION: &'static str = Self::TITLE;
    /// Optional monochrome SVG asset shown before the switcher label.
    const ICON: Option<&'static str> = None;
    /// Embedded artwork owned by this board, captured without creating a view.
    const ARTWORK: BoardArtwork = BoardArtwork::new(&[]);
    /// The board's own help, compiled in with `include_str!` from a Markdown
    /// file in the board crate. The shell renders it and the `board_help`
    /// agent tool returns the same text. Empty means the board has no help
    /// yet, and then neither the shell nor the tool offers one.
    const HELP_MARKDOWN: &'static str = "";
    /// At most three bundled PNG/WebP illustrations, each at most 512 KiB.
    /// Reference an id in help with `![caption](subspace-help-image:id)`.
    const HELP_IMAGES: &'static [HelpImage] = &[];
    /// Explicit adoption of the Framework's persistent full-text index.
    const SEARCHABLE: bool = false;
    /// Keep the view alive, without a tab or background refresh, when
    /// Settings disables the board while the app runs. Set it for a board
    /// that owns processes, such as shells or tunnels, that must not end
    /// silently; enabling the board again shows the same view.
    const RETAIN_WHEN_DISABLED: bool = false;

    const FIND_VIRTUAL_CONTENT: bool = false;

    /// Optional cheap revision for provider refresh; None follows every owner notification.
    fn find_revision(&self, _cx: &App) -> Option<u64> {
        None
    }

    /// Whether the current view uses loaded virtual documents rather than rendered text.
    fn find_virtual_content(&self, _cx: &App) -> bool {
        Self::FIND_VIRTUAL_CONTENT
    }

    /// Loaded virtual text for local find; ordinary shared text registers itself.
    fn find_documents(&self, _cx: &App) -> (SharedString, Vec<find_in_page::Document>) {
        (SharedString::default(), Vec::new())
    }
    fn reveal_find_document(&mut self, _id: &str, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Safe data-only refresh. Implementations must not activate views, start
    /// runtimes or request interactive credentials.
    fn background_refresh(&mut self, _cx: &mut Context<Self>) {}

    /// Static settings pages and feature declarations owned by this board.
    /// Registration calls this without constructing the dashboard.
    fn settings() -> settings::BoardSettings
    where
        Self: Sized,
    {
        settings::BoardSettings::default()
    }

    /// Board-owned settings controls, retained within the outer Save/Cancel draft.
    fn settings_editor() -> Option<settings::SettingsEditorFactory>
    where
        Self: Sized,
    {
        None
    }

    /// Requirements for the shell-owned shared service editor. Registration
    /// captures these without constructing a board or loading credentials.
    fn connection_settings() -> settings::ConnectionSettings
    where
        Self: Sized,
    {
        settings::ConnectionSettings::default()
    }

    /// The contract messages this board publishes, subscribes to, requests
    /// and answers. Registration captures them without constructing the
    /// board; the broker routes nothing else (see [`contracts`]).
    fn contracts() -> contracts::BoardContracts
    where
        Self: Sized,
    {
        contracts::BoardContracts::default()
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self
    where
        Self: Sized;

    /// Starts process-owned GUI work after services and notification delivery exist,
    /// at startup and again when Settings enables the board after a stop.
    /// Headless tool registration never calls this hook.
    fn start_background(_cx: &mut App)
    where
        Self: Sized,
    {
    }

    /// Stops the work [`Dashboard::start_background`] started, when Settings
    /// disables the board while the app runs. A later start must work again.
    fn stop_background(_cx: &mut App)
    where
        Self: Sized,
    {
    }

    /// Resolve a process-local notification target against current account/access state.
    /// Called before activation on notification-driven window creation.
    fn open_notification(&mut self, _target: u64, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// The agent tools this dashboard advertises. Copilot chat and the MCP
    /// server register them when the product selects the dashboard. Called
    /// without a window, so it must not touch GPUI.
    fn agent_tools(_environment: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>>
    where
        Self: Sized,
    {
        Vec::new()
    }

    /// Bind this dashboard's own keys, in the context `view_keys(Self::ID)`.
    /// Runs once per process, when the app first creates the dashboard.
    fn bind_keys(_cx: &mut App)
    where
        Self: Sized,
    {
    }

    /// Called each time the dashboard becomes the visible one.
    fn activate(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Called when another dashboard becomes the visible one, such as to
    /// stop a timer or close a menu.
    fn deactivate(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Called when a shell overlay, such as the palette, help or a dialog,
    /// starts or stops covering the board area. GPUI draws overlays above
    /// the board, but a native child view, such as a web view, draws above
    /// GPUI, so a board that owns one hides it while `covered` is true.
    fn covered(&mut self, _covered: bool, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Optional context appended after the registered board title by the shell.
    /// `None` gives the board name without context.
    fn window_title(&self, _cx: &App) -> Option<SharedString> {
        None
    }

    /// Active configuration source offered only for explicit per-report path
    /// consent. Return a path, never file contents, URLs or credential values.
    /// The shell redacts it before the form can display or include it.
    fn diagnostic_config_path(&self, _cx: &App) -> Option<std::path::PathBuf> {
        None
    }

    /// Controls in the toolbar after the dashboard switcher, such as the
    /// sprint picker.
    fn render_toolbar(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        None
    }

    /// Short status for the toolbar, such as item counts.
    fn status(&self, _cx: &App) -> Option<SharedString> {
        None
    }

    /// The docks this Space has on the workspace shell, and whether each
    /// shows now. The shell's status bar draws a switch for each dock
    /// reported: the navigator's at its leading end, the inspector's at its
    /// trailing end. The default, [`workspace::SpaceDocks::NONE`], draws none.
    fn docks(&self, _cx: &App) -> workspace::SpaceDocks {
        workspace::SpaceDocks::NONE
    }

    /// Show or hide `dock`: its status bar switch was used. Keep the choice
    /// with the Space's other stored choices and notify. The default does
    /// nothing.
    fn toggle_dock(
        &mut self,
        _dock: workspace::SpaceDock,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// A short mark on this dashboard's switcher tab, such as a count of
    /// running work or `!` for something that waits. `None`, the default,
    /// leaves the tab plain.
    fn badge(&self, _cx: &App) -> Option<SharedString> {
        None
    }

    /// True while the dashboard loads: the reload button shows it.
    fn busy(&self, _cx: &App) -> bool {
        false
    }

    /// The selected row, which item commands act on: the palette's header.
    fn subject(&self, _cx: &App) -> Option<Row> {
        None
    }

    /// Selection identity for agent queries. Do not export credentials or secret
    /// finding previews. Specific dashboards add service identifiers as needed.
    fn agent_context(&self, cx: &App) -> Option<AgentContext> {
        let row = self.subject(cx)?;
        Some(AgentContext {
            key: format!("{}:{}", Self::ID, row.key),
            label: format!("{}: {}", Self::TITLE, row.title),
            content: format!(
                "Dashboard: {}\nItem key: {}\nTitle: {}",
                Self::ID,
                row.key,
                row.title
            ),
        })
    }

    /// Commands for the palette, in order. The app adds its own after them.
    fn commands(&self, _cx: &Context<Self>) -> Vec<PaletteCommand> {
        Vec::new()
    }

    /// The item search for `/` and the palette; `None` when there is
    /// nothing to search.
    fn search(&self, _cx: &App) -> Option<Search> {
        None
    }

    /// Reveal the item with document id `id`, select it and open its details.
    fn open_search_result(&mut self, _id: &str, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Open a link owned by this board, if it can be read in its current scope.
    fn open_link(&mut self, _url: &str, _cx: &mut Context<Self>) -> bool {
        false
    }

    /// The place the person is looking at, such as the channel they read,
    /// for a Back on another board that returns here. `None`, the default,
    /// returns to the board as it is.
    fn return_location(&self, _cx: &App) -> Option<ReturnLocation> {
        None
    }

    /// The shell is about to make this board current, coming from `from`:
    /// the board view shown before, or `None` when no board was showing. A
    /// link the shell opens next ([`Dashboard::open_link`]) came from there.
    fn arrived_from(&mut self, _from: Option<ReturnPoint>, _cx: &mut Context<Self>) {}

    /// Show a place this board gave as [`Dashboard::return_location`].
    /// False when it no longer exists, such as a channel left since.
    fn return_to(&mut self, _key: &str, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        false
    }

    /// Open a validated retained search hit through board-owned navigation.
    fn open_indexed_result(
        &mut self,
        hit: &full_text::Hit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_search_result(&hit.document.navigation, window, cx);
    }

    /// Retained views keep the permit and validate it at presentation boundaries.
    fn open_indexed_result_fenced(
        &mut self,
        hit: &full_text::Hit,
        permit: full_text::ReadPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if permit.valid() {
            self.open_indexed_result(hit, window, cx);
        }
    }

    /// Whether a menu or a dialog of the dashboard is open.
    fn modal(&self, _cx: &App) -> Option<Modal> {
        None
    }

    /// Close the dashboard's menu; the shell calls this before it opens the
    /// palette.
    fn dismiss(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Help rows for this dashboard's keys.
    fn keys(&self) -> &'static [KeyHelp] {
        LIST_KEYS
    }

    /// A note under the help's key list.
    fn help_foot(&self) -> Option<SharedString> {
        None
    }

    /// Programs currently running in this dashboard, shown before quitting.
    fn quit_warnings(&self, _cx: &App) -> Vec<SharedString> {
        Vec::new()
    }

    /// Load the dashboard's data again.
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>);
}

mod handle;

pub use handle::{DashboardHandle, EventSink};
