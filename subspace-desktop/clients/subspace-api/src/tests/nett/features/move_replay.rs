//! Replayable move confirmation against the loopback mock: with
//! `issueMoveReplay`, a check after a lost reply sends the identical
//! confirmation again, and NETT's commit record answers it.
use super::super::server::NettMock;
use super::moves::preview_move;
use super::{org_path, own_client};
use crate::Client;
use crate::issue_moves::{MovePreview, MoveResult};
use crate::issue_writes::IssueSaveError;
use serde_json::json;
use std::time::Duration;

const REPLAY: [&str; 2] = ["issueMoves", "issueMoveReplay"];

/// The mock serving `features`, a client of it with its own bearer, and a
/// preview of OPS-2 with its two sub-issues moving to WEB.
fn previewed(features: &[&str], token: &str) -> (NettMock, Client, MovePreview) {
    let mock = NettMock::serving(features);
    let client = own_client(&mock, token);
    let (preview, ..) = preview_move(&client);
    assert_eq!(preview.issues.len(), 3);
    (mock, client, preview)
}

/// Confirms `preview` with the reply lost after NETT committed it.
fn lose_the_reply(mock: &NettMock, client: &Client, preview: &MovePreview) {
    mock.config(&json!({"dropMoveReply": 1}));
    assert_eq!(
        client.move_issue(preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
}

/// The tail of the issue `id`'s path.
fn issue_path(id: &str) -> String {
    format!("/issues/{id}")
}

/// PATCHes of the root and reads of its sub-issues since the log was
/// cleared.
fn traffic(mock: &NettMock, preview: &MovePreview) -> (usize, usize) {
    let patches = mock.count("PATCH", &issue_path(&preview.issues[0].id));
    let reads = preview.issues[1..]
        .iter()
        .map(|item| mock.count("GET", &issue_path(&item.id)))
        .sum();
    (patches, reads)
}

/// The result is NETT's record of this plan: every previewed issue in
/// WEB, in order, with nothing left to check.
fn assert_recorded(result: &MoveResult, preview: &MovePreview, replayed: bool) {
    let commit = result.commit.as_ref().expect("NETT's record of the commit");
    assert_eq!(commit.replayed, replayed);
    assert_eq!(commit.token, preview.token);
    assert!(result.unreconciled.is_empty());
    let ids: Vec<_> = result.moved.iter().map(|item| item.id.as_str()).collect();
    let planned: Vec<_> = preview.issues.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, planned);
    assert!(result.moved.iter().all(|item| {
        item.new_identifier
            .as_deref()
            .is_some_and(|id| id.starts_with("WEB-"))
    }));
    assert_eq!(
        result.recorded_root(),
        result.moved[0].new_identifier.as_deref()
    );
}

#[test]
fn a_confirmation_with_replay_reads_its_numbers_from_the_reply() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-first");
    mock.clear_requests();
    let result = client.move_issue(&preview).unwrap();
    assert_recorded(&result, &preview, false);
    assert_eq!(traffic(&mock, &preview), (1, 0), "no sub-issue read back");
}

#[test]
fn a_check_after_a_lost_reply_replays_the_confirmation_and_finds_it_committed() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-lost");
    lose_the_reply(&mock, &client, &preview);
    mock.clear_requests();
    let result = client.verify_issue_move(&preview).unwrap();
    assert_recorded(&result, &preview, true);
    assert_eq!(
        traffic(&mock, &preview),
        (1, 0),
        "the identical confirmation, and no read back"
    );
    let sent = mock
        .requests()
        .into_iter()
        .find(|request| request["method"] == "PATCH")
        .unwrap();
    assert!(
        sent["path"]
            .as_str()
            .is_some_and(|path| path.ends_with(&preview.issues[0].id))
    );
}

