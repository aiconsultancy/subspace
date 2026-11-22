//! Owns one network's transport, reconnect policy and session delivery.

#[cfg(test)]
mod cleanroom_acceptance;
mod offline;
mod secrets;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, ReadHalf, WriteHalf};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{self, Instant};

use crate::codec::{self, LineBuffer};
use crate::connection::{self, BoxStream, Timeouts};
use crate::model::{Command, ConnectionStatus, Event, NetworkConfig, Update};
use crate::proto::{self, Message};
use crate::session::{self, Carry, Output, Session};

const TICK: Duration = Duration::from_secs(10);
const IDLE_LIMIT: Duration = Duration::from_secs(120);
const PING_LIMIT: Duration = Duration::from_secs(60);
const WRITE_LIMIT: Duration = Duration::from_secs(30);
const READ_BUFFER_SIZE: usize = 4096;
const MAX_RETRY_DELAY_SECS: u64 = 60;

#[derive(Debug)]
pub(crate) enum NetCommand {
    Connect,
    Session(Command),
    Shutdown,
}

/// Everything reported to owners, in order: events and reaction updates.
#[derive(Clone, Default)]
pub(crate) struct EventSink(pub Arc<Mutex<VecDeque<Update>>>);

impl EventSink {
    pub fn extend(&self, events: impl IntoIterator<Item = Event>) {
        self.extend_updates(events.into_iter().map(Update::Event));
    }

    pub fn extend_updates(&self, updates: impl IntoIterator<Item = Update>) {
        if let Ok(mut queue) = self.0.lock() {
            queue.extend(updates);
        }
    }

    /// The events reported so far, without reaction updates.
    #[cfg(test)]
    pub fn events(&self) -> Vec<Event> {
        self.0
            .lock()
            .map(|queue| queue.iter().filter_map(event_of).collect())
            .unwrap_or_default()
    }
}

/// The event an update carries, if it is one.
pub(crate) fn event_of(update: &Update) -> Option<Event> {
    match update {
        Update::Event(event) => Some(event.clone()),
        _ => None,
    }
}

pub(crate) async fn run(
    config: NetworkConfig,
    commands: UnboundedReceiver<NetCommand>,
    events: EventSink,
) {
    run_with(config, commands, events, Timeouts::default()).await;
}

async fn run_with(
    config: NetworkConfig,
    commands: UnboundedReceiver<NetCommand>,
    events: EventSink,
    timeouts: Timeouts,
) {
    let connect_now = config.auto_connect;
    let mut network = Network {
        config,
        commands,
        events,
        carry: Carry::default(),
        attempt: 0,
    };
    if !connect_now && !network.wait(Waiting::Disconnected).await {
        return;
    }
    loop {
        let end = match network.connect(timeouts).await {
            Ok(stream) => network.run_link(stream).await,
            Err(end) => end,
        };
        let waiting = match end {
            End::Quit => Waiting::Disconnected,
            End::Shutdown => break,
            End::Fatal(error) => {
                network.status(ConnectionStatus::Failed { error });
                Waiting::Failed
            }
            End::Lost { error, registered } => network.reconnect(error, registered),
        };
        if !network.wait(waiting).await {
            break;
        }
    }
}

#[derive(Debug)]
enum End {
    Quit,
    Shutdown,
    Lost { error: String, registered: bool },
    Fatal(String),
}

#[derive(Clone, Copy)]
enum Waiting {
    Disconnected,
    Failed,
    RetryAt(Instant),
}

struct Network {
    config: NetworkConfig,
    commands: UnboundedReceiver<NetCommand>,
    events: EventSink,
    carry: Carry,
    attempt: u32,
}

impl Network {
    fn status(&self, status: ConnectionStatus) {
        self.events.extend([Event::Status {
            network: self.config.id.clone(),
            status,
        }]);
    }

    fn reconnect(&mut self, error: String, registered: bool) -> Waiting {
        self.attempt = if registered {
            1
        } else {
            self.attempt.saturating_add(1)
        };
        let delay_secs = backoff(self.attempt);
        self.status(ConnectionStatus::Reconnecting {
            attempt: self.attempt,
            delay_secs,
            error,
        });
        Waiting::RetryAt(Instant::now() + Duration::from_secs(delay_secs))
    }

    async fn wait(&mut self, mut waiting: Waiting) -> bool {
        if matches!(waiting, Waiting::Disconnected) {
            self.status(ConnectionStatus::Disconnected);
        }
        loop {
            let command = tokio::select! {
                command = self.commands.recv() => command,
                () = retry_when_due(waiting) => return true,
            };
            match command {
                None | Some(NetCommand::Shutdown) => return false,
                Some(NetCommand::Connect) => return true,
                Some(NetCommand::Session(Command::Disconnect { .. })) => {
                    self.attempt = 0;
                    waiting = Waiting::Disconnected;
                    self.status(ConnectionStatus::Disconnected);
                }
                Some(NetCommand::Session(command)) => {
                    answer_offline(&self.config, &self.events, &mut self.carry, &command);
                }
            }
        }
    }

    async fn connect(&mut self, timeouts: Timeouts) -> Result<BoxStream, End> {
        self.status(ConnectionStatus::Connecting);
        let connecting = connection::connect(&self.config, timeouts);
        tokio::pin!(connecting);
        loop {
            let command = tokio::select! {
                result = &mut connecting => break result,
                command = self.commands.recv() => command,
            };
            match command {
                None | Some(NetCommand::Shutdown) => return Err(End::Shutdown),
                Some(NetCommand::Session(Command::Disconnect { .. })) => return Err(End::Quit),
                Some(NetCommand::Connect) => {}
                Some(NetCommand::Session(command)) => {
                    answer_offline(&self.config, &self.events, &mut self.carry, &command);
                }
            }
        }
        .map_err(connection_failed)
    }

