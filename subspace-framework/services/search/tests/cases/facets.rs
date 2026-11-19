use super::{document, enable, query, setup};
use subspace_search::{
    Filters, IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch, RelatedQuery,
};

#[test]
fn every_facet_narrows_results_and_values_are_parameters() {
    let (_root, service, now) = facet_fixture();
    for filters in [
        Filters {
            kind: Some("pull-request".into()),
            ..Default::default()
        },
        Filters {
            account: Some("account".into()),
            project: Some("repo-one".into()),
            ..Default::default()
        },
        Filters {
            parent_id: Some("parent-1".into()),
            ..Default::default()
        },
        Filters {
            refreshed_before: Some(now - 5),
            ..Default::default()
        },
        Filters {
            refreshed_after: Some(now - 5),
            ..Default::default()
        },
    ] {
        assert_eq!(
            service
                .search_filtered(&query("needle"), &filters)
                .unwrap()
                .hits
                .len(),
            1
        );
    }
    let injection = Filters {
        project: Some("repo-one' OR 1=1 --".into()),
        ..Default::default()
    };
    assert!(
        service
            .search_filtered(&query("needle"), &injection)
            .unwrap()
            .hits
            .is_empty()
    );
    assert_eq!(service.search(&query("needle")).unwrap().hits.len(), 2);
}

#[test]
fn exact_cross_board_links_and_labelled_heuristics_are_isolated() {
    let (_root, service, github, commit, task) = relation_fixture();
    let related = service
        .related_items(&github, &related_query(vec![]))
        .unwrap();
    assert_eq!(related.len(), 4);
    assert!(
        related
            .iter()
            .all(|item| item.matched == ReferenceMatch::Exact)
    );
    assert_eq!(
        service
            .related_items(&task, &related_query(vec![]))
            .unwrap()
            .len(),
        2
    );
    let heuristic = service
        .related_items(&commit, &related_query(vec!["irc".into()]))
        .unwrap();
    assert_eq!(heuristic.len(), 1);
    assert!(
        matches!(&heuristic[0].matched, ReferenceMatch::Heuristic { evidence, confidence } if evidence.contains("commit prefix") && *confidence == 0.8)
    );
}

fn relation_fixture() -> (
    tempfile::TempDir,
    subspace_search::Service,
    ReferenceIdentity,
    ReferenceIdentity,
    ReferenceIdentity,
) {
    let root = tempfile::tempdir().unwrap();
    let boards = ["github", "actions", "irc", "github-copilot-agents"];
    let service = subspace_search::Service::open(
        root.path().join("search"),
        "app".into(),
        boards.iter().map(|board| (*board).into()).collect(),
    )
    .unwrap();
    let github = ReferenceIdentity::github("owner/repo", 42).unwrap();
    let commit = ReferenceIdentity::commit("owner/repo", "abc123").unwrap();
    let task = ReferenceIdentity::github_copilot_agents("task-7").unwrap();
    for board in boards {
        let scope = subspace_search::Scope {
            app: "app".into(),
            board: board.into(),
            endpoint: "https://example.test".into(),
            tenant: "tenant".into(),
            account: "account".into(),
        };
        enable_board(&service, &scope);
        let mut item = document(board, "linked needle");
        item.id = board.into();
        service.upsert(&scope, &[item]).unwrap();
        let references = references(board, &github, &commit, &task);
        service
            .index_metadata(&scope, board, &metadata("owner/repo", references))
            .unwrap();
    }
    (root, service, github, commit, task)
}

#[test]
fn inactive_account_never_leaks_through_a_filter_or_relation() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .upsert(&scope, &[document("old", "needle")])
        .unwrap();
    let identity = ReferenceIdentity::repository("owner/repo").unwrap();
    service
        .index_metadata(
            &scope,
            "old",
            &metadata("repo", vec![exact(identity.clone(), "repository")]),
        )
        .unwrap();
    let mut other = scope;
    other.account = "other".into();
    service.activate(&other).unwrap();
    let filters = Filters {
        account: Some("account".into()),
        ..Default::default()
    };
    assert!(
        service
            .search_filtered(&query("needle"), &filters)
            .unwrap()
            .hits
            .is_empty()
    );
    assert!(
        service
            .related_items(
                &identity,
                &RelatedQuery {
                    boards: vec![],
                    relation_types: vec![],
                    limit: 20
                }
            )
            .unwrap()
            .is_empty()
    );
}

fn facet_fixture() -> (tempfile::TempDir, subspace_search::Service, i64) {
    let (root, service, scope) = setup();
    enable(&service, &scope);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut first = document("first", "shared needle");
    first.kind = "pull-request".into();
    first.parent_id = Some("parent-1".into());
    first.refreshed_at = now - 10;
    let mut second = document("second", "shared needle");
    second.kind = "issue".into();
    second.parent_id = Some("parent-2".into());
    second.refreshed_at = now;
    service.upsert(&scope, &[first, second]).unwrap();
    service
        .index_metadata(&scope, "first", &metadata("repo-one", vec![]))
        .unwrap();
    service
        .index_metadata(&scope, "second", &metadata("repo-two", vec![]))
        .unwrap();
    (root, service, now)
}

fn references(
    board: &str,
    github: &ReferenceIdentity,
    commit: &ReferenceIdentity,
    task: &ReferenceIdentity,
) -> Vec<Reference> {
    match board {
        "github" => vec![
            exact(github.clone(), "pull-request"),
            exact(task.clone(), "result"),
        ],
        "actions" => vec![exact(github.clone(), "pull-request-run")],
        "irc" => vec![
            exact(github.clone(), "mentioned-pull-request"),
            Reference {
                identity: commit.clone(),
                relation_type: "mentioned-commit".into(),
                matched: ReferenceMatch::Heuristic {
                    evidence: "service supplied commit prefix".into(),
                    confidence: 0.8,
                },
            },
        ],
        _ => vec![
            exact(github.clone(), "resulting-pull-request"),
            exact(task.clone(), "task"),
        ],
    }
}

fn related_query(boards: Vec<String>) -> RelatedQuery {
    RelatedQuery {
        boards,
        relation_types: vec![],
        limit: 20,
    }
}

fn metadata(project: &str, references: Vec<Reference>) -> IndexMetadata {
    IndexMetadata {
        project: Some(project.into()),
        references,
    }
}

fn exact(identity: ReferenceIdentity, relation_type: &str) -> Reference {
    Reference {
        identity,
        relation_type: relation_type.into(),
        matched: ReferenceMatch::Exact,
    }
}

fn enable_board(service: &subspace_search::Service, scope: &subspace_search::Scope) {
    service
        .set_policy(
            &scope.board,
            subspace_search::Policy {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap();
    service.activate(scope).unwrap();
}
