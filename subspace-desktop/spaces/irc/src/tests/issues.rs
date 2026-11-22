//! Issue references end to end through the real broker: IRC's declared
//! roles, a fixture board that answers the `issues` contract in place of
//! the Issues board, and the fallback when no board answers.

mod hover;
mod interaction;
mod previews;
mod reserved;
mod summaries;
mod tiles;

use std::collections::HashMap;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, Entity, TestAppContext, VisualTestContext};
use subspace_framework::contracts::issues::{
    Changed, CurrentScope, IssueScope, IssueState, IssueSummary, Resolution, ResolutionState,
    Resolve, StateType,
};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, Role};

use super::{open_with_message, settings};
use crate::hub::{self, Hub};
use crate::{Irc, issues};

const TRACKER: &str = "fixture-issues";
const TRACKER_CONTRACTS: BoardContracts = BoardContracts::new(&[
    Declaration::responds::<CurrentScope>(),
    Declaration::responds::<Resolve>(),
    Declaration::publishes::<Changed>(),
]);

/// Answers like the Issues board: from memory, `Pending` for what it has
/// not read yet. Records every `resolve` it is asked.
pub(super) struct Tracker {
    scope: Option<IssueScope>,
    answers: HashMap<String, ResolutionState>,
    asked: Vec<Vec<String>>,
}

impl Tracker {
    fn resolve(&mut self, request: &Resolve) -> Vec<Resolution> {
        self.asked.push(request.identifiers().to_vec());
        request
            .identifiers()
            .iter()
            // `TST-30` is never answered until it has an answer: the state
            // every reference has before its first answer.
            .filter(|id| *id != "TST-30" || self.answers.contains_key(*id))
            .map(|id| {
                let state = self.answers.get(id).cloned();
                Resolution::new(id, state.unwrap_or(ResolutionState::Pending))
            })
            .collect()
    }
}

/// The tracker's answer for `identifier` becomes `state`, announced by
/// `changed`.
pub(super) fn answer(
    tracker: &Entity<Tracker>,
    identifier: &str,
    state: ResolutionState,
    cx: &mut VisualTestContext,
) {
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            tracker.answers.insert(identifier.into(), state);
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
}

fn scope() -> IssueScope {
    IssueScope::new("dev", "Subspace API", "https://api.example.test/api")
        .with_project_keys(["TST"])
}

fn summary(identifier: &str, title: &str) -> ResolutionState {
    ResolutionState::Available(IssueSummary::new(
        identifier,
        title,
        &format!("https://api.example.test/api/v1/issues/{identifier}"),
        IssueState::new("In Progress", StateType::Started),
    ).with_summary((identifier == "TST-12").then_some("Keep issue context beside the conversation so people can understand the work without leaving Channels.")))
}

/// Routes IRC and, when `tracker` is true, the fixture tracker, as the host
/// does for the running boards, then starts IRC's hub and attaches it.
pub(super) fn install(tracker: bool, cx: &mut App) -> (Entity<Hub>, Option<Entity<Tracker>>) {
    let mut boards = vec![(crate::ID, Irc::contracts())];
    if tracker {
        boards.push((TRACKER, TRACKER_CONTRACTS));
    }
    contracts::install(boards, cx);
    let tracker = tracker.then(|| attach_tracker(cx));
    let hub = hub::install_for_test(&settings(), cx);
    issues::attach(&hub, cx);
    (hub, tracker)
}

fn attach_tracker(cx: &mut App) -> Entity<Tracker> {
    let tracker = cx.new(|_| Tracker {
        scope: Some(scope()),
        answers: HashMap::from([
            ("TST-12".to_owned(), summary("TST-12", "Fixture title")),
            ("TST-18".to_owned(), summary("TST-18", "A longer issue title that wraps inside a narrow transcript beside the member and channel lists")),
        ]),
        asked: Vec::new(),
    });
    let scoped = contracts::respond(
        TRACKER,
        &tracker,
        |tracker: &mut Tracker, _: &CurrentScope, _| tracker.scope.clone(),
        cx,
    );
    let resolves = contracts::respond(
        TRACKER,
        &tracker,
        |tracker: &mut Tracker, request: &Resolve, _| tracker.resolve(request),
        cx,
    );
    assert!(scoped.is_ok() && resolves.is_ok());
    tracker
}

fn open_room<'a>(
    cx: &'a mut TestAppContext,
    tracker: bool,
    text: &str,
) -> (
    Entity<Hub>,
    Option<Entity<Tracker>>,
    &'a mut VisualTestContext,
) {
    let (_, hub, tracker, cx) = open_board(cx, tracker, text);
    (hub, tracker, cx)
}

/// The board showing `#subspace` with bob's message `text` last, routed
/// with the fixture tracker when `tracker` is true.
pub(super) fn open_board<'a>(
    cx: &'a mut TestAppContext,
    tracker: bool,
    text: &str,
) -> (
    Entity<Irc>,
    Entity<Hub>,
    Option<Entity<Tracker>>,
    &'a mut VisualTestContext,
) {
    open_with_message(cx, |cx| install(tracker, cx), text)
}

