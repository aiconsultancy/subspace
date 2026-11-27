//! The system capability on the host channel. One shared sampler serves
//! every running TSX Space. Each view answers only the tiers granted to its
//! package, and is sent new readings of those tiers while it is visible.
//! nsh refuses an undeclared or ungranted name before the call reaches
//! here; the handler checks the grant again and refuses with `not granted`.
//! Calls are answered on a worker thread: a first sample can take tens of
//! milliseconds, and the nsh engine thread must not wait for it. Only the
//! reply of a call is queued, never its data, and the queue is bounded: a
//! package that calls faster than the worker answers is told `busy` at
//! once, and its posts, which have no answer, are dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use gpui_kit::{App, Global, Window};
use nsh_engine::{HostCall, HostChannel, HostReply};
use serde_json::Value;
use subspace_tsx_system::{Display, FactsInput, Sampler, Subscription, Tier};

/// The refusal a package receives for a tier it was not granted.
pub(crate) const NOT_GRANTED: &str = "not granted";
/// The refusal a package receives while the worker is behind.
pub(crate) const BUSY: &str = "busy";
/// Calls waiting for the worker, across every running package.
const QUEUE_LIMIT: usize = 64;

/// Where a call's answer goes: nsh's reply, or a test's.
pub(crate) trait Reply: Send + 'static {
    fn send(self, result: Result<Value, String>);
}

impl Reply for HostReply {
    fn send(self, result: Result<Value, String>) {
        self.reply(result);
    }
}

/// A view the sampler sends readings to.
struct Receiver {
    id: u64,
    channel: HostChannel,
    tiers: Vec<Tier>,
}

type Receivers = Arc<Mutex<Vec<Receiver>>>;

/// A call to answer: where the answer goes, the tier and whether it is
/// granted.
type Job<R> = (R, Tier, bool);

/// The shared sampler, the visible views it sends to and the worker that
/// answers calls.
pub(crate) struct Hub {
    sampler: Sampler,
    receivers: Receivers,
    jobs: mpsc::SyncSender<Job<HostReply>>,
}

impl Global for Hub {}

/// The process's hub, made on first use.
pub(crate) fn hub(cx: &mut App) -> &Hub {
    if !cx.has_global::<Hub>() {
        cx.set_global(Hub::new());
    }
    cx.global::<Hub>()
}

impl Hub {
    fn new() -> Self {
        let receivers: Receivers = Arc::default();
        let targets = Arc::clone(&receivers);
        let sampler = Sampler::new(Arc::new(move |tier, reading: &Value| {
            send(&targets, tier, reading);
        }));
        let jobs = answer_on_worker(sampler.clone(), QUEUE_LIMIT);
        Self {
            sampler,
            receivers,
            jobs,
        }
    }

    /// Sets the facts only the host knows: displays and the app version.
    pub(crate) fn set_facts_input(&self, input: FactsInput) {
        self.sampler.set_facts_input(input);
    }

    /// Answers the package's calls on `channel`: the latest reading of each
    /// granted tier, `not granted` for the others.
    pub(crate) fn serve(&self, channel: &HostChannel, granted: &[Tier]) {
        for tier in Tier::ALL {
            let jobs = self.jobs.clone();
            let allowed = granted.contains(&tier);
            channel.handle(tier.host_name(), move |call: HostCall| {
                // A post has no reply and is dropped here, with its data.
                if let Some(reply) = call.into_reply() {
                    dispatch(&jobs, (reply, tier, allowed));
                }
            });
        }
    }

    /// Sends new readings of `granted` to `channel` until the returned
    /// guard is dropped, as the view stops being visible.
    pub(crate) fn watch(&self, channel: &HostChannel, granted: &[Tier]) -> Watch {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        lock(&self.receivers).push(Receiver {
            id,
            channel: channel.clone(),
            tiers: granted.to_vec(),
        });
        Watch {
            id,
            receivers: Arc::clone(&self.receivers),
            _sampling: self.sampler.subscribe(granted),
        }
    }
}

/// Sends readings to one view while it lives.
pub(crate) struct Watch {
    id: u64,
    receivers: Receivers,
    _sampling: Subscription,
}

impl Drop for Watch {
    fn drop(&mut self) {
        lock(&self.receivers).retain(|receiver| receiver.id != self.id);
    }
}

fn lock(receivers: &Receivers) -> std::sync::MutexGuard<'_, Vec<Receiver>> {
    receivers.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The answer to a call of `tier`: its latest reading, or `not granted`.
