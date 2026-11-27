use super::facts;
use serde_json::json;
use subspace_api_client::model::IssueDetail;

#[test]
fn properties_follow_api_scope_and_explain_absent_names_without_ids() {
    let mut value = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
    value["estimate"] = json!(0);
    value["milestoneId"] = json!("private-id");
    value["cycle"] = json!({"id":"cycle", "number":3, "state":"active"});
    value["assignee"] = crate::fixtures::person("Jo Fixture");
    value["assignee"]["isFormerMember"] = json!(true);
    value["owner"] = crate::fixtures::person("Rosa Fixture");
    value["owner"]["isFormerMember"] = json!(true);
    value["parentId"] = json!("missing-parent");
    let detail: IssueDetail = serde_json::from_value(value).unwrap();
    let rows: Vec<_> = facts(
        &detail,
        &[crate::fixtures::project("SUB", "Subspace", false)],
    )
    .into_iter()
    .map(|f| (f.label, f.value))
    .collect();
    assert_eq!(
        rows.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
        [
            "State",
            "Priority",
            "Owner",
            "Assignee",
            "Labels",
            "Due date",
            "Project",
            "Milestone",
            "Parent",
            "Type",
            "Estimate",
            "Cycle"
        ]
    );
    assert!(rows.contains(&("Project", "SUB · Subspace".into())));
    assert!(rows.contains(&("Milestone", "Unavailable".into())));
    assert!(rows.contains(&("Parent", "Unavailable".into())));
    assert!(rows.contains(&("Estimate", "0".into())));
    assert!(rows.contains(&("Owner", "Rosa Fixture (former member)".into())));
    assert!(rows.contains(&("Assignee", "Jo Fixture (former member)".into())));
    assert!(!rows.iter().any(|(_, value)| value.contains("private-id")));
}
