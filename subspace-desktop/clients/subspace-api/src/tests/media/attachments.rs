use super::{Error, IssueMedia, Mock, PageRequest, client, connected, descriptor, text_media};
use crate::tests as support;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn subspace_offers_file_actions_after_an_empty_list_but_not_a_missing_route() {
    for (status, accepts_files) in [(200, true), (404, false), (405, false), (501, false)] {
        let mock = Mock::new(move |req| {
            if req
                .path
                .split('?')
                .next()
                .is_some_and(|path| path.ends_with("/attachments"))
            {
                return (status, support::page(Vec::new(), None));
            }
            support::common(req)
        });
        let hub = connected(&mock);
        let protocol = hub.attachment_protocol();
        let snapshot = hub.snapshot();
        assert!(!snapshot.issue().unwrap().files().accepts_files(protocol));
        hub.load_attachments().unwrap();
        let snapshot = support::wait(&hub, |s| !s.issue().unwrap().files().is_loading());
        let files = snapshot.issue().unwrap().files();
        assert_eq!(files.accepts_files(protocol), accepts_files, "{status}");
        assert!(!files.is_failed());
        assert!(!files.is_idle());
        hub.stop();
    }
}

#[test]
fn subspace_only_allows_removal_with_an_explicit_available_state() {
    let file = |state: Option<&str>| {
        let mut value = descriptor(state.unwrap_or("legacy"));
        value["kind"] = json!("file");
        if let Some(state) = state {
            value["removal"] = json!(state);
        }
        value
    };
    let mock = Mock::new(move |_| {
        let states = [
            None,
            Some("available"),
            Some("linked"),
            Some("archived"),
            Some("future"),
        ];
        (200, support::page(states.map(file).to_vec(), None))
    });
    let page = client(&mock)
        .attachments("SUB-1", &PageRequest::default())
        .unwrap();
    assert!(page.items.iter().all(IssueMedia::can_save_attachment));
    assert_eq!(
        page.items
            .iter()
            .filter(|media| media.can_remove_attachment())
            .count(),
        1
    );
    assert!(page.items[1].can_remove_attachment());
    assert!(
        page.items[2]
            .attachment_removal_notice()
            .unwrap()
            .contains("Linked")
    );
}

#[test]
fn saving_a_subspace_file_uses_the_attachment_route_not_the_descriptor_url() {
    let mock = Mock::with_headers(|_| {
        (
            200,
            vec![
                ("Content-Type".into(), "application/pdf".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            "%PDF".into(),
        )
    });
    let mut media = text_media();
    media.content_type = "application/pdf".into();
    assert_eq!(client(&mock).fetch_file(&media, 4).unwrap(), b"%PDF");
    let request = &mock.requests()[0];
    assert_eq!(
        (&*request.method, &*request.path),
        ("GET", "/api/v1/issues/attachments/media-1")
    );
    assert_eq!(
        client(&mock).fetch_file(&media, 3),
        Err(Error::InvalidResponse)
    );
}

#[test]
fn a_subspace_delete_conflict_keeps_the_file_and_refreshes_its_removal_state() {
    let linked = Arc::new(AtomicBool::new(false));
    let mock = Mock::new(move |req| {
        if req.method == "DELETE" && req.path == "/api/v1/issues/attachments/file" {
            linked.store(true, Ordering::SeqCst);
            return (409, "{}".into());
        }
        if req.path.contains("/attachments") {
            let mut file = descriptor("file");
            file["kind"] = json!("file");
            file["removal"] = json!(if linked.load(Ordering::SeqCst) {
                "linked"
            } else {
                "available"
            });
            return (200, support::page(vec![file], None));
        }
        support::common(req)
    });
    let hub = connected(&mock);
    hub.load_attachments().unwrap();
    support::wait(&hub, |s| {
        s.issue()
            .unwrap()
            .files()
            .items()
            .first()
            .is_some_and(IssueMedia::can_remove_attachment)
    });
    hub.remove_attachment("file");
    let snapshot = support::wait(&hub, |s| {
        let files = s.issue().unwrap().files();
        !files.is_loading()
            && files
                .items()
                .first()
                .is_some_and(|file| !file.can_remove_attachment())
    });
    assert_eq!(
        snapshot.issue().unwrap().files().notice(),
        Some("Couldn't remove the attachment. The list was read again.")
    );
    hub.remove_attachment("file");
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|req| req.method == "DELETE")
            .count(),
        1
    );
    hub.stop();
}
