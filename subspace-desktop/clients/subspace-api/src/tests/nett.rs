//! The NETT backend end to end against `scripts/nett_api_test_server.py`,
//! a loopback mock built from NETT's OpenAPI document. No real NETT
//! environment is contacted.

mod features;
mod live;
mod server;

use crate::issue_writes::{IssueChanges, IssueProperty, IssueSave, IssueSaveError};
use crate::model::DecisionStatus;
use crate::{
    Client, CommentIntent, CommentPost, DecisionQuery, IssueQuery, ListQuery, ListTab, PageRequest,
};
use server::NettMock;
use subspace_framework::core::Secret;

pub(super) const TOKEN: &str = "nett-fixture-access";

pub(in crate::tests) fn signed_in(mock: &NettMock, org: Option<&str>) -> Client {
    Client::nett_loopback(&mock.api(), org, Some(Secret::new(TOKEN)))
        .unwrap()
        .without_live()
}

/// The caller's organisation called `name`.
pub(in crate::tests) fn organisation(mock: &NettMock, name: &str) -> String {
    let (_, orgs) = signed_in(mock, None).nett_context().unwrap();
    orgs.into_iter()
        .find(|org| org.name == name)
        .map(|org| org.id)
        .unwrap()
}

pub(in crate::tests) fn works(mock: &NettMock) -> Client {
    let org = organisation(mock, "Fixture Works");
    signed_in(mock, Some(&org))
}

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

#[test]
fn pickers_initiatives_decisions_and_lists_map_to_the_desktop() {
    use crate::issue_writes::ChoiceSource;
    let mock = NettMock::start();
    let client = works(&mock);
    let names = |source| {
        client
            .issue_choices(&source)
            .unwrap()
            .into_iter()
            .map(|choice| choice.name().to_owned())
            .collect::<Vec<_>>()
    };
    assert!(names(ChoiceSource::Assignees).contains(&"Grace Fixture".to_owned()));
    assert_eq!(names(ChoiceSource::Labels), ["bug", "docs"]);
    assert_eq!(
        names(ChoiceSource::Estimates("OPS".into())),
        ["1", "2", "3", "5", "8"]
    );
    assert!(names(ChoiceSource::Estimates("WEB".into())).is_empty());
    assert_eq!(names(ChoiceSource::States("OPS".into())).len(), 7);
    assert!(!names(ChoiceSource::Cycles("OPS".into())).is_empty());

    let initiatives = client.member_initiatives().unwrap();
    assert!(
        initiatives
            .iter()
            .any(|i| i.name == "Platform" && i.channels.is_empty() && i.fleet.is_none())
    );

    let query = DecisionQuery {
        issue: Some("OPS-1".into()),
        ..DecisionQuery::default()
    };
    let decisions = client.decisions(&query, &PageRequest::default()).unwrap();
    assert_eq!(decisions.items.len(), 2);
    let accepted = decisions
        .items
        .iter()
        .find(|d| d.status == DecisionStatus::Accepted)
        .unwrap();
    assert!(accepted.tags.is_empty() && accepted.project_key.is_empty());
    let detail = client.decision(&accepted.id).unwrap();
    assert_eq!(detail.item.id, accepted.id);
    let only_accepted = DecisionQuery {
        status: vec![DecisionStatus::Accepted],
        ..query
    };
    assert_eq!(
        client
            .decisions(&only_accepted, &PageRequest::default())
            .unwrap()
            .items
            .len(),
        1
    );

    let lists = client.issue_lists("OPS-1").unwrap();
    assert_eq!(lists[0].name, "Checklist");
    let items = client.issue_list_items("OPS-1", &lists[0].id).unwrap();
    assert_eq!(items.len(), 3);
    let open = items.iter().find(|item| !item.completed).unwrap();
    let done = client
        .set_issue_list_item_done("OPS-1", open, true)
        .unwrap();
    assert!(done.completed && done.version > open.version);
    assert_eq!(
        client.set_issue_list_item_done("OPS-1", open, true).err(),
        Some(IssueSaveError::Conflict),
        "the old version is refused"
    );
}

