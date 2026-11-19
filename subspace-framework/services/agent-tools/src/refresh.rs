//! Board-agnostic refresh coordination (REQ-17/REQ-18): concurrent refreshes
//! of the same target share one fetch, a repeated refresh within the
//! cooldown is refused and reported (never silently served from cache), and
//! `refreshedAt` only ever increases for a target. A board picks its own
//! target keys, such as `item:42` or `scope`.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

/// Minimum interval between two performed refreshes of the same target.
const COOLDOWN_SECS: i64 = 5;

#[derive(Default)]
struct Shared {
    running: bool,
    /// Callers waiting for the running fetch; tests use it to order threads.
    waiting: usize,
    last_refreshed_at: Option<i64>,
    last_outcome: Option<Result<Value, String>>,
}

type Slot = Arc<(Mutex<Shared>, Condvar)>;

/// One board's refresh coordinator. A target is a string key; every target
/// coalesces and throttles independently of every other target.
// qual:api
#[derive(Default)]
pub struct Refresher {
    slots: Mutex<HashMap<String, Slot>>,
}

impl Refresher {
    fn slot(&self, key: &str) -> Slot {
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        slots
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new((Mutex::new(Shared::default()), Condvar::new())))
            .clone()
    }

    /// Runs `fetch` for `key` at most once per overlapping or throttled
    /// window; `fetch` must succeed with a JSON object. `now` is the
    /// Unix-second request time, used for both the cooldown check and the
    /// returned `refreshedAt`.
    pub fn run(
        &self,
        key: &str,
        now: i64,
        fetch: impl FnOnce() -> Result<Value, String>,
    ) -> Result<Value, String> {
        let slot = self.slot(key);
        let (lock, condvar) = &*slot;
        let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.running {
            return join_running(state, condvar);
        }
        if let Some(retry_after) = cooldown_remaining(state.last_refreshed_at, now, COOLDOWN_SECS) {
            return Ok(rate_limited(retry_after));
        }
        let previous = state.last_refreshed_at;
        state.running = true;
        drop(state);
        lead(&slot, previous, now, fetch)
    }
}

fn join_running(
    mut state: std::sync::MutexGuard<'_, Shared>,
    condvar: &Condvar,
) -> Result<Value, String> {
    state.waiting += 1;
    let mut state = condvar
        .wait_while(state, |state| state.running)
        .unwrap_or_else(|poison| poison.into_inner());
    state.waiting -= 1;
    let outcome = state
        .last_outcome
        .clone()
        .unwrap_or_else(|| Err("Refresh coalesced without a recorded outcome".into()));
    finish(outcome, true)
}

fn lead(
    slot: &Slot,
    previous: Option<i64>,
    now: i64,
    fetch: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    let (lock, condvar) = &**slot;
    let refreshed_at = monotonic(previous, now);
    let outcome = fetch().map(|value| with_refreshed_at(value, refreshed_at));
    let mut state = lock.lock().unwrap_or_else(|poison| poison.into_inner());
    state.last_refreshed_at = Some(refreshed_at);
    state.last_outcome = Some(outcome.clone());
    state.running = false;
    condvar.notify_all();
    drop(state);
    finish(outcome, false)
}

fn finish(outcome: Result<Value, String>, coalesced: bool) -> Result<Value, String> {
    outcome.map(|mut value| {
        if let Value::Object(map) = &mut value {
            map.insert("coalesced".into(), Value::Bool(coalesced));
            map.insert("rateLimited".into(), Value::Bool(false));
        }
        value
    })
}

fn with_refreshed_at(mut value: Value, refreshed_at: i64) -> Value {
    if let Value::Object(map) = &mut value {
        map.insert("refreshedAt".into(), Value::from(refreshed_at));
    }
    value
}

fn rate_limited(retry_after: i64) -> Value {
    json!({
        "refreshed": false,
        "rateLimited": true,
        "retryAfterSeconds": retry_after,
        "coalesced": false,
    })
}

/// `None` once `now` is at least `cooldown` seconds past `last`; otherwise
/// the whole remaining seconds, always at least one.
pub fn cooldown_remaining(last: Option<i64>, now: i64, cooldown: i64) -> Option<i64> {
    let elapsed = now.saturating_sub(last?);
    (elapsed < cooldown).then_some((cooldown - elapsed).max(1))
}

