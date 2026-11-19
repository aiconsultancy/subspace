use super::{declaration, saved};
use crate::{
    hub::AttachmentState,
    tests::{common, connected},
    uploads::UploadError,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Mock;
#[test]
fn stopping_the_hub_cleans_ready_unposted_uploads() {
    let mock = Mock::new(|request| {
        if request.method == "DELETE" {
            return (200, "true".into());
        }
        if request.path.contains("/attachments") {
            return if request.method == "POST" {
                (201, saved().to_string())
            } else {
                (200, crate::tests::page(vec![], None))
            };
        }
        common(request)
    });
    let hub = connected(&mock);
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    assert!(
        intake
            .attach(declaration(), Arc::from(b"fixture".as_slice()))
            .is_ok()
    );
    drop(intake);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments_ready()
    });
    hub.stop();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !mock.requests().iter().any(|r| r.method == "DELETE") {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!hub.has_comment_drafts());
}

#[test]
fn retry_after_waits_without_automatically_resending_the_upload() {
    let first = Arc::new(AtomicBool::new(true));
    let refuse = first.clone();
    let mock = Mock::with_headers(move |request| {
        if request.method == "POST" && request.path.ends_with("/attachments") {
            return if refuse.swap(false, Ordering::SeqCst) {
                (429, vec![("Retry-After".into(), "1".into())], "{}".into())
            } else {
                (201, Vec::new(), saved().to_string())
            };
        }
        let (status, body) = if request.path.contains("/attachments") {
            (200, crate::tests::page(vec![], None))
        } else {
            common(request)
        };
        (status, Vec::new(), body)
    });
    let hub = connected(&mock);
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    let id = intake
        .attach(declaration(), Arc::from(b"fixture".as_slice()))
        .unwrap();
    drop(intake);
    crate::tests::wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", None).unwrap().attachments()[0].state(),
            AttachmentState::Failed(UploadError::Refused { status: 429, .. })
        )
    });
    assert!(!hub.comment_draft("1", None).unwrap().attachments()[0].retry_ready());
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments()[0].retry_ready()
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    hub.retry_comment_image("1", None, &id);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments_ready()
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        2
    );
    hub.stop();
}
