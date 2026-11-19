//! A bounded preflight for readable fallback; the vendored parser contains panics.

use std::{
    collections::{VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{Mutex, OnceLock},
};

use gpui_kit::SharedString;
use markdown::ParseOptions;

const MAX_ENTRIES: usize = 256;
// Match MAX_SYNC_FULL_REPLACE_BYTES in gpui-base 0.9.1/src/text/state.rs:45.
const MAX_PREFLIGHT_BYTES: usize = 4 * 1024;

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

pub(crate) fn prepare(source: SharedString) -> SharedString {
    if !needs_preflight(&source) {
        return source;
    }
    let key = Key::new(&source);
    let cached = with_cache(|cache| cache.get(key));
    let valid = cached.unwrap_or_else(|| {
        // Never hold the cache lock while parsing.
        let valid = parses(&source);
        with_cache(|cache| cache.insert(key, valid));
        valid
    });
    if valid {
        source
    } else {
        super::escape_markdown(&source).into()
    }
}

fn with_cache<T>(action: impl FnOnce(&mut Cache) -> T) -> T {
    action(
        &mut CACHE
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

fn needs_preflight(source: &str) -> bool {
    source.len() <= MAX_PREFLIGHT_BYTES
        && ["```", "~~~", "$$"].iter().any(|run| source.contains(run))
}

/// Only metadata is retained, including across profile changes and sign-out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    hash: u64,
    len: usize,
}

impl Key {
    fn new(source: &str) -> Self {
        let mut hash = DefaultHasher::new();
        source.hash(&mut hash);
        Self {
            hash: hash.finish(),
            len: source.len(),
        }
    }
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<(Key, bool)>,
}

impl Cache {
    fn get(&mut self, key: Key) -> Option<bool> {
        let index = self.entries.iter().position(|entry| entry.0 == key)?;
        let entry = self.entries.remove(index)?;
        self.entries.push_back(entry);
        Some(entry.1)
    }

    fn insert(&mut self, key: Key, valid: bool) {
        self.entries.retain(|entry| entry.0 != key);
        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back((key, valid));
    }
}

fn parses(source: &str) -> bool {
    markdown::to_mdast(source, &kit_options()).is_ok()
}

/// Match MarkdownExtensions::default().parse_options() in gpui-base 0.9.1.
fn kit_options() -> ParseOptions {
    let mut options = ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    options
}

#[cfg(test)]
mod tests;