fn answer(sampler: &Sampler, tier: Tier, allowed: bool) -> Result<Value, String> {
    if allowed {
        Ok(sampler.latest(tier))
    } else {
        Err(NOT_GRANTED.into())
    }
}

/// Refuses an ungranted call at once, on the calling thread, so it never
/// takes a place in the queue; queues a granted one.
fn dispatch<R: Reply>(jobs: &mpsc::SyncSender<Job<R>>, job: Job<R>) {
    let (reply, tier, allowed) = job;
    if allowed {
        enqueue(jobs, (reply, tier, allowed));
    } else {
        reply.send(Err(NOT_GRANTED.into()));
    }
}

/// Queues a call without waiting: a full queue answers `busy` at once, and
/// a queue without its worker fails the call.
fn enqueue<R: Reply>(jobs: &mpsc::SyncSender<Job<R>>, job: Job<R>) {
    match jobs.try_send(job) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full((reply, ..))) => reply.send(Err(BUSY.into())),
        Err(mpsc::TrySendError::Disconnected((reply, ..))) => {
            reply.send(Err("the host is not answering".into()));
        }
    }
}

/// Starts the worker that answers calls, and returns its queue of at most
/// `limit` waiting calls.
fn answer_on_worker<R: Reply>(sampler: Sampler, limit: usize) -> mpsc::SyncSender<Job<R>> {
    let (jobs, queue) = mpsc::sync_channel::<Job<R>>(limit);
    // Without the worker every call fails in the app.
    let _ = std::thread::Builder::new()
        .name("subspace-tsx-host".into())
        .spawn(move || {
            for (reply, tier, allowed) in queue {
                reply.send(answer(&sampler, tier, allowed));
            }
        });
    jobs
}

fn send(receivers: &Receivers, tier: Tier, reading: &Value) {
    for receiver in lock(receivers)
        .iter()
        .filter(|receiver| receiver.tiers.contains(&tier))
    {
        // A view whose app stopped drops the event; it reads again on start.
        let _ = receiver.channel.send(tier.host_name(), reading.clone());
    }
}

/// The facts only the host knows: this app's version and the displays.
pub(crate) fn share_facts(window: &Window, cx: &mut App) {
    let input = FactsInput {
        subspace_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        displays: Some(displays(window, cx)),
    };
    hub(cx).set_facts_input(input);
}

