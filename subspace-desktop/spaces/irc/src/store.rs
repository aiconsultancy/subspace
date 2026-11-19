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
use subspace_framework::dashboard::full_text::{Filters, Policy, RecordQuery, Scope, Service};
use subspace_framework::dashboard::search_adapter::inactive_scope;

pub(crate) use mapping::{Navigation, document_id, navigation, scope, stored};

use crate::state::LINE_CAP;

/// The most records one write publishes, as the store allows.
const BATCH: usize = 100;
/// The most buffers a restore reads.
const MAX_BUFFERS: usize = 1000;
/// Records per page while restoring.
const PAGE: usize = 1000;

/// One operation on the store, run in the order the hub queued it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    /// Stores these lines; lines that are not stored are skipped.
    Publish(Vec<Line>),
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
            Op::Publish(lines) => self.publish(lines, now),
            Op::Forget(buffer) => self.forget(buffer),
        }
    }

    fn publish(&self, lines: &[Line], now: DateTime<Utc>) -> Result<(), String> {
        let records: Vec<(Line, _)> = lines
            .iter()
            .filter_map(|line| Some((line.clone(), mapping::document(line, now)?)))
            .collect();
        for batch in records.chunks(BATCH) {
            let documents: Vec<_> = batch.iter().map(|(_, document)| document.clone()).collect();
            self.service.upsert(&self.scope, &documents)?;
            let metadata: Vec<_> = batch
                .iter()
                .map(|(line, document)| (document.id.clone(), mapping::metadata(line)))
                .collect();
            self.service.index_metadata_batch(&self.scope, &metadata)?;
        }
        Ok(())
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

    /// Each stored buffer's newest `LINE_CAP` lines, oldest first. `None`
    /// when the store is off.
    pub fn restore(&self) -> Result<Option<Vec<(BufferKey, Vec<Line>)>>, String> {
        self.scoped(Self::read_all)
    }

    fn read_all(&self) -> Result<Vec<(BufferKey, Vec<Line>)>, String> {
        let groups = self
            .service
            .groups(&self.scope, &Filters::default(), MAX_BUFFERS)?;
        let mut buffers = Vec::new();
        for group in groups {
            let lines = self.read_buffer(&group.parent_id)?;
            if let Some(first) = lines.first() {
                buffers.push((first.buffer.clone(), lines));
            }
        }
        Ok(buffers)
    }

    /// The newest `LINE_CAP` lines of the buffer with record parent
    /// `parent`, oldest first, all under one buffer key.
    fn read_buffer(&self, parent: &str) -> Result<Vec<Line>, String> {
        let mut query = RecordQuery {
            filters: Filters {
                parent_id: Some(parent.to_owned()),
                ..Default::default()
            },
            before: None,
            limit: PAGE.min(LINE_CAP),
        };
        let mut lines = Vec::new();
        while lines.len() < LINE_CAP {
            let page = self.service.records(&self.scope, &query)?;
            lines.extend(page.documents.iter().filter_map(mapping::line));
            match page.next {
                Some(next) => query.before = Some(next),
                None => break,
            }
        }
        Ok(ordered(lines))
    }
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
