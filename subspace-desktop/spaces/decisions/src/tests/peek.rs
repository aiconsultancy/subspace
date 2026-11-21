use subspace_api_client::model::DecisionStatus;

use super::{Evidence, Issue, Related, facts, peek, prose, relations};
use crate::fixtures;

fn fact<'a>(facts: &'a [(&'static str, String)], label: &str) -> Option<&'a str> {
    facts
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, value)| value.as_str())
}

#[test]
fn the_peek_holds_the_facts_the_list_read_gives() {
    let item = fixtures::item("d2");
    let peek = peek(&item, &fixtures::people(), &fixtures::projects());
    assert_eq!(peek.status, DecisionStatus::Accepted);
    assert_eq!(
        peek.title,
        "Read the relay position after the correction is applied"
    );
    let labels: Vec<_> = peek.facts.iter().map(|(label, _)| *label).collect();
    assert_eq!(
        labels,
        ["Decided on", "Made by", "Recorded by", "Project", "Tags"]
    );
    assert_eq!(fact(&peek.facts, "Decided on"), Some("19 November 2026"));
    assert_eq!(
        fact(&peek.facts, "Made by"),
        Some("aurora-lyra · captain · fixture-model (agent)")
    );
    assert_eq!(fact(&peek.facts, "Recorded by"), Some("local-operator"));
    assert_eq!(fact(&peek.facts, "Project"), Some("NAV · Navigation"));
}

#[test]
fn a_person_s_decision_names_them_from_the_account_s_people() {
    let item = fixtures::item("d3");
    let peek = peek(&item, &fixtures::people(), &[]);
    assert_eq!(fact(&peek.facts, "Made by"), Some("local-operator"));
    let unknown = super::peek(&item, &Default::default(), &[]);
    assert_eq!(fact(&unknown.facts, "Made by"), Some("A person"));
    assert_eq!(fact(&unknown.facts, "Recorded by"), Some("Not known"));
}

#[test]
fn relations_name_both_directions_the_issues_and_the_evidence() {
    let newer = relations(&fixtures::item("d2"));
    assert_eq!(
        newer.supersedes,
        Some(Related {
            id: "d6".into(),
            title: "Read the relay position before each burn".into(),
        })
    );
    assert_eq!(newer.superseded_by, None);
    assert_eq!(
        newer.issues,
        [
            Issue {
                identifier: "NAV-2".into(),
                title: "Course correction preview".into(),
            },
            Issue {
                identifier: "NAV-5".into(),
                title: "Calibrate the star tracker fixtures".into(),
            },
        ]
    );
    assert_eq!(newer.more_issues, 0);
    assert_eq!(
        newer.evidence,
        Some(Evidence {
            kind: Some("IRC message"),
            reference: Some("https://example.invalid/aurora-nav".into()),
            excerpt: Some("The preview reads the relay position after the correction.".into()),
        })
    );
    let older = relations(&fixtures::item("d6"));
    assert_eq!(
        older.superseded_by.map(|related| related.id),
        Some("d2".into())
    );
    assert_eq!(older.evidence, None, "nothing recorded, nothing shown");
}

#[test]
fn an_opened_decision_s_facts_add_its_stamps_and_revision() {
    let detail = fixtures::detail("d2").unwrap();
    let facts = facts(&detail, &fixtures::people(), &fixtures::projects());
    let labels: Vec<_> = facts.iter().map(|(label, _)| *label).collect();
    assert_eq!(
        labels,
        [
            "Status",
            "Decided on",
            "Made by",
            "Recorded by",
            "Accepted",
            "Project",
            "Tags",
            "Revision"
        ]
    );
    assert_eq!(fact(&facts, "Status"), Some("Accepted"));
    assert!(fact(&facts, "Accepted").is_some_and(|stamp| stamp.ends_with(" by local-operator")));
    assert_eq!(fact(&facts, "Revision"), Some("1"));
    let proposed = fixtures::detail("d1").unwrap();
    let facts = super::facts(&proposed, &fixtures::people(), &[]);
    assert!(fact(&facts, "Accepted").is_none() && fact(&facts, "Rejected").is_none());
}

#[test]
fn the_page_leaves_out_an_empty_section() {
    let detail = fixtures::detail("d2").unwrap();
    let headings: Vec<_> = prose(&detail)
        .into_iter()
        .map(|(heading, _)| heading)
        .collect();
    assert_eq!(headings, ["Decision", "Context", "Why", "Implications"]);
}
