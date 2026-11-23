//! One subscription catches up on ready and coalesces subsequent issue hints.
use super::{
    LiveStatus, SLICE, hints,
    pace::{Due, ISSUE_SPACING, LIST_SPACING, READY_TIMEOUT, STABLE},
    set_status,
};
use crate::{
    Client,
    hub::{ScopeGeneration, Shared, lock},
    live::{self, EventStream, IssueChanged, LiveError, Record},
};
use std::{collections::BTreeSet, sync::Weak, time::Instant};
use tokio::time::{Interval, MissedTickBehavior, interval, timeout};

pub(super) struct Session {
    generation: ScopeGeneration,
    issue: Due,
    list: Due,
    live_since: Option<Instant>,
    /// The id of this stream's `ready` while its catch-up is not taken.
    #[cfg(test)]
    ready_owed: Option<u64>,
}

impl Session {
    pub(super) fn new(generation: ScopeGeneration) -> Self {
        Self {
            generation,
            issue: Due::new(ISSUE_SPACING),
            list: Due::new(LIST_SPACING),
            live_since: None,
            #[cfg(test)]
            ready_owed: None,
        }
    }

    /// Whether the stream reached `ready`.
    pub(super) fn went_live(&self) -> bool {
        self.live_since.is_some()
    }

    pub(super) fn stable(&self) -> bool {
        self.live_since
            .is_some_and(|since| since.elapsed() >= STABLE)
    }

    pub(super) async fn run(
        &mut self,
        weak: &Weak<Shared>,
        http: &reqwest::Client,
        client: &Client,
        projects: &BTreeSet<String>,
    ) -> Result<(), LiveError> {
        let mut stream = live::connect(http, client, projects).await?;
        timeout(READY_TIMEOUT, await_ready(&mut stream))
            .await
            .map_err(|_| LiveError::Protocol)??;
        if !set_status(weak, LiveStatus::Live, Some(self.generation)) {
            return Ok(());
        }
        let Some(shared) = weak.upgrade() else {
            return Ok(());
        };
        self.note_ready(&mut lock(&shared.state));
        drop(shared);
        self.forward(weak, &mut stream).await
    }

    async fn forward(
        &mut self,
        weak: &Weak<Shared>,
        stream: &mut EventStream,
    ) -> Result<(), LiveError> {
        let mut tick = interval(SLICE);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let record = receive_or_tick(&mut tick, stream).await?;
            let Some(shared) = weak.upgrade() else {
                return Ok(());
            };
            match record {
                Some(Record::Changed(event)) => self.hint(&shared, &event),
                Some(_) => {}
                None => self.apply(&shared),
            }
        }
    }

    fn note_ready(&mut self, state: &mut crate::hub::State) {
        #[cfg(test)]
        {
            state.live_reads.latest_ready += 1;
            self.ready_owed = Some(state.live_reads.latest_ready);
        }
        let now = Instant::now();
        self.live_since = Some(now);
        self.issue.note(state.issue_key(), now);
        self.list.note(state.list_key(), now);
    }

    fn hint(&mut self, shared: &Shared, event: &IssueChanged) {
        let state = lock(&shared.state);
        if !state.current(self.generation) {
            return;
        }
        let (issue, list) = hints::affects(&state, event);
        let now = Instant::now();
        if issue {
            self.issue.note(state.issue_key(), now);
        }
        if list {
            self.list.note(state.list_key(), now);
        }
    }

    fn apply(&mut self, shared: &Shared) {
        let now = Instant::now();
        let mut state = lock(&shared.state);
        if !state.current(self.generation) {
            return;
        }
        if self.issue.take(state.issue_key(), now) {
            hints::update_issue(shared, &mut state);
        }
        if self.list.take(state.list_key(), now) {
            hints::reread_list(shared, &mut state);
        }
        #[cfg(test)]
        if !self.issue.waiting()
            && !self.list.waiting()
            && let Some(ready) = self.ready_owed.take()
        {
            state.live_reads.caught_up_ready = ready;
        }
    }
}

async fn receive_or_tick(
    tick: &mut Interval,
    stream: &mut EventStream,
) -> Result<Option<Record>, LiveError> {
    tokio::select! {
        biased;
        _ = tick.tick() => Ok(None),
        record = stream.next() => record.map(Some),
    }
}

async fn await_ready(stream: &mut EventStream) -> Result<(), LiveError> {
    loop {
        match stream.next().await? {
            Record::Ready => return Ok(()),
            Record::Changed(_) => return Err(LiveError::Protocol),
            Record::Other => {}
        }
    }
}
