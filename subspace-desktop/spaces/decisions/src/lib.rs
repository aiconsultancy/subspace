//! The Decisions Space: Subspace API decisions in one searchable list,
//! grouped by status, with authoring, review and immutable revisions.

mod account;
mod author;
mod board;
mod docks;
mod history;
mod initiative;
mod inspector;
mod keys;
mod lanes;
mod listing;
mod load;
mod menus;
mod model;
mod navigator;
mod open_issue;
mod page;
mod palette;
mod peek;
mod render;
mod search;
mod state;
mod toolbar;
mod words;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

use std::time::Instant;

use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels, SharedString,
    Subscription, Task, WeakEntity, Window,
};
use subspace_framework::dashboard::contracts::BoardContracts;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::settings::BoardSettings;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{
    AgentContext, BoardArtwork, Dashboard, DashboardEvent, Detail, Group, KeyHelp, ListDashboard,
    ListState, Modal, PaletteCommand, ReturnLocation, ReturnPoint, Row, RowMenu,
};
use subspace_framework::lucide::IconName;

/// The Space's id, also its key context and the prefix of its preferences.
pub(crate) const ID: &str = "decisions";
/// The switcher glyph, Lucide `scale`, under the Space's own key so it
/// never shadows GPUI Kit's copy.
const ICON_PATH: &str = "icons/decisions.svg";

pub struct Decisions {
    weak: WeakEntity<Self>,
    focus: FocusHandle,
    list: ListState,
    choice: model::Choice,
    search: Entity<InputState>,
    /// The search text the list was read with.
    query: String,
    docks: docks::Docks,
    session: state::Session,
    initiatives: Entity<subspace_api_client::initiatives::Catalog>,
    initiative_id: Option<String>,
    listing: state::Listing,
    /// The decision opened as a page, which keeps its tab until closed.
    page: Option<page::Page>,
    arrival: Option<ReturnPoint>,
    author: Option<author::Editor>,
    history: history::History,
    #[cfg(test)]
    test_writer: Option<author::TestWriter>,
    /// Whether the pane shows the page rather than the list.
    showing_page: bool,
    reader: load::Reader,
    /// Each list read's number: an answer to an older read is dropped.
    reads: u64,
    /// Ends the latest list read early once a newer read supersedes it.
    stop: load::Stop,
    /// Each page read's number, as `reads` is for the list.
    page_reads: u64,
    /// The account and organisation what the Space holds was read for.
    lease: Option<load::Lease>,
    /// Counts account changes; a return location from before one no
    /// longer returns.
    account_epoch: u64,
    /// Whether the Space shows, so an account change reads the new list.
    active: bool,
    /// An account change left the search field and a menu to clear.
    tidy: bool,
    folds: board::Folds,
    /// The text edge of the last frame, which the shared list asks for.
    text_edge: Pixels,
    menu_anchors: MenuAnchors,
    menu: Option<Entity<Menu>>,
    _read: Option<Task<()>>,
    _page_read: Option<Task<()>>,
    _search_task: Option<Task<()>>,
    _watch: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DashboardEvent> for Decisions {}

impl Focusable for Decisions {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Dashboard for Decisions {
    const ID: &'static str = ID;
    const TITLE: &'static str = "Decisions";
    const DESCRIPTION: &'static str = "Subspace API decisions";
    const ICON: Option<&'static str> = Some(ICON_PATH);
    const ARTWORK: BoardArtwork =
        BoardArtwork::new(&[(ICON_PATH, include_bytes!("../assets/decisions.svg"))]).with_order(14);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> BoardSettings {
        BoardSettings {
            icon: Some(IconName::Scale),
            description: "Record, review and browse decisions, with their evidence and revision history.",
            notifications: false,
            background_updates: false,
            ..Default::default()
        }
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        board::new(window, cx)
    }

    fn contracts() -> BoardContracts {
        open_issue::CONTRACTS
    }

    fn bind_keys(cx: &mut App) {
        keys::bind(cx);
        author::bind_keys(cx);
    }

