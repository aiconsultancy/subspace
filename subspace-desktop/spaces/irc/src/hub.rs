//! One process-wide owner of the IRC runtime and the state it reports,
//! independent of product windows, so connections and notifications stay
//! up while the board is hidden.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use chrono::{DateTime, Utc};
use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use subspace_api_client::initiatives::Catalog;
use subspace_client_irc::{BufferKey, Command, Event, NetworkConfig, Runtime};
use subspace_framework::dashboard::{Services, full_text_service, notifications, prefs};

use crate::config::{self, Settings};
use crate::initiatives::{InitiativeIrc, loopback_fixture};
use crate::issues::{self, IssueReferences};
use crate::pull_requests::{self, PullRequestReferences};
use crate::state::{Attention, DIRECTORY_CACHE, Directories, IrcState, REMOVED_KEY, Removed};
use crate::{agent_tools, notice, store};

/// Lines asked for around a message that is not held.
const AROUND: u16 = 50;

mod captain_notifications;
mod configuration;
mod persist;
mod running;

/// How often the UI drains runtime events.
const DRAIN_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) struct Process(pub Entity<Hub>);
impl Global for Process {}

/// Where commands go: the live runtime, or a list a test reads back.
enum Outbox {
    Runtime(Runtime),
    #[cfg(test)]
    Recorded(Vec<Command>),
}

const SECONDS_PER_MINUTE: i64 = 60;

pub(crate) struct Hub {
    pub state: IrcState,
    pub initiatives: InitiativeIrc,
    outbox: Outbox,
    /// The buffer open in a visible, active board, which reads as it arrives.
    visible: Option<BufferKey>,
    /// Every conversation on screen while the board and its window are in
    /// front, splits included; none of them notifies.
    shown: Vec<BufferKey>,
    /// Notification targets: index + 1 is the target number.
    targets: Vec<BufferKey>,
    /// The settings the runtime and state follow.
    applied: Settings,
    /// The networks the runtime was last configured with: the settings
    /// without the channels the user left.
    running: Vec<NetworkConfig>,
    /// The removals as last written to the state file.
    saved_removed: String,
    /// Agent tool calls of this process, answered on each drain.
    tools: Option<Receiver<agent_tools::Request>>,
    /// Keeping messages: the store, the restore and requests around a line.
    pub recall: persist::Recall,
    /// Issue references: the contract's scope and the answers shown.
    pub issues: IssueReferences,
    pub pull_requests: PullRequestReferences,
    /// Clock sampled by the existing drain timer, never by tile rendering.
    pub now: DateTime<Utc>,
    pub uploads: crate::uploads::cache::References,
    captain_notifications: captain_notifications::Monitor,
    _timer: Option<Task<()>>,
    _settings: Option<Subscription>,
}

/// Starts the runtime with the configured networks; once per process.
pub(crate) fn start(cx: &mut App) {
    if cx.has_global::<Process>() {
        return;
    }
    let settings = config::load(cx);
    let removed = Removed::decode(prefs::get(cx, REMOVED_KEY).as_deref());
    let runtime = Runtime::start();
    let link = full_text_service(cx).map(store::Link::new);
    let initiatives = InitiativeIrc::new(Catalog::shared(cx), loopback_fixture());
    let hub = cx.new(|cx| {
        let mut hub = Hub::new(&settings, removed, Outbox::Runtime(runtime), initiatives);
        hub.state.directories = cached_directories(&settings);
        hub.tools = Some(agent_tools::open());
        hub.uploads.session.connect();
        hub.recall.persist = link.map(persist::Persist::new);
        hub._settings = Some(follow_settings(cx));
        hub.follow_initiatives(cx);
        hub.begin(cx);
        hub._timer = Some(cx.spawn(async |entity, cx| {
            loop {
                cx.background_executor().timer(DRAIN_INTERVAL).await;
                if entity.update(cx, |hub: &mut Hub, cx| hub.tick(cx)).is_err() {
                    break;
                }
            }
        }));
        hub
    });
    cx.set_global(Process(hub.clone()));
    issues::attach(&hub, cx);
    pull_requests::attach(&hub, cx);
}

