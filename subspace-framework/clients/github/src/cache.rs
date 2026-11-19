//! A size-bounded LRU and a time-to-live cache. Both are behind a mutex, so the client
//! can be shared between threads.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Keeps values up to a total size and evicts the least recently used.
/// Linear in the number of entries, which stays small (20 diffs, a few
/// hundred file texts).
pub struct Lru<V> {
    inner: Mutex<LruInner<V>>,
    max_size: usize,
    size_of: fn(&V) -> usize,
}

struct LruInner<V> {
    /// Oldest first.
    entries: VecDeque<(String, V, usize)>,
    size: usize,
}

impl<V: Clone> Lru<V> {
    pub fn new(max_size: usize, size_of: fn(&V) -> usize) -> Self {
        Self {
            inner: Mutex::new(LruInner {
                entries: VecDeque::new(),
                size: 0,
            }),
            max_size,
            size_of,
        }
    }

    fn lock(&self) -> MutexGuard<'_, LruInner<V>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get(&self, key: &str) -> Option<V> {
        let mut inner = self.lock();
        let index = inner.entries.iter().position(|(known, ..)| known == key)?;
        let entry = inner.entries.remove(index)?;
        let value = entry.1.clone();
        inner.entries.push_back(entry);
        Some(value)
    }

    pub fn set(&self, key: String, value: V) {
        let size = (self.size_of)(&value);
        let mut inner = self.lock();
        if let Some(index) = inner.entries.iter().position(|(known, ..)| *known == key)
            && let Some((.., old)) = inner.entries.remove(index)
        {
            inner.size -= old;
        }
        if size > self.max_size {
            return;
        }
        inner.entries.push_back((key, value, size));
        inner.size += size;
        while inner.size > self.max_size {
            match inner.entries.pop_front() {
                Some((.., old)) => inner.size -= old,
                None => break,
            }
        }
    }
}

/// Keeps values for a fixed time. Failed loads are not kept.
pub struct Ttl<V> {
    entries: Mutex<HashMap<String, (Instant, V)>>,
    ttl: Duration,
}

impl<V: Clone> Ttl<V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, (Instant, V)>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The value for `key`, loading it when it is missing or expired. The
    /// lock is not held while `load` runs.
    pub fn get_or_load<E>(&self, key: &str, load: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
        let now = Instant::now();
        if let Some((at, value)) = self.lock().get(key)
            && now.duration_since(*at) < self.ttl
        {
            return Ok(value.clone());
        }
        let value = load()?;
        self.lock().insert(key.to_owned(), (now, value.clone()));
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Lru, Ttl};

    #[test]
    fn the_lru_evicts_the_least_recently_used_by_size() {
        let lru: Lru<String> = Lru::new(6, String::len);
        lru.set("a".into(), "aa".into());
        lru.set("b".into(), "bb".into());
        assert_eq!(lru.get("a").as_deref(), Some("aa"));
        lru.set("c".into(), "cccc".into());
        assert_eq!(lru.get("b"), None);
        assert_eq!(lru.get("a").as_deref(), Some("aa"));
        lru.set("big".into(), "x".repeat(7));
        assert_eq!(lru.get("big"), None);
        lru.set("a".into(), "a".into());
        assert_eq!(lru.get("c").as_deref(), Some("cccc"));
    }

    #[test]
    fn the_ttl_cache_keeps_successes_only() {
        let cache: Ttl<u32> = Ttl::new(Duration::from_secs(60));
        assert_eq!(cache.get_or_load("k", || Err::<u32, ()>(())), Err(()));
        assert_eq!(cache.get_or_load("k", || Ok::<u32, ()>(1)), Ok(1));
        assert_eq!(cache.get_or_load("k", || Ok::<u32, ()>(2)), Ok(1));
        let expired: Ttl<u32> = Ttl::new(Duration::ZERO);
        assert_eq!(expired.get_or_load("k", || Ok::<u32, ()>(1)), Ok(1));
        assert_eq!(expired.get_or_load("k", || Ok::<u32, ()>(2)), Ok(2));
    }
}
