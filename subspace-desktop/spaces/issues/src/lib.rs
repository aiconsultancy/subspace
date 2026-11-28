//! Subspace API issues and text discussions, kept in memory.
mod activity;
mod activity_words;
mod authoring;
mod back;
mod board;
mod detail;
mod docks;
mod editing;
mod filter_menu;
mod filters;
#[cfg(debug_assertions)]
mod fixture;
mod initiative;
mod keys;
mod lines;
mod link;
mod listing;
pub mod markdown;
mod media;
mod model;
mod navigation;
mod navigator;
mod organisation;
mod palette;
mod peek;
mod pickers;
mod project_move;
mod requests;
mod responder;
mod scope;
mod seam;
mod settings;
mod sync;
mod view;
mod view_menu;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Task, WeakEntity, Window,
};
use subspace_api_client::hub::Hub;
use subspace_api_client::model::Project;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::settings::{BoardSettings, SettingsEditorFactory};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{
    AgentContext, BoardArtwork, Dashboard, DashboardEvent, Detail, Group, KeyHelp, ListDashboard,
    ListState, Modal, Row, RowMenu,
};
use subspace_framework::lucide::IconName;

use crate::lines::Folds;
use crate::listing::Listing;
use crate::model::{ProjectScope, SubIssues, Tab};
use crate::requests::Request;
use crate::scope::Access;

/// The board id, also its key context and the prefix of its preferences.
pub(crate) const ID: &str = "issues";

mod decision;

pub struct Issues {
    authoring: authoring::AuthoringView,
    /// Where the issue page's Back goes.
    back: back::Back,
    editor: Option<editing::IssueEditor>,
    project_move: Option<project_move::ProjectMove>,
    media: media::MediaView,
    decision: Option<decision::DecisionPanel>,
    weak: WeakEntity<Self>,
    focus: FocusHandle,
    list: ListState,
    activity: activity::Feed,
    issue_view: detail::IssueView,
    /// The fixed Subspace API scope id.
    profile: String,
    access: Access,
    /// Projects, archived ones included.
    projects: Vec<Project>,
    initiatives: Entity<subspace_api_client::initiatives::Catalog>,
    initiative_id: Option<String>,
    /// Whether the person belongs to no initiative where the API scopes the
    /// tracker by them: Issues shows why, and no projects.
    not_a_member: bool,
    tab: Tab,
    project: ProjectScope,
    sub_issues: SubIssues,
    filters: filters::Filters,
    search: Entity<InputState>,
    /// The search text the list was asked for.
    query: String,
    search_fallback: board::SearchFallback,
    listing: Listing,
    /// Each issue row's group and position in the listing.
    index: HashMap<SharedString, (usize, usize)>,
    folds: Folds,
    search_folds: HashMap<String, bool>,
    search_pages: HashMap<String, usize>,
    /// What the board asked Subspace API for, oldest first, until a hub takes it.
    requests: Vec<Request>,
    hub: Option<Hub>,
    /// Which start of the hub the board follows.
    hub_id: Option<u64>,
    /// The hub snapshot version last applied.
    seen: Option<u64>,
    /// Applying a snapshot, which may ask for reads in turn.
    syncing: bool,
    /// The selected line while the board is hidden and holds no rows,
    /// selected again when the hub's snapshot rebuilds them.
    hidden_selection: Option<SharedString>,
    /// The hub's scope epoch last seen: a change reads initiatives again.
    scope_epoch: u64,
    /// Why the last attachment could not be added or saved.
    attachment_error: Option<String>,
    /// How changes made elsewhere reach the board.
    live: subspace_api_client::hub::LiveStatus,
    /// The hub's name for the profile, such as *Subspace API fixture*.
    profile_label: Option<String>,
    /// Follows the retained hub's timer while the board shows.
    poll: Option<Subscription>,
    hub_changes: Option<Subscription>,
    menu_anchors: MenuAnchors,
    menu: Option<Entity<Menu>>,
    /// Whether the navigator and the inspector are chosen to show.
    docks: docks::Docks,
    /// The window's width at the last render, which decides which docks
    /// fit, and so the pane's width.
    width: f32,
    /// The workspace's text edge at the last render. The shared list asks
    /// for its inset without the tokens, so it reads this.
    text_edge: gpui_kit::Pixels,
    _search_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DashboardEvent> for Issues {}

impl Focusable for Issues {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Dashboard for Issues {
    const ID: &'static str = ID;
    const TITLE: &'static str = "Issues";
    const DESCRIPTION: &'static str = "Subspace API issues and comments";
    /// Lucide `circle-dot`, under the board's own key so it never shadows
    /// GPUI Kit's copy.
    const ICON: Option<&'static str> = Some("icons/issues.svg");
    const ARTWORK: BoardArtwork =
        BoardArtwork::new(&[("icons/issues.svg", include_bytes!("../assets/issues.svg"))])
            .with_order(6);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn find_revision(&self, _: &App) -> Option<u64> {
        use std::hash::{Hash as _, Hasher as _};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.profile.hash(&mut hash);
        self.issue_view
            .snapshot
            .as_ref()
            .map(|issue| issue.identifier())
            .hash(&mut hash);
        Some(hash.finish())
    }

