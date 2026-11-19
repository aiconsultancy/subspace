//! Fictional JSON fixtures mirror API 68531fc Decisions/Shared/Contracts.cs
//! and Domain/Projects/DecisionProvenance.cs (no production records).
use super::{client, page};
use crate::{
    DecisionQuery, Error, MAX_DECISION_PAGE_SIZE, PageRequest,
    model::{
        AgentRank, DecisionDetail, DecisionEvidenceKind, DecisionListItem, DecisionMakerType,
        DecisionStatus, Page,
    },
};
use chrono::NaiveDate;
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

fn row() -> Value {
    json!({
        "id":"00000000-0000-0000-0000-000000000001", "title":"Use the decision client",
        "status":"accepted", "decidedOn":"2026-11-17",
        "projectId":"00000000-0000-0000-0000-000000000002", "projectKey":"SUB",
        "project":{"id":"00000000-0000-0000-0000-000000000002","name":"Fixture project","archived":false},
        "supersedesId":"00000000-0000-0000-0000-000000000003", "supersededById":null,
        "supersedes":{"id":"00000000-0000-0000-0000-000000000003","title":"Prior choice","status":"superseded"},
        "supersededBy":null,
        "issues":{"items":[{"id":"00000000-0000-0000-0000-000000000004","identifier":"SUB-53","title":"Fixture issue"}],"count":1},
        "tags":["client"], "madeByType":"agent", "madeByUserId":null,
        "madeByAgentName":"Fixture crew", "madeByAgentRank":"crew", "madeByAgentModel":"fixture-model",
        "evidenceKind":"documentOrUrl", "evidenceReference":"https://example.invalid/design", "evidenceExcerpt":null,
        "recordedByUserId":"00000000-0000-0000-0000-000000000005", "version":7,
        "revisionNumber":2, "createdAt":"2026-11-17T12:00:00Z", "updatedAt":"2026-11-17T13:00:00Z"
    })
}

