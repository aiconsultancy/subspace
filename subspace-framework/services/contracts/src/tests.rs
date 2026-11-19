use crate::issues::{
    Changed, CurrentScope, IssueScope, IssueState, IssueSummary, MAX_RESOLVE, Resolution,
    ResolutionState, Resolve, StateType,
};
use crate::{Contract, Event, MessageKey, MessageKind};

struct Odd;

impl Contract for Odd {
    const NAME: &'static str = "Odd_name";
    const VERSION: u32 = 0;
}

struct Ping;

impl Event for Ping {
    type Contract = Odd;
    const NAME: &'static str = "ping";
}

#[test]
fn keys_name_the_contract_version_and_message() {
    let changed = MessageKey::event::<Changed>();
    assert_eq!(changed.label(), "issues@1/changed");
    assert_eq!(changed.kind, MessageKind::Event);
    let resolve = MessageKey::request::<Resolve>();
    assert_eq!(
        (resolve.contract, resolve.version, resolve.message),
        ("issues", 1, "resolve")
    );
    assert_eq!(resolve.kind, MessageKind::Request);
    assert_ne!(resolve, MessageKey::request::<CurrentScope>());
    assert!(changed.is_valid() && resolve.is_valid());
}

#[test]
fn malformed_names_and_version_zero_are_invalid() {
    assert!(!MessageKey::event::<Ping>().is_valid());
    assert!(!super::valid_name(""));
    assert!(!super::valid_name("1st"));
    assert!(super::valid_name("current-scope"));
}

#[test]
fn resolve_keeps_the_first_distinct_identifiers_up_to_the_batch_limit() {
    let many = (0..80).map(|number| format!("TST-{}", number % 60 + 1));
    let request = Resolve::new(
        "subspace-api",
        ["TST-2", "TST-1", "TST-2"].map(String::from),
    );
    assert_eq!(request.identifiers(), ["TST-2", "TST-1"]);
    assert_eq!(request.profile_id(), "subspace-api");
    let capped = Resolve::new("subspace-api", many);
    assert_eq!(capped.identifiers().len(), MAX_RESOLVE);
    assert_eq!(capped.identifiers()[0], "TST-1");
}

#[test]
fn summaries_and_scopes_build_without_struct_literals() {
    let state = IssueState::new("In Progress", StateType::Started);
    let summary = IssueSummary::new(
        "TST-827",
        "Fix it",
        "https://app.example/issues/TST-827",
        state,
    )
    .with_markers(true, false)
    .with_assignee(Some("Jo Fixture"));
    assert_eq!(summary.state.kind, StateType::Started);
    assert_eq!(summary.assignee.as_deref(), Some("Jo Fixture"));
    assert!(summary.tracker && !summary.archived);
    let resolution = Resolution::new("TST-827", ResolutionState::Available(summary));
    assert!(matches!(resolution.state, ResolutionState::Available(_)));
    let scope = IssueScope::new("subspace-api", "Subspace API", "https://app.example")
        .with_project_keys(["TST", "OPS"]);
    assert_eq!(scope.project_keys, ["TST", "OPS"]);
    assert_eq!(scope.profile_id, "subspace-api");
    assert_eq!(scope.profile_name, "Subspace API");
    assert_eq!(scope.api_origin, "https://app.example");
}