/// The displays the facts tier reports: their size in points and the
/// window's scale.
fn displays(window: &Window, cx: &App) -> Vec<Display> {
    let scale = window.scale_factor();
    cx.displays()
        .iter()
        .map(|display| {
            let size = display.bounds().size;
            Display {
                width: f32::from(size.width).round() as u32,
                height: f32::from(size.height).round() as u32,
                scale,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        BUSY, HostCall, Hub, Job, NOT_GRANTED, Reply, answer, answer_on_worker, dispatch, enqueue,
    };
    use serde_json::Value;
    use std::sync::mpsc;
    use subspace_tsx_system::Sampler;

    /// A reply a test reads back.
    struct Answer(mpsc::Sender<Result<Value, String>>);

    impl Reply for Answer {
        fn send(self, result: Result<Value, String>) {
            let _ = self.0.send(result);
        }
    }

    fn sampler() -> Sampler {
        Sampler::new(std::sync::Arc::new(|_, _: &Value| {}))
    }
    use nsh_engine::HostChannel;
    use subspace_tsx_system::Tier;

    #[test]
    fn a_watch_stops_sampling_when_it_is_dropped() {
        let hub = Hub::new();
        let channel = HostChannel::default();
        let watch = hub.watch(&channel, &[Tier::Metrics]);
        assert!(hub.sampler.is_running());
        assert_eq!(hub.receivers.lock().map(|list| list.len()).unwrap_or(0), 1);
        drop(watch);
        assert_eq!(hub.receivers.lock().map(|list| list.len()).unwrap_or(1), 0);
    }

    #[test]
    fn an_ungranted_tier_is_refused_and_a_granted_one_answered() {
        let hub = Hub::new();
        assert_eq!(
            answer(&hub.sampler, Tier::Apps, false),
            Err(NOT_GRANTED.to_owned())
        );
        assert_eq!(NOT_GRANTED, "not granted");
        let facts = answer(&hub.sampler, Tier::Facts, true).unwrap();
        assert!(facts.is_object());
    }

    #[test]
    fn calls_go_through_the_queue_to_the_worker_and_back() {
        let jobs = answer_on_worker::<Answer>(sampler(), 4);
        let (tx, rx) = mpsc::channel();
        enqueue(&jobs, (Answer(tx.clone()), Tier::Facts, true));
        enqueue(&jobs, (Answer(tx), Tier::Apps, false));
        let mut answers: Vec<_> = (0..2).map(|_| rx.recv().unwrap()).collect();
        answers.sort_by_key(Result::is_ok);
        assert_eq!(answers[0], Err(NOT_GRANTED.to_owned()));
        assert!(answers[1].as_ref().unwrap().is_object());
    }

    struct Sink;

    impl nsh_engine::MountSink for Sink {
        fn submit(&self, _: nsh_core::MountBatch) {}
    }

    #[test]
    fn a_package_call_reaches_the_worker_through_the_host_channel() {
        let hub = Hub::new();
        let channel = HostChannel::new();
        hub.serve(&channel, &[Tier::Facts]);
        let (seen, answers) = mpsc::channel();
        let seen = std::sync::Mutex::new(seen);
        channel.handle("seen", move |mut call: HostCall| {
            let _ = seen.lock().unwrap().send(call.take_data());
        });
        let source = "import { host } from 'nsh';
            const report = (name) => host.call(name).then(
                (value) => host.post('seen', { name, ok: typeof value === 'object' }),
                (error) => host.post('seen', { name, error: String(error.message ?? error) }));
            report('subspace.system.facts');
            report('subspace.system.apps');";
        let mut config = nsh_engine::EngineConfig::new(nsh_engine::Bundle::source(source.into()));
        if let nsh_engine::App::Js(bundle) = &mut config.app {
            bundle.policy.declared = Some(
                nsh_engine::Capabilities::from_json(&serde_json::json!({ "host": true })).unwrap(),
            );
        }
        config.host = channel.clone();
        let engine =
            nsh_engine::Engine::start(config, || nsh_core::ApproxTextEngine, Sink).unwrap();
        let wait = std::time::Duration::from_secs(10);
        let mut got: Vec<Value> = (0..2)
            .map(|_| answers.recv_timeout(wait).unwrap())
            .collect();
        got.sort_by_key(|answer| answer["name"].as_str().map(str::to_owned));
        assert_eq!(got[0]["name"], "subspace.system.apps");
        assert!(
            got[0]["error"].as_str().unwrap().contains(NOT_GRANTED),
            "{got:?}"
        );
        assert_eq!(got[1]["ok"], true, "{got:?}");
        drop(engine);
    }

    #[test]
    fn ungranted_calls_are_refused_without_taking_a_place_in_the_queue() {
        // A queue whose worker is held, with one place.
        let (jobs, held) = mpsc::sync_channel::<Job<Answer>>(1);
        let (tx, rx) = mpsc::channel();
        for _ in 0..100 {
            dispatch(&jobs, (Answer(tx.clone()), Tier::Apps, false));
        }
        let refused: Vec<_> = rx.try_iter().collect();
        assert_eq!(refused.len(), 100);
        assert!(
            refused
                .iter()
                .all(|answer| answer == &Err(NOT_GRANTED.to_owned()))
        );
        dispatch(&jobs, (Answer(tx), Tier::Metrics, true));
        assert!(
            rx.try_recv().is_err(),
            "the granted call is queued, not busy"
        );
        assert!(held.try_recv().is_ok());
    }

    #[test]
    fn an_overloaded_queue_answers_busy_at_once() {
        // A queue whose worker is held: its one place fills, the next is busy.
        let (jobs, _held) = mpsc::sync_channel::<Job<Answer>>(1);
        let (tx, rx) = mpsc::channel();
        enqueue(&jobs, (Answer(tx.clone()), Tier::Metrics, true));
        for _ in 0..1000 {
            enqueue(&jobs, (Answer(tx.clone()), Tier::Metrics, true));
        }
        let refused: Vec<_> = rx.try_iter().collect();
        assert_eq!(refused.len(), 1000);
        assert!(refused.iter().all(|answer| answer == &Err(BUSY.to_owned())));
    }

    #[test]
    fn replacing_a_watch_keeps_the_new_one() {
        let hub = Hub::new();
        let channel = HostChannel::default();
        let first = hub.watch(&channel, &[Tier::Apps]);
        let second = hub.watch(&channel, &[Tier::Apps]);
        drop(first);
        assert_eq!(hub.receivers.lock().map(|list| list.len()).unwrap_or(0), 1);
        drop(second);
    }
}
