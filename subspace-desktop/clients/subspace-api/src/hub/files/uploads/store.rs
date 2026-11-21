//! Standalone uploads, in one store. Each is kept under the stable ID of
//! the issue it was added to, known when the file is added, since a file is
//! only ever added to an issue whose detail is read. The open issue shows
//! its own. When an issue closes, its uploads still under way, or whose
//! bytes may be stored and wait for a check that can still change the
//! outcome, are kept, and show again however the issue opens next: under
//! the reference it was open under, by its ID, or under a new key after a
//! move once its detail is read. The rest, which stored nothing, are let
//! go and their reservations cancelled under the issue's ID.
//!
//! An upload whose completion may have landed is never cancelled by the
//! store: only its outcome, a check or the person's Dismiss, ends it. What
//! the store does let go is its bytes, which a check does not need, once no
//! attempt is under way to hold them too: once its issue has been closed
//! for [`KEPT_FOR`], once more than [`MAX_CLOSED`] closed issues hold
//! bytes, or when a new file would not fit in the shared budget and
//! letting them go is what lets it in. Past [`MAX_UNRESOLVED`] closed
//! uploads that only wait for a check, the oldest are forgotten, still
//! without cancelling. Every use of the store, a snapshot included, first
//! follows the open issue, so this needs no timer; a use that finds the
//! open issue and the store as the last one left them, with no bytes
//! fallen due, does no work, and a snapshot shows the open issue's uploads
//! without going through the rest.
use super::AttachmentUpload;
use crate::hub::IssueSnapshot;
use crate::uploads::UploadError;
use std::time::{Duration, Instant};

/// How long a closed issue's uploads keep their bytes for it to open again.
pub(in crate::hub) const KEPT_FOR: Duration = Duration::from_secs(15 * 60);
/// Closed issues whose uploads keep their bytes at once. Past this, the
/// issue that closed first lets its uploads' bytes go.
const MAX_CLOSED: usize = 8;
/// Closed uploads that only wait for a check, their bytes gone, kept at
/// once. Past this, the oldest are forgotten, never cancelled: their
/// completion may have landed.
const MAX_UNRESOLVED: usize = 64;

/// A reservation to cancel: the issue it was made on and its transfer.
pub(in crate::hub) type Cancel = (String, String);

/// The issue an upload was added to: its stable ID, and the reference it
/// was open under then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::hub) struct UploadIssue {
    pub(in crate::hub) id: String,
    pub(in crate::hub) identifier: String,
}

/// The open issue, as the store matches uploads to it.
#[derive(Clone, PartialEq, Eq)]
pub(in crate::hub) struct Open {
    opening: u64,
    identifier: String,
    /// Its ID, once its detail is read.
    id: Option<String>,
}

impl Open {
    /// The open issue `issue`, if it is a tracker issue that can hold
    /// uploads.
    fn of(issue: &IssueSnapshot) -> Option<Self> {
        (issue.profile_id == crate::PROFILE_ID).then(|| Self {
            opening: issue.opening,
            identifier: issue.identifier.clone(),
            id: issue.detail.as_ref().map(|detail| detail.fields.id.clone()),
        })
    }

    /// Whether this is `issue`: by ID once the detail is read, otherwise by
    /// the reference it opened under, a key or the ID.
    fn names(&self, issue: &UploadIssue) -> bool {
        match &self.id {
            Some(id) => &issue.id == id,
            None => issue.identifier == self.identifier || issue.id == self.identifier,
        }
    }

    /// Whether an upload of `issue` last shown in `opening` shows here. In
    /// the same opening it stays shown while the detail is read again.
    fn shows(&self, issue: &UploadIssue, opening: u64) -> bool {
        self.names(issue) || (self.id.is_none() && opening == self.opening)
    }
}

/// One upload in the store.
struct Entry {
    upload: AttachmentUpload,
    issue: UploadIssue,
    /// The opening of its issue it was last shown in.
    opening: u64,
    /// When its issue stopped being open; `None` while it is.
    closed: Option<Instant>,
}

impl Entry {
    /// Shows it in `open` if it belongs there; otherwise it counts as
    /// closed from `now`, unless it already was.
    fn follow(&mut self, open: Option<&Open>, now: Instant) {
        match open.filter(|open| open.shows(&self.issue, self.opening)) {
            Some(open) => {
                self.opening = open.opening;
                self.closed = None;
            }
            None => {
                self.closed.get_or_insert(now);
            }
        }
    }

