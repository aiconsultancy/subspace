//! Revoking site sessions. A grant's revocation runs once, off the UI
//! thread: when the grant is dropped (a renewal replaced it, the account
//! changed or signed out, or the Space closed), or when the app quits.
//! Quitting revokes every live grant and waits, for at most a bound, for
//! all revocations to finish, so none dies with the process unattempted.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

pub(super) type Revoke = Box<dyn FnOnce() + Send>;

/// The longest quitting waits for revocations to finish.
pub(super) const QUIT_BOUND: Duration = Duration::from_secs(2);

/// The app's revocations.
pub(super) static REVOCATIONS: Revocations = Revocations::new();

pub(super) struct Revocations {
    next: AtomicU64,
    /// The revocations of grants that are still live.
    live: Mutex<BTreeMap<u64, Revoke>>,
    /// How many revocations are running.
    running: Mutex<usize>,
    finished: Condvar,
}

/// A live grant's revocation, which starts when this is dropped.
pub(super) struct Revocation {
    id: u64,
    owner: &'static Revocations,
}

impl Revocation {
    /// Runs `revoke` instead of the revocation registered before, which
    /// never runs.
    pub(super) fn replace(&self, revoke: Revoke) {
        // Either closure may own a grant: drop whichever is left over only
        // after the lock is released.
        let left_over = {
            let mut live = lock(&self.owner.live);
            match live.get_mut(&self.id) {
                Some(held) => std::mem::replace(held, revoke),
                None => revoke,
            }
        };
        drop(left_over);
    }
}

impl Drop for Revocation {
    fn drop(&mut self) {
        self.owner.release(self.id);
    }
}

impl Revocations {
    pub(super) const fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            live: Mutex::new(BTreeMap::new()),
            running: Mutex::new(0),
            finished: Condvar::new(),
        }
    }

    /// Keeps `revoke` until the returned handle is dropped or the app quits.
    pub(super) fn register(&'static self, revoke: Revoke) -> Revocation {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.live).insert(id, revoke);
        Revocation { id, owner: self }
    }

    /// Revokes every live grant, then waits at most `bound` for all
    /// revocations, earlier ones included, to finish. Returns whether they
    /// did.
    pub(super) fn revoke_all(&'static self, bound: Duration) -> bool {
        let live = std::mem::take(&mut *lock(&self.live));
        for revoke in live.into_values() {
            self.spawn(revoke);
        }
        self.wait(bound)
    }

    fn release(&'static self, id: u64) {
        let revoke = lock(&self.live).remove(&id);
        if let Some(revoke) = revoke {
            self.spawn(revoke);
        }
    }

    fn spawn(&'static self, revoke: Revoke) {
        *lock(&self.running) += 1;
        let started = std::thread::Builder::new()
            .name("web-space-revoke".into())
            .spawn(move || {
                let _finished = Finished(self);
                revoke();
            });
        if started.is_err() {
            self.finish();
        }
    }

    fn finish(&self) {
        let mut running = lock(&self.running);
        *running = running.saturating_sub(1);
        self.finished.notify_all();
    }

    fn wait(&self, bound: Duration) -> bool {
        let running = lock(&self.running);
        let (running, _) = self
            .finished
            .wait_timeout_while(running, bound, |running| *running > 0)
            .unwrap_or_else(PoisonError::into_inner);
        *running == 0
    }
}

/// Counts a revocation finished, even if it panicked.
struct Finished(&'static Revocations);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.finish();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::Revocations;

    fn revocations() -> &'static Revocations {
        Box::leak(Box::new(Revocations::new()))
    }

    #[test]
    fn dropping_a_handle_revokes_once_off_the_calling_thread() {
        let revocations = revocations();
        let caller = std::thread::current().id();
        let (sender, received) = mpsc::channel();
        let handle = revocations.register(Box::new(move || {
            let _ = sender.send(std::thread::current().id());
        }));
        drop(handle);
        let thread = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(thread, caller);
        assert!(revocations.wait(Duration::from_secs(5)));
        assert!(received.recv_timeout(Duration::from_millis(50)).is_err());
    }

    #[test]
    fn quitting_revokes_live_grants_and_waits_for_them() {
        let revocations = revocations();
        let (sender, received) = mpsc::channel();
        let handles = (0..3)
            .map(|index| {
                let sender = sender.clone();
                revocations.register(Box::new(move || {
                    std::thread::sleep(Duration::from_millis(50));
                    let _ = sender.send(index);
                }))
            })
            .collect::<Vec<_>>();
        assert!(revocations.revoke_all(Duration::from_secs(5)));
        let mut revoked = received.try_iter().collect::<Vec<_>>();
        revoked.sort_unstable();
        assert_eq!(revoked, [0, 1, 2]);
        drop(handles);
        assert!(revocations.wait(Duration::from_secs(1)));
        assert!(received.try_recv().is_err(), "a quit revocation runs once");
    }

    #[test]
    fn quitting_waits_no_longer_than_its_bound() {
        let revocations = revocations();
        let (release, hold) = mpsc::channel::<()>();
        let _handle = revocations.register(Box::new(move || {
            let _ = hold.recv_timeout(Duration::from_secs(30));
        }));
        let started = Instant::now();
        assert!(!revocations.revoke_all(Duration::from_millis(100)));
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(release);
        assert!(revocations.wait(Duration::from_secs(5)));
    }

    #[test]
    fn a_replaced_revocation_never_runs() {
        let revocations = revocations();
        let (sender, received) = mpsc::channel();
        let first = sender.clone();
        let handle = revocations.register(Box::new(move || {
            let _ = first.send("first");
        }));
        handle.replace(Box::new(move || {
            let _ = sender.send("second");
        }));
        assert!(received.try_recv().is_err(), "replacing revokes nothing");
        drop(handle);
        assert_eq!(received.recv_timeout(Duration::from_secs(5)), Ok("second"));
        assert!(revocations.wait(Duration::from_secs(5)));
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn replacing_a_revocation_that_owns_a_grant_does_not_deadlock() {
        let revocations = revocations();
        let (sender, received) = mpsc::channel();
        let owned = revocations.register(Box::new(move || {
            let _ = sender.send("owned");
        }));
        let handle = revocations.register(Box::new(move || drop(owned)));
        handle.replace(Box::new(|| {}));
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)),
            Ok("owned"),
            "dropping the replaced revocation released the grant it owned"
        );
        drop(handle);
        assert!(revocations.wait(Duration::from_secs(5)));
    }

    #[test]
    fn a_revocation_that_panics_still_counts_as_finished() {
        let revocations = revocations();
        let handle = revocations.register(Box::new(|| panic!("fixture revocation failed")));
        drop(handle);
        assert!(revocations.wait(Duration::from_secs(5)));
    }
}
