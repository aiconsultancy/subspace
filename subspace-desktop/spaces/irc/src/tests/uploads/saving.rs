use super::super::completion::composer;
use super::connect;
use crate::conversation::Conversation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, test::TestWindowExt as _};
use sha2::{Digest as _, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_api_client::chat_uploads::UploadDraft;
use subspace_framework::client_test_support::Mock;

fn server() -> (Mock, UploadDraft, Arc<AtomicUsize>) {
    let mut bytes = b"PK\x05\x06".to_vec();
    bytes.extend([0; 18]);
    bytes[20] = 3;
    bytes.extend([0xff, 0, 0xfe]);
    let draft = UploadDraft::new(
        "captain.zip".into(),
        "application/zip".into(),
        bytes.clone(),
    )
    .unwrap();
    let metadata = serde_json::json!({
        "id":draft.id(),"reference":draft.reference(),"fileName":draft.name(),
        "contentType":draft.mime(),"sizeBytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes)),
        "uploadedByUserId":"me","createdAt":"2026-11-17T00:00:00Z","expiresAt":"2099-01-01T00:00:00Z",
        "url":"/unused"
    }).to_string();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let mock = Mock::with_bytes(move |request| {
        if request.path.ends_with("/content") {
            count.fetch_add(1, Ordering::Relaxed);
            return (
                200,
                vec![
                    ("Content-Type".into(), "application/zip".into()),
                    ("X-Content-Type-Options".into(), "nosniff".into()),
                ],
                bytes.clone(),
            );
        }
        let body = if request.path.ends_with("/me") {
            r#"{"userId":"me","kind":"human","displayName":"Fixture"}"#.to_owned()
        } else {
            metadata.clone()
        };
        (
            200,
            vec![("Content-Type".into(), "application/json".into())],
            body.into_bytes(),
        )
    });
    (mock, draft, reads)
}

fn open_file<'a>(
    cx: &'a mut TestAppContext,
    mock: &Mock,
    draft: &UploadDraft,
) -> (Entity<Conversation>, &'a mut VisualTestContext) {
    let (view, _, cx) = composer(cx);
    connect(&view, mock, cx);
    cx.update(|window, cx| {
        crate::uploads::viewer::claim(&view.downgrade())(&draft.reference(), window, cx)
    });
    cx.run_until_parked();
    (view, cx)
}

#[gpui_kit::test]
fn binary_details_load_without_bytes_and_save_downloads_only_after_path_confirmation(
    cx: &mut TestAppContext,
) {
    let (mock, draft, reads) = server();
    let (_conversation, cx) = open_file(cx, &mock, &draft);
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("chat-upload-file-details").is_some());
        window.click("chat-upload-save", cx);
    });
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("chat-upload-save", cx);
    });
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("saved.zip");
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert_eq!(reads.load(Ordering::Relaxed), 1);
    assert_eq!(std::fs::read(path).unwrap(), draft.bytes());
}

#[gpui_kit::test]
fn closing_the_viewer_before_choosing_a_path_prevents_download_and_write(cx: &mut TestAppContext) {
    let (mock, draft, reads) = server();
    let (_conversation, cx) = open_file(cx, &mock, &draft);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("chat-upload-save", cx);
        window.click("chat-upload-viewer-close", cx);
    });
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("cancelled.zip");
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    cx.run_until_parked();
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    assert!(!path.exists());
}
