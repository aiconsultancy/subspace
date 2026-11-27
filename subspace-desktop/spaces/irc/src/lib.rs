//! IRC client board in the shared workspace shell: the networks and their
//! buffers in the navigator, each opened conversation as a panel of a GPUI
//! Kit dock area in the pane (tabs and splits), and the focused channel's
//! members in the inspector.
//! The runtime (`subspace-client-irc`) owns every connection; this board
//! applies its events and sends commands.

mod agent_tools;
mod back;
mod commands;
mod compose;
mod config;
mod context;
mod conversation;
mod decision_references;
mod dock;
mod editor;
mod fleet;
mod hub;
mod initiatives;
mod issues;
mod join;
mod lines;
mod listing;
mod live;
mod managed;
mod markup;
mod members;
mod notice;
mod prompt;
mod pull_requests;
mod reactions;
mod remove;
mod reveal;
mod rows;
mod schema;
mod setup;
mod state;
mod store;
mod style;
mod text;
mod thread;
mod thread_status;
mod tiles;
mod toolbar;
mod tracker;
mod transcript;
mod uploads;
mod view;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription,
    WeakEntity, Window,
};
use subspace_client_irc::{BufferKey, Command, NetworkId, Target};
use subspace_framework::dashboard::notifications::{Mode, Preferences, Sound, Style};
use subspace_framework::dashboard::settings::{BoardSettings, SettingsEditorFactory};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{BoardArtwork, Dashboard, DashboardEvent, ListState};

/// The board id, also the notification source and the settings key.
pub(crate) const ID: &str = "irc";

