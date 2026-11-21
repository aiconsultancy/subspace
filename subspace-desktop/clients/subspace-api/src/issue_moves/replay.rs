//! Replaying a NETT move confirmation whose reply was lost. With
//! `issueMoveReplay`, NETT records each commit under its plan token, in the
//! same transaction as the move, and answers an identical confirmation with
//! that record: a 200 proves the plan committed and names exactly what it
//! moved, and a 409 with the plan-changed code proves it never did. So a
//! check re-sends the confirmation instead of reading the issues back,
//! which a teammate's move or a sub-issue moved again since can fool.
//! NETT keeps its records for at least seven days; the desktop replays
//! only within 24 hours of first sending a confirmation, and after that
//! checks by reading back as before. Only a confirmation sent while NETT
//! served replay can be replayed, since only then did NETT keep a record,
//! and only by the account that sent it.
use super::{MoveItem, MovePreview, MoveResult, preview_number};
use crate::issue_writes::IssueSaveError;
use crate::{Client, model::IssueDetail, transport};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

/// How long after first sending a confirmation it may be replayed.
const REPLAY_FOR: Duration = Duration::from_secs(24 * 60 * 60);
/// NETT's problem code for a confirmation whose plan no longer matches.
const PLAN_CHANGED: &str = "IssueMove.PlanChanged";

/// NETT's record of a move that committed, as its confirmation's reply
/// carries it, first time or replayed.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveCommit {
    /// The preview token the plan committed under.
    pub token: String,
    /// When it committed, as NETT sent it.
    pub committed_at: String,
    /// Whether this reply replayed a commit made earlier.
    pub replayed: bool,
    /// Who confirmed it: `human`, `agent` or `integration`.
    pub actor_type: String,
    /// Exactly the issues this commit moved, the root first, in preview
    /// order.
    pub issues: Vec<CommittedIssue>,
}

/// One issue a recorded commit moved, as committed: later changes to it do
/// not show here.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct CommittedIssue {
    pub id: String,
    pub old_identifier: String,
    pub new_identifier: String,
    pub version: u32,
}

/// An API origin, organisation, account and plan token.
type Sent = (String, String, u64, String);

/// When each confirmation was first sent, by origin, organisation, account
/// and token. Entries older than [`REPLAY_FOR`] are dropped as others arrive.
static SENT: Mutex<BTreeMap<Sent, SystemTime>> = Mutex::new(BTreeMap::new());

fn sent_table() -> MutexGuard<'static, BTreeMap<Sent, SystemTime>> {
    SENT.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key(client: &Client, token: &str) -> Sent {
    (
        client.api_origin().to_owned(),
        client.organisation().unwrap_or_default().to_owned(),
        client.account_key(),
        token.to_owned(),
    )
}

/// Whether a confirmation first sent at `sent` may still be replayed at
/// `now`. A clock that went back before it is not trusted.
fn replayable_at(sent: SystemTime, now: SystemTime) -> bool {
    now.duration_since(sent)
        .is_ok_and(|elapsed| elapsed < REPLAY_FOR)
}

/// Notes that `client` is sending the confirmation of `token` while NETT
/// serves replay, unless it already sent it: the replay window runs from
/// the first send. A confirmation sent without replay is never noted, so
/// its check reads back even once NETT serves replay.
pub(super) fn note_sent(client: &Client, token: &str) {
    let now = SystemTime::now();
    let mut table = sent_table();
    table.retain(|_, sent| replayable_at(*sent, now));
    table.entry(key(client, token)).or_insert(now);
}

/// Whether `client` sent the confirmation of `token` within the last 24
/// hours, so a check may replay it.
pub(super) fn within_window(client: &Client, token: &str) -> bool {
    sent_table()
        .get(&key(client, token))
        .is_some_and(|sent| replayable_at(*sent, SystemTime::now()))
}

/// Moves the first send of `token` back by `age`, as if it was that long
/// ago.
#[cfg(test)]
pub(crate) fn backdate(client: &Client, token: &str, age: Duration) {
    let sent = SystemTime::now()
        .checked_sub(age)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    sent_table().insert(key(client, token), sent);
}

/// What a replayed confirmation's 409 says: with NETT's plan-changed code,
/// that the plan never committed, a conflict; otherwise nothing certain.
pub(super) fn refusal(response: ureq::http::Response<ureq::Body>) -> IssueSaveError {
    let problem: Option<Value> = transport::read_json(response).ok();
    let code = problem.as_ref().and_then(|problem| {
        ["code", "errorCode"]
            .into_iter()
            .find_map(|name| problem.get(name).and_then(Value::as_str))
    });
    if code == Some(PLAN_CHANGED) {
        IssueSaveError::Conflict
    } else {
        IssueSaveError::Unconfirmed
    }
}

/// The move as NETT recorded it, when `commit` is this plan's record: its
/// token, exactly the previewed issues in order, each numbered in the
/// destination, and `issue` the root as it is now. Nothing is read back,
/// and nothing is left unreconciled.
pub(super) fn recorded(
    preview: &MovePreview,
    issue: IssueDetail,
    commit: Option<Value>,
) -> Option<MoveResult> {
    let commit: MoveCommit = serde_json::from_value(commit?).ok()?;
    let matches = commit.token == preview.token
        && commit.issues.len() == preview.issues.len()
        && preview
            .issues
            .first()
            .is_some_and(|root| root.id == issue.fields.id)
        && commit
            .issues
            .iter()
            .zip(&preview.issues)
            .all(|(committed, previewed)| {
                committed.id == previewed.id
                    && preview_number(&committed.new_identifier, &preview.destination.key)
            });
    if !matches {
        return None;
    }
    let moved = preview
        .issues
        .iter()
        .zip(&commit.issues)
        .map(|(previewed, committed)| MoveItem {
            new_identifier: Some(committed.new_identifier.clone()),
            ..previewed.clone()
        })
        .collect();
    Some(MoveResult {
        issue,
        moved,
        unreconciled: Vec::new(),
        commit: Some(commit),
    })
}

// qual:allow(coupling, oi) reason: "A check replays the confirmation through the client's NETT write."
impl Client {
    /// Checks a NETT move whose confirmation went unanswered by sending the
    /// identical confirmation again. A 200 is the recorded move; a 409 with
    /// the plan-changed code means the plan never committed; anything else
    /// leaves the move to be checked again.
    pub(super) fn replay_move(&self, preview: &MovePreview) -> Result<MoveResult, IssueSaveError> {
        let (issue, commit) = self.send_confirmation(preview, true)?;
        recorded(preview, issue, commit).ok_or(IssueSaveError::Unconfirmed)
    }

    /// Whether a check of `preview` replays its confirmation: NETT serves
    /// replayable moves, and this account sent it within the last 24 hours
    /// while NETT served them.
    pub(crate) fn replays(&self, preview: &MovePreview) -> bool {
        self.serves(crate::backend::Feature::IssueMoves)
            && self.serves(crate::backend::Feature::IssueMoveReplay)
            && within_window(self, &preview.token)
    }
}

#[cfg(test)]
mod tests {
    use super::{REPLAY_FOR, replayable_at};
    use std::time::{Duration, SystemTime};

    #[test]
    fn a_confirmation_replays_for_a_day_and_never_across_a_clock_set_back() {
        let sent = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(replayable_at(sent, sent));
        assert!(replayable_at(
            sent,
            sent + REPLAY_FOR - Duration::from_secs(1)
        ));
        assert!(!replayable_at(sent, sent + REPLAY_FOR));
        assert!(!replayable_at(sent, sent - Duration::from_secs(1)));
    }
}