    fn find_documents(
        &self,
        _: &App,
    ) -> (
        gpui_kit::SharedString,
        Vec<subspace_framework::dashboard::find_in_page::Document>,
    ) {
        (
            format!(
                "{:?}:{}",
                self.profile,
                self.issue_view
                    .snapshot
                    .as_ref()
                    .map_or("list", |issue| issue.identifier())
            )
            .into(),
            Vec::new(),
        )
    }

    fn settings() -> BoardSettings {
        BoardSettings {
            icon: Some(IconName::CircleDot),
            description: "Read Subspace API issues and post text comments and replies.",
            notifications: false,
            background_updates: false,
            ..Default::default()
        }
    }

    fn quit_warnings(&self, cx: &App) -> Vec<gpui_kit::SharedString> {
        let mut warnings = Vec::new();
        if self.hub.as_ref().is_some_and(|h| h.has_comment_drafts()) {
            warnings.push("Unsent or unconfirmed issue comment drafts will be discarded.".into());
        }

        if self.issue_editor_dirty(cx) {
            warnings.push("Unsaved or unconfirmed issue edits will be discarded.".into());
        }
        warnings
    }

    fn settings_editor() -> Option<SettingsEditorFactory> {
        Some(settings::factory)
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        board::new(window, cx)
    }

    fn contracts() -> subspace_framework::dashboard::contracts::BoardContracts {
        responder::DECLARATIONS
    }

    /// One retained hub: answers and announces references through the broker.
    fn start_background(cx: &mut App) {
        link::start(cx);
    }

    /// Stops reads and polls and clears everything the hub read.
    fn stop_background(cx: &mut App) {
        link::stop(cx);
    }

    fn bind_keys(cx: &mut App) {
        keys::bind(cx);
        editing::view::bind(cx);
        media::bind_keys(cx);
    }

    fn open_link(&mut self, url: &str, cx: &mut Context<Self>) -> bool {
        self.open_issue_link(url, cx)
    }

    fn arrived_from(
        &mut self,
        from: Option<subspace_framework::dashboard::ReturnPoint>,
        _cx: &mut Context<Self>,
    ) {
        self.back.arrived(from);
    }

    /// Loads start here: the hub's snapshot rebuilds the list at once, and
    /// the hub reads it when it is new, changed or over a minute old.
    fn activate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // A link opened on the way in has taken the arrival already.
        self.back.arrived(None);
        #[cfg(not(test))]
        initiative::refresh(self, cx);
        self.poll_hub(cx);
        self.restore_editor_issue(cx);
        self.restore_selection();
        self.read_if_stale(cx);
        self.start_polling(cx);
    }

    /// Hidden, the board stops checking the hub and holds no issue content.
    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_polling();
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        self.hide_content(cx);
    }

    fn window_title(&self, _cx: &App) -> Option<SharedString> {
        Some(self.profile_name().to_owned().into())
    }

    fn commands(&self, cx: &Context<Self>) -> Vec<subspace_framework::dashboard::PaletteCommand> {
        let mut commands = self.palette_commands();
        commands.extend(initiative::command(self, cx));
        commands
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        Some(self.status_line(std::time::Instant::now()).into())
    }

    fn docks(&self, cx: &App) -> SpaceDocks {
        self.space_docks(cx)
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        Issues::toggle_dock(self, dock.into(), cx);
    }

    fn busy(&self, _cx: &App) -> bool {
        if self.issue_editor_busy() {
            return true;
        }
        if self.issue_view.snapshot.is_none() && self.activity.visible {
            return self.activity.busy();
        }
        self.issue_view
            .snapshot
            .as_ref()
            .map_or(self.listing.status == listing::Status::Loading, |issue| {
                *issue.status() == subspace_api_client::hub::IssueStatus::Loading
            })
    }

    /// Issue content never reaches chat (spec section 8).
    fn agent_context(&self, _cx: &App) -> Option<AgentContext> {
        None
    }