pub struct Irc {
    me: WeakEntity<Self>,
    focus: FocusHandle,
    list: ListState,
    /// The buffer each row key names.
    buffers: HashMap<SharedString, BufferKey>,
    /// The channel each listed row we are not in names.
    listed: HashMap<SharedString, BufferKey>,
    /// The network each group header stands for.
    group_networks: HashMap<SharedString, NetworkId>,
    /// Each ship row's activity, for its dot.
    ships: HashMap<SharedString, rows::Activity>,
    /// Each `People` row's presence, for its mark.
    people: HashMap<SharedString, rows::Presence>,
    /// The captions inside each fleet group, by group key.
    captions: HashMap<SharedString, Vec<(SharedString, &'static str)>>,
    /// The fleets followed while the navigator or the conversation in front
    /// shows them.
    watch: live::watch::Watch,
    /// The inspector's open tab for a fleet channel: its ship or Starbase,
    /// or its people.
    inspector_tab: view::InspectorTab,
    ship_panel: view::ShipPanel,
    /// The thread chosen in each conversation, for the inspector's Thread
    /// tab.
    threads: thread::Threads,
    /// The fleet groups shown so far, so a fleet's group folds only when it
    /// first appears.
    seen_fleets: std::collections::HashSet<SharedString>,
    /// The fleet groups that took in their network's own rows at the last
    /// layout, so a group that begins to fold as the network's group did.
    absorbing: std::collections::HashSet<SharedString>,
    /// The group that held the selected row at the last layout, so a
    /// folded group the selection moves into opens.
    selection_group: Option<SharedString>,
    hub: Option<Entity<hub::Hub>>,
    hub_subscription: Option<Subscription>,
    /// The networks as last synced, for panels that read without a context.
    networks: Vec<state::Network>,
    active: bool,
    find_revision: u64,
    window_active: bool,
    /// The conversations, and whether the buffer list and the member list
    /// show beside them.
    dock: dock::IrcDock,
    join: Option<join::Dialog>,
    initiative_menu: initiatives::Picker,
    toolbar: Option<Entity<subspace_framework::dashboard::overflow_toolbar::OverflowToolbar>>,
    /// The pane toolbar's leading pickers, which overflow into a menu.
    leading: Option<Entity<subspace_framework::dashboard::overflow_toolbar::OverflowToolbar>>,
    /// A search result waiting for its line.
    reveal: Option<reveal::Pending>,
    /// The buffers `/` and the palette search, rebuilt on each sync.
    search: std::rc::Rc<subspace_framework::dashboard::ItemSearch>,
    available_height: Entity<f32>,
    /// The workspace's one text edge, where the navigator's rows start;
    /// refreshed on each render, as the theme's density may change.
    text_edge: gpui_kit::Pixels,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DashboardEvent> for Irc {}

impl Focusable for Irc {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Dashboard for Irc {
    const ID: &'static str = ID;
    const TITLE: &'static str = "Channels";
    const DESCRIPTION: &'static str = "IRC networks, channels and private messages";
    const ICON: Option<&'static str> = Some("logos/irc.svg");
    const ARTWORK: BoardArtwork =
        BoardArtwork::new(&[("logos/irc.svg", include_bytes!("../assets/logos/irc.svg"))])
            .with_order(5);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");
    const SEARCHABLE: bool = true;
    const FIND_VIRTUAL_CONTENT: bool = true;
    fn find_revision(&self, _: &App) -> Option<u64> {
        Some(self.find_revision)
    }
    fn find_virtual_content(&self, cx: &App) -> bool {
        self.dock
            .focused
            .as_ref()
            .and_then(|key| self.dock.open.get(key))
            .is_none_or(|hosted| hosted.entity.read(cx).upload_viewer.is_none())
    }

    fn find_documents(
        &self,
        cx: &App,
    ) -> (
        gpui_kit::SharedString,
        Vec<subspace_framework::dashboard::find_in_page::Document>,
    ) {
        let Some(key) = &self.dock.focused else {
            return Default::default();
        };
        let Some(hosted) = self.dock.open.get(key) else {
            return Default::default();
        };
        let view = hosted.entity.read(cx);
        if let Some(scope) = crate::uploads::viewer::find_scope(view) {
            return (scope, Vec::new());
        }
        (
            conversation::find::identity(view),
            conversation::find::documents(view, cx)
                .into_iter()
                .map(|(_, doc)| doc)
                .collect(),
        )
    }
    fn reveal_find_document(&mut self, id: &str, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(hosted) = self
            .dock
            .focused
            .as_ref()
            .and_then(|key| self.dock.open.get(key))
        {
            hosted
                .entity
                .update(cx, |view, cx| conversation::find::reveal(view, id, cx));
        }
    }

    fn settings() -> BoardSettings {
        BoardSettings {
            description: "Connects to IRC networks with IRCv3 support. You can read and send messages in channels and private conversations, and choose notifications for mentions and private messages.",
            notifications: true,
            // A mention or private message posts an OS notification with a
            // sound until the person chooses otherwise.
            notification_defaults: Preferences {
                mode: Mode::System,
                style: Style::Standard,
                sound: Sound::Glass,
            },
            // Messages are kept on this computer, restored on launch and
            // searchable, until the person turns Cache Channels off in Search.
            store_by_default: true,
            ..Default::default()
        }
    }

    fn settings_editor() -> Option<SettingsEditorFactory> {
        Some(editor::factory)
    }

    fn agent_tools(
        environment: &subspace_framework::dashboard::ToolEnvironment,
    ) -> Vec<std::sync::Arc<dyn subspace_framework::dashboard::AgentTool>> {
        agent_tools::agent_tools(environment)
    }

    fn contracts() -> subspace_framework::dashboard::contracts::BoardContracts {
        issues::CONTRACTS
    }

    fn start_background(cx: &mut App) {
        hub::start(cx);
    }

    fn stop_background(cx: &mut App) {
        hub::stop(cx);
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        view::new(window, cx)
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = true;
        self.find_revision = self.find_revision.wrapping_add(1);
        self.window_active = window.is_window_active();
        view::bind_hub(self, cx);
        view::sync(self, cx);
        initiatives::refresh(self, cx);
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = false;
        toolbar::close_menus(self, window, cx);
        join::close(self, window, cx);
        view::publish_visible(self, cx);
        live::watch::sync(self, cx);
    }

    fn open_notification(&mut self, target: u64, _window: &mut Window, cx: &mut Context<Self>) {
        let buffer = self
            .hub
            .as_ref()
            .and_then(|hub| hub.read(cx).resolve(target));
        if let Some(buffer) = buffer {
            self.active = true;
            view::show_notified(self, &buffer, cx);
        }
    }

    fn return_location(&self, _cx: &App) -> Option<subspace_framework::dashboard::ReturnLocation> {
        back::location(self)
    }

    fn return_to(&mut self, key: &str, _window: &mut Window, cx: &mut Context<Self>) -> bool {
        back::show(self, key, cx)
    }

    fn open_indexed_result_fenced(
        &mut self,
        hit: &subspace_framework::dashboard::full_text::Hit,
        permit: subspace_framework::dashboard::full_text::ReadPermit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        reveal::open(self, hit, &permit, cx);
    }

    /// The board's settings file, offered (never its contents) to an issue
    /// report.
    fn diagnostic_config_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        config::file(cx)
    }

    /// The service, the networks and how many are connected, then any
    /// initiative hint: what the Space's heading used to say. While Channels
    /// is closed no network runs, so only the hint follows the service.
    fn status(&self, cx: &App) -> Option<SharedString> {
        let state = &self.hub.as_ref()?.read(cx).state;
        let hint = initiatives::hint(self, cx);
        let (total, connected) = match view::closed(self, cx) {
            true => (0, 0),
            false => (state.networks().len(), state.connected_count()),
        };
        view::status_line(total, connected, hint.as_deref()).map(SharedString::from)
    }

    fn docks(&self, cx: &App) -> SpaceDocks {
        view::space_docks(self, cx)
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        view::toggle_space_dock(self, dock, cx);
    }

    fn badge(&self, cx: &App) -> Option<SharedString> {
        if view::closed(self, cx) {
            return None;
        }
        let count = self.hub.as_ref()?.read(cx).state.attention_count();
        (count > 0).then(|| count.to_string().into())
    }

    /// The selected row, none while Channels is closed.
    fn subject(&self, cx: &App) -> Option<subspace_framework::dashboard::Row> {
        if view::closed(self, cx) {
            return None;
        }
        self.list.selected_row().cloned()
    }

    /// The conversation in front, else the selected buffer, with its recent
    /// messages; none while Channels is closed.
    fn agent_context(&self, cx: &App) -> Option<subspace_framework::dashboard::AgentContext> {
        if view::closed(self, cx) {
            return None;
        }
        let buffer = self
            .dock
            .focused
            .as_ref()
            .or_else(|| self.selected_buffer())?;
        let held = self.hub.as_ref()?.read(cx).state.buffer(buffer)?;
        Some(context::reference(
            &self.network_name(&buffer.network),
            buffer,
            held,
        ))
    }

    fn commands(&self, cx: &Context<Self>) -> Vec<subspace_framework::dashboard::PaletteCommand> {
        if view::closed(self, cx) {
            return Vec::new();
        }
        let mut commands = commands::commands(self, cx);
        commands.extend(initiatives::command(self, cx));
        commands
    }

    /// Channels and conversations to search, none while Channels is closed.
    fn search(&self, cx: &App) -> Option<subspace_framework::dashboard::Search> {
        if view::closed(self, cx) {
            return None;
        }
        Some(subspace_framework::dashboard::Search {
            placeholder: "Search channels and conversations".into(),
            items: self.search.clone(),
        })
    }

    /// Shows the buffer of row `id`, from `/`, the palette or recent items.
    /// A channel we are not in is selected, for Enter to join.
    fn open_search_result(&mut self, id: &str, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(buffer) = self.buffers.get(id).cloned() {
            view::reveal(self, &buffer, cx);
            cx.emit(DashboardEvent::Opened(id.to_owned().into()));
        } else if let Some(channel) = self.listed.get(id).cloned() {
            listing::reveal(self, &channel, cx);
        }
    }

    fn modal(&self, cx: &App) -> Option<subspace_framework::dashboard::Modal> {
        use subspace_framework::dashboard::Modal;
        if self.join.is_some() || tracker::message::any_open(self, cx) {
            Some(Modal::Dialog)
        } else {
            // Open from its own button or the overflow menu's chevron.
            self.initiative_menu.is_open().then_some(Modal::Menu)
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        toolbar::close_menus(self, window, cx);
        join::close(self, window, cx);
        tracker::message::close_all(self, window, cx);
    }

    fn keys(&self) -> &'static [subspace_framework::dashboard::KeyHelp] {
        conversation::KEYS
    }

    fn bind_keys(cx: &mut App) {
        join::bind_keys(cx);
        tracker::message::bind_keys(cx);
        conversation::bind_keys(cx);
    }

    /// Reconnects every network that is not connected.
    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        initiatives::refresh(self, cx);
        let Some(hub) = self.hub.clone() else {
            return;
        };
        hub.update(cx, |hub, _| {
            // The account's check and logins are asked again now, so an API
            // that came back is no longer named unreachable for minutes.
            hub.managed.retry_now();
            let idle: Vec<NetworkId> = hub
                .state
                .networks()
                .iter()
                .filter(|network| !toolbar::is_online(&network.status))
                .map(|network| network.id.clone())
                .collect();
            for network in idle {
                hub.send(Command::Connect(network));
            }
        });
    }
}

impl Irc {
    /// The buffer of the selected row.
    fn selected_buffer(&self) -> Option<&BufferKey> {
        self.list.selected().and_then(|key| self.buffers.get(key))
    }