#[test]
fn a_check_of_a_confirmation_that_never_committed_finds_the_plan_changed() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-changed");
    // The confirmation fails in transit; nothing moved.
    mock.fail_next(
        "PATCH",
        &org_path(&mock, &format!("issues/{}", preview.issues[0].id)),
        502,
        None,
        1,
    );
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
    // A teammate archives a sub-issue, so the plan no longer matches.
    let org = super::super::organisation(&mock, "Fixture Works");
    mock.archive_issue(&org, &preview.issues[2].old_identifier);
    assert_eq!(
        client.verify_issue_move(&preview).err(),
        Some(IssueSaveError::Conflict),
        "NETT's plan-changed 409 proves the move never happened"
    );
    let root = client.issue(&preview.issues[0].id).unwrap();
    assert_eq!(root.fields.identifier, "OPS-2", "nothing moved");
}

#[test]
fn a_check_while_the_confirmation_is_still_committing_waits_for_its_record() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-slow");
    mock.config(&json!({"slowMoveCommit": 1.5}));
    let confirming = {
        let preview = preview.clone();
        std::thread::spawn(move || client.move_issue(&preview))
    };
    // Wait until the confirmation reached NETT, then check from another
    // client of the same account and organisation.
    let path = issue_path(&preview.issues[0].id);
    super::super::wait_for("the confirmation to arrive", &|| {
        mock.count("PATCH", &path) == 1
    });
    let checker = own_client(&mock, "nett-fixture-replay-slow");
    let checked = checker.verify_issue_move(&preview).unwrap();
    let confirmed = confirming.join().unwrap().unwrap();
    assert_recorded(&confirmed, &preview, false);
    assert_recorded(&checked, &preview, true);
    assert_eq!(checked.moved.len(), confirmed.moved.len());
    assert!(
        checked
            .moved
            .iter()
            .zip(&confirmed.moved)
            .all(|(a, b)| a.new_identifier == b.new_identifier)
    );
}

#[test]
fn a_replay_after_a_sub_issue_moved_again_still_reports_the_rows_it_committed() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-again");
    lose_the_reply(&mock, &client, &preview);
    // A teammate moves one of the sub-issues back to OPS. Reading back would
    // find it in the source team and take the plan for never having moved.
    let mover = own_client(&mock, "nett-fixture-replay-mover");
    let child = mover.issue(&preview.issues[1].id).unwrap();
    let ops = mover
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|team| team.key == "OPS")
        .unwrap()
        .id;
    let onward = mover
        .preview_issue_move(
            &child.fields.id,
            &ops,
            u32::try_from(child.fields.version).unwrap(),
        )
        .unwrap();
    let moved_back = mover.move_issue(&onward).unwrap();
    assert!(moved_back.issue.fields.identifier.starts_with("OPS-"));
    let result = client.verify_issue_move(&preview).unwrap();
    assert_recorded(&result, &preview, true);
    assert_eq!(
        result.moved[1].new_identifier.as_deref(),
        Some(child.fields.identifier.as_str()),
        "the number it was given when this plan committed"
    );
}

#[test]
fn without_the_capability_a_check_reads_the_move_back_without_sending_it() {
    let (mock, client, preview) = previewed(&["issueMoves"], "nett-fixture-replay-absent");
    lose_the_reply(&mock, &client, &preview);
    mock.clear_requests();
    let result = client.verify_issue_move(&preview).unwrap();
    assert!(result.commit.is_none());
    assert_eq!(result.moved.len(), 3);
    let (patches, reads) = traffic(&mock, &preview);
    assert_eq!(patches, 0, "nothing is sent again");
    assert_eq!(reads, 2, "each sub-issue is read back");
}

#[test]
fn a_check_more_than_a_day_after_the_confirmation_reads_back_instead() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-late");
    lose_the_reply(&mock, &client, &preview);
    crate::issue_moves::replay::backdate(
        &client,
        &preview.token,
        Duration::from_secs(24 * 60 * 60 + 60),
    );
    mock.clear_requests();
    let result = client.verify_issue_move(&preview).unwrap();
    assert!(result.commit.is_none(), "read back, not replayed");
    let (patches, reads) = traffic(&mock, &preview);
    assert_eq!((patches, reads), (0, 2));
}

