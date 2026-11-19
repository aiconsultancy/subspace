//! IRC client board: a GPUI Kit dock with the networks and their buffers on
//! the left, each opened conversation as a panel in the centre (tabs and
//! splits), and the focused channel's members on the right.
//! The runtime (`subspace-client-irc`) owns every connection; this board
//! applies its events and sends commands.

mod agent_tools;
mod back;
mod commands;
mod compose;
mod config;
mod context;
mod conversation;
mod dock;
mod editor;
mod hub;
mod initiatives;
mod issues;
mod join;
mod lines;
mod listing;
mod markup;
mod members;
mod notice;
mod prompt;
mod pull_requests;
mod remove;
mod reveal;
mod rows;
mod schema;
mod setup;
mod state;
mod store;
mod style;
mod text;
mod toolbar;
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
    hub: Option<Entity<hub::Hub>>,
    hub_subscription: Option<Subscription>,
    /// The networks as last synced, for panels that read without a context.
    networks: Vec<state::Network>,
    active: bool,
    find_revision: u64,
    window_active: bool,
    /// The buffer list, conversations and member list.
    dock: dock::IrcDock,
    join: Option<join::Dialog>,
    initiative_menu: initiatives::Picker,
    toolbar: Option<Entity<subspace_framework::dashboard::overflow_toolbar::OverflowToolbar>>,
    /// A search result waiting for its line.
    reveal: Option<reveal::Pending>,
    /// The buffers `/` and the palette search, rebuilt on each sync.
    search: std::rc::Rc<subspace_framework::dashboard::ItemSearch>,
    available_height: Entity<f32>,
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
        self.initiative_menu.dismiss(window, cx);
        if let Some(toolbar) = &self.toolbar {
            toolbar.update(cx, |toolbar, cx| toolbar.dismiss(window, cx));
        }
        join::close(self, window, cx);
        view::publish_visible(self, cx);
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

    fn status(&self, cx: &App) -> Option<SharedString> {
        let hub = self.hub.as_ref()?.read(cx);
        let networks = hub.state.networks().len();
        if networks == 0 {
            return None;
        }
        let noun = if networks == 1 { "network" } else { "networks" };
        Some(
            format!(
                "{networks} {noun} · {} connected",
                hub.state.connected_count()
            )
            .into(),
        )
    }

    fn badge(&self, cx: &App) -> Option<SharedString> {
        let count = self.hub.as_ref()?.read(cx).state.attention_count();
        (count > 0).then(|| count.to_string().into())
    }

    fn subject(&self, _cx: &App) -> Option<subspace_framework::dashboard::Row> {
        self.list.selected_row().cloned()
    }

    /// The conversation in front, else the selected buffer, with its recent
    /// messages.
    fn agent_context(&self, cx: &App) -> Option<subspace_framework::dashboard::AgentContext> {
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
        let mut commands = commands::commands(self, cx);
        commands.extend(initiatives::command(self, cx));
        commands
    }

    fn search(&self, _cx: &App) -> Option<subspace_framework::dashboard::Search> {
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

    fn modal(&self, _cx: &App) -> Option<subspace_framework::dashboard::Modal> {
        use subspace_framework::dashboard::Modal;
        if self.join.is_some() {
            Some(Modal::Dialog)
        } else {
            self.initiative_menu.host().map(|_| Modal::Menu)
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.initiative_menu.dismiss(window, cx);
        join::close(self, window, cx);
    }

    fn keys(&self) -> &'static [subspace_framework::dashboard::KeyHelp] {
        conversation::KEYS
    }

    fn bind_keys(cx: &mut App) {
        join::bind_keys(cx);
        conversation::bind_keys(cx);
    }

    /// Reconnects every network that is not connected.
    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        initiatives::refresh(self, cx);
        let Some(hub) = self.hub.clone() else {
            return;
        };
        hub.update(cx, |hub, _| {
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
    fn selected_network(&self, cx: &App) -> Option<NetworkId> {
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
        let hub = self.hub.as_ref()?.read(cx);
        hub.state
            .networks()
            .iter()
            .find(|network| &rows::group_key(&network.id) == selected)
            .map(|network| network.id.clone())
    }

    fn send(&self, command: Command, cx: &mut App) {
        if let Some(hub) = &self.hub {
            hub.update(cx, |hub, _| hub.send(command));
        }
    }

    /// Opens a private conversation with `nick` and shows it.
    fn open_query(&mut self, network: &NetworkId, nick: &str, cx: &mut Context<Self>) {
        let Some(hub) = self.hub.clone() else {
            return;
        };
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

    /// A row's buffer state, copied out of the hub for list hooks.
    fn buffer_state(&self, key: &str, cx: &App) -> Option<state::Buffer> {
        let buffer = self.buffers.get(key)?;
        self.hub.as_ref()?.read(cx).state.buffer(buffer).cloned()
    }

    fn is_query_row(&self, row: &subspace_framework::dashboard::Row) -> bool {
        self.buffers.get(&row.key).is_some_and(Irc::is_query)
    }

    /// The network's display name as last synced, else its id.
    fn network_name(&self, id: &NetworkId) -> String {
        self.networks
            .iter()
            .find(|network| &network.id == id)
            .map_or_else(|| id.0.clone(), |network| network.name.clone())
    }

    fn is_query(buffer: &BufferKey) -> bool {
        matches!(buffer.target, Target::Query(_))
    }
}
