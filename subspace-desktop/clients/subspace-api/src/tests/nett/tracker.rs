use super::{organisation, server::NettMock, signed_in, works};
use crate::issue_writes::{IssueChanges, IssueProperty, IssueSave, IssueSaveError};
use crate::{CommentIntent, CommentPost, IssueQuery, ListQuery, ListTab, PageRequest};

#[test]
fn the_caller_and_organisations_come_from_the_auth_context() {
    let mock = NettMock::start();
    let client = signed_in(&mock, None);
    let me = client.me().unwrap();
    assert_eq!(me.display_name, "Ada Fixture");
    assert_eq!(me.kind, "human");
    let (_, orgs) = client.nett_context().unwrap();
    let eligible: Vec<_> = orgs
        .iter()
        .filter(|org| org.eligible())
        .map(|org| org.name.as_str())
        .collect();
    assert_eq!(eligible, ["Fixture Works", "Second Fixture Org"]);
    assert_eq!(
        crate::nett::scope::resolve(&orgs, None).1,
        crate::nett::Status::Choose
    );
    // Without an organisation, tracker routes are refused before any request.
    assert_eq!(
        client.all_projects().err(),
        Some(crate::Error::OrganisationNeeded)
    );
}

#[test]
fn teams_states_lists_and_groups_read_as_projects() {
    let mock = NettMock::start();
    let client = works(&mock);
    let projects = client.all_projects().unwrap();
    let keys: Vec<_> = projects.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, ["OPS", "WEB"]);
    let states = client
        .workflow_states("OPS", &PageRequest::default())
        .unwrap();
    assert_eq!(states.items.len(), 7);
    assert!(
        states
            .items
            .iter()
            .all(|state| state.project_id == projects[0].id)
    );

    let query = ListQuery::new(ListTab::All).project(Some("OPS"));
    let groups = client.issue_groups(&query).unwrap();
    assert_eq!(groups.group_by, "state");
    assert_eq!(groups.groups.len(), 7);
    let started = groups
        .groups
        .iter()
        .find(|g| g.state_type == "started")
        .unwrap();
    assert!(started.items.iter().all(|row| row.project_key == "OPS"));
    let page = client.issue_group_page(&query, started, None).unwrap();
    assert!(!page.items.is_empty());

    let all = client
        .issue_groups(&ListQuery::new(ListTab::Active))
        .unwrap();
    assert_eq!(all.group_by, "stateType");

    let mut children = IssueQuery::new("OPS");
    children.parent = Some("OPS-2".into());
    let rows = client.issues(&children).unwrap().items;
    let ids: Vec<_> = rows.iter().map(|row| row.identifier.as_str()).collect();
    assert!(ids.contains(&"OPS-3") && ids.contains(&"OPS-4"), "{ids:?}");
}

#[test]
fn issue_detail_discussion_and_links_read_from_the_selected_organisation() {
    let mock = NettMock::start();
    let client = works(&mock);
    let detail = client.issue("OPS-1").unwrap();
    assert_eq!(detail.fields.project_key, "OPS");
    assert!(
        detail
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("nett-media:")
    );
    let by_id = client.issue(&detail.fields.id).unwrap();
    assert_eq!(by_id.fields.identifier, "OPS-1");

    let comments = client.comments("OPS-1", &PageRequest::default()).unwrap();
    let root = comments.items.iter().find(|c| c.has_replies).unwrap();
    let replies = client
        .replies("OPS-1", &root.id, &PageRequest::default())
        .unwrap();
    assert_eq!(replies.items.len(), 2);
    let relations = client.relations("OPS-1", &PageRequest::default()).unwrap();
    assert!(
        relations
            .items
            .iter()
            .any(|r| r.issue.identifier == "WEB-1")
    );
    let activity = client
        .issue_activity("OPS-1", &PageRequest::default())
        .unwrap();
    assert!(!activity.items.is_empty());
    let pulls = client
        .pull_requests("OPS-1", &PageRequest::default())
        .unwrap();
    assert_eq!(pulls.items.len(), 1);

    // The same identifier in another organisation is another issue.
    let second = organisation(&mock, "Second Fixture Org");
    let other = signed_in(&mock, Some(&second)).issue("OPS-1").unwrap();
    assert_ne!(other.fields.id, detail.fields.id);
}

#[test]
fn images_and_files_are_read_through_grants_without_the_bearer() {
    let mock = NettMock::start();
    let client = works(&mock);
    let images = client
        .issue_images("OPS-1", &PageRequest::default())
        .unwrap();
    let image = images.items.into_iter().next().unwrap().descriptor();
    mock.clear_requests();
    let bytes = client
        .fetch_image(&image, crate::media::MAX_IMAGE_BYTES)
        .unwrap();
    assert!(bytes.starts_with(b"\x89PNG"));
    let attachments = client
        .attachments("OPS-1", &PageRequest::default())
        .unwrap();
    let notes = attachments.items.first().unwrap();
    assert_eq!(notes.content_type, "text/markdown");
    let text = client
        .fetch_text(notes, crate::media::MAX_TEXT_BYTES)
        .unwrap();
    assert!(String::from_utf8(text).unwrap().starts_with('#'));

    let requests = mock.requests();
    let files: Vec<_> = requests
        .iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/files/"))
        .collect();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|r| r["authorization"] == false));
    let scoped: Vec<_> = requests
        .iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v1/orgs/"))
        .collect();
    assert!(!scoped.is_empty());
    assert!(
        scoped
            .iter()
            .all(|r| r["authorization"] == true && r["orgHeader"].is_string())
    );
}

#[test]
fn issues_and_comments_are_created_and_edited_with_versions() {
    let mock = NettMock::start();
    let client = works(&mock);
    let team = client
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|p| p.key == "OPS")
        .unwrap();

    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Title("Rotate the backup keys".into()));
    changes.set(IssueProperty::Priority(2));
    let IssueSave::Saved(created) = client.create_issue(&team.id, &changes).unwrap();
    assert!(created.fields.identifier.starts_with("OPS-"));
    assert_eq!(created.fields.version, 1);

    let mut edit = IssueChanges::default();
    edit.set(IssueProperty::Assignee(None));
    edit.set(IssueProperty::Title("Rotate the backup keys now".into()));
    let IssueSave::Saved(edited) = client.update_issue(&created.fields.id, 1, &edit).unwrap();
    assert_eq!(edited.fields.version, 2);
    assert_eq!(edited.fields.title, "Rotate the backup keys now");
    assert_eq!(
        client.update_issue(&created.fields.id, 1, &edit).err(),
        Some(IssueSaveError::Conflict)
    );

    let intent = CommentIntent::new("Keys rotated.", None).unwrap();
    let CommentPost::Created(comment) = client
        .post_comment(&created.fields.identifier, &intent)
        .unwrap()
    else {
        panic!("the comment was not created");
    };
    assert_eq!(comment.body, "Keys rotated.");
    assert_eq!(comment.actual_author.kind, "human");
    let reply = CommentIntent::new("Thanks.", Some(&comment.id)).unwrap();
    assert!(matches!(
        client.post_comment(&created.fields.identifier, &reply),
        Ok(CommentPost::Created(_))
    ));
}
