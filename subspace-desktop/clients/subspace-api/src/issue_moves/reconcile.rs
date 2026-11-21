//! Reading a NETT move back: every previewed issue in the destination
//! team, with a newer version and a number there. A validated PATCH reply
//! is this plan's own, so it proves the move committed. A check's read of
//! the root in the destination could be someone else's move of it, so the
//! plan's sub-issues must tie it to this plan: NETT moves a plan's whole
//! subtree at once, so at least one must have moved with the root and none
//! may still be where it was. Once the move is proven, a sub-issue found
//! elsewhere, unread or not read in time is reported unreconciled rather
//! than doubting it. A root still where it was, or moved without its
//! sub-issues, shows the plan did not happen; what cannot be read leaves
//! the move to be checked again.
//! The reads share one time budget, so a move that committed never waits
//! behind a request timeout per sub-issue, and each is a fresh read, never
//! one already in flight that could predate the move. A read that finds
//! the account or organisation changed ends the check: what it found
//! belongs to no one.
use super::{MoveItem, MovePreview, MoveResult, preview_number};
use crate::issue_writes::IssueSaveError;
use crate::{Client, Error, model::IssueDetail};
use std::time::{Duration, Instant};

/// How long reading a move back may take in all.
const RECONCILE_FOR: Duration = Duration::from_secs(20);
/// The least time worth starting a read with.
const MIN_READ: Duration = Duration::from_millis(100);

/// A previewed sub-issue that a committed NETT move could not confirm in
/// the destination.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unreconciled {
    /// The sub-issue's ID.
    pub id: String,
    /// Its number before the move.
    pub old_identifier: String,
    /// What reading it back found.
    pub reason: UnreconciledReason,
}

/// Why a moved sub-issue is not confirmed in the destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnreconciledReason {
    /// It is in a third team, newer than previewed: someone moved it again
    /// since.
    Elsewhere,
    /// It was read, but not as moved: still where it was, or not yet newer
    /// than previewed. Nothing shows it moved again.
    NotConfirmed,
    /// It could not be read back, for this reason.
    Unread(crate::Error),
    /// The time for reading the move back ran out before it was read.
    NotChecked,
}

impl Unreconciled {
    /// The sub-issue `id`, numbered `old_identifier` before the move, not
    /// confirmed in the destination for `reason`.
    pub fn new(
        id: impl Into<String>,
        old_identifier: impl Into<String>,
        reason: UnreconciledReason,
    ) -> Self {
        Self {
            id: id.into(),
            old_identifier: old_identifier.into(),
            reason,
        }
    }

    fn of(previewed: &MoveItem, reason: UnreconciledReason) -> Self {
        Self::new(&previewed.id, &previewed.old_identifier, reason)
    }
}

/// What reading a previewed sub-issue back found.
enum ReadBack {
    /// It is in the destination, with its number there.
    Moved(MoveItem),
    /// It is still in the source team: the root moved without it.
    Stayed(Unreconciled),
    /// It is not confirmed there, after a move that committed.
    Unreconciled(Unreconciled),
}

/// What reading a plan's sub-issues back found, in all.
struct SubIssues {
    moved: Vec<MoveItem>,
    unreconciled: Vec<Unreconciled>,
    /// How many were still in the source team.
    stayed: usize,
}

impl SubIssues {
    /// Whether a check's reads tie the root's move to this plan. NETT moves
    /// a plan's whole subtree at once, so a sub-issue still where it was
    /// shows the root was moved some other way, and so does no sub-issue
    /// moved with it once every one was read: a conflict. With none moved
    /// and some not read, the move is left to be checked again. A plan
    /// without sub-issues is the root alone.
    fn tie(&self, planned: usize) -> Result<(), IssueSaveError> {
        let unread = self.unreconciled.iter().any(|item| {
            matches!(
                item.reason,
                UnreconciledReason::Unread(_) | UnreconciledReason::NotChecked
            )
        });
        match (self.stayed, self.moved.len()) {
            (0, _) if planned == 0 => Ok(()),
            (0, moved) if moved > 0 => Ok(()),
            (0, _) if unread => Err(IssueSaveError::Unconfirmed),
            _ => Err(IssueSaveError::Conflict),
        }
    }
}

