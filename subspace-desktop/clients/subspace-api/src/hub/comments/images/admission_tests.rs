use super::super::{CommentDraft, DraftKey};
use super::{Attachment, Row};
use crate::hub::files::UploadIssue;
use crate::hub::{AttachmentState, Hub, IssueSnapshot, IssueStatus, State, lock};
use crate::uploads::{ImageDeclaration, UploadError};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use subspace_framework::client_test_support::Mock;

const MIB: usize = 1024 * 1024;
const KEPT: &str = "00000000-0000-4000-8000-0000000000c7";

/// Whether the kept upload is still kept.
fn kept(hub: &Hub) -> bool {
    lock(&hub.0.state)
        .uploads
        .contains_transfer("transfer-kept")
}

/// A text file to attach, with its bytes.
fn image(text: &str) -> (ImageDeclaration, Arc<[u8]>) {
    let bytes = text.as_bytes().to_vec();
    let declaration = ImageDeclaration::new("notes.txt", "text/plain", &bytes).unwrap();
    (declaration, Arc::from(bytes))
}

/// Keeps a failed upload, whose bytes may be stored, for the closed issue
/// SUB-2.
fn keep_upload(state: &mut State) {
    let mut issue = IssueSnapshot::new("SUB-2", crate::PROFILE_ID, IssueStatus::Loaded);
    issue.detail = Some(serde_json::from_value(crate::tests::detail(KEPT)).unwrap());
    let opening = issue.opening;
    let upload = crate::hub::AttachmentUpload::awaiting_check_for_test(4 * MIB, "transfer-kept");
    state.attachment_buffers.push(upload.buffer_for_test());
    state.snapshot.issue = Some(issue);
    let issue = UploadIssue {
        id: KEPT.into(),
        identifier: "SUB-2".into(),
    };
    state.uploads.add(upload, issue, opening);
    state.snapshot.issue = None;
    state.follow_uploads(Instant::now());
}

/// A row for an image the draft already holds, as `id`.
fn existing(id: &str, (declaration, bytes): (ImageDeclaration, Arc<[u8]>)) -> Attachment {
    Attachment {
        inner: Arc::new(Mutex::new(Row {
            id: id.into(),
            media_draft: "media".into(),
            request_key: None,
            declaration,
            bytes,
            state: AttachmentState::Uploading,
            media: None,
            existing: None,
            uncertain: false,
            checked_absent: false,
            removed: false,
            retry_at: None,
        })),
        running: Arc::new(AtomicBool::new(true)),
    }
}

#[test]
fn an_image_the_draft_refuses_or_already_has_lets_no_kept_upload_go() {
    let mock = Mock::new(|_| (500, "{}".into()));
    let client = crate::Client::loopback(&format!("{}/api", mock.base_url()), None)
        .unwrap()
        .without_live();
    let hub = Hub::from_client(client, "Fixture");
    let key = DraftKey::new("SUB-1", None);
    // Bytes held elsewhere leave too little room unless the kept upload
    // goes.
    let elsewhere: Arc<[u8]> = vec![0; 60 * MIB].into();
    let (generation, lease) = {
        let mut state = lock(&hub.0.state);
        state.attachment_buffers.push(Arc::downgrade(&elsewhere));
        keep_upload(&mut state);
        let mut draft = CommentDraft::new();
        draft.images.push(existing("existing", image("First.\n")));
        let lease = draft.lease;
        state.drafts.insert(key.clone(), draft);
        (state.snapshot.generation, lease)
    };
    // A draft the image was not chosen for refuses it before anything goes.
    let (declaration, bytes) = image("Second.\n");
    assert_eq!(
        hub.attach_image_at(generation, (&key, lease + 1), declaration, bytes)
            .err(),
        Some(UploadError::Removed)
    );
    assert!(kept(&hub));
    // An image the draft already has is that image, whatever the budget.
    let (declaration, bytes) = image("First.\n");
    assert_eq!(
        hub.attach_image_at(generation, (&key, lease), declaration, bytes),
        Ok("existing".into())
    );
    assert!(kept(&hub));
    // A new image that only fits once the kept upload goes lets it go.
    let (declaration, bytes) = image("Second.\n");
    assert!(
        hub.attach_image_at(generation, (&key, lease), declaration, bytes)
            .is_ok()
    );
    assert!(!kept(&hub), "the kept upload made room");
    drop(elsewhere);
    hub.stop();
}