    /// Whether it is still kept: shown, or closed and still worth keeping.
    /// What stored nothing is not.
    fn kept(&self) -> bool {
        self.closed.is_none() || self.upload.worth_keeping()
    }

    /// When its issue closed, if no attempt is under way: only then does
    /// letting its bytes go free them, and an attempt that fails before
    /// sending can still be retried.
    fn settled_since(&self) -> Option<Instant> {
        self.closed.filter(|_| !self.upload.in_flight())
    }

    /// When its issue closed, if it still holds bytes for it.
    fn holding_since(&self) -> Option<Instant> {
        self.closed.filter(|_| !self.upload.released)
    }

    /// When its bytes fall due to go: [`KEPT_FOR`] after its issue closed,
    /// while it holds them and no attempt is under way.
    fn due(&self) -> Option<Instant> {
        self.settled_since()
            .filter(|_| !self.upload.released)
            .and_then(|closed| closed.checked_add(KEPT_FOR))
    }

    /// When its issue closed, if it only waits for a check: its bytes gone
    /// and no attempt under way.
    fn unresolved_since(&self) -> Option<Instant> {
        self.settled_since().filter(|_| self.upload.released)
    }

    /// When its issue closed, if letting its bytes go makes room: a closed
    /// issue's upload with no attempt under way, whose bytes nothing else
    /// holds, so letting them go frees them.
    fn idle_since(&self) -> Option<Instant> {
        self.settled_since().filter(|_| {
            !self.upload.released && std::sync::Arc::strong_count(&self.upload.bytes) == 1
        })
    }

    /// The reservation to cancel as it is let go.
    fn let_go(&self) -> Option<Cancel> {
        self.upload.cancel_on(&self.issue.id)
    }
}

/// Every standalone upload, the earliest added first.
#[derive(Default)]
pub(in crate::hub) struct UploadStore {
    entries: Vec<Entry>,
    /// What the last follow left, while nothing in the store changed since.
    followed: Option<Followed>,
}

/// What following the store left. Until the open issue or the store
/// changes, or a closed upload's bytes fall due, following again would
/// change nothing.
struct Followed {
    /// The open issue it followed.
    open: Option<Open>,
    /// When the first closed upload's bytes fall due.
    due: Option<Instant>,
    /// Where the uploads it showed are in the store.
    shown: Vec<usize>,
}

impl Followed {
    /// Whether following `open` at `now` would change nothing.
    fn holds(&self, open: Option<&Open>, now: Instant) -> bool {
        self.open.as_ref() == open && self.due.is_none_or(|due| now < due)
    }
}

impl UploadStore {
    /// Forgets every upload, without cancelling: for an account or hub that
    /// is gone, whose reservations no request can reach any more.
    pub(in crate::hub) fn clear(&mut self) {
        self.followed = None;
        self.entries.clear();
    }

    /// Adds `upload`, of `issue`, open in `opening`.
    pub(in crate::hub) fn add(
        &mut self,
        upload: AttachmentUpload,
        issue: UploadIssue,
        opening: u64,
    ) {
        self.followed = None;
        self.entries.push(Entry {
            upload,
            issue,
            opening,
            closed: None,
        });
    }

    /// Follows the open issue at `now`, unless the last follow still holds;
    /// see [`UploadStore::follow_all`]. Returns the reservations to cancel.
    fn follow(&mut self, open: Option<&Open>, now: Instant) -> Vec<Cancel> {
        if self.holds(open, now) {
            return Vec::new();
        }
        self.follow_all(open, now)
    }

    /// Whether following `open` at `now` would change nothing: the last
    /// follow still holds.
    fn holds(&self, open: Option<&Open>, now: Instant) -> bool {
        self.followed
            .as_ref()
            .is_some_and(|followed| followed.holds(open, now))
    }

    /// Follows the open issue at `now`. Uploads due let their bytes go; the
    /// open issue's uploads show, and the others count as closed from now
    /// if they did not already. Closed uploads that stored nothing go,
    /// closed issues past [`MAX_CLOSED`] that hold bytes let them go, and
    /// past [`MAX_UNRESOLVED`] uploads waiting for a check, the oldest are
    /// forgotten. Only an attempt's end lets go of bytes it holds too.
    /// Returns the reservations to cancel: only those of uploads that
    /// stored nothing.
    fn follow_all(&mut self, open: Option<&Open>, now: Instant) -> Vec<Cancel> {
        self.release_bytes(|entry| entry.due().is_some_and(|due| due <= now));
        self.entries
            .iter_mut()
            .for_each(|entry| entry.follow(open, now));
        let cancels = self.release(Entry::kept);
        let kept = self.closed_issues_kept();
        self.release_bytes(|entry| {
            entry.settled_since().is_some() && !kept.contains(&entry.issue.id)
        });
        self.forget_unresolved_past(MAX_UNRESOLVED);
        self.followed = Some(self.left(open));
        cancels
    }