#[test]
fn irc_declares_that_it_follows_and_asks_the_issues_contract() {
    let declared: Vec<(Role, String)> = Irc::contracts()
        .declarations()
        .iter()
        .map(|declaration| (declaration.role(), declaration.key().label()))
        .collect();
    assert_eq!(
        declared,
        [
            (Role::Subscribes, "issues@1/changed".to_owned()),
            (Role::Requests, "issues@1/current-scope".to_owned()),
            (Role::Requests, "issues@1/resolve".to_owned()),
            (Role::Subscribes, "github@1/changed".to_owned()),
            (Role::Requests, "github@1/resolve-pull-requests".to_owned()),
            (Role::Requests, "github@1/open-pull-request".to_owned()),
        ]
    );
}

#[gpui_kit::test]
fn shown_references_resolve_in_one_request_through_the_broker(cx: &mut TestAppContext) {
    let (hub, tracker, cx) = open_room(cx, true, "TST-12 blocks TST-13, see TST-12");
    let tracker = tracker.expect("a tracker");
    cx.update(|_, cx| {
        assert_eq!(
            tracker.read(cx).asked,
            [vec!["TST-12".to_owned(), "TST-13".to_owned()]],
            "one request, duplicates removed"
        );
        let issues = &hub.read(cx).issues;
        assert!(matches!(
            issues.state("TST-12"),
            Some(ResolutionState::Available(_))
        ));
        assert!(matches!(
            issues.state("TST-13"),
            Some(ResolutionState::Pending)
        ));
        assert_eq!(issues.profile_name(), Some("Subspace API"));
    });
}

#[gpui_kit::test]
fn changed_asks_again_for_what_was_pending(cx: &mut TestAppContext) {
    let (hub, tracker, cx) = open_room(cx, true, "TST-13 landed");
    let tracker = tracker.expect("a tracker");
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            tracker
                .answers
                .insert("TST-13".into(), ResolutionState::Unavailable);
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(tracker.read(cx).asked.len(), 2, "asked again after changed");
        assert!(hub.read(cx).issues.is_unavailable("TST-13"));
    });
}

#[gpui_kit::test]
fn with_no_answering_board_references_stay_plain_text(cx: &mut TestAppContext) {
    let (hub, _, cx) = open_room(
        cx,
        false,
        "TST-12 and https://api.example.test/api/v1/issues/TST-9",
    );
    cx.update(|window, cx| {
        let issues = &hub.read(cx).issues;
        assert!(issues.profile_name().is_none());
        assert!(issues.state("TST-12").is_none(), "nothing is asked");
        assert_eq!(
            window.find("irc-line:2:row").label(),
            Some("bob: TST-12 and https://api.example.test/api/v1/issues/TST-9")
        );
    });
}

#[gpui_kit::test]
fn the_tracker_stopping_stops_recognition(cx: &mut TestAppContext) {
    let (hub, _, cx) = open_room(cx, true, "TST-12");
    cx.update(|_, cx| {
        contracts::install(vec![(crate::ID, Irc::contracts())], cx);
        hub.update(cx, |hub, cx| {
            hub.issues.start_round();
            issues::want(hub, vec!["TST-12".into()], cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            hub.read(cx).issues.profile_name().is_none(),
            "an unavailable resolve drops the scope"
        );
    });
}

/// The ids of the state dots drawn before inline issue references.
fn state_dots(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        gpui_kit::base::test_support::snapshots(window)
            .into_iter()
            .filter_map(|node| {
                let id = node.path().last()?.to_string();
                id.starts_with("irc-reference-state:").then_some(id)
            })
            .collect()
    })
}

#[gpui_kit::test]
fn an_answered_issue_reference_leads_with_its_state_dot_and_a_loading_one_keeps_its_look(
    cx: &mut TestAppContext,
) {
    let (_, tracker, cx) = open_room(cx, true, "TST-12 blocks TST-13");
    let tracker = tracker.expect("a tracker");
    assert_eq!(
        state_dots(cx).len(),
        1,
        "TST-12 is answered; TST-13 is still pending and has no dot"
    );
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            tracker
                .answers
                .insert("TST-13".into(), ResolutionState::Unavailable);
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    assert_eq!(state_dots(cx).len(), 1, "an unavailable reference has none");
}

#[gpui_kit::test]
fn a_pending_reference_gains_its_state_dot_when_its_answer_arrives(cx: &mut TestAppContext) {
    let (_, tracker, cx) = open_room(cx, true, "TST-12 blocks TST-13");
    let tracker = tracker.expect("a tracker");
    assert_eq!(state_dots(cx).len(), 1, "TST-13 is still pending");
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            tracker
                .answers
                .insert("TST-13".into(), summary("TST-13", "Pan the cold cache"));
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    assert_eq!(
        state_dots(cx).len(),
        2,
        "pending to available draws its dot"
    );
}

#[gpui_kit::test]
fn with_no_answering_board_no_reference_has_a_state_dot(cx: &mut TestAppContext) {
    let (_, _, cx) = open_room(cx, false, "TST-12 blocks TST-13");
    assert!(state_dots(cx).is_empty());
}
