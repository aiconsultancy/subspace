use super::{document, enable, setup};
use subspace_search::{Filters, IndexMetadata, Policy, RecordQuery, Service};

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Five records in two parents, a second apart, `a-0` the oldest.
fn records_fixture() -> (tempfile::TempDir, Service, subspace_search::Scope) {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    let now = now();
    let documents: Vec<_> = (0..5)
        .map(|index| {
            let parent = if index % 2 == 0 { "a" } else { "b" };
            let mut record = document(&format!("{parent}-{index}"), "stored line");
            record.parent_id = Some(parent.into());
            record.refreshed_at = now - 10 + index;
            record
        })
        .collect();
    service.upsert(&scope, &documents).unwrap();
    (root, service, scope)
}

fn ids(page: &subspace_search::RecordPage) -> Vec<&str> {
    page.documents
        .iter()
        .map(|record| record.id.as_str())
        .collect()
}

#[test]
fn records_page_newest_first_by_facet_with_a_cursor_and_payloads() {
    let (_root, service, scope) = records_fixture();
    let mut query = RecordQuery {
        filters: Filters {
            parent_id: Some("a".into()),
            ..Default::default()
        },
        before: None,
        limit: 2,
    };
    let first = service.records(&scope, &query).unwrap();
    assert_eq!(ids(&first), ["a-4", "a-2"]);
    assert_eq!(
        first.documents[0].payload.as_deref(),
        Some("{\"cached\":true}")
    );
    query.before = first.next.clone();
    let second = service.records(&scope, &query).unwrap();
    assert_eq!(ids(&second), ["a-0"]);
    assert!(second.next.is_none());
    query.limit = 0;
    assert!(service.records(&scope, &query).is_err());
}

#[test]
fn groups_list_each_parent_with_its_count_newest_first() {
    let (_root, service, scope) = records_fixture();
    let groups = service.groups(&scope, &Filters::default(), 10).unwrap();
    let summary: Vec<(&str, u64)> = groups
        .iter()
        .map(|group| (group.parent_id.as_str(), group.records))
        .collect();
    assert_eq!(summary, [("a", 3), ("b", 2)]);
    assert!(groups[0].newest > groups[1].newest);
}

#[test]
fn remove_matching_deletes_one_parent_and_refuses_an_empty_filter() {
    let (_root, service, scope) = records_fixture();
    assert!(
        service
            .remove_matching(&scope, &Filters::default())
            .is_err()
    );
    let by_parent = Filters {
        parent_id: Some("b".into()),
        ..Default::default()
    };
    assert_eq!(service.remove_matching(&scope, &by_parent).unwrap(), 2);
    let all = RecordQuery {
        limit: 10,
        ..Default::default()
    };
    assert_eq!(
        ids(&service.records(&scope, &all).unwrap()),
        ["a-4", "a-2", "a-0"]
    );
}

#[test]
fn records_need_the_active_scope_and_caching_on() {
    let (_root, service, scope) = records_fixture();
    let all = RecordQuery {
        limit: 10,
        ..Default::default()
    };
    let mut other = scope.clone();
    other.account = "other".into();
    assert!(service.records(&other, &all).is_err());
    service.set_policy("irc", Policy::default()).unwrap();
    assert!(service.records(&scope, &all).is_err());
}

#[test]
fn metadata_batch_sets_the_project_facet_of_many_records() {
    let (_root, service, scope) = records_fixture();
    let entries: Vec<(String, IndexMetadata)> = ["a-0", "a-2"]
        .iter()
        .map(|id| {
            (
                (*id).to_owned(),
                IndexMetadata {
                    project: Some("network".into()),
                    references: vec![],
                },
            )
        })
        .collect();
    service.index_metadata_batch(&scope, &entries).unwrap();
    let query = RecordQuery {
        filters: Filters {
            project: Some("network".into()),
            ..Default::default()
        },
        before: None,
        limit: 10,
    };
    assert_eq!(
        ids(&service.records(&scope, &query).unwrap()),
        ["a-2", "a-0"]
    );
}

#[test]
fn a_board_default_applies_until_the_person_chooses() {
    let (_root, service, _scope) = setup();
    let default_on = Policy {
        enabled: true,
        ..Policy::default()
    };
    assert!(service.offer_default("irc", default_on.clone()).unwrap());
    assert!(service.policy("irc").unwrap().enabled);
    assert!(!service.offer_default("irc", default_on.clone()).unwrap());
    service.set_policy("irc", Policy::default()).unwrap();
    assert!(!service.offer_default("irc", default_on).unwrap());
    assert!(!service.policy("irc").unwrap().enabled);
}

#[test]
fn an_excerpt_comes_from_the_body_even_when_the_title_matched() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let mut record = document("one", "Rollout is green on staging");
    record.title = "#deploys · alice".into();
    service.upsert(&scope, &[record]).unwrap();
    let hits = service.search(&super::query("deploy")).unwrap().hits;
    assert_eq!(hits[0].excerpt, "Rollout is green on staging");
}
