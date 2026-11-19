use super::{client, detail, row};
use crate::{
    issue_writes::{IssueChanges, IssueProperty, IssueSaveError, SUMMARY_LIMIT},
    model::{IssueDetail, IssueListItem, IssueReference},
};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

#[test]
fn detail_list_and_reference_parse_summary_text_and_legacy_null() {
    for summary in [
        json!("Keep the team informed about the problem this issue solves."),
        Value::Null,
    ] {
        let mut value = detail("1");
        value["summary"] = summary.clone();
        let detail: IssueDetail = serde_json::from_value(value).unwrap();
        let mut value = row("1");
        value["summary"] = summary.clone();
        let list: IssueListItem = serde_json::from_value(value.clone()).unwrap();
        value["archived"] = json!(false);
        let reference: IssueReference = serde_json::from_value(value).unwrap();
        for actual in [
            &detail.fields.summary,
            &list.fields.summary,
            &reference.summary,
        ] {
            assert_eq!(actual.as_deref(), summary.as_str());
        }
    }
}

#[test]
fn create_requires_summary_and_checks_the_character_and_paragraph_limits_before_transport() {
    let mock = Mock::new(|_| (201, detail("1").to_string()));
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Title("Fixture summary".into()));
    assert_eq!(
        client(&mock)
            .create_issue("project-1", &changes)
            .unwrap_err(),
        IssueSaveError::Summary
    );
    for text in [
        String::new(),
        " ".into(),
        "a\nb".into(),
        "a\rb".into(),
        "a\tb".into(),
        "🙂".repeat(SUMMARY_LIMIT),
        "a\u{2029}b".into(),
        "x".repeat(SUMMARY_LIMIT + 1),
    ] {
        changes.set(IssueProperty::Summary(text));
        assert_eq!(changes.validate(true), Err(IssueSaveError::Summary));
        assert_eq!(
            client(&mock)
                .create_issue("project-1", &changes)
                .unwrap_err(),
            IssueSaveError::Summary
        );
        assert_eq!(changes.validate(false), Err(IssueSaveError::Summary));
    }
    assert!(mock.requests().is_empty());
    let summary = "🙂".repeat(SUMMARY_LIMIT / 2);
    changes.set(IssueProperty::Summary(format!("  {summary}\n")));
    client(&mock).create_issue("project-1", &changes).unwrap();
    let body: Value = serde_json::from_str(&mock.requests()[0].body).unwrap();
    assert_eq!(body["summary"], summary);
}
