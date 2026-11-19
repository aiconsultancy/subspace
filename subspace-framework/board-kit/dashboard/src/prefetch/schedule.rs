use super::{Config, Failure, MAX_CONCURRENCY};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use subspace_core::task_state::PollBackoff;

const MAX_ITEMS: usize = 1000;
const MAX_DEBOUNCE: Duration = Duration::from_secs(5);
const CAPACITY_POLL: Duration = Duration::from_millis(25);

pub(super) enum Next<T> {
    Item(T, u8),
    Wait(Duration),
    Done,
}

pub(super) struct Schedule<T> {
    queue: VecDeque<(T, u8)>,
    in_flight: usize,
    concurrency: usize,
    ready: Instant,
    backoff: PollBackoff,
    stopped: bool,
}

impl<T> Schedule<T> {
    pub(super) fn new(loaded: Vec<T>, config: Config, now: Instant) -> Self {
        Self {
            queue: loaded
                .into_iter()
                .take(config.max_items.min(MAX_ITEMS))
                .map(|item| (item, 0))
                .collect(),
            in_flight: 0,
            concurrency: config.concurrency.clamp(1, MAX_CONCURRENCY),
            ready: now + config.debounce.min(MAX_DEBOUNCE),
            backoff: PollBackoff::new(),
            stopped: false,
        }
    }

    pub(super) fn next(&mut self, now: Instant) -> Next<T> {
        if self.stopped || (self.queue.is_empty() && self.in_flight == 0) {
            return Next::Done;
        }
        if now < self.ready {
            return Next::Wait(self.ready - now);
        }
        if self.in_flight >= self.concurrency {
            return Next::Wait(CAPACITY_POLL);
        }
        match self.queue.pop_front() {
            Some((item, attempt)) => {
                self.in_flight += 1;
                Next::Item(item, attempt)
            }
            None => Next::Wait(CAPACITY_POLL),
        }
    }

    pub(super) fn complete(
        &mut self,
        item: T,
        attempt: u8,
        result: Result<(), Failure>,
        now: Instant,
    ) {
        self.in_flight = self.in_flight.saturating_sub(1);
        match result {
            Ok(()) => {
                if now >= self.ready {
                    self.backoff.succeeded();
                }
            }
            Err(Failure::Authentication) => {
                self.stopped = true;
                self.queue.clear();
            }
            Err(_) => {
                self.ready = now + self.backoff.failed();
                if attempt < 2 {
                    self.queue.push_front((item, attempt + 1));
                }
            }
        }
    }
}