fn detail() -> Value {
    let mut value = row();
    let fields = json!({
        "decision":"Add read calls", "context":"Issue details", "why":"Retain context",
        "implications":"Read-only access", "alternatives":"Browser",
        "acceptedAt":"2026-11-17T13:00:00Z", "acceptedByUserId":"00000000-0000-0000-0000-000000000005",
        "rejectedAt":null, "rejectedByUserId":null,
        "provenance":{"madeByType":"human","madeByUserId":"00000000-0000-0000-0000-000000000005",
            "madeByAgentName":null,"madeByAgentRank":null,"madeByAgentModel":null,
            "evidenceKind":"ircMessage","evidenceReference":"fixture-message","evidenceExcerpt":"Proceed"},
        "recordedBy":{"userId":"00000000-0000-0000-0000-000000000005","displayName":"Fixture captain","available":true,"kind":"agent"}
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

#[test]
fn list_and_detail_decode_the_contract() {
    let list: Page<DecisionListItem> = serde_json::from_str(&page(vec![row()], None)).unwrap();
    let item = &list.items[0];
    assert_eq!(item.title, "Use the decision client");
    assert_eq!(item.status, DecisionStatus::Accepted);
    assert_eq!(item.decided_on(), NaiveDate::from_ymd_opt(2026, 11, 17));
    assert_eq!(item.project_key, "SUB");
    assert_eq!(item.project.as_ref().unwrap().name, "Fixture project");
    assert!(!item.project.as_ref().unwrap().archived);
    assert_eq!(
        item.supersedes.as_ref().unwrap().status,
        DecisionStatus::Superseded
    );
    assert!(item.superseded_by.is_none());
    assert_eq!(item.issues.items[0].identifier, "SUB-53");
    assert_eq!(item.issues.count, 1);
    assert_eq!(item.tags, ["client"]);
    assert_eq!(item.made_by_type, DecisionMakerType::Agent);
    assert_eq!(item.made_by_agent_rank, Some(AgentRank::Crew));
    assert_eq!(
        item.evidence_kind,
        Some(DecisionEvidenceKind::DocumentOrUrl)
    );
    assert!(item.made_by_user_id.is_none());
    assert_eq!(item.version, Some(7));
    assert_eq!(item.revision_number, 2);
    let detail: DecisionDetail = serde_json::from_value(detail()).unwrap();
    assert_eq!(detail.item.id, item.id);
    assert_eq!(detail.decision, "Add read calls");
    assert_eq!(detail.context, "Issue details");
    assert_eq!(detail.why, "Retain context");
    assert_eq!(detail.implications, "Read-only access");
    assert_eq!(detail.alternatives, "Browser");
    assert!(detail.accepted_at.is_some());
    assert!(detail.accepted_by_user_id.is_some());
    assert!(detail.rejected_at.is_none());
    assert!(detail.rejected_by_user_id.is_none());
    assert_eq!(detail.provenance.made_by_type, DecisionMakerType::Human);
    assert_eq!(
        detail.provenance.evidence_kind,
        Some(DecisionEvidenceKind::IrcMessage)
    );
    assert_eq!(detail.recorded_by.kind, DecisionMakerType::Agent);
    assert!(detail.recorded_by.available);
}

#[test]
fn null_and_omitted_optionals_include_historical_version() {
    let mut value = detail();
    for key in [
        "project",
        "supersedesId",
        "supersededById",
        "supersedes",
        "supersededBy",
        "madeByUserId",
        "madeByAgentName",
        "madeByAgentRank",
        "madeByAgentModel",
        "evidenceKind",
        "evidenceReference",
        "evidenceExcerpt",
        "version",
        "acceptedAt",
        "acceptedByUserId",
        "rejectedAt",
        "rejectedByUserId",
    ] {
        value[key] = Value::Null;
    }
    let decoded: DecisionDetail = serde_json::from_value(value.clone()).unwrap();
    assert!(decoded.item.version.is_none());
    assert!(decoded.item.project.is_none());
    assert!(decoded.item.evidence_kind.is_none());
    value.as_object_mut().unwrap().retain(|_, v| !v.is_null());
    let decoded: DecisionDetail = serde_json::from_value(value).unwrap();
    assert!(decoded.item.version.is_none());
    assert!(decoded.item.made_by_agent_rank.is_none());
    assert!(decoded.accepted_at.is_none());
}

#[test]
fn unknown_enums_are_forward_compatible() {
    let mut value = detail();
    for key in ["status", "madeByType", "madeByAgentRank", "evidenceKind"] {
        value[key] = json!("futureValue");
    }
    value["provenance"]["madeByAgentRank"] = json!("futureRank");
    value["recordedBy"]["kind"] = json!("futureKind");
    let decoded: DecisionDetail = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.item.status, DecisionStatus::Other);
    assert_eq!(decoded.item.made_by_type, DecisionMakerType::Other);
    assert_eq!(decoded.item.made_by_agent_rank, Some(AgentRank::Other));
    assert_eq!(
        decoded.item.evidence_kind,
        Some(DecisionEvidenceKind::Other)
    );
    assert_eq!(
        decoded.provenance.made_by_agent_rank,
        Some(AgentRank::Other)
    );
    assert_eq!(decoded.recorded_by.kind, DecisionMakerType::Other);
}

#[test]
fn unset_and_invalid_dates_are_not_shown_as_real_dates() {
    for date in ["0001-01-01", "not-a-date", "2026-02-30", ""] {
        let mut value = row();
        value["decidedOn"] = json!(date);
        let decoded: DecisionListItem = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.decided_on(), None);
    }
}

#[test]
fn list_and_detail_use_the_expected_paths_and_preserve_not_found() {
    let mock = Mock::new(|request| match request.path.as_str() {
        "/api/v1/decisions?issue=SUB-53&limit=50" => (200, page(vec![row()], Some("opaque+/="))),
        "/api/v1/decisions/00000000-0000-0000-0000-000000000001" => (200, detail().to_string()),
        "/api/v1/decisions/00000000-0000-0000-0000-000000000099" => (404, "{}".into()),
        _ => (400, "{}".into()),
    });
    let api = client(&mock);
    let query = DecisionQuery {
        issue: Some("SUB-53".into()),
        ..Default::default()
    };
    let list = api.decisions(&query, &PageRequest::default()).unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.next_cursor.as_deref(), Some("opaque+/="));
    assert_eq!(
        api.decision("00000000-0000-0000-0000-000000000001")
            .unwrap()
            .item
            .title,
        "Use the decision client"
    );
    assert!(matches!(
        api.decision("00000000-0000-0000-0000-000000000099"),
        Err(Error::NotFound)
    ));
    assert_eq!(mock.requests()[0].method, "GET");
    assert_eq!(
        mock.requests()[1].path,
        "/api/v1/decisions/00000000-0000-0000-0000-000000000001"
    );
}