#[test]
fn the_hub_follows_changes_made_elsewhere_over_the_update_stream() {
    use crate::hub::{Hub, LiveStatus};
    use std::time::{Duration, Instant};
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    let client = Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(TOKEN))).unwrap();
    let hub = Hub::from_client(client, "NETT");
    hub.activate(ListQuery::new(ListTab::All).project(Some("OPS")))
        .unwrap();
    let wait = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait("the first list and a live stream", &|| {
        let snapshot = hub.snapshot();
        snapshot.list().is_some() && snapshot.live() == LiveStatus::Live
    });
    let streams: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|request| request["path"] == "/api/v1/updates")
        .collect();
    assert!(!streams.is_empty());
    assert!(
        streams
            .iter()
            .all(|request| request["authorization"] == true
                && request["orgHeader"] == serde_json::Value::String(org.clone())
                && request["query"]["project"] == serde_json::json!(["OPS"]))
    );
    mock.clear_requests();
    mock.touch(&org, "OPS-1");
    wait("a re-read of the list head after the hint", &|| {
        mock.requests().iter().any(|request| {
            request["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/issues/groups"))
        })
    });
    hub.stop();
}

#[test]
fn an_initiative_keeps_the_list_to_its_planning_projects() {
    let mock = NettMock::start();
    let client = works(&mock);
    let initiatives = client.member_initiatives().unwrap();
    let id = |name: &str| {
        initiatives
            .iter()
            .find(|initiative| initiative.name == name)
            .map(|initiative| initiative.id.clone())
            .unwrap()
    };
    let shown = |initiative: &str| {
        let query = ListQuery::new(ListTab::All).initiative(Some(initiative));
        let groups = client.issue_groups(&query).unwrap();
        let mut ids: Vec<String> = groups
            .groups
            .iter()
            .flat_map(|group| &group.items)
            .map(|row| row.identifier.clone())
            .collect();
        ids.sort();
        (
            groups.groups.iter().map(|group| group.count).sum::<u32>(),
            ids,
        )
    };
    let (count, ids) = shown(&id("Platform"));
    assert!(count > 0 && ids.contains(&"OPS-1".to_owned()), "{ids:?}");
    assert!(
        !ids.contains(&"WEB-2".to_owned()),
        "an issue with no project is not in the initiative"
    );
    assert_eq!(shown(&id("Empty")), (0, Vec::new()));
}

#[test]
fn coming_features_turn_on_when_nett_serves_them() {
    let plain = NettMock::start();
    let client = works(&plain);
    assert!(!client.serves(crate::backend::Feature::InitiativeChannels));
    let initiatives = client.member_initiatives().unwrap();
    assert!(
        initiatives
            .iter()
            .all(|i| i.channels.is_empty() && i.fleet.is_none())
    );
    assert!(client.fleet("fixture-fleet").is_err());

    let serving = NettMock::serving(&["initiativeChannels"]);
    let client = works(&serving);
    let initiatives = client.member_initiatives().unwrap();
    assert!(client.serves(crate::backend::Feature::InitiativeChannels));
    assert!(!client.serves(crate::backend::Feature::ChatUploads));
    let platform = initiatives.iter().find(|i| i.name == "Platform").unwrap();
    assert_eq!(platform.channels, "#platform");
    let fleet = platform.fleet.as_ref().unwrap();
    assert_eq!(fleet.irc_host.as_deref(), Some("irc.example.test"));
    assert_eq!(client.fleet("fixture-fleet").unwrap().id, fleet.id);
    // Chat uploads are still refused before any request: not served here.
    assert_eq!(
        client.chat_upload("UPL-AAAAAAAAAAAAAAAAAAAAAA").err(),
        Some(crate::chat_uploads::UploadError::Unavailable)
    );
}
