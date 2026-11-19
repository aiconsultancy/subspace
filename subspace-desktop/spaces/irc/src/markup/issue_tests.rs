//! Issue references in message text, plain and formatted.

use subspace_framework::contracts::issues::IssueScope;

use super::link::ref_url;
use super::{IssueKeys, Mentions, Ref, issues, parse_ref_url, refs, to_markdown};

fn scoped() -> Mentions {
    let scope = IssueScope::new("dev", "Subspace API", "https://api.example.test/api")
        .with_project_keys(["TST", "OPS"]);
    Mentions::new(["theo"]).with_issues(IssueKeys::new(&scope))
}

fn found(text: &str, mentions: &Mentions) -> Vec<(String, Ref)> {
    refs(text, mentions)
        .into_iter()
        .map(|(range, found)| (text[range].to_owned(), found))
        .collect()
}

fn issue(identifier: &str) -> Ref {
    Ref::Issue(identifier.into())
}

#[test]
fn bare_identifiers_with_known_keys_are_issue_references() {
    let text = "(TST-12), OPS-4. tst-5 PRE-TST-6 XYZ-7 TST-2026-11-15 TST-12";
    assert_eq!(
        found(text, &scoped()),
        [
            ("TST-12".to_owned(), issue("TST-12")),
            ("OPS-4".to_owned(), issue("OPS-4")),
            ("TST-12".to_owned(), issue("TST-12")),
        ]
    );
    assert_eq!(
        issues(text, &scoped(), None),
        ["TST-12", "OPS-4"],
        "distinct"
    );
}

#[test]
fn legacy_issue_links_stay_ordinary_links() {
    let text = "https://app.example.test/issues/TST-827 \
         https://linear.example.com/workspace/issue/OPS-3/a-title /issues/TST-4 \
         https://api.example.test/api/v1/issues/TST-5";
    assert!(issues(text, &scoped(), None).is_empty());
}

#[test]
fn without_a_scope_nothing_is_an_issue() {
    let text = "TST-12 https://api.example.test/api/v1/issues/TST-827 /issues/TST-4";
    assert!(issues(text, &Mentions::new(["theo"]), None).is_empty());
    let markdown = to_markdown(&format!("*see* {text}"), &Mentions::default())
        .map(|rendered| rendered.markdown)
        .unwrap_or_default();
    assert!(!markdown.contains("subspace-irc:issue"), "{markdown}");
    assert!(
        markdown.contains("(https://api.example.test/api/v1/issues/TST-827)"),
        "the link stays a link: {markdown}"
    );
}

#[test]
fn formatted_messages_link_issue_references_for_the_board() {
    let text =
        "*ship* TST-12, [the tracker](https://api.example.test/api/v1/issues/OPS-1) and `TST-3`";
    let markdown = to_markdown(text, &scoped())
        .map(|rendered| rendered.markdown)
        .unwrap_or_default();
    assert!(
        markdown.contains("[TST\\-12](subspace-irc:issue/TST-12)"),
        "{markdown}"
    );
    assert!(
        markdown.contains("[the tracker](https://api.example.test/api/v1/issues/OPS-1)"),
        "{markdown}"
    );
    assert!(markdown.contains("`TST-3`"), "code stays plain: {markdown}");
}

#[test]
fn preview_identifiers_come_from_visible_references_outside_code() {
    let text = "TST-1 *TST-3* [Tracker](https://linear.example.com/workspace/issue/TST-2/example) TST-1 `show TST-4 only as code`\n\n```\nsee TST-5\n```";
    let mentions = scoped();
    let rendered = to_markdown(text, &mentions).unwrap();
    assert_eq!(issues(text, &mentions, Some(&rendered)), ["TST-1", "TST-3"]);
}

#[test]
fn issue_reference_links_round_trip() {
    let url = ref_url(&issue("TST-12"));
    assert_eq!(url, "subspace-irc:issue/TST-12");
    assert_eq!(parse_ref_url(&url), Some(issue("TST-12")));
}