#[test]
fn another_account_never_replays_a_confirmation_it_did_not_send() {
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-owner");
    lose_the_reply(&mock, &client, &preview);
    mock.clear_requests();
    let other = own_client(&mock, "nett-fixture-replay-other");
    let result = other.verify_issue_move(&preview).unwrap();
    assert!(result.commit.is_none(), "read back, not replayed");
    assert_eq!(traffic(&mock, &preview), (0, 2));
}

#[test]
fn a_confirmation_sent_before_replay_was_served_is_checked_by_reading_back() {
    let (mock, client, preview) = previewed(&["issueMoves"], "nett-fixture-replay-before");
    lose_the_reply(&mock, &client, &preview);
    // NETT starts serving replay, and the client reads the list again; it
    // kept no record of the move sent before.
    let org = super::super::organisation(&mock, "Fixture Works");
    mock.capabilities(&org, &REPLAY);
    crate::backend::reread_served(client.base.as_str(), &org, client.flow_account());
    assert!(client.serves(crate::backend::Feature::IssueMoveReplay));
    mock.clear_requests();
    let result = client.verify_issue_move(&preview).unwrap();
    assert!(result.commit.is_none(), "read back, not replayed");
    assert_eq!(result.moved.len(), 3, "found committed");
    assert_eq!(traffic(&mock, &preview), (0, 2));
}

#[test]
fn the_hub_replays_only_while_both_teams_are_writable() {
    let mock = NettMock::serving(&REPLAY);
    let org = super::super::organisation(&mock, "Fixture Works");
    let hub = crate::hub::Hub::from_client(own_client(&mock, "nett-fixture-replay-hub"), "NETT");
    hub.activate(crate::ListQuery::new(crate::ListTab::All).project(Some("OPS")))
        .unwrap();
    super::super::wait_for("the teams", &|| {
        hub.snapshot().projects().len() >= 2 && hub.issue_write_scope().is_some()
    });
    let scope = hub.issue_write_scope().unwrap();
    let web = hub
        .snapshot()
        .projects()
        .iter()
        .find(|team| team.key == "WEB")
        .unwrap()
        .id
        .clone();
    let preview = hub.preview_issue_move(&scope, "OPS-2", &web).unwrap();
    // The first send never reaches NETT.
    let root = issue_path(&preview.issues[0].id);
    mock.fail_next("PATCH", &org_path(&mock, &root[1..]), 502, None, 1);
    assert_eq!(
        hub.move_issue(&scope, &preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
    // The destination is archived before the check: a replay would write
    // the move, so the hub reads it back instead.
    mock.archive_team(&org, "WEB");
    hub.reload(crate::ListQuery::new(crate::ListTab::All).project(Some("OPS")))
        .unwrap();
    super::super::wait_for("the archived team", &|| {
        hub.issue_write_scope().is_some_and(|scope| {
            hub.move_issue(&scope, &preview).err() == Some(IssueSaveError::Unavailable)
        })
    });
    mock.clear_requests();
    assert_eq!(
        hub.verify_issue_move(&scope, &preview).err(),
        Some(IssueSaveError::Conflict),
        "read back: the root is still where it was"
    );
    assert_eq!(mock.count("PATCH", &root), 0, "nothing sent");
    hub.stop();
}

#[test]
fn a_check_told_to_read_back_never_sends_the_confirmation() {
    // The hub decides once whether a check replays; the client follows that
    // decision even when, asked again, it would replay.
    let (mock, client, preview) = previewed(&REPLAY, "nett-fixture-replay-decided");
    lose_the_reply(&mock, &client, &preview);
    assert!(client.replays(&preview));
    mock.clear_requests();
    let result = client.check_issue_move(&preview, false).unwrap();
    assert!(result.commit.is_none(), "read back, not replayed");
    assert_eq!(traffic(&mock, &preview), (0, 2));
}
