//! Controlled time through the running actor, including stale stdio responses.
#![cfg(test)]
use super::cancellation::CANCEL_GRACE_PERIOD;
use super::fixture::{actor, chat, is_chat, ready, turn_done};
use super::resilience_tests::waiting_agent;
use super::{AcpMessage, ChatCommand, ChatEvent, ChatPreferences, Command, Event, Incoming, json};
use crate::runtime::{EventQueue, lock};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

const WAIT: Duration = Duration::from_secs(10);
const TIMER_TICK: Duration = Duration::from_millis(1);

struct Listening {
    _directory: tempfile::TempDir,
    commands: mpsc::Sender<Command>,
    incoming: mpsc::Sender<AcpMessage>,
    events: Arc<EventQueue>,
    first: Arc<AtomicU64>,
    latest: Arc<AtomicU64>,
    seen: Vec<Event>,
    cursor: usize,
}

#[cfg(test)]
fn start() -> (
    Listening,
    oneshot::Receiver<()>,
    tokio::task::JoinHandle<Result<(), String>>,
) {
    let (directory, mut actor, events) = actor(&waiting_agent(true), None, None);
    let incoming = actor.sender.clone();
    let (proxy, mut records) = mpsc::channel::<AcpMessage>(32);
    actor.sender = proxy;
    let first = Arc::new(AtomicU64::new(0));
    let latest = Arc::new(AtomicU64::new(0));
    let (first_record, last_record, forward) = (first.clone(), latest.clone(), incoming.clone());
    let (cancelled, acknowledged) = oneshot::channel();
    tokio::spawn(async move {
        let mut cancelled = Some(cancelled);
        while let Some((id, record)) = records.recv().await {
            let _ = first_record.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst);
            last_record.store(id, Ordering::SeqCst);
            if matches!(&record, Incoming::Record(value) if value["method"] == "cancel_seen")
                && let Some(sender) = cancelled.take()
            {
                let _ = sender.send(());
            }
            if forward.send((id, record)).await.is_err() {
                break;
            }
        }
    });
    let (commands, receiver) = mpsc::channel(16);
    let task = tokio::spawn(actor.listen(receiver, ChatPreferences::default()));
    (
        Listening {
            _directory: directory,
            commands,
            incoming,
            events,
            first,
            latest,
            seen: vec![],
            cursor: 0,
        },
        acknowledged,
        task,
    )
}

#[cfg(test)]
async fn message(fixture: &Listening, text: &str) -> Result<(), String> {
    fixture
        .commands
        .send(chat(ChatCommand::Send {
            text: text.into(),
            context: vec![],
            images: vec![],
        }))
        .await
        .map_err(|error| error.to_string())
}

impl Listening {
    #[cfg(test)]
    async fn event(&mut self, wanted: impl Fn(&Event) -> bool) -> Result<Event, String> {
        let deadline = Instant::now() + WAIT;
        loop {
            self.seen.extend(lock(&self.events.items).drain(..));
            self.events.space.notify_one();
            if let Some(offset) = self.seen[self.cursor..].iter().position(&wanted) {
                self.cursor += offset + 1;
                return Ok(self.seen[self.cursor - 1].clone());
            }
            if Instant::now() >= deadline {
                return Err(format!("missing fixture event: {:?}", self.seen));
            }
            tokio::task::yield_now().await;
        }
    }
}

#[cfg(test)]
async fn stop_and_recover(
    fixture: &mut Listening,
    cancelled: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    message(fixture, "Wait").await.unwrap();
    fixture
        .event(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::Entry(entry) if entry.entry.tool().is_some()),
            )
        })
        .await
        .unwrap();
    let runnable = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    tokio::time::pause();
    fixture
        .commands
        .send(chat(ChatCommand::Abort))
        .await
        .unwrap();
    cancelled.await.unwrap();
    tokio::time::advance(CANCEL_GRACE_PERIOD - Duration::from_nanos(1)).await;
    assert!(!lock(&fixture.events.items).iter().any(turn_done));
    // Tokio rounds registered timer deadlines to its millisecond tick.
    tokio::time::advance(TIMER_TICK).await;
    fixture.event(turn_done).await.unwrap();
    fixture.event(ready).await.unwrap();
    let first = fixture.first.load(Ordering::SeqCst);
    let replacement = fixture.latest.load(Ordering::SeqCst);
    assert_ne!(first, replacement);
    runnable
}

#[cfg(test)]
async fn next_turn_with_late_response(fixture: &mut Listening) {
    let first = fixture.first.load(Ordering::SeqCst);
    let replacement = fixture.latest.load(Ordering::SeqCst);
    message(fixture, "Write my notes").await.unwrap();
    let approval = fixture
        .event(|event| is_chat(event, |event| matches!(event, ChatEvent::Approval { .. })))
        .await
        .unwrap();
    fixture
        .incoming
        .send((
            first,
            Incoming::Record(json!({"id":3,"result":{"stopReason":"end_turn"}})),
        ))
        .await
        .unwrap();
    fixture.incoming.send((replacement, Incoming::Record(json!({"method":"session/update","params":{"update":{"sessionUpdate":"session_info_update","title":"fenced barrier"}}})))).await.unwrap();
    fixture
        .event(|event| {
            is_chat(
                event,
                |event| matches!(event, ChatEvent::Title(title) if title == "fenced barrier"),
            )
        })
        .await
        .unwrap();
    assert_eq!(
        fixture.seen.iter().filter(|event| turn_done(event)).count(),
        1
    );
    let Event::Chat {
        event: ChatEvent::Approval { request_id, .. },
        ..
    } = approval
    else {
        unreachable!()
    };
    fixture
        .commands
        .send(Command::Approve {
            request_id,
            allow: true,
        })
        .await
        .unwrap();
    fixture.event(turn_done).await.unwrap();
}

#[tokio::test]
async fn the_running_listener_expires_stop_replaces_the_agent_and_fences_a_late_response() {
    let (mut fixture, cancelled, task) = start();
    fixture.event(ready).await.unwrap();
    let runnable = stop_and_recover(&mut fixture, cancelled).await;
    next_turn_with_late_response(&mut fixture).await;
    assert_eq!(
        fixture.seen.iter().filter(|event| turn_done(event)).count(),
        2
    );
    tokio::time::resume();
    runnable.abort();
    fixture.commands.send(Command::Shutdown).await.unwrap();
    assert!(task.await.unwrap().is_ok());
}
