use super::{client, comment, detail, page, row};
use crate::{IssueQuery, ListQuery, ListTab, PageRequest};
use serde_json::json;
use subspace_framework::client_test_support::Mock;

#[test]
fn groups_continue_with_state_id_and_same_filters_without_group_parameters() {
    let mock = Mock::new(|request| {
        if request.path.starts_with("/api/v1/issues/groups?") {
            (200, json!({"groupBy":"state","groups":[{"stateId":"state-1","stateType":"started","count":2,"items":[row("1")],"nextCursor":"opaque+/="}]}).to_string())
        } else {
            (200, page(vec![row("2")], None))
        }
    });
    let api = client(&mock);
    let query = ListQuery::new(ListTab::Active)
        .project(Some("SUB"))
        .search("fixture");
    let groups = api.issue_groups(&query).unwrap();
    let group = &groups.groups[0];
    let more = api
        .issue_group_page(&query, group, group.next_cursor.as_deref())
        .unwrap();
    assert_eq!(more.items[0].fields.project_key, "SUB");
    assert_eq!(more.items[0].fields.todo_counts.done, 1);
    let requests = mock.requests();
    let query: Vec<_> = url::Url::parse(&format!("http://127.0.0.1{}", requests[1].path))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert!(query.contains(&("stateId".into(), "state-1".into())));
    assert!(query.contains(&("project".into(), "SUB".into())));
    assert!(query.contains(&("cursor".into(), "opaque+/=".into())));
    assert!(query.contains(&("q".into(), "fixture".into())));
    assert!(
        !query
            .iter()
            .any(|(key, _)| key == "groupBy" || key == "limitPerGroup" || key == "team")
    );
}

#[test]
fn detail_children_relations_comments_replies_and_timeline_use_org_free_paging() {
    let mock = Mock::new(|request| {
        let path = request.path.split('?').next().unwrap();
        match path {
            "/api/v1/issues/SUB-1" => (200, detail("1").to_string()),
            "/api/v1/issues" => (200, page(vec![row("2")], None)),
            "/api/v1/issues/SUB-1/relations" => {
                let mut relation = row("2");
                relation["kind"] = json!("blocks");
                (200, page(vec![relation], None))
            }
            "/api/v1/issues/SUB-1/activity" => (200, page(vec![], None)),
            _ => (
                200,
                page(
                    vec![comment("comment-1", "Fixture text", None)],
                    Some("next"),
                ),
            ),
        }
    });
    let api = client(&mock);
    let request = PageRequest {
        limit: 50,
        cursor: Some("opaque".into()),
    };
    assert!(api.issue("SUB-1").unwrap().children.is_some());
    let mut children = IssueQuery::new("");
    children.parent = Some("SUB-1".into());
    children.page = request.clone();
    assert_eq!(api.issues(&children).unwrap().items.len(), 1);
    assert_eq!(
        api.relations("SUB-1", &request).unwrap().items[0].kind,
        "blocks"
    );
    assert_eq!(
        api.comments("SUB-1", &request)
            .unwrap()
            .next_cursor
            .as_deref(),
        Some("next")
    );
    assert_eq!(
        api.replies("SUB-1", "root-1", &request)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        api.issue_activity("SUB-1", &request)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(mock.requests().iter().all(|request| !request.path.contains("/orgs/") && request.path.starts_with("/api/v1/")));
}

#[test]
fn invalid_page_sizes_and_query_contradictions_are_rejected_offline() {
    let mock = Mock::new(|_| panic!("invalid query sent HTTP"));
    let api = client(&mock);
    assert!(
        api.comments(
            "SUB-1",
            &PageRequest {
                limit: 201,
                cursor: None
            }
        )
        .is_err()
    );
    assert!(
        api.issue_groups(&ListQuery::new(ListTab::Active).state_types(&["completed"]))
            .is_err()
    );
    assert!(
        api.issue_groups(&ListQuery::default().search("x".repeat(201)))
            .is_err()
    );
    assert!(mock.requests().is_empty());
}

#[test]
fn assignee_ids_and_repeated_states_and_priorities_keep_or_semantics() {
    let query = ListQuery::new(ListTab::All)
        .project(Some("SUB"))
        .assignee(Some("fixture-user-1"))
        .states(&["state-1", "state-2"])
        .priorities(&[1, 3])
        .kind(Some("tracker"))
        .due(Some("thisWeek"));
    let parameters = query.grouped_parameters().unwrap();
    assert!(parameters.contains(&("assignee".into(), "fixture-user-1".into())));
    assert_eq!(
        parameters.iter().filter(|(key, _)| key == "state").count(),
        2
    );
    assert_eq!(
        parameters
            .iter()
            .filter(|(key, _)| key == "priority")
            .count(),
        2
    );
    assert!(parameters.contains(&("type".into(), "tracker".into())));
}

#[test]
fn issue_rows_require_non_null_activity_time_and_keep_nullable_metadata() {
    let mut fixture = row("1");
    let issue: crate::model::IssueListItem = serde_json::from_value(fixture.clone()).unwrap();
    assert!(
        issue.fields.assignee.is_none()
            && issue.fields.estimate.is_none()
            && issue.fields.last_activity.is_none()
    );
    fixture["lastActivityAt"] = json!(null);
    assert!(serde_json::from_value::<crate::model::IssueListItem>(fixture).is_err());
}

#[test]
fn relations_accept_one_through_one_hundred_and_reject_larger_pages_offline() {
    let mock = Mock::new(|_| (200, page(vec![], None)));
    let api = client(&mock);
    for limit in [1, 100] {
        assert!(
            api.relations(
                "SUB-1",
                &PageRequest {
                    limit,
                    cursor: None
                }
            )
            .is_ok()
        );
    }
    for limit in [0, 101, 200] {
        assert!(matches!(
            api.relations(
                "SUB-1",
                &PageRequest {
                    limit,
                    cursor: None
                }
            ),
            Err(crate::Error::InvalidRequest)
        ));
    }
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].path, "/api/v1/issues/SUB-1/relations?limit=100");
}