#[test]
fn malformed_decision_id_preserves_the_api_bad_request_status() {
    let mock = Mock::new(|_| (400, "{}".into()));
    assert!(matches!(
        client(&mock).decision("not-a-guid"),
        Err(Error::UnexpectedStatus(400))
    ));
    assert_eq!(mock.requests().len(), 1);
    assert_eq!(mock.requests()[0].path, "/api/v1/decisions/not-a-guid");
}

#[test]
fn cursor_is_opaque_limits_are_capped_and_zero_is_invalid() {
    let mock = Mock::new(|_| (200, page(vec![], None)));
    let cursor = "a+/= %&?雪";
    let page = PageRequest {
        limit: 200,
        cursor: Some(cursor.into()),
    };
    let query = DecisionQuery {
        issue: Some("SUB-53".into()),
        ..Default::default()
    };
    assert!(client(&mock).decisions(&query, &page).is_ok());
    let request = &mock.requests()[0];
    let url = url::Url::parse(&format!("http://localhost{}", request.path)).unwrap();
    let pairs: Vec<_> = url.query_pairs().collect();
    assert_eq!(MAX_DECISION_PAGE_SIZE, 100);
    assert_eq!(pairs[1].1, "100");
    assert_eq!(pairs[2].1, cursor);
    assert!(matches!(
        client(&mock).decisions(
            &query,
            &PageRequest {
                limit: 0,
                cursor: None
            }
        ),
        Err(Error::InvalidRequest)
    ));
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn all_filters_encode_repeated_status_and_escaped_text() {
    let query = DecisionQuery {
        issue: Some("SUB-53".into()),
        project: Some("SUB".into()),
        status: vec![
            DecisionStatus::Proposed,
            DecisionStatus::Accepted,
            DecisionStatus::Rejected,
            DecisionStatus::Superseded,
        ],
        made_by_type: Some(DecisionMakerType::Human),
        tag: Some("a&b".into()),
        q: Some("why this?".into()),
    };
    let parameters = query.parameters(&PageRequest::default()).unwrap();
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(&parameters)
        .finish();
    assert_eq!(
        encoded,
        "issue=SUB-53&project=SUB&tag=a%26b&q=why+this%3F&status=proposed&status=accepted&status=rejected&status=superseded&madeByType=human&limit=50"
    );
    let agent = DecisionQuery {
        made_by_type: Some(DecisionMakerType::Agent),
        ..Default::default()
    };
    assert!(
        agent
            .parameters(&PageRequest::default())
            .unwrap()
            .contains(&("madeByType".into(), "agent".into()))
    );
    let unknown = DecisionQuery {
        status: vec![DecisionStatus::Other],
        ..Default::default()
    };
    assert!(matches!(
        unknown.parameters(&PageRequest::default()),
        Err(Error::InvalidRequest)
    ));
    let unknown = DecisionQuery {
        made_by_type: Some(DecisionMakerType::Other),
        ..Default::default()
    };
    assert!(matches!(
        unknown.parameters(&PageRequest::default()),
        Err(Error::InvalidRequest)
    ));
}