    /// What following `open` left.
    fn left(&self, open: Option<&Open>) -> Followed {
        Followed {
            open: open.cloned(),
            due: self.entries.iter().filter_map(Entry::due).min(),
            shown: self.showing(open),
        }
    }

    /// Where the uploads `open` shows are in the store.
    fn showing(&self, open: Option<&Open>) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| open.is_some_and(|open| open.shows(&entry.issue, entry.opening)))
            .map(|(at, _)| at)
            .collect()
    }

    /// Forgets the oldest closed uploads that only wait for a check, past
    /// `max`, without cancelling their reservations: their completion may
    /// have landed.
    fn forget_unresolved_past(&mut self, max: usize) {
        let mut waiting: Vec<(Instant, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(at, entry)| Some((entry.unresolved_since()?, at)))
            .collect();
        let excess = waiting.len().saturating_sub(max);
        waiting.sort_unstable();
        let mut forget = vec![false; self.entries.len()];
        for (_, at) in waiting.into_iter().take(excess) {
            forget[at] = true;
        }
        let mut forget = forget.into_iter();
        self.entries.retain(|_| !forget.next().unwrap_or_default());
    }

    /// Lets go of the bytes of every upload `release` names.
    fn release_bytes(&mut self, release: impl Fn(&Entry) -> bool) {
        self.entries
            .iter_mut()
            .filter(|entry| release(entry))
            .for_each(|entry| entry.upload.release_bytes());
    }

    /// Lets go of every upload `keep` does not hold, with the reservations
    /// to cancel.
    fn release(&mut self, keep: impl Fn(&Entry) -> bool) -> Vec<Cancel> {
        let (kept, gone): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| keep(entry));
        self.entries = kept;
        gone.iter().filter_map(Entry::let_go).collect()
    }

    /// The closed issues whose uploads keep their bytes: of those that hold
    /// any, the [`MAX_CLOSED`] that closed last.
    fn closed_issues_kept(&self) -> Vec<String> {
        let mut holding: Vec<(Instant, &String)> = self
            .entries
            .iter()
            .filter_map(|entry| Some((entry.holding_since()?, &entry.issue.id)))
            .collect();
        holding.sort_by_key(|(closed, _)| std::cmp::Reverse(*closed));
        let mut issues: Vec<String> = Vec::new();
        for (_, issue) in holding {
            if issues.len() == MAX_CLOSED {
                break;
            }
            if !issues.contains(issue) {
                issues.push(issue.clone());
            }
        }
        issues
    }

    /// The uploads `open` shows: those the last follow found, while it
    /// followed `open` and nothing changed since.
    pub(in crate::hub) fn shown<'a>(
        &'a self,
        open: Option<&'a Open>,
    ) -> impl Iterator<Item = &'a AttachmentUpload> + 'a {
        let followed = self
            .followed
            .as_ref()
            .filter(|followed| followed.open.as_ref() == open);
        let positions =
            followed.map_or_else(|| self.showing(open), |followed| followed.shown.clone());
        positions.into_iter().map(|at| &self.entries[at].upload)
    }

    /// The upload `id`, if `open` shows it, with its issue.
    pub(super) fn shown_row(
        &mut self,
        open: Option<&Open>,
        id: &str,
    ) -> Option<(&mut AttachmentUpload, &UploadIssue)> {
        self.followed = None;
        self.entries
            .iter_mut()
            .find(|entry| {
                entry.upload.id == id
                    && open.is_some_and(|open| open.shows(&entry.issue, entry.opening))
            })
            .map(|entry| (&mut entry.upload, &entry.issue))
    }

    /// Room for `size` more bytes beside the `held` bytes the app holds.
    /// Nothing goes if they fit. Otherwise closed issues' uploads with no
    /// attempt under way, whose bytes only the store holds, let their bytes
    /// go, the issue that closed first first, only as far as lets the file
    /// fit; if even all of those would not, nothing goes and the file is
    /// refused. The uploads themselves stay, to be checked.
    pub(in crate::hub) fn room(&mut self, held: usize, size: usize) -> Result<(), UploadError> {
        let needed = held
            .saturating_add(size)
            .saturating_sub(crate::uploads::MAX_TOTAL_BYTES);
        let chosen = self.releasable(needed).ok_or(UploadError::Budget)?;
        self.followed = None;
        self.release_bytes(|entry| chosen.contains(&entry.upload.id));
        Ok(())
    }

    /// The uploads whose bytes to let go to free `needed` bytes, the issue
    /// that closed first first, or `None` when letting go of all that can go
    /// would not.
    fn releasable(&self, needed: usize) -> Option<Vec<String>> {
        let mut idle: Vec<(Instant, &AttachmentUpload)> = self
            .entries
            .iter()
            .filter_map(|entry| Some((entry.idle_since()?, &entry.upload)))
            .collect();
        idle.sort_by_key(|(closed, _)| *closed);
        let mut freed = 0;
        let mut chosen = Vec::new();
        for (_, upload) in idle {
            if freed >= needed {
                break;
            }
            freed += upload.bytes.len();
            chosen.push(upload.id.clone());
        }
        (freed >= needed).then_some(chosen)
    }

    /// Whether the upload `id` is in the store.
    pub(super) fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|entry| entry.upload.id == id)
    }

    /// The upload `id`, wherever its issue is.
    pub(super) fn row(&mut self, id: &str) -> Option<&mut AttachmentUpload> {
        self.followed = None;
        self.entries
            .iter_mut()
            .find(|entry| entry.upload.id == id)
            .map(|entry| &mut entry.upload)
    }

    /// Takes the upload `id` out, with its issue.
    pub(super) fn remove(&mut self, id: &str) -> Option<(AttachmentUpload, UploadIssue)> {
        self.followed = None;
        let index = self
            .entries
            .iter()
            .position(|entry| entry.upload.id == id)?;
        let entry = self.entries.remove(index);
        Some((entry.upload, entry.issue))
    }
}

