//! The `issues` contract for a web app page, as IRC uses it: the page asks
//! for the scope and for summaries of the identifiers it shows, opens an
//! issue in the answering Space, and hears `subspace:issues-changed` after
//! each `changed`. The web Space never names the Space that answers, keeps
//! no issue content and hands nothing to search or notifications.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use gpui_kit::App;
use serde_json::{Value, json};
use subspace_framework::contracts::issues::recognition::ReferenceRecognizer;
use subspace_framework::contracts::issues::{
    Changed, CurrentScope, IssueScope, IssueSummary, MAX_RESOLVE, Resolution, ResolutionState,
    Resolve, StateType,
};
use subspace_framework::dashboard::contracts::{BoardContracts, Declaration};

use super::protocol::Operation;

/// The web Space's part in the `issues` contract, declared by every
/// configured web Space. The `issues` permission decides what a page reaches.
pub(crate) const CONTRACTS: BoardContracts = BoardContracts::new(&[
    Declaration::subscribes::<Changed>(),
    Declaration::requests::<CurrentScope>(),
    Declaration::requests::<Resolve>(),
]);

/// The longest identifier a page may send, including its project key.
const MAX_IDENTIFIER_CHARS: usize = 32;
const TOO_MANY: &str = "Resolve at most 50 distinct issue identifiers.";
const BAD_IDENTIFIER: &str =
    "Use issue identifiers of at most 32 characters, without control characters.";
const NOT_RECOGNISED: &str =
    "No Space that answers issue requests recognises this issue identifier.";

/// What the page asked of the `issues` contract, after validation.
pub(crate) enum IssueCall {
    Scope,
    Resolve(Vec<String>),
    Open(String),
}

impl IssueCall {
    /// The validated call for an `issues.*` operation.
    pub(crate) fn parse(operation: &Operation) -> Result<Self, &'static str> {
        match operation {
            Operation::IssueResolve(identifiers) => Ok(Self::Resolve(distinct(identifiers)?)),
            Operation::IssueOpen(identifier) if valid_identifier(identifier) => {
                Ok(Self::Open(identifier.clone()))
            }
            Operation::IssueOpen(_) => Err(BAD_IDENTIFIER),
            _ => Ok(Self::Scope),
        }
    }
}

/// The identifiers in order without repeats, or why they cannot be asked.
pub(crate) fn distinct(identifiers: &[String]) -> Result<Vec<String>, &'static str> {
    let mut seen = HashSet::new();
    let mut distinct = Vec::new();
    for identifier in identifiers {
        if !valid_identifier(identifier) {
            return Err(BAD_IDENTIFIER);
        }
        if seen.insert(identifier.as_str()) {
            distinct.push(identifier.clone());
        }
    }
    if distinct.len() > MAX_RESOLVE {
        return Err(TOO_MANY);
    }
    Ok(distinct)
}

fn valid_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier.chars().count() <= MAX_IDENTIFIER_CHARS
        && !identifier.chars().any(char::is_control)
}

/// A configured board ID as the broker needs it. Each distinct ID is leaked
/// once, as the host does when it registers the board.
pub(crate) fn board_id(id: &str) -> &'static str {
    static IDS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut ids = IDS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(id) = ids.get(id) {
        return id;
    }
    let id: &'static str = Box::leak(id.to_owned().into_boxed_str());
    ids.insert(id);
    id
}

/// The scope now; no answering Space, or a refusal, is no scope.
pub(crate) fn scope(board: &'static str, cx: &mut App) -> Option<IssueScope> {
    subspace_framework::dashboard::contracts::request(board, &CurrentScope, cx)
        .ok()
        .flatten()
}

