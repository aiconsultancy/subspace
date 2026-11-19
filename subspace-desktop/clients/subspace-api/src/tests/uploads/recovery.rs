use super::declaration;
use crate::{
    hub::{AttachmentState, Hub},
    tests::{common, connected, page, wait},
    uploads::UploadError,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::{Mock, Request};

fn attach(hub: &Hub) -> String {
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    let id = intake
        .attach(declaration(), Arc::from(b"fixture".as_slice()))
        .unwrap();
    drop(intake);
    wait(hub, |_| {
        state(hub) == AttachmentState::Failed(UploadError::Checking)
    });
    id
}

fn state(hub: &Hub) -> AttachmentState {
    hub.comment_draft("1", None).unwrap().attachments()[0].state()
}

fn route(r: &Request, uploaded: &AtomicBool, mode: &str) -> (u16, String) {
    if r.path
        .split('?')
        .next()
        .is_some_and(|p| p.ends_with("/attachments"))
    {
        if r.method == "POST" {
            uploaded.store(true, Ordering::SeqCst);
            return (503, "{}".into());
        }
        if uploaded.load(Ordering::SeqCst) {
            return match mode {
                "failed" => (503, "{}".into()),
                "incomplete" if r.path.contains("cursor=") => (503, "{}".into()),
                "incomplete" => (200, page(vec![], Some("tail"))),
                "multiple" => {
                    let mut second = super::saved();
                    second["mediaId"] = serde_json::json!("uploaded-2");
                    (200, page(vec![super::saved(), second], None))
                }
                _ => (200, page(vec![], None)),
            };
        }
        return (200, page(vec![], None));
    }
    if r.method == "POST" && r.path.ends_with("/comments") {
        return (
            201,
            crate::tests::comment("posted", "Review", None).to_string(),
        );
    }
    common(r)
}

#[test]
fn unclear_then_absent_then_remove_allows_post_without_upload_resend() {
    let uploaded = AtomicBool::new(false);
    let mock = Mock::new(move |r| route(r, &uploaded, "absent"));
    let hub = connected(&mock);
    let id = attach(&hub);
    hub.edit_comment("1", None, "Review").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    assert!(
        !mock
            .requests()
            .iter()
            .any(|r| r.path.ends_with("/comments") && r.method == "POST")
    );
    hub.retry_comment_image("1", None, &id);
    wait(&hub, |_| state(&hub) == AttachmentState::NotFound);
    assert!(!hub.comment_draft("1", None).unwrap().attachments_ready());
    hub.remove_comment_image("1", None, &id);
    wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments_ready()
    });
    assert_eq!(
        hub.comment_draft("1", None).unwrap().notice(),
        Some("Attachment removed. An orphaned upload may still exist on the server.")
    );
    hub.submit_comment("1", None, false).unwrap();
    wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().confirmation() > 0
    });
    let requests = mock.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "POST" && r.path.ends_with("/attachments"))
            .count(),
        1
    );
    let post = requests
        .iter()
        .find(|r| r.method == "POST" && r.path.ends_with("/comments"))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&post.body).unwrap()["body"],
        "Review"
    );
    assert!(!requests.iter().any(|r| r.method == "DELETE"));
    hub.stop();
}

#[test]
fn failed_incomplete_and_multiple_match_scans_keep_the_row_and_block_posting() {
    for mode in ["failed", "incomplete", "multiple"] {
        let uploaded = AtomicBool::new(false);
        let mock = Mock::new(move |r| route(r, &uploaded, mode));
        let hub = connected(&mock);
        let id = attach(&hub);
        hub.remove_comment_image("1", None, &id);
        wait(&hub, |_| {
            state(&hub) == AttachmentState::Failed(UploadError::Checking)
        });
        let draft = hub.comment_draft("1", None).unwrap();
        assert_eq!(draft.attachments().len(), 1);
        assert!(!draft.attachments_ready());
        hub.stop();
    }
}

#[test]
fn discard_drops_an_unclear_upload_after_a_complete_absent_scan() {
    let uploaded = AtomicBool::new(false);
    let mock = Mock::new(move |r| route(r, &uploaded, "absent"));
    let hub = connected(&mock);
    let id = attach(&hub);
    hub.retry_comment_image("1", None, &id);
    wait(&hub, |_| state(&hub) == AttachmentState::NotFound);
    hub.discard_comment("1", None);
    wait(&hub, |_| !hub.has_comment_drafts());
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    assert_ne!(
        intake
            .attach(declaration(), Arc::from(b"fixture".as_slice()))
            .unwrap(),
        id
    );
    drop(intake);
    hub.stop();
}