/// Strictly greater than `previous` whenever one exists, even if the clock
/// has not advanced or has gone backward.
pub fn monotonic(previous: Option<i64>, now: i64) -> i64 {
    match previous {
        Some(last) if now <= last => last + 1,
        _ => now,
    }
}

#[cfg(test)]
mod tests {
    use super::{Refresher, cooldown_remaining, monotonic};
    use serde_json::json;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cooldown_blocks_until_the_interval_has_elapsed() {
        assert_eq!(cooldown_remaining(None, 100, 5), None);
        assert_eq!(cooldown_remaining(Some(100), 104, 5), Some(1));
        assert_eq!(cooldown_remaining(Some(100), 105, 5), None);
        assert_eq!(cooldown_remaining(Some(100), 200, 5), None);
    }

    #[test]
    fn refreshed_at_never_goes_backward_or_stalls_within_the_same_second() {
        assert_eq!(monotonic(None, 100), 100);
        assert_eq!(monotonic(Some(100), 105), 105);
        assert_eq!(monotonic(Some(100), 100), 101);
        assert_eq!(monotonic(Some(100), 50), 101);
    }

    #[test]
    fn a_sequential_refresh_within_the_cooldown_is_reported_not_silently_served() {
        let refresher = Refresher::default();
        let calls = AtomicUsize::new(0);
        let first = refresher
            .run("item:1", 1_000, || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"documents": 1}))
            })
            .unwrap();
        assert_eq!(first["coalesced"], false);
        assert_eq!(first["rateLimited"], false);
        let second = refresher
            .run("item:1", 1_001, || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"documents": 1}))
            })
            .unwrap();
        assert_eq!(second["rateLimited"], true);
        assert!(second["retryAfterSeconds"].as_i64().unwrap() > 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// Callers currently waiting on `key`'s running fetch.
    fn waiting(refresher: &Refresher, key: &str) -> usize {
        let slots = refresher.slots.lock().unwrap();
        slots
            .get(key)
            .map_or(0, |slot| slot.0.lock().unwrap().waiting)
    }

    /// Blocks the leader's fetch until a follower waits on it, so the
    /// follower always joins the running fetch instead of racing it.
    fn hold_until_joined(refresher: &Refresher, key: &str) {
        while waiting(refresher, key) == 0 {
            std::thread::yield_now();
        }
    }

    #[test]
    fn two_concurrent_refreshes_of_the_same_target_share_one_fetch() {
        let refresher = Refresher::default();
        let calls = AtomicUsize::new(0);
        let started = Barrier::new(2);
        let (results, call_count) = std::thread::scope(|scope| {
            let leader = scope.spawn(|| {
                refresher.run("item:1", 1_000, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    started.wait();
                    hold_until_joined(&refresher, "item:1");
                    Ok(json!({"documents": 1}))
                })
            });
            started.wait();
            let follower = scope.spawn(|| {
                refresher.run("item:1", 1_000, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({"documents": 1}))
                })
            });
            (
                (leader.join().unwrap(), follower.join().unwrap()),
                calls.load(Ordering::SeqCst),
            )
        });
        let (leader, follower) = results;
        let (leader, follower) = (leader.unwrap(), follower.unwrap());
        assert_eq!(call_count, 1, "only one service call ran");
        assert_eq!(leader["coalesced"], false);
        assert_eq!(follower["coalesced"], true);
        assert_eq!(follower["refreshedAt"], leader["refreshedAt"]);
    }

    #[test]
    fn a_failed_leader_run_is_shared_with_its_followers() {
        let refresher = Refresher::default();
        let started = Barrier::new(2);
        std::thread::scope(|scope| {
            let leader = scope.spawn(|| {
                refresher.run("item:9", 2_000, || {
                    started.wait();
                    hold_until_joined(&refresher, "item:9");
                    Err("upstream failed".to_owned())
                })
            });
            started.wait();
            let follower =
                scope.spawn(|| refresher.run("item:9", 2_000, || Ok(json!({"documents": 1}))));
            assert!(leader.join().unwrap().is_err());
            assert_eq!(follower.join().unwrap().unwrap_err(), "upstream failed");
        });
    }
}