/// One resolution per identifier, in order. Without a scope nothing is
/// asked: each is `signed-out` when a Space answers but has no scope, and
/// `unavailable` when none answers.
pub(crate) fn resolve(board: &'static str, identifiers: Vec<String>, cx: &mut App) -> Value {
    let request = subspace_framework::dashboard::contracts::request(board, &CurrentScope, cx);
    let answers = match request {
        Ok(Some(scope)) => {
            let ask = Resolve::new(&scope.profile_id, identifiers.iter().cloned());
            subspace_framework::dashboard::contracts::request(board, &ask, cx)
                .unwrap_or_else(|_| uniform(&identifiers, &ResolutionState::Unavailable))
        }
        Ok(None) => uniform(&identifiers, &ResolutionState::SignedOut),
        Err(_) => uniform(&identifiers, &ResolutionState::Unavailable),
    };
    Value::Array(answers.iter().map(resolution_json).collect())
}

/// The scope-bound address the answering Space opens for `identifier`.
pub(crate) fn open(
    board: &'static str,
    identifier: &str,
    cx: &mut App,
) -> Result<String, &'static str> {
    let scope = scope(board, cx).ok_or(NOT_RECOGNISED)?;
    let ask = Resolve::new(&scope.profile_id, [identifier]);
    let answers =
        subspace_framework::dashboard::contracts::request(board, &ask, cx).unwrap_or_default();
    navigation(&scope, identifier, &answers).ok_or(NOT_RECOGNISED)
}

/// IRC's rule: a recognised identifier opens its summary's page when one
/// is known, otherwise its issue address in the scope's API.
pub(crate) fn navigation(
    scope: &IssueScope,
    identifier: &str,
    answers: &[Resolution],
) -> Option<String> {
    let keys = scope.project_keys.iter().map(String::as_str);
    if !ReferenceRecognizer::new(keys, []).recognises(identifier) {
        return None;
    }
    let summary = answers.iter().find_map(|answer| match &answer.state {
        ResolutionState::Available(summary) if answer.identifier == identifier => Some(summary),
        _ => None,
    });
    let url = summary.map_or_else(
        || {
            format!(
                "{}/v1/issues/{identifier}",
                scope.api_origin.trim_end_matches('/')
            )
        },
        |summary| summary.url.clone(),
    );
    scope.navigation_url(&url)
}

fn uniform(identifiers: &[String], state: &ResolutionState) -> Vec<Resolution> {
    identifiers
        .iter()
        .map(|identifier| Resolution::new(identifier, state.clone()))
        .collect()
}

/// Preserve the web scope's field names while projecting the API scope.
pub(crate) fn scope_json(scope: Option<&IssueScope>) -> Value {
    scope.map_or(Value::Null, |scope| {
        json!({
            "profileName": scope.profile_name,
            "appOrigin": scope.api_origin,
            "teamKeys": scope.project_keys,
            "linearWorkspace": Value::Null,
        })
    })
}

pub(crate) fn resolution_json(resolution: &Resolution) -> Value {
    let state = match &resolution.state {
        ResolutionState::Available(summary) => {
            return json!({
                "identifier": resolution.identifier,
                "state": "available",
                "summary": summary_json(summary),
            });
        }
        ResolutionState::Unavailable => "unavailable",
        ResolutionState::Pending => "pending",
        ResolutionState::SignedOut => "signed-out",
        // Failed, and any state this Subspace does not know yet.
        _ => "failed",
    };
    json!({"identifier": resolution.identifier, "state": state})
}

fn summary_json(summary: &IssueSummary) -> Value {
    json!({
        "identifier": summary.identifier,
        "title": summary.title,
        "url": summary.url,
        "state": {"name": summary.state.name, "kind": state_kind(summary.state.kind)},
        "assignee": summary.assignee,
        "archived": summary.archived,
    })
}

