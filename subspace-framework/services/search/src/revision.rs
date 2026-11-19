use crate::Document;
#[cfg(test)]
#[cfg(unix)]
mod tests;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(unix)]
#[path = "revision/unix.rs"]
mod platform;
#[cfg(not(unix))]
#[path = "revision/unsupported.rs"]
mod platform;
#[derive(Default)]
#[repr(C)]
struct Counters {
    revision: AtomicU64,
    writer: AtomicU64,
}

pub(crate) struct Revision {
    mapping: platform::Mapping,
}

impl Revision {
    pub const SUPPORTED: bool = platform::SUPPORTED;
    pub fn open(path: &Path) -> Result<Self, String> {
        Ok(Self {
            mapping: platform::Mapping::open(path)?,
        })
    }
    pub fn observed(&self) -> u64 {
        self.mapping.counters().revision.load(Ordering::SeqCst)
    }
    pub fn current(&self, observed: u64) -> bool {
        let counters = self.mapping.counters();
        counters.writer.load(Ordering::SeqCst) == 0
            && counters.revision.load(Ordering::SeqCst) == observed
    }
    pub fn revoke(&self) {
        self.mapping
            .counters()
            .revision
            .fetch_add(1, Ordering::SeqCst);
    }
    pub fn begin(&self) -> u64 {
        let counters = self.mapping.counters();
        let token = counters
            .revision
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        counters.writer.store(token, Ordering::SeqCst);
        token
    }
    pub fn finish(&self, token: u64) {
        let counters = self.mapping.counters();
        counters.revision.fetch_add(1, Ordering::SeqCst);
        let _ = counters
            .writer
            .compare_exchange(token, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
    pub fn busy(&self) -> bool {
        self.mapping.counters().writer.load(Ordering::SeqCst) != 0
    }
}

#[derive(Clone)]
pub struct ReadPermit {
    pub(crate) revision: Arc<Revision>,
    pub(crate) observed: u64,
    pub(crate) expires: i64,
}

impl ReadPermit {
    /// Nonblocking presentation checkpoint; keep this permit with the cached view
    /// and check again before rendering or using its content.
    pub fn valid(&self) -> bool {
        self.revision.current(self.observed) && crate::now() <= self.expires
    }
}

pub struct CachedRead {
    pub document: Document,
    pub permit: ReadPermit,
}

pub(crate) struct Mutation {
    revision: Arc<Revision>,
    token: u64,
}
impl Mutation {
    pub fn begin(revision: &Arc<Revision>) -> Self {
        Self {
            revision: revision.clone(),
            token: revision.begin(),
        }
    }
}
impl Drop for Mutation {
    fn drop(&mut self) {
        self.revision.finish(self.token);
    }
}
