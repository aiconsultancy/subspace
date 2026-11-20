use super::super::{server::NettMock, works};
use super::{file_host, org_path, own_client};
use crate::model::MediaBinding;
use crate::uploads::{ImageDeclaration, UploadError};
use crate::{CommentIntent, CommentPost};
use serde_json::json;

#[test]
fn comment_files_are_managed_media_the_comment_claims_without_the_attachments_feature() {
    let mock = NettMock::start();
    let client = works(&mock);
    let draft = uuid::Uuid::new_v4().to_string();
    let key = uuid::Uuid::new_v4().to_string();
    let bytes = b"Restore output, attached to a comment.\n";
    let declaration = ImageDeclaration::new("restore.txt", "text/plain", bytes).unwrap();
    let media = client
        .upload_comment_media("OPS-1", (&draft, &key), &declaration, bytes)
        .unwrap();
    assert!(matches!(media.binding, MediaBinding::Managed));
    // The same request key finds the same upload again.
    let found = client
        .nett_check_comment_media("OPS-1", &draft, &key, &declaration)
        .unwrap()
        .unwrap();
    assert_eq!(found.media_id, media.media_id);
    assert_eq!(
        mock.count("PUT", ""),
        1,
        "one byte upload, none on the check"
    );

    let body = format!(
        "See the output.\n\n[Attached file](nett-media:{})",
        media.media_id
    );
    let intent = CommentIntent::new(&body, None)
        .unwrap()
        .with_media_draft(&draft);
    let Ok(CommentPost::Created(comment)) = client.post_comment("OPS-1", &intent) else {
        panic!("the comment was not created");
    };
    assert!(comment.media.iter().any(|m| m.media_id == media.media_id));

    // A second upload removed before posting is cancelled by its key.
    let other = uuid::Uuid::new_v4().to_string();
    client
        .upload_comment_media("OPS-1", (&draft, &other), &declaration, bytes)
        .unwrap();
    client
        .nett_cancel_comment_media("OPS-1", &draft, &other, &declaration)
        .unwrap();
    assert_eq!(mock.count("PUT", "/upload"), 1);
}

#[test]
fn comment_media_whose_completion_reply_was_lost_or_refused_is_checked_not_resent() {
    let mock = NettMock::start();
    let client = own_client(&mock, "nett-fixture-comment-retry");
    let complete = org_path(&mock, "issues/media/from-upload");
    let draft = uuid::Uuid::new_v4().to_string();
    for (n, arrange) in ["drop", "refuse"].into_iter().enumerate() {
        match arrange {
            "drop" => mock
                .config(&json!({"dropNext": [{"method": "POST", "path": complete, "times": 1}]})),
            _ => mock.fail_next("POST", &complete, 401, None, 1),
        }
        let key = uuid::Uuid::new_v4().to_string();
        let bytes = format!("Comment file {n}\n").into_bytes();
        let declaration = ImageDeclaration::new("comment.txt", "text/plain", &bytes).unwrap();
        let uploads = |mock: &NettMock| {
            file_host(mock)
                .iter()
                .filter(|r| r["method"] == "PUT")
                .count()
        };
        let puts = uploads(&mock);
        assert_eq!(
            client
                .upload_comment_media("OPS-1", (&draft, &key), &declaration, &bytes)
                .err(),
            Some(UploadError::Checking),
            "{arrange}: checked, not refused"
        );
        let found = client
            .nett_check_comment_media("OPS-1", &draft, &key, &declaration)
            .unwrap();
        assert!(
            found.is_some(),
            "{arrange}: the same transfer was completed"
        );
        assert_eq!(uploads(&mock), puts + 1, "{arrange}: one byte upload");
    }
}

#[test]
fn comment_media_prepared_for_a_lease_that_ended_is_never_sent() {
    let mock = NettMock::start();
    crate::native::test_lease::set(Some(1));
    // Bound to the test thread's lease, as a native client is bound to its
    // account and organisation.
    let mut client = own_client(&mock, "nett-fixture-comment-lease");
    client.native_lease = Some(crate::NativeLease(crate::native::generation()));
    // The account or organisation changes right after the reservation's
    // reply was accepted, before the bytes would go.
    crate::native::test_lease::AFTER_REPLY.with(|next| next.set(Some(2)));
    let bytes = b"Prepared for an ended lease.\n".to_vec();
    let declaration = ImageDeclaration::new("comment.txt", "text/plain", &bytes).unwrap();
    let (draft, key) = (
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
    );
    let result = client.upload_comment_media("OPS-1", (&draft, &key), &declaration, &bytes);
    crate::native::test_lease::set(None);
    assert_eq!(result.err(), Some(UploadError::SignInNeeded));
    assert!(
        file_host(&mock).iter().all(|r| r["method"] != "PUT"),
        "the upload URL was never used"
    );
}
