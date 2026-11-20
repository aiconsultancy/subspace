use super::{organisation, server::NettMock, signed_in, works};
use crate::issue_writes::IssueSaveError;
use crate::model::DecisionStatus;
use crate::{DecisionQuery, ListQuery, ListTab, PageRequest};

#[test]
fn pickers_initiatives_decisions_and_lists_map_to_the_desktop() {
    let mock = NettMock::start();
    let client = works(&mock);
    assert_picker_choices(&client);
    assert_initiative_mapping(&client);
    assert_decision_mapping(&client);
    assert_versioned_list_items(&client);
}

fn assert_picker_choices(client: &crate::Client) {
    use crate::issue_writes::ChoiceSource;
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
}

fn assert_initiative_mapping(client: &crate::Client) {
    let initiatives = client.member_initiatives().unwrap();
    assert!(
        initiatives
            .iter()
            .any(|i| i.name == "Platform" && i.channels.is_empty() && i.fleet.is_none())
    );
}

fn assert_decision_mapping(client: &crate::Client) {
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
}

fn assert_versioned_list_items(client: &crate::Client) {
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

#[test]
fn initiative_discovery_reads_nested_and_fleetless_rows_and_leaves_archived_out() {
    let mock = NettMock::serving(&["initiativeChannels"]);
    let initiatives = works(&mock).member_initiatives().unwrap();
    let names: Vec<_> = initiatives.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Platform › Desktop"), "{names:?}");
    assert!(
        !names
            .iter()
            .any(|n| n.contains("Retired") || n.contains("Legacy"))
    );
    let research = initiatives.iter().find(|i| i.name == "Research").unwrap();
    assert!(research.fleet_id.is_none() && research.fleet.is_none());
}

#[test]
fn names_nett_advertises_that_the_desktop_does_not_use_change_nothing() {
    let mock = NettMock::start();
    let org = organisation(&mock, "Fixture Works");
    mock.capabilities(
        &org,
        &[
            "issueSubscribers",
            "notificationUnread",
            "issueDocumentArchive",
            "laterThing",
        ],
    );
    let client = signed_in(&mock, Some(&org));
    assert!(client.all_projects().is_ok());
    for feature in [
        crate::backend::Feature::InitiativeChannels,
        crate::backend::Feature::ChatUploads,
        crate::backend::Feature::Attachments,
        crate::backend::Feature::IssueMoves,
        crate::backend::Feature::Updates,
    ] {
        assert!(!client.serves(feature));
    }
}