    /// The listed channel of the selected row, when we are not in it.
    fn selected_listed(&self) -> Option<&BufferKey> {
        self.list.selected().and_then(|key| self.listed.get(key))
    }

    /// The network of the selected row or group header.
    fn selected_network(&self) -> Option<NetworkId> {
        if let Some(buffer) = self.selected_buffer().or_else(|| self.selected_listed()) {
            return Some(buffer.network.clone());
        }
        let Some(selected) = self.list.selected() else {
            return self
                .dock
                .focused
                .as_ref()
                .map(|buffer| buffer.network.clone());
        };
        self.group_networks.get(selected).cloned()
    }

    /// The buffer the pane's toolbar acts on. While the navigator is hidden
    /// its selection is out of sight, so the toolbar acts on the
    /// conversation it names; otherwise on the selected row.
    fn toolbar_buffer(&self) -> Option<&BufferKey> {
        match self.dock.focused.as_ref() {
            Some(focused) if !self.dock.buffers_open() => Some(focused),
            _ => self.selected_buffer(),
        }
    }

    /// The network the pane's toolbar acts on, by the same rule as
    /// [`Self::toolbar_buffer`].
    fn toolbar_network(&self) -> Option<NetworkId> {
        match self.dock.focused.as_ref() {
            Some(focused) if !self.dock.buffers_open() => Some(focused.network.clone()),
            _ => self.selected_network(),
        }
    }

