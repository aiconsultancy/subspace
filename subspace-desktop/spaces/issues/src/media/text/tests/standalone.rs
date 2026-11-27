//! The Attachments panel's standalone controls: shown only where the
//! organisation serves standalone attachments, and wired to the hub.
use crate::media::files::serve_standalone_for_test;
use crate::tests::{
    has,
    offline::{self, Offline, Switches, board_with_hub, settle_board},
    render,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use subspace_api_client::hub::{AttachmentTarget, Hub};
use subspace_api_client::uploads::UploadError;
use subspace_framework::client_test_support::Mock;

use crate::Issues;

const ADDED: &[u8] = b"Added from the panel.\n";
const ADDED_SHA256: &str = "2adc398650f8af005f37cac02a4bd256d81327e6a541ee22f31027d88dc6998a";

fn linked_download() -> (u16, Vec<(String, String)>, Vec<u8>) {
    (
        200,
        vec![
            ("Content-Type".into(), "application/pdf".into()),
            ("X-Content-Type-Options".into(), "nosniff".into()),
        ],
        b"%PDF".to_vec(),
    )
}

fn fixture(added: Arc<AtomicBool>, posts: Arc<AtomicUsize>) -> Offline {
    let switches = Arc::new(Switches::default());
    let state = switches.clone();
    let file = |id: &str, name: &str, mime: &str, size: usize, sha: &str| {
        json!({"mediaId": id, "kind": "file", "fileName": name, "contentType": mime,
               "sizeBytes": size, "url": format!("/api/v1/issues/attachments/{id}"), "sha256": sha})
    };
    let mock = Mock::with_bytes(move |r| {
        let path = r.path.split('?').next().unwrap_or_default();
        if path == "/api/v1/issues/attachments/added" && r.method == "DELETE" {
            added.store(false, Ordering::SeqCst);
            return (200, Vec::new(), b"true".to_vec());
        }
        if path == "/api/v1/issues/attachments/linked" && r.method == "GET" {
            return linked_download();
        }
        let body = if path.ends_with("/attachments") && r.method == "POST" {
            posts.fetch_add(1, Ordering::SeqCst);
            added.store(true, Ordering::SeqCst);
            file(
                "added",
                "added.txt",
                "text/plain",
                ADDED.len(),
                ADDED_SHA256,
            )
        } else if path.contains("/images") {
            json!({"items": [], "nextCursor": null})
        } else if path.contains("/attachments") {
            let mut linked = file("linked", "Linked.pdf", "application/pdf", 4, "fixture");
            linked["removal"] = json!("linked");
            let mut items = vec![
                file("pdf", "Spec.pdf", "application/pdf", 4, "fixture"),
                linked,
            ];
            if added.load(Ordering::SeqCst) {
                let mut added = file(
                    "added",
                    "added.txt",
                    "text/plain",
                    ADDED.len(),
                    ADDED_SHA256,
                );
                added["removal"] = json!("available");
                items.push(added);
            }
            json!({"items": items, "nextCursor": null})
        } else {
            offline::api(&r.path, &state)
        };
        (200, Vec::new(), body.to_string().into_bytes())
    });
    Offline { mock, switches }
}

fn opened(
    cx: &mut TestAppContext,
) -> (
    Offline,
    gpui_kit::Entity<crate::Issues>,
    &mut gpui_kit::VisualTestContext,
    Arc<AtomicUsize>,
) {
    let posts = Arc::new(AtomicUsize::new(0));
    let fixture = fixture(Arc::default(), posts.clone());
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, crate::Issues::is_ready);
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| !issue.files().items().is_empty())
    });
    render(cx);
    (fixture, board, cx, posts)
}

#[gpui_kit::test]
fn without_the_capability_the_panel_lists_files_but_offers_no_changes(cx: &mut TestAppContext) {
    serve_standalone_for_test(Some(false));
    let (_fixture, _board, cx, posts) = opened(cx);
    assert!(!has(cx, "attachment-add"));
    assert!(!has(cx, "attachment-remove:pdf") && !has(cx, "attachment-save:pdf"));
    assert_eq!(posts.load(Ordering::SeqCst), 0);
}

