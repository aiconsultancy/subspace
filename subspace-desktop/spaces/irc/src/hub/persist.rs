//! The hub's side of the store: lines waiting for their write, the ordered
//! queue of writes and deletes, and the restore before the runtime starts.
//! One batch of operations runs at a time, on the background executor.

use std::time::{Duration, Instant};

use chrono::Utc;
use gpui_kit::{AppContext as _, Context};
use subspace_client_irc::{BufferKey, Command, Event, Line, NetworkConfig};
use subspace_framework::core::log::{Level, Logger};

use super::Hub;
use crate::state::IrcState;
use crate::store::{self, Ended, Op, Restored};

/// How long new lines wait, so a burst is written as one batch.
const DEBOUNCE: Duration = Duration::from_secs(1);
/// Lines that are written at once, without waiting.
const FULL: usize = 100;

/// What the hub keeps for message history.
pub(crate) struct Recall {
    /// The store, when the product has one.
    pub persist: Option<Persist>,
    /// The networks to run once the stored buffers are restored.
    pub waiting: Option<Vec<NetworkConfig>>,
    /// Buffers waiting for the lines around a message.
    pub around: Vec<BufferKey>,
}

impl Recall {
    /// No store yet, and `networks` to run once the restore finished.
    pub fn waiting(networks: Vec<NetworkConfig>) -> Self {
        Self {
            persist: None,
            waiting: Some(networks),
            around: Vec::new(),
        }
    }

    /// Keeps `lines` for the next write, once their buffers are in `state`:
    /// lines of a buffer the user removed are not stored.
    pub fn keep(&mut self, state: &IrcState, lines: Vec<Line>) {
        let Some(persist) = self.persist.as_mut() else {
            return;
        };
        let kept: Vec<Line> = lines
            .into_iter()
            .filter(|line| state.buffer(&line.buffer).is_some())
            .collect();
        if !kept.is_empty() {
            persist.since.get_or_insert_with(Instant::now);
            persist.waiting.extend(kept);
        }
    }

    /// Keeps the requests that ended in `state` for the next write, with
    /// the lines waiting then. Without the store they are dropped, and stay
    /// in memory only.
    pub fn keep_endings(&mut self, state: &mut IrcState) {
        let ended: Vec<Ended> = state
            .take_endings()
            .into_iter()
            .flat_map(|(buffer, ids)| {
                ids.into_iter().map(move |(request, ending)| Ended {
                    buffer: buffer.clone(),
                    request,
                    outcome: ending.outcome,
                    at: ending.at,
                })
            })
            .collect();
        let Some(persist) = self.persist.as_mut() else {
            return;
        };
        if !ended.is_empty() {
            persist.since.get_or_insert_with(Instant::now);
            persist.ended.extend(ended);
        }
    }

    /// Deletes `buffer`'s stored lines and endings, and drops those still
    /// waiting.
    pub fn forget(&mut self, buffer: &BufferKey) {
        if let Some(persist) = self.persist.as_mut() {
            persist.waiting.retain(|line| &line.buffer != buffer);
            persist.ended.retain(|ended| &ended.buffer != buffer);
            persist.queue.push(Op::Forget(buffer.clone()));
        }
    }

    /// `buffer` waits for the lines around a message.
    pub fn await_around(&mut self, buffer: BufferKey) {
        if !self.around.contains(&buffer) {
            self.around.push(buffer);
        }
    }

    /// Whether `buffer` still waits for the lines around a message.
    pub fn awaiting(&self, buffer: &BufferKey) -> bool {
        self.around.contains(buffer)
    }

    /// The buffers whose lines around a message arrived among `events`
    /// wait no more.
    pub fn answered(&mut self, events: &[Event]) {
        self.around.retain(|buffer| {
            !events.iter().any(
                |event| matches!(event, Event::Backfill { buffer: answered, .. } if answered == buffer),
            )
        });
    }
}

pub(crate) struct Persist {
    link: store::Link,
    /// Lines waiting for the next write, oldest first.
    waiting: Vec<Line>,
    /// Request endings waiting for the next write.
    ended: Vec<Ended>,
    /// When the oldest waiting line or ending arrived.
    since: Option<Instant>,
    queue: Vec<Op>,
    /// A batch of operations is running.
    busy: bool,
}