#[cfg(test)]
impl UploadStore {
    /// Whether an upload holding transfer `transfer` is kept.
    pub(in crate::hub) fn contains_transfer(&self, transfer: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.upload.transfer.as_deref() == Some(transfer))
    }

    /// Whether the upload holding transfer `transfer` let its bytes go.
    pub(in crate::hub) fn released_transfer(&self, transfer: &str) -> bool {
        self.entries.iter().any(|entry| {
            entry.upload.transfer.as_deref() == Some(transfer) && entry.upload.released
        })
    }
}

impl crate::hub::State {
    /// The open issue, as the upload store matches it.
    pub(in crate::hub) fn open_for_uploads(&self) -> Option<Open> {
        self.snapshot.issue.as_ref().and_then(Open::of)
    }

    /// Follows the open issue in the upload store at `now`; see
    /// [`UploadStore::follow`]. Returns the reservations to cancel.
    pub(in crate::hub) fn follow_uploads(&mut self, now: Instant) -> Vec<Cancel> {
        let open = self.open_for_uploads();
        self.uploads.follow(open.as_ref(), now)
    }

    /// Whether the open issue is `issue`.
    pub(super) fn open_names(&self, issue: &UploadIssue) -> bool {
        self.open_for_uploads()
            .is_some_and(|open| open.names(issue))
    }

    /// The snapshot handed out: the hub's, with the open issue's uploads.
    pub(in crate::hub) fn view(&self) -> crate::hub::HubSnapshot {
        let mut snapshot = self.snapshot.clone();
        let open = self.open_for_uploads();
        if let Some(issue) = snapshot.issue.as_mut() {
            let uploads = self
                .uploads
                .shown(open.as_ref())
                .map(AttachmentUpload::without_bytes)
                .collect();
            issue.files.show_uploads(uploads);
        }
        snapshot
    }

    /// The bytes still held by drafts and uploads across the app.
    pub(in crate::hub) fn held_bytes(&mut self) -> usize {
        self.attachment_buffers
            .retain(|buffer| buffer.strong_count() > 0);
        self.attachment_buffers
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .map(|buffer| buffer.len())
            .sum()
    }
}

mod upload;

#[cfg(test)]
mod bound_tests;
#[cfg(test)]
mod hub_tests;
#[cfg(test)]
mod store_tests;