#[gpui_kit::test]
fn subspace_adds_and_removes_files_but_only_saves_linked_or_legacy_files(cx: &mut TestAppContext) {
    let (fixture, board, cx, posts) = opened(cx);
    assert!(has(cx, "attachment-add"));
    assert!(has(cx, "attachment-save:pdf") && !has(cx, "attachment-remove:pdf"));
    assert!(has(cx, "attachment-save:linked") && !has(cx, "attachment-remove:linked"));
    assert!(has(cx, "attachment-removal-notice:linked"));
    board.update(cx, |board, _| {
        board.add_attachment("added.txt", "text/plain", ADDED.to_vec())
    });
    settle_board(&board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(|issue| {
            issue.files().uploads().is_empty()
                && issue
                    .files()
                    .items()
                    .iter()
                    .any(|m| m.file_name == "added.txt")
        })
    });
    render(cx);
    assert_eq!(posts.load(Ordering::SeqCst), 1, "sent once");
    assert!(board.read_with(cx, |board, _| board.attachment_error.is_none()));
    assert!(has(cx, "attachment-save:added") && has(cx, "attachment-remove:added"));
    cx.update(|window, cx| window.click("attachment-remove:added", cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| issue.files().items().iter().all(|m| m.media_id != "added"))
    });
    render(cx);
    assert!(!has(cx, "attachment-save:added"));
    assert!(fixture.mock.requests().iter().any(|request| {
        request.method == "DELETE" && request.path == "/api/v1/issues/attachments/added"
    }));
}

#[gpui_kit::test]
fn a_linked_subspace_file_can_be_saved_to_a_chosen_path(cx: &mut TestAppContext) {
    let (_fixture, board, cx, _) = opened(cx);
    cx.update(|window, cx| window.click("attachment-save:linked", cx));
    assert!(cx.did_prompt_for_new_path());
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("saved.pdf");
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    settle_board(&board, cx, |board| {
        std::fs::read(&path).is_ok_and(|bytes| bytes == b"%PDF") || board.attachment_error.is_some()
    });
    assert_eq!(std::fs::read(path).unwrap(), b"%PDF");
}

#[gpui_kit::test]
fn a_file_chosen_for_one_issue_is_not_added_to_another(cx: &mut TestAppContext) {
    serve_standalone_for_test(Some(true));
    let (fixture, board, cx, posts) = opened(cx);
    let changed = Some(crate::media::files::CHANGED.to_owned());
    // The person starts choosing a file on SUB-1, then opens SUB-2 before
    // the file has been read.
    let (hub, target) = board.read_with(cx, |board, _| {
        let hub = board.hub.clone().unwrap();
        let target = hub.attachment_target().unwrap();
        (hub, target)
    });
    open_issue(&board, cx, "SUB-2");
    board.update(cx, |board, _| {
        board.attach_picked(&hub, &target, picked_file())
    });
    assert_eq!(attachment_error(&board, cx), changed);

    // Chosen on SUB-2, which is then closed and opened again: the same
    // identifier, but another opening.
    let reopened = hub.attachment_target().unwrap();
    open_issue(&board, cx, "SUB-1");
    open_issue(&board, cx, "SUB-2");
    board.update(cx, |board, _| {
        board.attach_picked(&hub, &reopened, picked_file())
    });
    assert_eq!(attachment_error(&board, cx), changed);

    // Chosen in another hub that is still running, as after an
    // organisation switch: not the board's, so nothing is added.
    let (other, other_target) = another_hubs_attachment_target(&fixture);
    board.update(cx, |board, _| {
        board.attach_picked(&other, &other_target, picked_file())
    });
    assert_eq!(attachment_error(&board, cx), changed);
    other.stop();

    // A file the client refuses keeps its own reason.
    let current = hub.attachment_target().unwrap();
    let large = vec![b'a'; 10_000_001];
    board.update(cx, |board, _| {
        board.attach_picked(
            &hub,
            &current,
            Ok(("large.txt".into(), "text/plain".into(), large)),
        );
    });
    assert_eq!(
        attachment_error(&board, cx).as_deref(),
        Some("Choose a supported file of up to 10 MB.")
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(posts.load(Ordering::SeqCst), 0, "nothing was sent");
    serve_standalone_for_test(None);
}

fn picked_file() -> Result<(String, String, Vec<u8>), UploadError> {
    Ok(("added.txt".into(), "text/plain".into(), ADDED.to_vec()))
}

fn attachment_error(board: &Entity<Issues>, cx: &mut VisualTestContext) -> Option<String> {
    board.read_with(cx, |board, _| board.attachment_error.clone())
}

fn open_issue(board: &Entity<Issues>, cx: &mut VisualTestContext, identifier: &'static str) {
    board.update(cx, |board, cx| board.open_detail(identifier.into(), cx));
    settle_board(board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| issue.identifier() == identifier && issue.detail().is_some())
    });
}

fn another_hubs_attachment_target(fixture: &Offline) -> (Hub, AttachmentTarget) {
    let (other, _) = offline::fixture_hub(fixture);
    other
        .activate(subspace_api_client::ListQuery::new(
            subspace_api_client::ListTab::All,
        ))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let other_target = loop {
        _ = other.open_issue("SUB-2", subspace_api_client::PROFILE_ID);
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(target) = other.attachment_target() {
            break target;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the other hub's issue"
        );
    };
    (other, other_target)
}
