//! The background runtime that owns every network connection.
//!
//! Contract for owners: [`Runtime::start`] once, [`Runtime::send`] commands,
//! and [`Runtime::drain`] events (in order) from a UI timer. Dropping the
//! last clone disconnects every network and stops the runtime thread.

use std::collections::BTreeMap;
use std::time::Duration;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::model::{Command, Event, NetworkConfig, NetworkId, Update};
use crate::network::{self, EventSink, NetCommand};

const THREAD_NAME: &str = "subspace-irc";
const WORKER_THREADS: usize = 2;
/// How long networks get to send `QUIT` when the runtime stops.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct Runtime {
    commands: UnboundedSender<Command>,
    events: EventSink,
}

impl Runtime {
    pub fn start() -> Self {
        let (commands, receiver) = mpsc::unbounded_channel();
        let events = EventSink::default();
        let sink = events.clone();
        // Without the thread, commands are dropped and no events arrive.
        let _ = std::thread::Builder::new()
            .name(THREAD_NAME.to_string())
            .spawn(move || run_thread(receiver, sink));
        Self { commands, events }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Every event reported since the last drain, oldest first. Reaction
    /// updates are dropped: an owner that reads reactions calls
    /// [`Runtime::drain_updates`] instead, never both.
    pub fn drain(&self) -> Vec<Event> {
        self.drain_updates()
            .into_iter()
            .filter_map(network::into_event)
            .collect()
    }

    /// Every event and reaction update reported since the last drain,
    /// oldest first, in one order.
    pub fn drain_updates(&self) -> Vec<Update> {
        self.events
            .0
            .lock()
            .map(|mut updates| updates.drain(..).collect())
            .unwrap_or_default()
    }
}

fn run_thread(commands: UnboundedReceiver<Command>, events: EventSink) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .thread_name(THREAD_NAME)
        .enable_all()
        .build();
    if let Ok(runtime) = runtime {
        runtime.block_on(manage(commands, events));
        runtime.shutdown_timeout(SHUTDOWN_GRACE);
    }
}

struct Handle {
    config: NetworkConfig,
    commands: UnboundedSender<NetCommand>,
    task: JoinHandle<()>,
}

/// Routes commands to network tasks until every `Runtime` clone is gone.
async fn manage(mut commands: UnboundedReceiver<Command>, events: EventSink) {
    let mut networks = BTreeMap::new();
    while let Some(command) = commands.recv().await {
        route(&mut networks, command, &events);
    }
    let tasks: Vec<JoinHandle<()>> = networks.into_values().map(stop).collect();
    for task in tasks {
        let _ = tokio::time::timeout(SHUTDOWN_GRACE, task).await;
    }
}

fn route(networks: &mut BTreeMap<NetworkId, Handle>, command: Command, events: &EventSink) {
    match command {
        Command::Configure(configs) => configure(networks, configs, events),
        command => forward(networks, command, events),
    }
}

/// Hands `command` to its network. A reaction that no network takes, as
/// for a network not configured or stopping, is answered as not connected.
fn forward(networks: &BTreeMap<NetworkId, Handle>, command: Command, events: &EventSink) {
    let handle = command.network().and_then(|network| networks.get(network));
    let command = match command {
        Command::Connect(_) => NetCommand::Connect,
        command => NetCommand::Session(command),
    };
    let unsent = match handle {
        Some(handle) => handle.commands.send(command).err().map(|error| error.0),
        None => Some(command),
    };
    if let Some(NetCommand::Session(command)) = unsent {
        events.extend_updates(network::refuse_reaction(&command));
    }
}

/// Keeps unchanged networks connected, restarts changed ones, stops removed
/// ones and starts new ones.
fn configure(
    networks: &mut BTreeMap<NetworkId, Handle>,
    configs: Vec<NetworkConfig>,
    events: &EventSink,
) {
    let wanted: BTreeMap<NetworkId, NetworkConfig> = configs
        .into_iter()
        .map(|config| (config.id.clone(), config))
        .collect();
    let stale: Vec<NetworkId> = networks
        .iter()
        .filter(|(id, handle)| wanted.get(*id) != Some(&handle.config))
        .map(|(id, _)| id.clone())
        .collect();
    stale
        .iter()
        .filter_map(|id| networks.remove(id))
        .for_each(|handle| {
            stop(handle);
        });
    wanted
        .into_iter()
        .filter(|(id, _)| !networks.contains_key(id))
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|(id, config)| {
            networks.insert(id, spawn(config, events));
        });
}

fn spawn(config: NetworkConfig, events: &EventSink) -> Handle {
    let (commands, receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(network::run(config.clone(), receiver, events.clone()));
    Handle {
        config,
        commands,
        task,
    }
}

fn stop(handle: Handle) -> JoinHandle<()> {
    let _ = handle.commands.send(NetCommand::Shutdown);
    handle.task
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::forward;
    use crate::model::{BufferKey, Command, NetworkId, ReactionError, Target, Update};
    use crate::network::EventSink;

    #[test]
    fn a_reaction_no_network_takes_is_refused() {
        let events = EventSink::default();
        let buffer = BufferKey {
            network: NetworkId("gone".into()),
            target: Target::Channel("#c".into()),
        };
        let react = Command::Unreact {
            buffer: buffer.clone(),
            msgid: "m1".into(),
            reaction: "👍".into(),
        };
        forward(&BTreeMap::new(), react, &events);
        forward(
            &BTreeMap::new(),
            Command::Connect(buffer.network.clone()),
            &events,
        );
        let reported: Vec<Update> = events.0.lock().unwrap().drain(..).collect();
        assert_eq!(
            reported,
            [Update::ReactionRefused {
                buffer,
                msgid: "m1".into(),
                error: ReactionError::NotConnected,
            }]
        );
    }
}
