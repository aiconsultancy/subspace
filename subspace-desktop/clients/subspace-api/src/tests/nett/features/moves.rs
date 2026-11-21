use super::super::{organisation, server::NettMock, works};
use super::{org_path, own_client};
use crate::Client;
use crate::issue_moves::MovePreview;
use crate::issue_writes::IssueSaveError;

#[test]
fn moves_preview_without_numbers_and_confirm_the_subtree_with_the_token() {
    let mock = NettMock::serving(&["issueMoves"]);
    let client = works(&mock);
    let (destination, issue, version) = move_inputs(&client);
    assert_eq!(
        client
            .preview_issue_move(&issue, &destination, version + 1)
            .err(),
        Some(IssueSaveError::Conflict)
    );
    let preview = client
        .preview_issue_move(&issue, &destination, version)
        .unwrap();
    assert_eq!(
        preview.issues.len(),
        3,
        "the tracker and its two sub-issues"
    );
    assert!(preview.issues.iter().all(|i| i.new_identifier.is_none()));
    assert_eq!(preview.destination.key, "WEB");
    assert!(!preview.moves_archived_sub_issue());
    let sent = mock
        .requests()
        .into_iter()
        .rev()
        .find(|r| {
            r["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("/move-preview"))
        })
        .unwrap();
    assert_eq!(sent["method"], "POST");

    let result = client.move_issue(&preview).unwrap();
    assert_eq!(result.issue.fields.project_id, destination);
    assert!(result.issue.fields.identifier.starts_with("WEB-"));
    assert!(result.moved.iter().all(|item| {
        item.new_identifier
            .as_deref()
            .is_some_and(|id| id.starts_with("WEB-"))
    }));
    assert_eq!(mock.count("PATCH", &format!("/issues/{issue}")), 1);
    // The token is spent: confirming again asks for a fresh preview.
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::Conflict)
    );
}

#[test]
fn a_move_with_an_archived_sub_issue_is_refused_before_it_is_sent() {
    let mock = NettMock::serving(&["issueMoves"]);
    let org = organisation(&mock, "Fixture Works");
    mock.archive_issue(&org, "OPS-4");
    let client = works(&mock);
    let (preview, ..) = preview_move(&client);
    assert!(preview.moves_archived_sub_issue());
    mock.clear_requests();
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::ArchivedSubIssue)
    );
    assert_eq!(mock.count("PATCH", ""), 0);
}

#[test]
fn a_move_whose_sub_issue_cannot_be_read_back_stays_committed_and_checks_without_moving_again() {
    let mock = NettMock::serving(&["issueMoves"]);
    let client = own_client(&mock, "nett-fixture-move-check");
    let (preview, ..) = preview_move(&client);
    let child = preview.issues[1].id.clone();
    // A read back is tried once, within the check's time.
    mock.fail_next(
        "GET",
        &org_path(&mock, &format!("issues/{child}")),
        503,
        Some(0),
        1,
    );
    // The validated PATCH reply proves the move committed: the sub-issue
    // that could not be read is listed to check, not the move doubted.
    let result = client.move_issue(&preview).unwrap();
    assert!(result.issue.fields.identifier.starts_with("WEB-"));
    assert_eq!(result.unreconciled.len(), 1);
    assert_eq!(result.unreconciled[0].id, child);
    assert!(matches!(
        result.unreconciled[0].reason,
        crate::issue_moves::UnreconciledReason::Unread(_)
    ));
    assert!(result.moved.iter().all(|item| item.id != child));
    let checked = client.verify_issue_move(&preview).unwrap();
    assert!(checked.moved.iter().all(|item| {
        item.new_identifier
            .as_deref()
            .is_some_and(|id| id.starts_with("WEB-"))
    }));
    assert_eq!(mock.count("PATCH", ""), 1, "the move was sent once");
    // A root in the destination with an issue the plan named still where it
    // was is not this plan's move: the check finds a conflict.
    let mut other = preview.clone();
    other.issues[1].id = client.issue("OPS-1").unwrap().fields.id;
    assert_eq!(
        client.verify_issue_move(&other).err(),
        Some(IssueSaveError::Conflict)
    );
}

#[test]
fn a_move_that_never_reached_nett_can_be_previewed_again() {
    let mock = NettMock::serving(&["issueMoves"]);
    let client = own_client(&mock, "nett-fixture-move-lost");
    let (preview, destination, issue, version) = preview_move(&client);
    // The confirmation fails in transit: NETT never moved anything.
    mock.fail_next(
        "PATCH",
        &org_path(&mock, &format!("issues/{issue}")),
        502,
        None,
        1,
    );
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
    // The check finds the root where it was, so it says the plan did not
    // happen instead of asking for another check.
    assert_eq!(
        client.verify_issue_move(&preview).err(),
        Some(IssueSaveError::Conflict)
    );
    assert!(
        client
            .preview_issue_move(&issue, &destination, version)
            .is_ok()
    );
}

fn move_inputs(client: &Client) -> (String, String, u32) {
    move_inputs_for(client, "OPS-2")
}

/// The WEB team's ID, and `issue`'s ID and version, to move it there.
pub(super) fn move_inputs_for(client: &Client, issue: &str) -> (String, String, u32) {
    let destination = client
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|team| team.key == "WEB")
        .unwrap();
    let root = client.issue(issue).unwrap();
    let version = u32::try_from(root.fields.version).unwrap();
    (destination.id, root.fields.id, version)
}

pub(super) fn preview_move(client: &Client) -> (MovePreview, String, String, u32) {
    let (destination, issue, version) = move_inputs(client);
    let preview = client
        .preview_issue_move(&issue, &destination, version)
        .unwrap();
    (preview, destination, issue, version)
}
