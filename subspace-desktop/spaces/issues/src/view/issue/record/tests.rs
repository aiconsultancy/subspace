use super::facts;
use serde_json::json;
use subspace_api_client::model::IssueDetail;

#[test]
fn each_original_date_falls_back_independently_to_the_native_fact() {
    let mut value = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
    value["provenance"] = json!({"sourceSystem":"linear", "originalIdentifier":"SUB-10",
        "originalCreatedAt":"2020-01-01T00:00:00Z", "originalCompletedAt":null});
    value["startedAt"] = json!("2026-02-02T00:00:00Z");
    value["completedAt"] = json!("2026-03-03T00:00:00Z");
    value["createdBy"] = json!({"userId":"former", "displayName":"Jo Fixture", "isFormerMember":true, "kind":"human"});
    let detail: IssueDetail = serde_json::from_value(value).unwrap();
    let rows: Vec<_> = facts(&detail)
        .into_iter()
        .map(|f| (f.label, f.value))
        .collect();
    assert!(
        rows.contains(&("Originally created", "1 Jan 2020".into())),
        "{rows:?}"
    );
    assert!(rows.contains(&("Started", "2 Feb 2026".into())));
    assert!(rows.contains(&("Completed", "3 Mar 2026".into())));
    assert!(rows.contains(&("Created by", "Jo Fixture (former member)".into())));
    assert!(!rows.iter().any(|(label, _)| *label == "Cancelled"));
}