    async fn run_link(&mut self, stream: BoxStream) -> End {
        self.status(ConnectionStatus::Registering);
        let config = match secrets::resolve(&self.config) {
            Ok(config) => config,
            Err(error) => return End::Fatal(error),
        };
        let session = Session::new(config, self.carry.clone());
        let mut link = Link::new(stream, session, self.events.clone());
        let end = link.run(&mut self.commands).await;
        self.events.extend(link.session.abandon());
        self.carry = link.session.into_carry();
        end
    }
}

async fn retry_when_due(waiting: Waiting) {
    match waiting {
        Waiting::RetryAt(deadline) => time::sleep_until(deadline).await,
        Waiting::Disconnected | Waiting::Failed => std::future::pending().await,
    }
}

fn answer_offline(
    config: &NetworkConfig,
    events: &EventSink,
    carry: &mut Carry,
    command: &Command,
) {
    events.extend(session::configure_autojoin(carry, config, command));
    carry.forget(command);
    events.extend(offline::reply(config, command));
    events.extend_updates(offline::refuse_reaction(command));
}

fn connection_failed(error: connection::ConnectError) -> End {
    if error.retryable() {
        End::Lost {
            error: error.to_string(),
            registered: false,
        }
    } else {
        End::Fatal(error.to_string())
    }
}

fn backoff(attempt: u32) -> u64 {
    2_u64
        .saturating_pow(attempt.saturating_sub(1))
        .min(MAX_RETRY_DELAY_SECS)
}

struct Link {
    read: ReadHalf<BoxStream>,
    write: WriteHalf<BoxStream>,
    session: Session,
    events: EventSink,
    lines: LineBuffer,
    last_input: Instant,
    ping_sent: Option<Instant>,
}

impl Link {
    fn new(stream: BoxStream, session: Session, events: EventSink) -> Self {
        let (read, write) = tokio::io::split(stream);
        Self {
            read,
            write,
            session,
            events,
            lines: LineBuffer::default(),
            last_input: Instant::now(),
            ping_sent: None,
        }
    }

    async fn run(&mut self, commands: &mut UnboundedReceiver<NetCommand>) -> End {
        let start = self.session.start(Utc::now());
        if let Some(end) = self.deliver(start).await {
            return end;
        }
        let mut ticker = time::interval(TICK);
        let mut bytes = [0; READ_BUFFER_SIZE];
        loop {
            let end = tokio::select! {
                read = self.read.read(&mut bytes) => match read {
                    Ok(0) => Some(self.lost("The server closed the connection")),
                    Ok(count) => self.input(&bytes[..count]).await,
                    Err(error) => Some(self.lost(error)),
                },
                command = commands.recv() => self.command(command).await,
                _ = ticker.tick() => self.tick().await,
            };
            if let Some(end) = end {
                return end;
            }
        }
    }

    async fn input(&mut self, bytes: &[u8]) -> Option<End> {
        self.last_input = Instant::now();
        self.ping_sent = None;
        self.lines.extend(bytes);
        loop {
            let line = match self.lines.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => return None,
                Err(error) => return Some(self.lost(error)),
            };
            if let Ok(message) = proto::parse(&line) {
                let output = self.session.receive(message, Utc::now());
                if let Some(end) = self.deliver(output).await {
                    return Some(end);
                }
            }
        }
    }

    async fn command(&mut self, command: Option<NetCommand>) -> Option<End> {
        match command {
            Some(NetCommand::Session(command)) => {
                let output = self.session.command(command, Utc::now());
                self.deliver(output).await
            }
            Some(NetCommand::Connect) => None,
            None | Some(NetCommand::Shutdown) => {
                let output = self.session.command(
                    Command::Disconnect {
                        network: self.session.network(),
                        reason: "Subspace closed".into(),
                    },
                    Utc::now(),
                );
                let _ = self.deliver(output).await;
                Some(End::Shutdown)
            }
        }
    }

    async fn tick(&mut self) -> Option<End> {
        let now = Utc::now();
        let output = self.session.tick(now);
        if let Some(end) = self.deliver(output).await {
            return Some(end);
        }
        if let Some(sent) = self.ping_sent {
            return (sent.elapsed() > PING_LIMIT)
                .then(|| self.lost("The server stopped responding (ping timeout)"));
        }
        if self.last_input.elapsed() < IDLE_LIMIT {
            return None;
        }
        let ping = Message::new("PING", [format!("subspace-{}", now.timestamp())]);
        self.ping_sent = Some(Instant::now());
        self.deliver(Output {
            send: vec![ping],
            ..Output::default()
        })
        .await
    }

    async fn deliver(&mut self, output: Output) -> Option<End> {
        self.events.extend(output.events);
        self.events.extend_updates(output.updates);
        let bytes: Vec<u8> = output.send.iter().flat_map(codec::encode).collect();
        if !bytes.is_empty()
            && let Err(error) = self.write_output(&bytes).await
        {
            self.events.extend(output.unwritten);
            return Some(self.lost(error));
        }
        self.session.written(&output.written);
        self.events.extend(output.written);
        output
            .fatal
            .map(End::Fatal)
            .or(output.quit.then_some(End::Quit))
    }

    async fn write_output(&mut self, bytes: &[u8]) -> io::Result<()> {
        let write = async {
            self.write.write_all(bytes).await?;
            self.write.flush().await
        };
        time::timeout(WRITE_LIMIT, write)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "The IRC write timed out"))?
    }

    fn lost(&self, error: impl ToString) -> End {
        End::Lost {
            error: error.to_string(),
            registered: self.session.registered(),
        }
    }
}
