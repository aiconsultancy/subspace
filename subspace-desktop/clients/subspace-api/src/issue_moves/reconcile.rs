//! Reading a NETT move back: every previewed issue in the destination
//! team, with a newer version and a number there. After a validated PATCH,
//! which proves the move committed, a sub-issue found elsewhere or that
//! cannot be read is reported unreconciled rather than doubting the move.
use super::{MoveItem, MovePreview, MoveResult, preview_number};
use crate::issue_writes::IssueSaveError;
use crate::{Client, model::IssueDetail};

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
    /// It is outside the destination now: someone moved it again since.
    Elsewhere,
    /// It could not be read back, for this reason.
    Unread(crate::Error),
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
    /// It is not confirmed there, after a move that committed.
    Unreconciled(Unreconciled),
}

/// Whether `current` is `previewed` moved into the preview's destination:
/// newer than previewed, with a number there.
fn moved_into(preview: &MovePreview, current: &IssueDetail, previewed: &MoveItem) -> bool {
    current.fields.id == previewed.id
        && current.fields.project_id == preview.destination.id
        && current.fields.version > i64::from(previewed.version)
        && preview_number(&current.fields.identifier, &preview.destination.key)
}

// qual:allow(coupling, oi) reason: "Move confirmation reads the issues back through the client's NETT reads."
impl Client {
    /// Checks a confirmed or possibly confirmed NETT move without sending it
    /// again: every previewed issue must now be in the destination team,
    /// with a newer version than previewed and a number there. A read that
    /// fails leaves the move unconfirmed, to be checked again; an issue
    /// that did not move means the plan did not happen.
    pub fn verify_issue_move(&self, preview: &MovePreview) -> Result<MoveResult, IssueSaveError> {
        if self.backend != crate::Backend::Nett {
            return Err(IssueSaveError::Unavailable);
        }
        self.nett_verify_move(preview, None)
    }

    pub(super) fn nett_verify_move(
        &self,
        preview: &MovePreview,
        root: Option<IssueDetail>,
    ) -> Result<MoveResult, IssueSaveError> {
        let first = preview.issues.first().ok_or(IssueSaveError::Invalid)?;
        // A check reads the root itself: one still where it was proves the
        // move never happened, so a fresh preview may follow. A root from a
        // validated PATCH reply proves the whole subtree moved.
        let checking = root.is_none();
        let root = match root {
            Some(root) => root,
            None => self
                .issue(&first.id)
                .map_err(|_| IssueSaveError::Unconfirmed)?,
        };
        if !moved_into(preview, &root, first) || !preview.root_moved_to(&root.fields.identifier) {
            return Err(if checking {
                IssueSaveError::Conflict
            } else {
                IssueSaveError::Unconfirmed
            });
        }
        let mut root_item = first.clone();
        root_item.new_identifier = Some(root.fields.identifier.clone());
        let mut moved = vec![root_item];
        let mut unreconciled = Vec::new();
        for previewed in preview.issues.iter().skip(1) {
            match self.read_back(preview, previewed, checking)? {
                ReadBack::Moved(item) => moved.push(item),
                ReadBack::Unreconciled(item) => unreconciled.push(item),
            }
        }
        Ok(MoveResult {
            issue: root,
            moved,
            unreconciled,
        })
    }

    /// Reads a previewed sub-issue back. A check needs each one in the
    /// destination: one elsewhere means the plan did not happen, and one
    /// that cannot be read leaves the move to be checked again. After a
    /// validated PATCH, which proves the move committed, neither undoes it:
    /// the sub-issue is reported unreconciled, with what reading it found.
    fn read_back(
        &self,
        preview: &MovePreview,
        previewed: &MoveItem,
        checking: bool,
    ) -> Result<ReadBack, IssueSaveError> {
        match self.issue(&previewed.id) {
            Ok(current) if moved_into(preview, &current, previewed) => {
                let mut item = previewed.clone();
                item.new_identifier = Some(current.fields.identifier);
                Ok(ReadBack::Moved(item))
            }
            Ok(_) if checking => Err(IssueSaveError::Conflict),
            Err(_) if checking => Err(IssueSaveError::Unconfirmed),
            Ok(_) => Ok(ReadBack::Unreconciled(Unreconciled::of(
                previewed,
                UnreconciledReason::Elsewhere,
            ))),
            Err(error) => Ok(ReadBack::Unreconciled(Unreconciled::of(
                previewed,
                UnreconciledReason::Unread(error),
            ))),
        }
    }
}
