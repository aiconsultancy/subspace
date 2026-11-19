//! Automatic detail fetching for a snapshot of an already loaded list.
//! Keep the returned handle on the board. Drop/cancel it on deactivate, scope
//! change or list replacement. Blocking calls run only on the background executor.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, BackgroundExecutor};
use subspace_core::http::Error;
use subspace_search::{Document, Lease, Scope, Service};

const MAX_CONCURRENCY: usize = 4;
const CANCEL_POLL: Duration = Duration::from_millis(250);

mod schedule;
use schedule::{Next, Schedule};

#[derive(Clone, Copy)]
pub struct Config {
    pub concurrency: usize,
    pub max_items: usize,
    pub debounce: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            concurrency: 2,
            max_items: 400,
            debounce: Duration::from_millis(80),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Authentication,
    RateLimit,
    Transient,
}

impl From<&Error> for Failure {
    fn from(error: &Error) -> Self {
        match error {
            Error::Credential(_)
            | Error::Status {
                status: 401 | 403, ..
            } => Self::Authentication,
            Error::Status { status: 429, .. } => Self::RateLimit,
            _ => Self::Transient,
        }
    }
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Verified source. Construct this in a background task before starting a run.
#[derive(Clone)]
pub struct Source {
    service: Service,
    lease: Lease,
}

impl Source {
    /// Capture the activation ticket before credential binding. `bind` must
    /// return the credential-bound scope, never a display name or config guess.
    pub fn bind(
        service: Service,
        board: &str,
        bind: impl FnOnce() -> Result<Scope, String>,
    ) -> Result<Option<Self>, String> {
        if !service.policy(board)?.enabled {
            return Ok(None);
        }
        let ticket = service.begin_activation(board)?;
        let scope = bind()?;
        let activation = service.prepare_activation_ticket(&ticket, &scope)?;
        service.activate_prepared(&activation)?;
        let lease = service.lease(&scope)?;
        Ok(Some(Self { service, lease }))
    }

    fn context(&self, cancellation: Cancellation) -> Result<Work, String> {
        let lease = self.service.renew_lease(&self.lease)?;
        Ok(Work {
            source: self.clone(),
            lease,
            cancellation,
        })
    }
}

/// A lease captured before fetching. Publishing uses the service's serialized
/// writer and retains generation/sequence fences. Batch at most 100 documents.
pub struct Work {
    source: Source,
    lease: Lease,
    pub cancellation: Cancellation,
}

impl Work {
    pub fn publish(&self, documents: &[Document]) -> Result<(), String> {
        if self.cancellation.cancelled() {
            return Err("Detail prefetch cancelled".into());
        }
        self.source.service.upsert_leased(&self.lease, documents)
    }
}

pub struct Prefetch {
    cancellation: Cancellation,
}

impl Drop for Prefetch {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl Prefetch {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

/// Call after a loaded-list update/on_open, never from render. `fetch` must
/// use the supplied Work to publish and must not update UI state directly.
pub fn start<T, F>(loaded: Vec<T>, source: Source, config: Config, fetch: F, cx: &App) -> Prefetch
where
    T: Clone + Send + 'static,
    F: Fn(T, &Work) -> Result<(), Failure> + Send + Sync + 'static,
{
    let cancellation = Cancellation::default();
    let schedule = Arc::new(Mutex::new(Schedule::new(loaded, config, Instant::now())));
    let fetch = Arc::new(fetch);
    let workers = (0..config.concurrency.clamp(1, MAX_CONCURRENCY)).map(|_| {
        let worker = Worker {
            schedule: schedule.clone(),
            source: source.clone(),
            cancellation: cancellation.clone(),
            fetch: fetch.clone(),
        };
        let executor = cx.background_executor().clone();
        cx.background_spawn(async move {
            worker.run(executor).await;
        })
    });
    workers.for_each(|task| task.detach());
    Prefetch { cancellation }
}

struct Worker<T, F> {
    schedule: Arc<Mutex<Schedule<T>>>,
    source: Source,
    cancellation: Cancellation,
    fetch: Arc<F>,
}

impl<T, F> Worker<T, F>
where
    T: Clone + Send + 'static,
    F: Fn(T, &Work) -> Result<(), Failure> + Send + Sync + 'static,
{
    async fn run(self, executor: BackgroundExecutor) {
        while !self.cancellation.cancelled() {
            let next = match self.schedule.lock() {
                Ok(mut schedule) => schedule.next(Instant::now()),
                Err(_) => return,
            };
            match next {
                Next::Done => return,
                Next::Wait(delay) => executor.timer(delay.min(CANCEL_POLL)).await,
                Next::Item(item, attempt) => self.fetch_one(item, attempt),
            }
        }
    }

    fn fetch_one(&self, item: T, attempt: u8) {
        let Ok(work) = self.source.context(self.cancellation.clone()) else {
            self.cancellation.cancel();
            return;
        };
        if self.cancellation.cancelled() {
            return;
        }
        let result = (self.fetch)(item.clone(), &work);
        if result == Err(Failure::Authentication) {
            self.cancellation.cancel();
        }
        if let Ok(mut schedule) = self.schedule.lock() {
            schedule.complete(item, attempt, result, Instant::now());
        }
    }
}

#[cfg(test)]
mod tests;