    fn send(&self, command: Command, cx: &mut App) {
        if let Some(hub) = &self.hub {
            hub.update(cx, |hub, _| hub.send(command));
        }
    }

    /// Opens a private conversation with `nick` and shows it. A fleet agent
    /// gets none: the conversation in front says where the agent talks.
    fn open_query(&mut self, network: &NetworkId, nick: &str, cx: &mut Context<Self>) {
        let Some(hub) = self.hub.clone() else {
            return;
        };
        let roster = hub.read(cx).roster(network, cx);
        let state = &hub.read(cx).state;
        let notice = compose::AgentNotice::refusing(&roster, nick, |channel| {
            compose::listed_channel(state, network, channel).is_some()
        });
        if let Some(notice) = notice {
            self.refuse_query(network, notice, cx);
            return;
        }
        let buffer = hub.update(cx, |hub, cx| {
            hub.send(Command::Query {
                network: network.clone(),
                nick: nick.to_owned(),
            });
            let key = hub.state.open_query(network, nick);
            cx.notify();
            key
        });
        view::sync(self, cx);
        view::reveal(self, &buffer, cx);
    }

    /// Shows `notice` above the composer of the conversation in front on
    /// `network`, in place of a private conversation with an agent. When
    /// nothing of that network is in front, as after a notification click
    /// while another network shows, it shows the agent's channel if the
    /// user's channel list holds it, else the network's server buffer, and
    /// the notice there, so the user sees why nothing opened.
    fn refuse_query(
        &mut self,
        network: &NetworkId,
        notice: compose::AgentNotice,
        cx: &mut Context<Self>,
    ) {
        let front = self
            .dock
            .focused
            .as_ref()
            .filter(|buffer| &buffer.network == network)
            .and_then(|buffer| self.dock.open.get(buffer))
            .map(|hosted| hosted.entity.clone());
        if let Some(conversation) = front {
            show_notice(&conversation, notice, cx);
            return;
        }
        let Some(hub) = self.hub.clone() else {
            return;
        };
        let place = notice_place(&hub.read(cx).state, network, &notice);
        view::reveal(self, &place, cx);
        dock::with_window(self, cx, move |board, _, cx| {
            if let Some(hosted) = board.dock.open.get(&place) {
                show_notice(&hosted.entity.clone(), notice, cx);
            }
        });
    }

    /// A row's buffer state, copied out of the hub for list hooks.
    fn buffer_state(&self, key: &str, cx: &App) -> Option<state::Buffer> {
        let buffer = self.buffers.get(key)?;
        self.hub.as_ref()?.read(cx).state.buffer(buffer).cloned()
    }

    /// The network's display name as last synced, else its id.
    fn network_name(&self, id: &NetworkId) -> String {
        self.networks
            .iter()
            .find(|network| &network.id == id)
            .map_or_else(|| id.0.clone(), |network| network.name.clone())
    }

    /// The buffer's name where it stands alone, as in a heading: the channel
    /// or nick, or the network's name for its server buffer.
    fn buffer_name(&self, buffer: &BufferKey) -> SharedString {
        match buffer.target {
            Target::Server => self.network_name(&buffer.network).into(),
            _ => rows::title(buffer).to_owned().into(),
        }
    }
}

/// Puts `notice` above `conversation`'s composer.
fn show_notice(
    conversation: &gpui_kit::Entity<conversation::Conversation>,
    notice: compose::AgentNotice,
    cx: &mut App,
) {
    conversation.update(cx, |conversation, cx| {
        conversation.draft.agent_notice = Some(notice);
        cx.notify();
    });
}

/// Where a notice shows when nothing of `network` is in front: the agent's
/// channel when the user's channel list holds it, else the server buffer.
fn notice_place(
    state: &state::IrcState,
    network: &NetworkId,
    notice: &compose::AgentNotice,
) -> BufferKey {
    notice
        .channel
        .as_deref()
        .and_then(|channel| compose::listed_channel(state, network, channel))
        .unwrap_or_else(|| BufferKey {
            network: network.clone(),
            target: Target::Server,
        })
}
