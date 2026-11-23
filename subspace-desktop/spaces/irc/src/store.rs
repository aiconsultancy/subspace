//! IRC's messages in the Framework's SQLite full-text store: the one place
//! they persist. Every stored line is one record (`mapping`); the hub
//! publishes them in order, reads them back to restore buffers on start, and
//! deletes a buffer's records when the user removes it. Search, chat and MCP
//! query the same records. The Search view's **Cache Channels** switch turns it
//! on or off; off erases the records and keeps lines in memory only.
//!
//! Every call blocks, so the hub runs them on the background executor.

mod links;
mod mapping;

#[cfg(test)]
pub(crate) mod tests;

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, Line};
use subspace_framework::dashboard::full_text::{
    Document, Filters, IndexMetadata, Policy, RecordQuery, Scope, Service,
};
use subspace_framework::dashboard::search_adapter::inactive_scope;

pub(crate) use mapping::{Ended, Navigation, document_id, ended, navigation, scope, stored};

use crate::state::LINE_CAP;
use crate::thread_status::REMEMBERED;

/// The most records one write publishes, as the store allows.
const BATCH: usize = 100;
/// The most buffers a restore reads.
const MAX_BUFFERS: usize = 1000;
/// Records per page while restoring.
const PAGE: usize = 1000;

/// One operation on the store, run in the order the hub queued it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    /// Stores these lines; lines that are not stored are skipped. Only
    /// tests queue lines without endings; the hub always sends a `Commit`.
    #[cfg(test)]
    Publish(Vec<Line>),
    /// Stores these request endings, then these lines, in the same batches,
    /// so each batch of up to `BATCH` records commits as one.
    Commit { ended: Vec<Ended>, lines: Vec<Line> },
    /// Deletes every record of this buffer.
    Forget(BufferKey),
}

/// The store and the board's scope in it, as background work takes them.
#[derive(Clone)]
pub(crate) struct Link {
    pub service: Service,
    pub scope: Scope,
}

impl Link {
    pub fn new(service: Service) -> Self {
        let scope = scope(service.app_id());
        Self { service, scope }
    }

    /// Whether the person keeps IRC messages on this computer.
    pub fn enabled(&self) -> bool {
        self.service
            .policy(crate::ID)
            .is_ok_and(|policy: Policy| policy.enabled)
    }

    /// Runs `call` in the board's scope, making the scope active first
    /// when it is not, as after the person enabled or cleared the source.
    /// `None` when the store is off.
    fn scoped<T>(&self, call: impl Fn(&Self) -> Result<T, String>) -> Result<Option<T>, String> {
        if !self.enabled() {
            return Ok(None);
        }
        match call(self) {
            Err(error) if inactive_scope(&error) => {
                self.activate()?;
                call(self).map(Some)
            }
            result => result.map(Some),
        }
    }

    fn activate(&self) -> Result<(), String> {
        let ticket = self.service.begin_activation(crate::ID)?;
        let activation = self
            .service
            .prepare_activation_ticket(&ticket, &self.scope)?;
        self.service.activate_prepared(&activation)
    }

    /// Runs `ops` in order; the first failure of each is returned, and the
    /// rest still run.
    pub fn run(&self, ops: Vec<Op>, now: DateTime<Utc>) -> Vec<String> {
        ops.into_iter()
            .filter_map(|op| self.scoped(|link| link.apply(&op, now)).err())
            .collect()
    }

    fn apply(&self, op: &Op, now: DateTime<Utc>) -> Result<(), String> {
        match op {
            #[cfg(test)]
            Op::Publish(lines) => self.publish(&[], lines, now),
            Op::Commit { ended, lines } => self.publish(ended, lines, now),
            Op::Forget(buffer) => self.forget(buffer),
        }
    }

    /// Writes `ended`, then `lines`, in batches of at most `BATCH` records,
    /// each one store transaction, with the lines' link metadata after it.
    /// An ending goes before the lines it may concern, so a write cut short
    /// can leave an ending without its notice, never the reverse.
    fn publish(&self, ended: &[Ended], lines: &[Line], now: DateTime<Utc>) -> Result<(), String> {
        let endings = ended.iter().filter_map(|ended| {
            Some((mapping::ending_document(ended, now)?, None::<IndexMetadata>))
        });
        let messages = lines.iter().filter_map(|line| {
            let document = mapping::document(line, now)?;
            Some((document, Some(mapping::metadata(line))))
        });
        let records: Vec<(Document, Option<IndexMetadata>)> = endings.chain(messages).collect();
        records
            .chunks(BATCH)
            .try_for_each(|batch| self.write(batch))
    }

