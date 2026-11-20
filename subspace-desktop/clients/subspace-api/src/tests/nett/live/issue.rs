use super::super::{server::NettMock, wait, works};
use crate::hub::Hub;
use std::time::Duration;

#[test]
fn todos_show_named_lists_and_save_checks_at_their_version() {
    let mock = NettMock::start();
    let hub = Hub::from_client(works(&mock), "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    let lists = |hub: &Hub| {
        hub.snapshot()
            .issue()
            .and_then(|issue| issue.todos().map(<[_]>::to_vec))
    };
    wait("the lists", Duration::from_secs(10), &|| {
        lists(&hub).is_some()
    });
    let shown = lists(&hub).unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].list.name, "Checklist");
    assert_eq!(shown[0].counts(), (1, 3));

    let second = shown[0].items[1].clone();
    hub.set_todo_done(&second.id, true).unwrap();
    wait("the saved check", Duration::from_secs(10), &|| {
        lists(&hub).is_some_and(|lists| lists[0].counts() == (2, 3))
    });

    // Someone else checks the third item; this hub still shows it unchecked
    // at the old version, so its write conflicts and the lists are read again.
    let third = shown[0].items[2].clone();
    works(&mock)
        .set_issue_list_item_done("OPS-1", &third, true)
        .unwrap();
    hub.set_todo_done(&third.id, true).unwrap();
    wait(
        "the conflict and a fresh read",
        Duration::from_secs(10),
        &|| {
            hub.snapshot().issue().is_some_and(|issue| {
                issue.todo_notice().is_some()
                    && issue
                        .todos()
                        .is_some_and(|lists| lists[0].counts() == (3, 3))
            })
        },
    );
    hub.stop();
}

#[test]
fn a_comment_draft_uploads_managed_media_and_its_comment_claims_it() {
    use crate::hub::AttachmentState;
    use crate::uploads::ImageDeclaration;
    use std::sync::Arc;
    let mock = NettMock::start();
    let hub = Hub::from_client(works(&mock), "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait("the issue", Duration::from_secs(10), &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.todos().is_some())
    });
    let attach = |name: &str| {
        let bytes: Arc<[u8]> = Arc::from(format!("{name} contents\n").into_bytes());
        let declaration = ImageDeclaration::new(name, "text/plain", &bytes).unwrap();
        let intake = hub
            .begin_comment_image_intake(hub.snapshot().generation(), "OPS-1", None)
            .unwrap();
        intake.attach(declaration, bytes).unwrap()
    };
    let kept = attach("kept.txt");
    let dropped = attach("dropped.txt");
    wait("both uploads", Duration::from_secs(10), &|| {
        hub.comment_draft("OPS-1", None)
            .is_some_and(|draft| draft.attachments_ready())
    });
    assert_eq!(mock.count("PUT", ""), 2, "one byte upload each");
    // Removing one cancels its reservation by its request key.
    hub.remove_comment_image("OPS-1", None, &dropped);
    wait("the removal", Duration::from_secs(10), &|| {
        hub.comment_draft("OPS-1", None)
            .is_some_and(|draft| draft.attachments().len() == 1)
    });
    assert_eq!(mock.count("PUT", "/upload"), 1);
    let draft = hub.comment_draft("OPS-1", None).unwrap();
    assert_eq!(draft.attachments()[0].id(), kept);
    assert_eq!(draft.attachments()[0].state(), AttachmentState::Ready);

    hub.edit_comment("OPS-1", None, "With the file.").unwrap();
    hub.submit_comment("OPS-1", None, false).unwrap();
    wait("the posted comment", Duration::from_secs(10), &|| {
        hub.comment_draft("OPS-1", None)
            .is_some_and(|draft| draft.notice() == Some("Posted."))
    });
    let comments = works(&mock)
        .comments("OPS-1", &crate::PageRequest::default())
        .unwrap();
    let posted = comments
        .items
        .iter()
        .find(|comment| comment.body.starts_with("With the file."))
        .unwrap();
    assert_eq!(posted.media.len(), 1);
    assert_eq!(posted.media[0].file_name, "kept.txt");
    hub.stop();
}
