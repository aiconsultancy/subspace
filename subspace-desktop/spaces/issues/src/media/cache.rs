use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
};

struct Entry<T> {
    issue: String,
    value: T,
    bytes: usize,
    used: Cell<u64>,
}

pub(super) struct Cache<T> {
    entries: HashMap<String, Entry<T>>,
    issues: VecDeque<String>,
    clock: Cell<u64>,
    limit: usize,
    bytes: usize,
    placeholder: T,
}

impl<T: Clone> Cache<T> {
    pub fn new(limit: usize, placeholder: T) -> Self {
        Self {
            entries: HashMap::new(),
            issues: VecDeque::new(),
            clock: Cell::new(0),
            limit,
            bytes: 0,
            placeholder,
        }
    }
    pub fn open(&mut self, issue: &str) {
        self.issues.retain(|old| old != issue);
        self.issues.push_back(issue.into());
        while self.issues.len() > 2 {
            self.issues.pop_front();
        }
        self.entries
            .retain(|_, entry| self.issues.contains(&entry.issue));
        self.recount();
    }
    pub fn get(&self, key: &str) -> Option<&T> {
        let entry = self.entries.get(key)?;
        self.clock.set(self.clock.get().wrapping_add(1));
        entry.used.set(self.clock.get());
        Some(&entry.value)
    }
    pub fn insert(
        &mut self,
        key: &str,
        issue: &str,
        (value, bytes): (T, usize),
        keep: Option<&str>,
    ) {
        self.store(key, issue, value, bytes);
        self.recount();
        self.evict(keep);
    }
    fn store(&mut self, key: &str, issue: &str, value: T, bytes: usize) {
        self.clock.set(self.clock.get().wrapping_add(1));
        self.entries.insert(
            key.into(),
            Entry {
                issue: issue.into(),
                value,
                bytes,
                used: Cell::new(self.clock.get()),
            },
        );
    }
    fn evict(&mut self, keep: Option<&str>) {
        while self.bytes > self.limit {
            let Some((_, entry)) = self
                .entries
                .iter_mut()
                .filter(|(_, entry)| entry.bytes > 0)
                .min_by_key(|(key, entry)| (keep == Some(key.as_str()), entry.used.get()))
            else {
                break;
            };
            self.bytes -= std::mem::take(&mut entry.bytes);
            entry.value = self.placeholder.clone();
        }
    }
    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
        self.recount();
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.issues.clear();
        self.bytes = 0;
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &T) -> bool) {
        self.entries
            .retain(|_, entry| keep(&entry.issue, &entry.value));
        self.recount();
    }
    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    fn recount(&mut self) {
        self.bytes = self.entries.values().map(|entry| entry.bytes).sum();
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;
    #[test]
    fn retains_two_issues_and_evicts_least_recently_used_within_budget() {
        let mut cache = Cache::new(12, 0);
        cache.open("A");
        cache.insert("a", "A", (1, 8), None);
        cache.open("B");
        cache.insert("b", "B", (2, 4), None);
        assert_eq!(cache.get("a"), Some(&1));
        cache.insert("c", "B", (3, 4), None);
        assert_eq!(cache.get("b"), Some(&0));
        assert_eq!(cache.get("a"), Some(&1));
        cache.open("C");
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("c"), Some(&3));
        cache.clear();
        assert_eq!(cache.get("c"), None);
        assert_eq!(cache.bytes(), 0);
    }
}