    /// Reads start here: the list is read again when it is over a minute
    /// old, or was read for another account.
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = true;
        #[cfg(not(test))]
        self.initiatives
            .update(cx, subspace_api_client::initiatives::Catalog::refresh);
        self.follow_account(cx);
        self.tidy(window, cx);
        self.read_if_stale(cx);
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = false;
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
    }

    fn commands(&self, _cx: &Context<Self>) -> Vec<PaletteCommand> {
        palette::commands(self)
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        Some(self.status_line(Instant::now()).into())
    }

    fn docks(&self, _cx: &App) -> SpaceDocks {
        self.space_docks()
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_space_dock(dock, cx);
    }

    fn busy(&self, _cx: &App) -> bool {
        self.listing.loading() || self.page_loading()
    }

    /// Decision content stays out of the chat, as issue content does.
    fn agent_context(&self, _cx: &App) -> Option<AgentContext> {
        None
    }

    fn arrived_from(&mut self, from: Option<ReturnPoint>, _cx: &mut Context<Self>) {
        self.arrival = from;
    }

    fn open_link(&mut self, url: &str, cx: &mut Context<Self>) -> bool {
        self.open_decision_link(url, cx)
    }

    fn return_location(&self, _cx: &App) -> Option<ReturnLocation> {
        page::return_location(self)
    }

    fn return_to(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.follow_account(cx);
        self.tidy(window, cx);
        self.return_to_place(key, cx)
    }

    fn modal(&self, _cx: &App) -> Option<Modal> {
        if self.author.is_some() {
            return Some(Modal::Dialog);
        }
        self.menu.is_some().then_some(Modal::Menu)
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.author.is_some() {
            self.close_author(window, cx);
            return;
        }
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
    }

    fn keys(&self) -> &'static [KeyHelp] {
        keys::HELP
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.follow_account(cx);
        self.tidy(window, cx);
        self.read(true, cx);
        if self.shown_page().is_some() {
            self.read_page(cx);
        }
    }
}

impl ListDashboard for Decisions {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    /// A decision opens as a page in the pane, not in the list's panel.
    fn detail(&self, _key: &str) -> Option<Detail> {
        None
    }

    fn list_side_inset(&self) -> Option<Pixels> {
        Some(self.text_edge)
    }

    fn group_count_after_name(&self) -> bool {
        true
    }

    fn comfortable_rows(&self) -> bool {
        true
    }

    fn row_trailing(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        lanes::trailing(self, row, cx)
    }

    fn group_leading(&self, group: &Group, cx: &App) -> Option<AnyElement> {
        lanes::group_mark(group, cx)
    }

    fn row_accessible_label(&self, row: &Row, _cx: &App) -> SharedString {
        self.row_label(row)
    }

    /// The row menu opens the same page as Enter and copies a reference
    /// when the API supplies one.
    fn row_menu(&self, row: &Row) -> RowMenu {
        let id = listing::decision_id(&row.key)
            .and_then(|id| self.item(id))
            .and_then(|item| item.identifier.as_deref())
            .map(SharedString::from);
        RowMenu {
            id,
            chat: MenuChat::Off,
            ..RowMenu::from_row(row)
        }
    }

    fn menu_chat(&self) -> MenuChat {
        MenuChat::Off
    }

    fn empty_message(&self) -> (SharedString, SharedString) {
        let (title, hint) = words::empty(&self.choice, &self.query);
        (title.into(), hint.into())
    }

    /// The Space has no settings to configure; Reload reads again.
    fn empty_actions(&self, _cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        Some(Vec::new())
    }

    /// One click selects a decision for the inspector; a double click or
    /// Enter opens it.
    fn activates_on_click(&self, row: &Row) -> bool {
        listing::decision_id(&row.key).is_none()
    }

    fn on_activate(&mut self, key: &SharedString, cx: &mut Context<Self>) -> bool {
        if self.follow_account(cx) {
            return true;
        }
        let Some(id) = listing::decision_id(key) else {
            return false;
        };
        self.open_decision(id, cx);
        true
    }
}