impl Persist {
    pub fn new(link: store::Link) -> Self {
        Self {
            link,
            waiting: Vec::new(),
            ended: Vec::new(),
            since: None,
            queue: Vec::new(),
            busy: false,
        }
    }

    /// Whether the waiting lines and endings are due at `now`.
    fn due(&self, now: Instant, force: bool) -> bool {
        (!self.waiting.is_empty() || !self.ended.is_empty())
            && (force
                || self.waiting.len() >= FULL
                || self
                    .since
                    .is_some_and(|since| now.duration_since(since) >= DEBOUNCE))
    }

    /// Queues the waiting endings and lines as one write when they are due.
    fn queue_due(&mut self, now: Instant, force: bool) {
        if self.due(now, force) {
            self.since = None;
            let ended = std::mem::take(&mut self.ended);
            let lines = std::mem::take(&mut self.waiting);
            self.queue.push(Op::Commit { ended, lines });
        }
    }
}

/// The stored lines among `events`, before they are applied.
pub(super) fn stored_lines(events: &[Event]) -> Vec<Line> {
    events
        .iter()
        .flat_map(|event| match event {
            Event::Line(line) | Event::Replayed(line) => std::slice::from_ref(line),
            Event::History { lines, .. } | Event::Backfill { lines, .. } => lines.as_slice(),
            _ => &[],
        })
        .filter(|line| store::stored(line))
        .cloned()
        .collect()
}

impl Hub {
    /// Starts the next batch of operations when none runs: the waiting
    /// lines once due (at once with `force`), then the queue, in order.
    pub fn flush_store(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(persist) = self.recall.persist.as_mut() else {
            return;
        };
        persist.queue_due(Instant::now(), force);
        if persist.busy || persist.queue.is_empty() {
            return;
        }
        persist.busy = true;
        let ops = std::mem::take(&mut persist.queue);
        let link = persist.link.clone();
        cx.spawn(async move |hub, cx| {
            let errors = cx
                .background_spawn(async move { link.run(ops, Utc::now()) })
                .await;
            _ = hub.update(cx, |hub, cx| hub.stored(&errors, cx));
        })
        .detach();
    }

    /// A batch finished: reports failures without content, then runs what
    /// was queued meanwhile.
    fn stored(&mut self, errors: &[String], cx: &mut Context<Self>) {
        if let Some(persist) = self.recall.persist.as_mut() {
            persist.busy = false;
        }
        for error in errors {
            Logger::new(crate::ID).record(
                Level::Warn,
                "IRC message store operation failed",
                &[("error", error)],
            );
        }
        self.flush_store(false, cx);
    }

    /// Writes everything waiting when the hub stops, without waiting for it.
    pub(super) fn flush_final(&mut self, cx: &mut Context<Self>) {
        let Some(persist) = self.recall.persist.as_mut() else {
            return;
        };
        persist.queue_due(Instant::now(), true);
        let ops = std::mem::take(&mut persist.queue);
        if ops.is_empty() {
            return;
        }
        let link = persist.link.clone();
        cx.background_spawn(async move {
            link.run(ops, Utc::now());
        })
        .detach();
    }

    /// Restores the stored buffers, then starts the runtime. Without the
    /// store, or with it off, the runtime starts at once.
    pub(super) fn begin(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self
            .recall
            .persist
            .as_ref()
            .map(|persist| persist.link.clone())
        else {
            self.resume(Vec::new());
            return;
        };
        cx.spawn(async move |hub, cx| {
            let restored = cx.background_spawn(async move { link.restore() }).await;
            _ = hub.update(cx, |hub, cx| {
                hub.resume(restored.ok().flatten().unwrap_or_default());
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies the restored buffers and their request endings, configures
    /// the runtime with the networks it waited for, and tells it each
    /// buffer's newest line.
    fn resume(&mut self, restored: Vec<Restored>) {
        for buffer in restored {
            self.state
                .restore_buffer(&buffer.buffer, buffer.lines, &buffer.ended);
        }
        self.recall.keep_endings(&mut self.state);
        if let Some(networks) = self.recall.waiting.take() {
            self.run(networks);
        }
        let networks: Vec<_> = self
            .state
            .networks()
            .iter()
            .map(|network| network.id.clone())
            .collect();
        for network in networks {
            let anchors = self.state.anchors(&network);
            if !anchors.is_empty() {
                self.send(Command::Anchors { network, anchors });
            }
        }
    }
}