/// Disconnects every network: dropping the runtime ends its connections.
pub(crate) fn stop(cx: &mut App) {
    agent_tools::close();
    if let Some(hub) = entity(cx) {
        hub.update(cx, |hub, cx| hub.flush_final(cx));
    }
    if cx.has_global::<Process>() {
        cx.remove_global::<Process>();
    }
}

pub(crate) fn entity(cx: &App) -> Option<Entity<Hub>> {
    cx.try_global::<Process>().map(|process| process.0.clone())
}

/// The channel lists of the last run, for the configured networks, which
/// show at once.
fn cached_directories(settings: &Settings) -> Directories {
    let mut cached = Directories::decode(prefs::read_cache(DIRECTORY_CACHE).as_deref());
    cached.retain(&settings.networks);
    cached
}

/// Writes the channel lists to their cache file when they changed. A test
/// hub, without a runtime, keeps them in memory only.
fn save_directories(hub: &mut Hub, cx: &App) {
    let live = matches!(hub.outbox, Outbox::Runtime(_));
    if hub.state.directories.take_changed() && live {
        prefs::write_cache(DIRECTORY_CACHE, hub.state.directories.encode(), cx);
    }
}

/// Settings saves the board's settings into the `Services` config; the hub
/// applies them without a restart.
fn follow_settings(cx: &mut Context<Hub>) -> Subscription {
    cx.observe_global::<Services>(|hub, cx| {
        let settings = config::load(cx);
        hub.apply(settings, cx);
    })
}

/// A hub with no runtime that records the commands it would send, with the
/// removals saved in the state file, as at startup.
#[cfg(test)]
pub(crate) fn install_for_test(settings: &Settings, cx: &mut App) -> Entity<Hub> {
    let removed = Removed::decode(prefs::get(cx, REMOVED_KEY).as_deref());
    let initiatives = InitiativeIrc::new(Catalog::shared(cx), true);
    let hub = cx.new(|cx| {
        let mut hub = Hub::new(settings, removed, Outbox::Recorded(Vec::new()), initiatives);
        hub._settings = Some(follow_settings(cx));
        hub.follow_initiatives(cx);
        hub.begin(cx);
        hub
    });
    cx.set_global(Process(hub.clone()));
    hub
}

/// [`install_for_test`] keeping messages in `service`, which restores the
/// stored buffers before the runtime is configured.
#[cfg(test)]
pub(crate) fn install_with_store_for_test(
    settings: &Settings,
    service: subspace_framework::dashboard::full_text::Service,
    cx: &mut App,
) -> Entity<Hub> {
    let removed = Removed::decode(prefs::get(cx, REMOVED_KEY).as_deref());
    let initiatives = InitiativeIrc::new(Catalog::shared(cx), true);
    let hub = cx.new(|cx| {
        let mut hub = Hub::new(settings, removed, Outbox::Recorded(Vec::new()), initiatives);
        hub.recall.persist = Some(persist::Persist::new(store::Link::new(service)));
        hub._settings = Some(follow_settings(cx));
        hub.follow_initiatives(cx);
        hub.begin(cx);
        hub
    });
    cx.set_global(Process(hub.clone()));
    hub
}

impl Hub {
    /// The hub for `settings`, keeping the removals of an earlier run out of
    /// the list and the runtime from joining the channels the user left.
    fn new(
        settings: &Settings,
        removed: Removed,
        outbox: Outbox,
        initiatives: InitiativeIrc,
    ) -> Self {
        let mut state = IrcState::new(&settings.networks, settings.highlights.clone());
        let saved_removed = removed.encode();
        let running = running::initial(&settings.networks, &removed);
        state.removed = removed;
        Self {
            state,
            initiatives,
            outbox,
            visible: None,
            shown: Vec::new(),
            targets: Vec::new(),
            applied: settings.clone(),
            running: Vec::new(),
            saved_removed,
            tools: None,
            recall: persist::Recall::waiting(running),
            issues: IssueReferences::default(),
            pull_requests: PullRequestReferences::default(),
            now: Utc::now(),
            captain_notifications: Default::default(),
            uploads: Default::default(),
            _timer: None,
            _settings: None,
        }
    }

