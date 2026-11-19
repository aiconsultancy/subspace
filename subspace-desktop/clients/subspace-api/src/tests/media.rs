use super::{client, connected, person};
use crate::{Error, PageRequest, model::IssueMedia};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

pub(super) fn descriptor(id: &str) -> Value {
    json!({"mediaId":id,"kind":"image","fileName":"Fixture.png","contentType":"image/png",
        "sizeBytes":4,"url":"https://untrusted.example.invalid/ignored","sha256":"fixture"})
}
fn media() -> IssueMedia {
    serde_json::from_value(descriptor("media-1")).unwrap()
}
pub(super) fn metadata(id: &str) -> Value {
    json!({"id":id,"issueId":"1","fileName":"Fixture.png","contentType":"image/png",
        "sizeBytes":4,"url":"/api/v1/issues/media/media-1","sha256":"fixture",
        "createdAt":"2026-11-16T12:00:00Z","uploadedBy":person("me")})
}

#[test]
fn image_bytes_use_the_api_origin_and_require_exact_mime_nosniff_and_size() {
    let mock = Mock::with_bytes(|_| {
        (
            200,
            vec![
                ("Content-Type".into(), "image/png".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            b"data".to_vec(),
        )
    });
    let api = client(&mock);
    assert_eq!(api.fetch_image(&media(), 4).unwrap(), b"data");
    let request = &mock.requests()[0];
    assert_eq!(request.path, "/api/v1/issues/media/media-1");
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| key == "user-agent" && value.starts_with("Subspace/"))
    );
    assert!(
        !request
            .headers
            .iter()
            .any(|(key, _)| key == "authorization")
    );
    assert_eq!(api.fetch_image(&media(), 3), Err(Error::InvalidResponse));
    for (mime, nosniff, bytes) in [
        ("image/png; charset=utf-8", "nosniff", "data"),
        ("image/svg+xml", "nosniff", "data"),
        ("image/png", "", "data"),
        ("image/png", "nosniff", "dat"),
    ] {
        let mock = Mock::with_headers(move |_| {
            (
                200,
                vec![
                    ("Content-Type".into(), mime.into()),
                    ("X-Content-Type-Options".into(), nosniff.into()),
                ],
                bytes.into(),
            )
        });
        assert_eq!(
            client(&mock).fetch_image(&media(), 4),
            Err(Error::InvalidResponse)
        );
    }
}

fn text_media() -> IssueMedia {
    let mut media = media();
    media.kind = "file".into();
    media.file_name = "Notes.md".into();
    media.content_type = "text/markdown".into();
    media
}

#[test]
fn text_attachments_use_the_attachment_read_with_the_image_byte_rules() {
    let mock = Mock::with_bytes(|_| {
        (
            200,
            vec![
                ("Content-Type".into(), "text/markdown".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            b"# Hi".to_vec(),
        )
    });
    let api = client(&mock);
    assert_eq!(api.fetch_text(&text_media(), 4).unwrap(), b"# Hi");
    assert_eq!(
        mock.requests()[0].path,
        "/api/v1/issues/attachments/media-1"
    );
    assert_eq!(
        api.fetch_text(&text_media(), 3),
        Err(Error::InvalidResponse)
    );
    let mut large = text_media();
    large.size_bytes = crate::media::MAX_TEXT_BYTES as i64 + 1;
    assert_eq!(
        api.fetch_text(&large, usize::MAX),
        Err(Error::InvalidResponse)
    );
    let requests = mock.requests().len();
    assert_eq!(api.fetch_text(&media(), 4), Err(Error::InvalidRequest));
    assert_eq!(
        api.fetch_image(&text_media(), 4),
        Err(Error::InvalidRequest)
    );
    assert_eq!(mock.requests().len(), requests);
    for (mime, nosniff) in [("text/html", "nosniff"), ("text/markdown", "")] {
        let mock = Mock::with_headers(move |_| {
            (
                200,
                vec![
                    ("Content-Type".into(), mime.into()),
                    ("X-Content-Type-Options".into(), nosniff.into()),
                ],
                "# Hi".into(),
            )
        });
        assert_eq!(
            client(&mock).fetch_text(&text_media(), 4),
            Err(Error::InvalidResponse)
        );
    }
}

#[test]
fn media_redirects_are_not_followed_and_access_challenges_are_preserved() {
    let mock = Mock::with_headers(|_| {
        (
            302,
            vec![("Location".into(), "https://login.example.test/login".into())],
            String::new(),
        )
    });
    assert_eq!(
        client(&mock).fetch_image(&media(), 4),
        Err(Error::SignInNeeded)
    );
    assert_eq!(mock.requests().len(), 1);
    let mut unsafe_media = media();
    unsafe_media.media_id = "../escape".into();
    assert_eq!(
        client(&mock).fetch_image(&unsafe_media, 4),
        Err(Error::InvalidRequest)
    );
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn flat_image_metadata_pages_are_deduplicated_and_inline_anchors_scan_cursors() {
    let mock = Mock::new(|request| {
        if request.path.contains("/images") {
            let (items, cursor) = if request.path.contains("cursor=next") {
                (vec![metadata("a"), metadata("b")], None)
            } else {
                (vec![metadata("a")], Some("next"))
            };
            return (200, super::page(items, cursor));
        }
        super::common(request)
    });
    let hub = connected(&mock);
    hub.load_images(Some("b")).unwrap();
    let snapshot = super::wait(&hub, |s| s.issue().unwrap().images().items().len() == 2);
    let images = snapshot.issue().unwrap().images();
    assert!(!images.has_more());
    assert!(!images.is_anchor_gone());
    assert_eq!(images.items()[1].media.media_id, "b");
    assert!(mock.requests().iter().all(|r| !r.path.contains("around=")));
    hub.stop();
}

#[test]
fn attachment_descriptors_accept_contract_fields_without_legacy_optional_fields() {
    let media: IssueMedia = serde_json::from_value(descriptor("id")).unwrap();
    assert_eq!(media.media_id, "id");
    assert_eq!(media.sha256, "fixture");
    let mock = Mock::new(|_| (200, super::page(vec![descriptor("id")], None)));
    assert_eq!(
        client(&mock)
            .attachments("SUB-1", &PageRequest::default())
            .unwrap()
            .items
            .len(),
        1
    );
}
