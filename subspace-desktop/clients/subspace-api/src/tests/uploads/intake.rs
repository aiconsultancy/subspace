use super::{declaration, saved};
use crate::{
    hub::AttachmentState,
    tests::{common, connected},
    uploads::{ImageDeclaration, UploadError},
};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Mock;
#[test]
fn uncertain_rows_check_metadata_without_a_second_post_then_cleanup_on_remove() {
    let uploaded = Arc::new(AtomicBool::new(false));
    let seen = uploaded.clone();
    let mock = Mock::new(move |request| {
        if request
            .path
            .split('?')
            .next()
            .is_some_and(|p| p.ends_with("/attachments"))
        {
            if request.method == "POST" {
                seen.store(true, Ordering::SeqCst);
                return (503, "{}".into());
            }
            let rows = if seen.load(Ordering::SeqCst) {
                vec![saved()]
            } else {
                vec![]
            };
            return (200, crate::tests::page(rows, None));
        }
        if request.method == "DELETE" {
            return (200, "true".into());
        }
        common(request)
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
        hub.comment_draft("1", None).unwrap().attachments()[0].state()
            == AttachmentState::Failed(UploadError::Checking)
    });
    assert!(hub.has_comment_drafts());
    hub.retry_comment_image("1", None, &id);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments_ready()
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "POST" && r.path.ends_with("/attachments"))
            .count(),
        1
    );
    hub.remove_comment_image("1", None, &id);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None)
            .unwrap()
            .attachments()
            .is_empty()
    });
    assert!(mock.requests().iter().any(|r| r.method == "DELETE"));
    hub.stop();
}

#[test]
fn pending_intake_blocks_post_and_successful_posts_bind_only_ready_media() {
    let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = bodies.clone();
    let mock = Mock::new(move |request| {
        if request
            .path
            .split('?')
            .next()
            .is_some_and(|p| p.ends_with("/attachments"))
        {
            return if request.method == "POST" {
                (201, saved().to_string())
            } else {
                (200, crate::tests::page(vec![], None))
            };
        }
        if request.method == "POST" && request.path.ends_with("/comments") {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            seen.lock()
                .unwrap()
                .push(body["body"].as_str().unwrap().into());
            return (
                201,
                crate::tests::comment("new", body["body"].as_str().unwrap(), None).to_string(),
            );
        }
        common(request)
    });
    let hub = connected(&mock);
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    hub.edit_comment("1", None, "Review").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    assert!(bodies.lock().unwrap().is_empty());
    assert!(
        intake
            .attach(declaration(), Arc::from(b"fixture".as_slice()))
            .is_ok()
    );
    drop(intake);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments_ready()
    });
    hub.submit_comment("1", None, false).unwrap();
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().confirmation() > 0
    });
    assert_eq!(
        bodies.lock().unwrap().as_slice(),
        ["Review\n\n[Attached file](nett-media:uploaded-1)"]
    );
    assert!(
        hub.comment_draft("1", None)
            .unwrap()
            .attachments()
            .is_empty()
    );
    hub.stop();
}

#[test]
fn intake_enforces_eight_rows_deduplicates_and_retires_discarded_leases() {
    let mock = Mock::new(|request| {
        if request.path.contains("/attachments") {
            return (503, "{}".into());
        }
        common(request)
    });
    let hub = connected(&mock);
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    let first = intake
        .attach(declaration(), Arc::from(b"fixture".as_slice()))
        .unwrap();
    assert_eq!(
        intake
            .attach(declaration(), Arc::from(b"fixture".as_slice()))
            .unwrap(),
        first
    );
    for index in 1..8 {
        let declaration =
            ImageDeclaration::new(&format!("Fixture-{index}.txt"), "text/plain", b"fixture")
                .unwrap();
        assert!(
            intake
                .attach(declaration, Arc::from(b"fixture".as_slice()))
                .is_ok()
        );
    }
    let ninth = ImageDeclaration::new("Ninth.txt", "text/plain", b"fixture").unwrap();
    assert_eq!(
        intake.attach(ninth, Arc::from(b"fixture".as_slice())),
        Err(UploadError::Budget)
    );
    assert_eq!(hub.comment_draft("1", None).unwrap().attachments().len(), 8);
    hub.discard_comment("1", None);
    assert!(!intake.current());
    assert_eq!(
        intake.attach(declaration(), Arc::from(b"fixture".as_slice())),
        Err(UploadError::Removed)
    );
    drop(intake);
    hub.stop();
}

#[test]
fn failed_cleanup_stays_visible_until_explicit_remove_retries_it() {
    let refuse = Arc::new(AtomicBool::new(true));
    let switch = refuse.clone();
    let mock = Mock::new(move |request| {
        if request.method == "DELETE" {
            return if switch.load(Ordering::SeqCst) {
                (503, "{}".into())
            } else {
                (200, "true".into())
            };
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
    let id = intake
        .attach(declaration(), Arc::from(b"fixture".as_slice()))
        .unwrap();
    drop(intake);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments_ready()
    });
    hub.discard_comment("1", None);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().attachments()[0].state()
            == AttachmentState::Failed(UploadError::Cleanup)
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        1
    );
    refuse.store(false, Ordering::SeqCst);
    hub.remove_comment_image("1", None, &id);
    crate::tests::wait(&hub, |_| {
        hub.comment_draft("1", None)
            .unwrap()
            .attachments()
            .is_empty()
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        2
    );
    hub.stop();
}