    /// Removes a channel or private conversation from the list and saves
    /// that, so it stays out after a restart.
    pub fn remove(&mut self, buffer: &BufferKey, cx: &mut Context<Self>) {
        if self.state.remove(buffer) {
            self.recall.forget(buffer);
            self.save_removed(cx);
            cx.notify();
        }
    }

    /// Writes the removals to the state file when they changed.
    fn save_removed(&mut self, cx: &mut App) {
        let text = self.state.removed.encode();
        if text != self.saved_removed {
            prefs::set(cx, REMOVED_KEY, text.clone());
            self.saved_removed = text;
        }
    }

    pub fn send(&mut self, command: Command) {
        match &mut self.outbox {
            Outbox::Runtime(runtime) => runtime.send(command),
            #[cfg(test)]
            Outbox::Recorded(sent) => sent.push(command),
        }
    }

    /// Commands a test hub recorded, oldest first.
    #[cfg(test)]
    pub fn sent(&self) -> &[Command] {
        match &self.outbox {
            Outbox::Recorded(sent) => sent,
            Outbox::Runtime(_) => &[],
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if !self.shown.is_empty() && self.uploads.has_cards() {
            self.uploads.demand.activate();
        }
        crate::uploads::session::refresh(self, cx);
        self.serve_tools(cx);
        let events = match &self.outbox {
            Outbox::Runtime(runtime) => runtime.drain(),
            #[cfg(test)]
            Outbox::Recorded(_) => Vec::new(),
        };
        let now = Utc::now();
        if now.timestamp() / SECONDS_PER_MINUTE != self.now.timestamp() / SECONDS_PER_MINUTE {
            cx.notify();
        }
        self.now = now;
        if events.is_empty() {
            if self.state.expire_typing(now) {
                cx.notify();
            }
        } else {
            self.ingest(events, now, cx);
        }
        self.flush_store(false, cx);
        captain_notifications::tick(self, cx);
    }

    /// Answers the agent tool calls that arrived since the last drain.
    pub fn serve_tools(&mut self, cx: &mut Context<Self>) {
        let requests: Vec<agent_tools::Request> = self
            .tools
            .as_ref()
            .map(|tools| tools.try_iter().collect())
            .unwrap_or_default();
        if requests.is_empty() {
            return;
        }
        for request in requests {
            for command in agent_tools::serve(&mut self.state, request) {
                self.send(command);
            }
        }
        cx.notify();
    }

    /// Takes agent tool calls from `requests`, as a test's own link sends them.
    #[cfg(test)]
    pub fn take_tools(&mut self, requests: Receiver<agent_tools::Request>) {
        self.tools = Some(requests);
    }

    /// Applies `events` in order, reads the visible buffer, raises
    /// notifications for the others and notifies observers once.
    pub fn ingest(&mut self, events: Vec<Event>, now: DateTime<Utc>, cx: &mut Context<Self>) {
        let stored = match self.recall.persist {
            Some(_) => persist::stored_lines(&events),
            None => Vec::new(),
        };
        self.recall.answered(&events);
        let attention: Vec<Attention> = events
            .into_iter()
            .filter_map(|event| self.apply_event(event, now))
            .collect();
        self.state.expire_typing(now);
        self.recall.keep(&self.state, stored);
        self.read_visible();
        self.save_removed(cx);
        save_directories(self, cx);
        for item in attention {
            if !self.in_view(&item.buffer) {
                self.raise(&item, cx);
            }
        }
        cx.notify();
    }

    fn apply_event(&mut self, event: Event, now: DateTime<Utc>) -> Option<Attention> {
        if let Event::Sent { id, result, .. } = &event
            && self.captain_notifications.answered(*id, result)
        {
            return None;
        }
        self.state.apply(event, now)
    }

    fn in_view(&self, buffer: &BufferKey) -> bool {
        self.visible.as_ref() == Some(buffer) || self.shown.contains(buffer)
    }

    /// Asks the shell to post `item`: who and where, on which network.
    fn raise(&mut self, item: &Attention, cx: &mut Context<Self>) {
        let target = self.target(&item.buffer);
        let network = self
            .state
            .network(&item.buffer.network)
            .map_or("", |network| network.name.as_str());
        let notice = notice::notice(item, network);
        notifications::request(crate::ID, target, notice, cx);
    }

    /// The buffer the user is reading and the conversations on screen; `None`
    /// and none when the board, its panel or the window is not in front.
    pub fn set_visible(
        &mut self,
        buffer: Option<BufferKey>,
        shown: Vec<BufferKey>,
        cx: &mut Context<Self>,
    ) {
        if self.visible == buffer && self.shown == shown {
            return;
        }
        self.state
            .switch_unread(self.visible.as_ref(), buffer.as_ref());
        self.visible = buffer;
        self.shown = shown;
        self.read_visible();
        cx.notify();
    }

    /// Marks everything in `buffer` read, here and on the server.
    pub fn mark_buffer_read(&mut self, buffer: &BufferKey, cx: &mut Context<Self>) {
        if let Some(time) = self.state.mark_read(buffer) {
            self.send(Command::MarkRead {
                buffer: buffer.clone(),
                time,
            });
            cx.notify();
        }
    }

    fn read_visible(&mut self) {
        let Some(key) = self.visible.clone() else {
            return;
        };
        if let Some(time) = self.state.mark_read(&key) {
            self.send(Command::MarkRead { buffer: key, time });
        }
    }

    fn target(&mut self, buffer: &BufferKey) -> u64 {
        let index = match self.targets.iter().position(|known| known == buffer) {
            Some(index) => index,
            None => {
                self.targets.push(buffer.clone());
                self.targets.len() - 1
            }
        };
        index as u64 + 1
    }

    /// Asks for the lines around a message the buffer does not hold, such
    /// as a search result older than what restored.
    pub fn load_around(&mut self, navigation: &store::Navigation) {
        let buffer = navigation.buffer();
        self.send(Command::LoadAround {
            buffer: buffer.clone(),
            id: navigation.msgid.clone(),
            time: navigation.time,
            limit: AROUND,
        });
        self.recall.await_around(buffer);
    }

    /// The buffer a notification click names.
    pub fn resolve(&self, target: u64) -> Option<BufferKey> {
        let index = usize::try_from(target.checked_sub(1)?).ok()?;
        self.targets.get(index).cloned()
    }
}

/// Bytes of text one message to `buffer` carries without the client
/// splitting it, from our nick and the network's configured username.
pub(crate) fn message_budget(hub: &Hub, buffer: &BufferKey) -> usize {
    let nick = hub
        .state
        .network(&buffer.network)
        .map(|network| network.nick.as_str());
    // Sending needs a connected network, so its nick is known; an unknown
    // one would overstate the budget.
    debug_assert!(nick.is_some(), "message budget for an unknown network");
    let username = hub
        .running
        .iter()
        .find(|network| network.id == buffer.network)
        .map(|network| network.username.as_str())
        .unwrap_or_default();
    let target = match &buffer.target {
        subspace_client_irc::Target::Channel(name) | subspace_client_irc::Target::Query(name) => {
            name.as_str()
        }
        subspace_client_irc::Target::Server => "",
    };
    subspace_client_irc::message_budget(nick.unwrap_or_default(), username, target)
}
