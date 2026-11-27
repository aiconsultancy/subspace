//! One shared sampler thread for every view.
//!
//! Views subscribe to the tiers they show and drop the [`Subscription`] when
//! they are hidden or lose their grant. The thread samples only tiers with
//! subscribers and exits when no metrics or apps subscriber remains; the next
//! subscription starts it again. Readings stay in memory.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::metrics::Collector;
use crate::{Apps, FactsInput, Metrics, Tier, now_ms, read_facts, to_json};

/// Receives each new reading on the sampler thread. The host forwards it
/// with `host.send` to the views granted the tier.
pub type Listener = Arc<dyn Fn(Tier, &Value) + Send + Sync>;

/// Shares sampling between views. Cheap to clone.
#[derive(Clone)]
pub struct Sampler {
    inner: Arc<Inner>,
}

struct Inner {
    state: Mutex<State>,
    wake: Condvar,
    collector: Mutex<Option<Collector>>,
    listener: Listener,
    metrics_every: Duration,
    apps_every: Duration,
}

#[derive(Default)]
struct State {
    subscribers: BTreeMap<Tier, usize>,
    latest: HashMap<Tier, Value>,
    facts_input: FactsInput,
    running: bool,
}

impl State {
    fn wants(&self, tier: Tier) -> bool {
        self.subscribers.get(&tier).is_some_and(|&n| n > 0)
    }

    fn needs_thread(&self) -> bool {
        self.wants(Tier::Metrics) || self.wants(Tier::Apps)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps its tiers sampled until dropped.
#[must_use = "sampling stops when the subscription is dropped"]
pub struct Subscription {
    inner: Arc<Inner>,
    tiers: Vec<Tier>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        for tier in &self.tiers {
            if let Some(count) = state.subscribers.get_mut(tier) {
                *count = count.saturating_sub(1);
            }
        }
        drop(state);
        self.inner.wake.notify_all();
    }
}

impl Sampler {
    /// Samples metrics at 1 Hz and apps every 2 s.
    pub fn new(listener: Listener) -> Self {
        Self::with_intervals(listener, Duration::from_secs(1), Duration::from_secs(2))
    }

    pub fn with_intervals(
        listener: Listener,
        metrics_every: Duration,
        apps_every: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                wake: Condvar::new(),
                collector: Mutex::new(None),
                listener,
                metrics_every,
                apps_every,
            }),
        }
    }

    /// Sets the facts only the host knows, such as displays and version.
    // qual:api — the TSX Space host calls it; tests cover it.
    pub fn set_facts_input(&self, input: FactsInput) {
        let mut state = lock(&self.inner.state);
        state.facts_input = input;
        state.latest.remove(&Tier::Facts);
    }

    /// Starts sampling `tiers` for one view. Facts are read on demand only.
    pub fn subscribe(&self, tiers: &[Tier]) -> Subscription {
        let mut state = lock(&self.inner.state);
        for &tier in tiers {
            *state.subscribers.entry(tier).or_default() += 1;
        }
        if state.needs_thread() && !state.running {
            state.running = true;
            let inner = Arc::clone(&self.inner);
            std::thread::Builder::new()
                .name("subspace-tsx-system".into())
                .spawn(move || run(&inner))
                .map_err(|_| state.running = false)
                .ok();
        }
        drop(state);
        self.inner.wake.notify_all();
        Subscription {
            inner: Arc::clone(&self.inner),
            tiers: tiers.to_vec(),
        }
    }

    /// The latest reading for `tier`, sampling once if there is none.
    pub fn latest(&self, tier: Tier) -> Value {
        if let Some(value) = lock(&self.inner.state).latest.get(&tier) {
            return value.clone();
        }
        let value = sample(&self.inner, tier);
        lock(&self.inner.state)
            .latest
            .entry(tier)
            .or_insert(value)
            .clone()
    }

    /// Whether the sampler thread is running.
    pub fn is_running(&self) -> bool {
        lock(&self.inner.state).running
    }
}