/// Whether `current` is `previewed` moved into the preview's destination:
/// newer than previewed, with a number there.
fn moved_into(preview: &MovePreview, current: &IssueDetail, previewed: &MoveItem) -> bool {
    current.fields.id == previewed.id
        && current.fields.project_id == preview.destination.id
        && current.fields.version > i64::from(previewed.version)
        && preview_number(&current.fields.identifier, &preview.destination.key)
}

/// What a read of `previewed` that did not show it moved says: `None` when
/// time ran out before it was read. Only that issue, newer than previewed
/// and in a third team, shows it moved again; a stale or unchanged one, or
/// a reply about another issue, shows nothing.
fn reason(
    preview: &MovePreview,
    previewed: &MoveItem,
    read: Option<Result<IssueDetail, Error>>,
) -> UnreconciledReason {
    match read {
        None => UnreconciledReason::NotChecked,
        Some(Err(error)) => UnreconciledReason::Unread(error),
        Some(Ok(current))
            if current.fields.id == previewed.id
                && current.fields.version > i64::from(previewed.version)
                && current.fields.project_id != preview.source.id
                && current.fields.project_id != preview.destination.id =>
        {
            UnreconciledReason::Elsewhere
        }
        Some(Ok(_)) => UnreconciledReason::NotConfirmed,
    }
}

// qual:allow(coupling, oi) reason: "Move confirmation reads the issues back through the client's NETT reads."
impl Client {
    /// Checks a confirmed or possibly confirmed NETT move. When NETT serves
    /// replayable moves and the confirmation was first sent within the last
    /// 24 hours, the identical confirmation is sent again and NETT's record
    /// answers (see `replay`). Otherwise nothing is sent: a root read back
    /// in the destination, newer than previewed and numbered there, with
    /// its sub-issues tying that move to this plan, means the move
    /// committed, any sub-issue not found there listed as unreconciled. A
    /// root still where it was, or one moved without its sub-issues, means
    /// the plan did not happen; a root, or sub-issues, that cannot be read
    /// leave the move unconfirmed, to be checked again.
    pub fn verify_issue_move(&self, preview: &MovePreview) -> Result<MoveResult, IssueSaveError> {
        if self.backend != crate::Backend::Nett {
            return Err(IssueSaveError::Unavailable);
        }
        self.check_issue_move(preview, self.replays(preview))
    }

    /// Checks a NETT move as the caller decided, once: replaying its
    /// confirmation when `replay` is true, otherwise reading it back and
    /// sending nothing. Nothing here decides again, so a capability list
    /// read meanwhile cannot turn a read back into a write.
    pub(crate) fn check_issue_move(
        &self,
        preview: &MovePreview,
        replay: bool,
    ) -> Result<MoveResult, IssueSaveError> {
        if self.backend != crate::Backend::Nett {
            return Err(IssueSaveError::Unavailable);
        }
        if replay {
            return self.replay_move(preview);
        }
        self.nett_verify_move(preview, None)
    }

    pub(super) fn nett_verify_move(
        &self,
        preview: &MovePreview,
        root: Option<IssueDetail>,
    ) -> Result<MoveResult, IssueSaveError> {
        self.nett_verify_move_within(preview, root, RECONCILE_FOR)
    }

