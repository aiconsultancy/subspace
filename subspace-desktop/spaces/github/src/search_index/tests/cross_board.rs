//! Cross-board relation identities: a GitHub document and another board's
//! document, both found through their matching, exact relation.
use super::super::mapping::{document, scope};
use super::super::relations;
use super::fixtures::detail;
use subspace_framework::client_github::model::GhItemDetail;

fn two_board_service(
    root: &std::path::Path,
    boards: [&str; 2],
) -> subspace_framework::dashboard::full_text::Service {
    use subspace_framework::dashboard::full_text::{Policy, Service};

    let service = Service::open(
        root.to_path_buf(),
        "app".into(),
        boards.iter().map(|board| (*board).into()).collect(),
    )
    .unwrap();
    let defaults = Policy::default();
    for board in boards {
        service
            .set_policy(
                board,
                Policy {
                    enabled: true,
                    retention_days: defaults.retention_days,
                    max_bytes: defaults.max_bytes,
                },
            )
            .unwrap();
    }
    service
}

struct OtherBoardFixture {
    board: &'static str,
    scope: subspace_framework::dashboard::full_text::Scope,
    document: subspace_framework::dashboard::full_text::Document,
    metadata: subspace_framework::dashboard::full_text::IndexMetadata,
}

struct CrossBoardCase {
    label: &'static str,
    query: subspace_framework::dashboard::full_text::ReferenceIdentity,
    fixture: OtherBoardFixture,
}

/// Builds a two-board `Service`, writes the GitHub document and `case.fixture`'s
/// document with their relation metadata, then asserts `related_items(case.query)`
/// finds both the GitHub document (through its own self identity) and the
/// other board's document (through its matching relation) as exact links.
fn assert_linked_by(case: CrossBoardCase) {
    use subspace_framework::dashboard::full_text::RelatedQuery;

    let detail = detail();
    let github_document =
        document(&detail, chrono::Utc::now().timestamp(), "Node-A", "ann").unwrap();
    let root = std::env::temp_dir().join(format!(
        "subspace-github-{}-link-{}-{}",
        case.label,
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let service = two_board_service(&root, ["github", case.fixture.board]);
    let github_scope = scope("app", "https://api.github.com", "Node-A").unwrap();
    publish_fixture(&service, &github_scope, &github_document, &detail);
    publish_fixture_other(&service, &case.fixture);

    let query = RelatedQuery {
        limit: 10,
        ..RelatedQuery::default()
    };
    let related = service.related_items(&case.query, &query).unwrap();
    assert_found(&related, &github_document.id, "the GitHub document");
    assert_found(
        &related,
        &case.fixture.document.id,
        &format!("the {} document", case.fixture.board),
    );

    drop(service);
    std::fs::remove_dir_all(root).unwrap();
}

fn publish_fixture(
    service: &subspace_framework::dashboard::full_text::Service,
    scope: &subspace_framework::dashboard::full_text::Scope,
    document: &subspace_framework::dashboard::full_text::Document,
    detail: &GhItemDetail,
) {
    service.activate(scope).unwrap();
    service
        .upsert(scope, std::slice::from_ref(document))
        .unwrap();
    service
        .index_metadata(scope, &document.id, &relations::metadata(&detail.item))
        .unwrap();
}

fn publish_fixture_other(
    service: &subspace_framework::dashboard::full_text::Service,
    fixture: &OtherBoardFixture,
) {
    service.activate(&fixture.scope).unwrap();
    service
        .upsert(&fixture.scope, std::slice::from_ref(&fixture.document))
        .unwrap();
    service
        .index_metadata(&fixture.scope, &fixture.document.id, &fixture.metadata)
        .unwrap();
}

fn assert_found(
    related: &[subspace_framework::dashboard::full_text::RelatedItem],
    document_id: &str,
    label: &str,
) {
    use subspace_framework::dashboard::full_text::ReferenceMatch;

    assert!(
        related.iter().any(
            |item| item.hit.document.id == document_id && item.matched == ReferenceMatch::Exact
        ),
        "expected {label} to be found through its matching relation"
    );
}

fn plain_document(
    id: &str,
    title: &str,
    kind: &str,
    repo: &str,
) -> subspace_framework::dashboard::full_text::Document {
    subspace_framework::dashboard::full_text::Document {
        id: id.into(),
        title: title.into(),
        body: String::new(),
        metadata: String::new(),
        parent_id: Some(repo.into()),
        kind: kind.into(),
        url: None,
        navigation: id.into(),
        refreshed_at: chrono::Utc::now().timestamp(),
        payload: None,
    }
}

#[test]
fn a_github_pull_request_and_the_agent_task_that_produced_it_are_both_found_by_the_exact_pull_request_identity()
 {
    use subspace_framework::dashboard::full_text::{
        IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch,
    };

    let repo = detail().item.repo;
    let number = u64::from(detail().item.number);
    let fixture = OtherBoardFixture {
        board: "github-copilot-agents",
        scope: subspace_framework::dashboard::search_adapter::scope(
            "app",
            "github-copilot-agents",
            "https://api.github.com",
            "config:github.token",
        )
        .unwrap(),
        document: plain_document("task-1", "Agent task", "Agent task result", &repo),
        metadata: IndexMetadata {
            project: Some(repo.clone()),
            references: vec![
                Reference {
                    identity: ReferenceIdentity::github_copilot_agents("task-1").unwrap(),
                    relation_type: "self".into(),
                    matched: ReferenceMatch::Exact,
                },
                Reference {
                    identity: ReferenceIdentity::repository(&repo).unwrap(),
                    relation_type: "repository".into(),
                    matched: ReferenceMatch::Exact,
                },
                Reference {
                    identity: ReferenceIdentity::github(&repo, number).unwrap(),
                    relation_type: "resulting-pull-request".into(),
                    matched: ReferenceMatch::Exact,
                },
            ],
        },
    };
    assert_linked_by(CrossBoardCase {
        label: "copilot agents pull request",
        query: ReferenceIdentity::github(&repo, number).unwrap(),
        fixture,
    });
}

#[test]
fn a_github_pull_request_and_an_agent_task_on_its_head_branch_are_both_found_by_the_exact_branch_identity()
 {
    use subspace_framework::dashboard::full_text::{
        IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch,
    };

    let repo = detail().item.repo;
    let head_ref = detail().item.head_ref.unwrap();
    let fixture = OtherBoardFixture {
        board: "github-copilot-agents",
        scope: subspace_framework::dashboard::search_adapter::scope(
            "app",
            "github-copilot-agents",
            "https://api.github.com",
            "config:github.token",
        )
        .unwrap(),
        document: plain_document("task-2", "Agent task", "Agent task on branch", &repo),
        metadata: IndexMetadata {
            project: Some(repo.clone()),
            references: vec![
                Reference {
                    identity: ReferenceIdentity::github_copilot_agents("task-2").unwrap(),
                    relation_type: "self".into(),
                    matched: ReferenceMatch::Exact,
                },
                Reference {
                    identity: ReferenceIdentity::repository(&repo).unwrap(),
                    relation_type: "repository".into(),
                    matched: ReferenceMatch::Exact,
                },
                Reference {
                    identity: ReferenceIdentity::branch(&repo, &head_ref).unwrap(),
                    relation_type: "branch".into(),
                    matched: ReferenceMatch::Exact,
                },
            ],
        },
    };
    assert_linked_by(CrossBoardCase {
        label: "copilot agents branch",
        query: ReferenceIdentity::branch(&repo, &head_ref).unwrap(),
        fixture,
    });
}