pub(crate) fn state_kind(kind: StateType) -> &'static str {
    match kind {
        StateType::Triage => "triage",
        StateType::Backlog => "backlog",
        StateType::Unstarted => "unstarted",
        StateType::Started => "started",
        StateType::Completed => "completed",
        StateType::Canceled => "canceled",
        StateType::Duplicate => "duplicate",
        // A category this Subspace does not know yet.
        _ => "unstarted",
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{App, AppContext as _, Entity, TestAppContext};
    use serde_json::json;
    use subspace_framework::contracts::issues::{
        CurrentScope, IssueScope, IssueState, IssueSummary, Resolution, ResolutionState, Resolve,
        StateType,
    };
    use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration};

    use super::{
        BAD_IDENTIFIER, CONTRACTS, TOO_MANY, board_id, distinct, navigation, open, resolution_json,
        resolve, scope_json, state_kind,
    };

    fn scope() -> IssueScope {
        IssueScope::new("profile-1", "Work", "https://issues.example/api")
            .with_project_keys(["ABC", "ENG"])
    }

    fn summary() -> IssueSummary {
        IssueSummary::new(
            "ABC-7",
            "Fix the build",
            "https://issues.example/api/v1/issues/ABC-7",
            IssueState::new("In Progress", StateType::Started),
        )
        .with_markers(false, true)
        .with_assignee(Some("Ada"))
    }

    #[test]
    fn the_scope_carries_no_profile_or_organisation_ids() {
        assert_eq!(scope_json(None), json!(null));
        let value = scope_json(Some(&scope()));
        assert_eq!(
            value,
            json!({"profileName": "Work", "appOrigin": "https://issues.example/api",
                "teamKeys": ["ABC", "ENG"], "linearWorkspace": null})
        );
    }

    #[test]
    fn resolutions_map_to_lowercase_states_and_summaries() {
        let available = Resolution::new("ABC-7", ResolutionState::Available(summary()));
        assert_eq!(
            resolution_json(&available),
            json!({"identifier": "ABC-7", "state": "available", "summary": {
                "identifier": "ABC-7", "title": "Fix the build",
                "url": "https://issues.example/api/v1/issues/ABC-7",
                "state": {"name": "In Progress", "kind": "started"},
                "assignee": "Ada", "archived": true}})
        );
        for (state, expected) in [
            (ResolutionState::Unavailable, "unavailable"),
            (ResolutionState::Pending, "pending"),
            (ResolutionState::SignedOut, "signed-out"),
            (ResolutionState::Failed, "failed"),
        ] {
            let value = resolution_json(&Resolution::new("ENG-1", state));
            assert_eq!(value, json!({"identifier": "ENG-1", "state": expected}));
        }
    }

    #[test]
    fn every_state_type_has_its_documented_kind() {
        for (kind, expected) in [
            (StateType::Triage, "triage"),
            (StateType::Backlog, "backlog"),
            (StateType::Unstarted, "unstarted"),
            (StateType::Started, "started"),
            (StateType::Completed, "completed"),
            (StateType::Canceled, "canceled"),
            (StateType::Duplicate, "duplicate"),
        ] {
            assert_eq!(state_kind(kind), expected);
        }
    }

    #[test]
    fn identifiers_are_distinct_bounded_and_at_most_fifty() {
        let ids = |items: &[&str]| items.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            distinct(&ids(&["ABC-1", "ABC-2", "ABC-1"])).unwrap(),
            ["ABC-1", "ABC-2"]
        );
        let many: Vec<String> = (1..=51).map(|n| format!("ABC-{n}")).collect();
        assert_eq!(distinct(&many), Err(TOO_MANY));
        let repeated: Vec<String> = many[..50].iter().chain(&many[..50]).cloned().collect();
        assert_eq!(distinct(&repeated).unwrap().len(), 50);
        assert_eq!(distinct(&ids(&["ABC-1\n"])), Err(BAD_IDENTIFIER));
        assert_eq!(distinct(&ids(&[""])), Err(BAD_IDENTIFIER));
        assert_eq!(distinct(&["N".repeat(33)]), Err(BAD_IDENTIFIER));
    }

    #[test]
    fn opening_binds_a_recognised_identifier_to_the_scope() {
        let scope = scope();
        let answers = [Resolution::new(
            "ABC-7",
            ResolutionState::Available(summary()),
        )];
        assert_eq!(
            navigation(&scope, "ABC-7", &answers).as_deref(),
            Some("https://issues.example/api/v1/issues/ABC-7?profile=profile-1")
        );
        let pending = [Resolution::new("ENG-3", ResolutionState::Pending)];
        let url = navigation(&scope, "ENG-3", &pending).unwrap();
        assert_eq!(
            url,
            "https://issues.example/api/v1/issues/ENG-3?profile=profile-1"
        );
        assert_eq!(scope.navigation_identifier(&url).as_deref(), Some("ENG-3"));
        assert_eq!(navigation(&scope, "OPS-3", &[]), None);
        assert_eq!(navigation(&scope, "abc-3", &[]), None);
    }

    #[test]
    fn opening_rejects_a_summary_from_another_api() {
        let mut summary = summary();
        summary.url = "https://other.example/api/v1/issues/ABC-7".into();
        let answers = [Resolution::new(
            "ABC-7",
            ResolutionState::Available(summary),
        )];
        assert_eq!(navigation(&scope(), "ABC-7", &answers), None);
    }

    const TRACKER: &str = "tracker";
    const TRACKER_CONTRACTS: BoardContracts = BoardContracts::new(&[
        Declaration::responds::<CurrentScope>(),
        Declaration::responds::<Resolve>(),
    ]);

    /// Routes a web board and, with `scope`, a tracker that answers with it
    /// and knows `ABC-7`.
    fn route(
        scope: Option<Option<IssueScope>>,
        cx: &mut App,
    ) -> Option<Entity<Option<IssueScope>>> {
        let mut boards = vec![("fleet", CONTRACTS)];
        if scope.is_some() {
            boards.push((TRACKER, TRACKER_CONTRACTS));
        }
        contracts::install(boards, cx);
        let tracker = cx.new(|_| scope.flatten());
        let scoped = contracts::respond(
            TRACKER,
            &tracker,
            |scope: &mut Option<IssueScope>, _: &CurrentScope, _| scope.clone(),
            cx,
        );
        let resolves = contracts::respond(
            TRACKER,
            &tracker,
            |_: &mut Option<IssueScope>, ask: &Resolve, _| {
                ask.identifiers()
                    .iter()
                    .map(|id| match id.as_str() {
                        "ABC-7" => Resolution::new(id, ResolutionState::Available(summary())),
                        _ => Resolution::new(id, ResolutionState::Pending),
                    })
                    .collect()
            },
            cx,
        );
        (scoped.is_ok() && resolves.is_ok()).then_some(tracker)
    }

    #[gpui_kit::test]
    fn the_page_reaches_the_contract_through_the_broker(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let tracker = route(Some(Some(scope())), cx);
            assert!(tracker.is_some());
            assert_eq!(
                super::scope(board_id("fleet"), cx).map(|scope| scope.profile_name),
                Some("Work".to_owned())
            );
            let resolved = resolve("fleet", vec!["ABC-7".into(), "ENG-9".into()], cx);
            assert_eq!(resolved[0]["summary"]["title"], "Fix the build");
            assert_eq!(
                resolved[1],
                json!({"identifier": "ENG-9", "state": "pending"})
            );
            assert_eq!(
                open("fleet", "ABC-7", cx).as_deref(),
                Ok("https://issues.example/api/v1/issues/ABC-7?profile=profile-1")
            );
            assert!(open("fleet", "OPS-1", cx).is_err());
        });
    }

    #[gpui_kit::test]
    fn without_a_scope_nothing_is_asked(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let tracker = route(Some(None), cx);
            assert!(tracker.is_some());
            assert_eq!(scope_json(super::scope("fleet", cx).as_ref()), json!(null));
            let signed_out = resolve("fleet", vec!["ABC-7".into()], cx);
            assert_eq!(
                signed_out,
                json!([{"identifier": "ABC-7", "state": "signed-out"}])
            );
            assert!(open("fleet", "ABC-7", cx).is_err());
            assert!(route(None, cx).is_none(), "no tracker is routed");
            let unanswered = resolve("fleet", vec!["ABC-7".into()], cx);
            assert_eq!(
                unanswered,
                json!([{"identifier": "ABC-7", "state": "unavailable"}])
            );
        });
    }

    #[test]
    fn board_ids_are_interned_once() {
        let first = board_id("fleet");
        let second = board_id(&String::from("fleet"));
        assert!(std::ptr::eq(first, second));
        assert_eq!(first, "fleet");
    }
}