fn sample(inner: &Inner, tier: Tier) -> Value {
    match tier {
        Tier::Facts => {
            let input = lock(&inner.state).facts_input.clone();
            to_json(&read_facts(input))
        }
        Tier::Metrics | Tier::Apps => {
            let mut slot = lock(&inner.collector);
            if slot.is_none() {
                *slot = Collector::new();
            }
            // A platform `sysinfo` cannot read gives readings of nulls.
            match (slot.as_mut(), tier) {
                (Some(collector), Tier::Metrics) => to_json(&collector.metrics()),
                (Some(collector), _) => to_json(&collector.apps()),
                (None, Tier::Metrics) => to_json(&Metrics {
                    at: now_ms(),
                    ..Metrics::default()
                }),
                (None, _) => to_json(&Apps {
                    at: now_ms(),
                    apps: Vec::new(),
                }),
            }
        }
    }
}

fn run(inner: &Inner) {
    let mut due: BTreeMap<Tier, Instant> = BTreeMap::new();
    loop {
        let now = Instant::now();
        let ready: Vec<Tier> = {
            let mut state = lock(&inner.state);
            if !state.needs_thread() {
                state.running = false;
                return;
            }
            [Tier::Metrics, Tier::Apps]
                .into_iter()
                .filter(|&tier| state.wants(tier))
                .filter(|tier| due.get(tier).is_none_or(|&at| at <= now))
                .collect()
        };
        for tier in ready {
            let every = if tier == Tier::Metrics {
                inner.metrics_every
            } else {
                inner.apps_every
            };
            due.insert(tier, now + every);
            let value = sample(inner, tier);
            lock(&inner.state).latest.insert(tier, value.clone());
            (inner.listener)(tier, &value);
        }
        let state = lock(&inner.state);
        let next = [Tier::Metrics, Tier::Apps]
            .into_iter()
            .filter(|&tier| state.wants(tier))
            .filter_map(|tier| due.get(&tier).copied())
            .min()
            .unwrap_or(now);
        let wait = next.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            drop(
                inner
                    .wake
                    .wait_timeout(state, wait)
                    .unwrap_or_else(PoisonError::into_inner),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn sampler() -> (Sampler, mpsc::Receiver<(Tier, Value)>) {
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let listener: Listener = Arc::new(move |tier, value: &Value| {
            lock(&sender).send((tier, value.clone())).ok();
        });
        let sampler = Sampler::with_intervals(
            listener,
            Duration::from_millis(20),
            Duration::from_millis(40),
        );
        (sampler, receiver)
    }

    fn wait_until(condition: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn samples_only_subscribed_tiers_and_stops_when_unused() {
        let (sampler, readings) = sampler();
        assert!(!sampler.is_running());
        let first = sampler.subscribe(&[Tier::Metrics]);
        let second = sampler.subscribe(&[Tier::Metrics]);
        assert!(sampler.is_running());
        for _ in 0..3 {
            let (tier, value) = readings
                .recv_timeout(Duration::from_secs(5))
                .expect("reading");
            assert_eq!(tier, Tier::Metrics);
            assert!(value["at"].is_u64());
        }
        drop(first);
        std::thread::sleep(Duration::from_millis(60));
        assert!(sampler.is_running(), "one view still subscribes");
        drop(second);
        assert!(wait_until(|| !sampler.is_running()));
        while readings.try_recv().is_ok() {}
        std::thread::sleep(Duration::from_millis(80));
        assert!(readings.try_recv().is_err(), "no samples after stopping");

        let _again = sampler.subscribe(&[Tier::Apps]);
        let (tier, value) = readings
            .recv_timeout(Duration::from_secs(5))
            .expect("reading");
        assert_eq!(tier, Tier::Apps);
        assert!(value["apps"].is_array());
    }

    #[test]
    fn facts_alone_start_no_thread() {
        let (sampler, _readings) = sampler();
        let _facts = sampler.subscribe(&[Tier::Facts]);
        assert!(!sampler.is_running());
        sampler.set_facts_input(FactsInput {
            subspace_version: Some("9.9.9".into()),
            displays: None,
        });
        assert_eq!(sampler.latest(Tier::Facts)["subspaceVersion"], "9.9.9");
    }

    #[test]
    fn latest_samples_once_without_a_thread() {
        let (sampler, _readings) = sampler();
        let metrics = sampler.latest(Tier::Metrics);
        assert!(metrics["cores"].is_array());
        assert_eq!(sampler.latest(Tier::Metrics), metrics, "cached");
        assert!(!sampler.is_running());
    }
}
