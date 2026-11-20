//! Debug capture only. Only a NETT move can commit with sub-issues it could
//! not confirm in the destination, and a capture build talks to the
//! fictional Subspace API, which confirms every move it commits.
//! `SUBSPACE_CAPTURE_MOVE_UNCONFIRMED` (`CAPTURE_MOVE_UNCONFIRMED=1` in
//! `scripts/capture.sh`) makes a capture build against that loopback fixture
//! report a committed move's sub-issues as not confirmed, so evidence can
//! show the move panel's list. The move itself is real; the reasons are
//! invented. Release builds never contain it.
use subspace_api_client::issue_moves::{MovePreview, Unreconciled, UnreconciledReason};

const ENV: &str = "SUBSPACE_CAPTURE_MOVE_UNCONFIRMED";

/// What a committed move reports: the sub-issues the client could not
/// confirm or, in a capture against the fixture with the switch on, every
/// previewed sub-issue.
pub(super) fn unreconciled(preview: &MovePreview, found: Vec<Unreconciled>) -> Vec<Unreconciled> {
    if found.is_empty() && enabled() {
        invented(preview)
    } else {
        found
    }
}

fn enabled() -> bool {
    let api = std::env::var("SUBSPACE_API_URL").ok();
    std::env::var_os(ENV).is_some()
        && std::env::var_os("SUBSPACE_CAPTURE_DIR").is_some()
        && subspace_api_client::fixture_from_env(true, api.as_deref()).is_some()
}

/// The previewed sub-issues as not confirmed: the first moved again since,
/// the rest not found when read back.
fn invented(preview: &MovePreview) -> Vec<Unreconciled> {
    preview
        .issues
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, item)| {
            let reason = if index == 0 {
                UnreconciledReason::Elsewhere
            } else {
                UnreconciledReason::Unread(subspace_api_client::Error::NotFound)
            };
            Unreconciled::new(&item.id, &item.old_identifier, reason)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Unreconciled, UnreconciledReason, invented, unreconciled};
    use serde_json::json;

    fn item(id: &str, old: &str, new: &str) -> serde_json::Value {
        json!({"id":id,"version":1,"oldIdentifier":old,"newIdentifier":new,"stateId":"state",
            "stateName":"Started","cycleCleared":false,"milestoneCleared":false,
            "estimateCleared":false,"parentDetached":false,"archived":false})
    }

    fn preview() -> subspace_api_client::issue_moves::MovePreview {
        serde_json::from_value(json!({"token":"c".repeat(64),
            "source":{"id":"project-SUB","key":"SUB","name":"Fixture"},
            "destination":{"id":"project-OPS","key":"OPS","name":"Operations"},
            "issues":[item("1","SUB-1","OPS-2"),item("2","SUB-2","OPS-3"),item("3","SUB-3","OPS-4")]}))
        .unwrap()
    }

    /// Every sub-issue is listed by its previous number, the root never.
    #[test]
    fn the_capture_reports_each_sub_issue_and_never_the_root() {
        assert_eq!(
            invented(&preview()),
            [
                Unreconciled::new("2", "SUB-2", UnreconciledReason::Elsewhere),
                Unreconciled::new(
                    "3",
                    "SUB-3",
                    UnreconciledReason::Unread(subspace_api_client::Error::NotFound)
                ),
            ]
        );
    }

    /// What the client found is kept, and without the capture switch an
    /// empty result stays empty.
    #[test]
    fn what_the_client_found_is_kept() {
        let found = vec![Unreconciled::new(
            "3",
            "SUB-3",
            UnreconciledReason::Elsewhere,
        )];
        assert_eq!(unreconciled(&preview(), found.clone()), found);
        assert!(unreconciled(&preview(), Vec::new()).is_empty());
    }
}