    /// One batch: its records, then the link metadata of its lines.
    fn write(&self, batch: &[(Document, Option<IndexMetadata>)]) -> Result<(), String> {
        let documents: Vec<Document> = batch.iter().map(|(document, _)| document.clone()).collect();
        self.service.upsert(&self.scope, &documents)?;
        let metadata: Vec<(String, IndexMetadata)> = batch
            .iter()
            .filter_map(|(document, metadata)| Some((document.id.clone(), metadata.clone()?)))
            .collect();
        if metadata.is_empty() {
            return Ok(());
        }
        self.service.index_metadata_batch(&self.scope, &metadata)
    }

    fn forget(&self, buffer: &BufferKey) -> Result<(), String> {
        let filters = Filters {
            parent_id: Some(mapping::parent_id(buffer)),
            ..Default::default()
        };
        self.service
            .remove_matching(&self.scope, &filters)
            .map(|_| ())
    }

    /// Each stored buffer's newest `LINE_CAP` lines, oldest first, with its
    /// newest `REMEMBERED` request endings, oldest first, also when no line
    /// survives. `None` when the store is off.
    pub fn restore(&self) -> Result<Option<Vec<Restored>>, String> {
        self.scoped(Self::read_all)
    }

    fn read_all(&self) -> Result<Vec<Restored>, String> {
        let groups = self
            .service
            .groups(&self.scope, &Filters::default(), MAX_BUFFERS)?;
        let mut buffers = Vec::new();
        for group in groups {
            let parent = group.parent_id.as_str();
            let lines = ordered(self.read_records(parent, None, LINE_CAP, mapping::line)?);
            let kind = Some(mapping::ENDING_KIND);
            let ended = self.read_records(parent, kind, REMEMBERED, mapping::ended)?;
            buffers.extend(restored(lines, ended));
        }
        Ok(buffers)
    }

    /// The newest `cap` records of record parent `parent`, of `kind` when
    /// given, as `map` reads them, newest first.
    fn read_records<T>(
        &self,
        parent: &str,
        kind: Option<&str>,
        cap: usize,
        map: impl Fn(&Document) -> Option<T>,
    ) -> Result<Vec<T>, String> {
        let mut query = RecordQuery {
            filters: Filters {
                parent_id: Some(parent.to_owned()),
                kind: kind.map(str::to_owned),
                ..Default::default()
            },
            before: None,
            limit: PAGE.min(cap),
        };
        let mut found = Vec::new();
        while found.len() < cap {
            let page = self.service.records(&self.scope, &query)?;
            found.extend(page.documents.iter().filter_map(&map));
            match page.next {
                Some(next) => query.before = Some(next),
                None => break,
            }
        }
        found.truncate(cap);
        Ok(found)
    }
}

/// One stored buffer as it restores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Restored {
    pub buffer: BufferKey,
    /// Its newest lines, oldest first; maybe none.
    pub lines: Vec<Line>,
    /// Its newest ended requests by message ID, with how each ended,
    /// oldest first.
    pub ended: Vec<(String, crate::thread_status::Ending)>,
}

/// The buffer `lines` (oldest first) and `ended` (newest first) restore,
/// filed under the lines' spelling, else the newest ending's. `None` when
/// both are empty.
fn restored(lines: Vec<Line>, ended: Vec<Ended>) -> Option<Restored> {
    let buffer = lines
        .first()
        .map(|line| line.buffer.clone())
        .or_else(|| ended.first().map(|ended| ended.buffer.clone()))?;
    let ended = ended
        .into_iter()
        .rev()
        .map(|ended| {
            let ending = ended.ending();
            (ended.request, ending)
        })
        .collect();
    Some(Restored {
        buffer,
        lines,
        ended,
    })
}

/// Restored lines, oldest first by their exact time, all filed under the
/// spelling of the newest: records order by the second only.
fn ordered(mut lines: Vec<Line>) -> Vec<Line> {
    lines.truncate(LINE_CAP);
    let Some(buffer) = lines.first().map(|line| line.buffer.clone()) else {
        return lines;
    };
    lines.sort_by_key(|line| line.time);
    for line in &mut lines {
        line.buffer = buffer.clone();
    }
    lines
}
