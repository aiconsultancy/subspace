//! URLs no longer carry issue-reference semantics.

use subspace_contracts::issues::recognition::ReferenceRecognizer;

#[test]
fn legacy_app_and_linear_urls_are_not_plain_identifiers() {
    let references = ReferenceRecognizer::new(["SUB", "OPS", "PROJECTKEY"], []);
    for url in [
        "https://tracker.example.test/issues/SUB-827",
        "/issues/SUB-827",
        "https://linear.example.com/workspace/issue/SUB-827/a-title",
        "https://api.example.test/api/v1/issues/OPS-3",
    ] {
        assert!(references.find(url).is_empty(), "{url}");
        assert!(!references.recognises(url), "{url}");
    }
    let found = references.find("See SUB-827, OPS-3 and PROJECTKEY-9.");
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].identifier(), "SUB-827");
    assert_eq!(found[1].identifier(), "OPS-3");
    assert_eq!(found[2].identifier(), "PROJECTKEY-9");
}