    fn modal(&self, cx: &App) -> Option<Modal> {
        if self.media.is_open() || self.decision.is_some() {
            return Some(Modal::Dialog);
        }
        (self.menu.is_some()
            || self
                .issue_view
                .tracker_help
                .as_ref()
                .is_some_and(|a| a.read(cx).is_open()))
        .then_some(Modal::Menu)
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.media.is_open() {
            self.close_image(window, cx);
            return;
        }
        if self.decision.is_some()
            && (self.close_text(window, cx) || decision::close(self, window, cx))
        {
            return;
        }
        self.menu_anchors.dismiss(window, cx);
        if let Some(anchor) = &self.issue_view.tracker_help {
            anchor.update(cx, |anchor, cx| anchor.dismiss(window, cx));
        }
        self.menu = None;
    }

    fn keys(&self) -> &'static [KeyHelp] {
        keys::HELP
    }

    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(test))]
        initiative::refresh(self, cx);
        if self.issue_view.snapshot.is_some() {
            self.reload_detail(cx);
        } else if self.activity.visible {
            self.read_activity(true, cx);
        } else {
            self.reload_list(cx);
        }
    }
}

impl ListDashboard for Issues {
    /// Rows start on the workspace's one text edge.
    fn list_side_inset(&self) -> Option<gpui_kit::Pixels> {
        Some(self.text_edge)
    }

    fn group_count_after_name(&self) -> bool {
        !self.activity.visible
    }

    fn group_heading_matches_row(&self) -> bool {
        !self.activity.visible
    }

    fn comfortable_rows(&self) -> bool {
        !self.activity.visible
    }

    fn list(&self) -> &ListState {
        if self.activity.visible {
            &self.activity.list
        } else {
            &self.list
        }
    }

    fn list_mut(&mut self) -> &mut ListState {
        if self.activity.visible {
            &mut self.activity.list
        } else {
            &mut self.list
        }
    }

    /// The issue page replaces the list instead of opening its split panel.
    fn detail(&self, _key: &str) -> Option<Detail> {
        None
    }

    fn row_leading(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        if self.activity.visible {
            return self.activity.leading(row, cx);
        }
        Some(view::lanes::leading(self, row, cx))
    }

    fn row_trailing(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        if self.activity.visible {
            return self.activity.trailing(row, cx);
        }
        view::lanes::trailing(self, row, cx)
    }

    fn group_leading(&self, group: &Group, cx: &App) -> Option<AnyElement> {
        view::lanes::group_mark(self, group, cx)
    }

    fn row_accessible_label(&self, row: &Row, _cx: &App) -> SharedString {
        if self.activity.visible {
            return self.activity.label(row);
        }
        self.row_label(row)
    }

    /// The row menu opens the same page as a click or Enter.
    fn row_menu(&self, row: &Row) -> RowMenu {
        if self.activity.visible {
            return self.activity.row_menu(row);
        }
        RowMenu {
            id: self
                .issue(&row.key)
                .map(|item| item.fields.identifier.clone().into()),
            opens_details: self.issue(&row.key).is_some(),
            chat: MenuChat::Off,
            ..RowMenu::from_row(row)
        }
    }

    /// The list's own empty state and Offline banner offer Copy only.
    fn menu_chat(&self) -> MenuChat {
        MenuChat::Off
    }

    fn empty_message(&self) -> (SharedString, SharedString) {
        if self.search_recovery().is_some() {
            return (
                "A matching issue is hidden".into(),
                "Change the filters that exclude it to see your match.".into(),
            );
        }
        if matches!(self.search_fallback, board::SearchFallback::Checking { .. }) {
            return ("Checking other filters…".into(), "".into());
        }
        view::empty_message(self.tab, &self.query, filters::is_active(&self.filters))
    }

    fn empty_actions(&self, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        use gpui_kit::IntoElement as _;
        use subspace_framework::dashboard::action::{Intent, action_button};

        let actions = self.search_recovery().map(|hidden| {
            action_button(
                "issues-search-reveal",
                Intent::Neutral,
                None,
                hidden.action(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.reveal_search(cx)))
            .into_any_element()
        });
        Some(actions.into_iter().collect())
    }

    fn on_activate(&mut self, key: &SharedString, cx: &mut Context<Self>) -> bool {
        if self.activity.visible {
            self.activate_activity_row(key, cx);
            return true;
        }
        self.activate_line(key, cx)
    }

    /// One click on an issue row only selects it, which fills the
    /// inspector's peek; a double click, Enter or the row's Open opens the
    /// page. Group headers, "more" lines and activity rows act on one click.
    fn activates_on_click(&self, row: &Row) -> bool {
        self.activity.visible || !matches!(lines::line(&row.key), Some(lines::Line::Issue(_)))
    }

    fn on_selection_changed(&mut self, selected: Option<SharedString>, cx: &mut Context<Self>) {
        if !self.activity.visible {
            self.load_more_near(selected, cx);
        }
    }
}