    /// [`Client::nett_verify_move`], reading everything back within
    /// `budget`.
    fn nett_verify_move_within(
        &self,
        preview: &MovePreview,
        root: Option<IssueDetail>,
        budget: Duration,
    ) -> Result<MoveResult, IssueSaveError> {
        let first = preview.issues.first().ok_or(IssueSaveError::Invalid)?;
        let deadline = Instant::now() + budget;
        // A check reads the root itself: one still where it was proves the
        // move never happened, so a fresh preview may follow. A root from a
        // validated PATCH reply proves the whole subtree moved.
        let checking = root.is_none();
        let root = match root {
            Some(root) => root,
            None => self.read_root(first, deadline)?,
        };
        if !moved_into(preview, &root, first) || !preview.root_moved_to(&root.fields.identifier) {
            return Err(if checking {
                IssueSaveError::Conflict
            } else {
                IssueSaveError::Unconfirmed
            });
        }
        let sub_issues = self.read_sub_issues(preview, deadline)?;
        // A validated PATCH reply is this plan's own; a check's read of the
        // root must be tied to the plan by its sub-issues.
        if checking {
            sub_issues.tie(preview.issues.len() - 1)?;
        }
        let mut root_item = first.clone();
        root_item.new_identifier = Some(root.fields.identifier.clone());
        Ok(MoveResult {
            issue: root,
            moved: std::iter::once(root_item).chain(sub_issues.moved).collect(),
            unreconciled: sub_issues.unreconciled,
            commit: None,
        })
    }

    /// Reads each previewed sub-issue back before `deadline`.
    fn read_sub_issues(
        &self,
        preview: &MovePreview,
        deadline: Instant,
    ) -> Result<SubIssues, IssueSaveError> {
        let mut found = SubIssues {
            moved: Vec::new(),
            unreconciled: Vec::new(),
            stayed: 0,
        };
        for previewed in preview.issues.iter().skip(1) {
            match self.read_back(preview, previewed, deadline)? {
                ReadBack::Moved(item) => found.moved.push(item),
                ReadBack::Stayed(item) => {
                    found.stayed += 1;
                    found.unreconciled.push(item);
                }
                ReadBack::Unreconciled(item) => found.unreconciled.push(item),
            }
        }
        Ok(found)
    }

    /// A check's read of the root, before `deadline`, in this account and
    /// organisation.
    fn read_root(
        &self,
        first: &MoveItem,
        deadline: Instant,
    ) -> Result<IssueDetail, IssueSaveError> {
        let read = self.read_within(&first.id, deadline);
        self.current().map_err(|_| IssueSaveError::SignIn)?;
        read.and_then(Result::ok).ok_or(IssueSaveError::Unconfirmed)
    }

    /// Reads a previewed sub-issue back before `deadline`, once the root
    /// was found in the destination: moved with it, still in the source
    /// team, or otherwise unreconciled, with what reading it found. A read
    /// that finds the account or organisation changed ends the check.
    fn read_back(
        &self,
        preview: &MovePreview,
        previewed: &MoveItem,
        deadline: Instant,
    ) -> Result<ReadBack, IssueSaveError> {
        let read = self.read_within(&previewed.id, deadline);
        self.current().map_err(|_| IssueSaveError::SignIn)?;
        match read {
            Some(Ok(current)) if moved_into(preview, &current, previewed) => {
                let mut item = previewed.clone();
                item.new_identifier = Some(current.fields.identifier);
                Ok(ReadBack::Moved(item))
            }
            Some(Ok(current))
                if current.fields.id == previewed.id
                    && current.fields.project_id == preview.source.id =>
            {
                Ok(ReadBack::Stayed(Unreconciled::of(
                    previewed,
                    UnreconciledReason::NotConfirmed,
                )))
            }
            read => Ok(ReadBack::Unreconciled(Unreconciled::of(
                previewed,
                reason(preview, previewed, read),
            ))),
        }
    }

    /// A fresh read of the issue `id` that ends by `deadline`, or `None`
    /// when too little time is left for one, or the time ran out before it
    /// got its turn or its reply.
    fn read_within(&self, id: &str, deadline: Instant) -> Option<Result<IssueDetail, Error>> {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|left| *left >= MIN_READ)?;
        let read = self.nett_issue_within(id, deadline);
        (read.is_ok() || Instant::now() < deadline).then_some(read)
    }
}

#[cfg(test)]
mod check_tests;
#[cfg(test)]
mod reconcile_tests;
