//! The retained hub answers the `issues` contract, even while its view is
//! hidden. The broker alone carries answers and change announcements.

mod summary;

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, Entity, Task};
use subspace_api_client::hub::{Hub, IrcSnapshot, ReferenceCard};
use subspace_framework::contracts::issues::{
    Changed, CurrentScope, IssueScope, IssueState, IssueSummary, Refresh, Resolution,
    ResolutionState, Resolve, StateType,
};
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration};

pub(crate) const DECLARATIONS: BoardContracts = BoardContracts::new(&[
    Declaration::requests::<subspace_framework::contracts::github::OpenPullRequest>(),
    Declaration::responds::<CurrentScope>(),
    Declaration::responds::<Resolve>(),
    Declaration::responds::<Refresh>(),
    Declaration::publishes::<Changed>(),
]);

/// The board's existing 100 ms hub poll lives here so it also runs hidden.
const POLL: Duration = Duration::from_millis(100);

pub(crate) struct Responder {
    pub(crate) hub: Hub,
    seen: u64,
    _poll: Task<()>,
}

pub(crate) fn attach(hub: Hub, cx: &mut App) -> Entity<Responder> {
    let responder = cx.new(|cx| Responder::new(hub, cx));
    // The broker reports registration and publication failures, including
    // the board and message name, without logging any issue content.
    let _ = contracts::respond(
        crate::ID,
        &responder,
        |this, _: &CurrentScope, _| scope(&this.hub.irc_snapshot()),
        cx,
    );
    let _ = contracts::respond(
        crate::ID,
        &responder,
        |this, request: &Resolve, _| this.resolve(request),
        cx,
    );
    let _ = contracts::respond(
        crate::ID,
        &responder,
        |this, request: &Refresh, _| this.refresh(request),
        cx,
    );
    announce(cx);
    responder
}

pub(crate) fn announce(cx: &mut App) {
    _ = contracts::publish(crate::ID, Changed, cx);
}

impl Responder {
    fn new(hub: Hub, cx: &mut Context<Self>) -> Self {
        Self {
            seen: hub.reference_version(),
            hub,
            _poll: cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                        return;
                    }
                }
            }),
        }
    }

    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) {
        // The board restarts the hub for a new account only while it shows,
        // but Channels resolves references through this responder at any
        // time, such as after the silent startup sign-in.
        if self.hub.account_changed() {
            cx.defer(|cx| {
                if crate::link::current(cx).is_some_and(|link| link.hub.account_changed()) {
                    crate::link::stop(cx);
                    crate::link::start(cx);
                }
            });
            return;
        }
        self.hub.reconnect_when_account_ready(cx);
        let version = self.hub.reference_version();
        if self.seen != version {
            self.seen = version;
            announce(cx);
        }
        // The visible view observes this same timer for list/detail updates.
        cx.notify();
    }

    fn resolve(&self, request: &Resolve) -> Vec<Resolution> {
        let snapshot = self.hub.irc_snapshot();
        request
            .identifiers()
            .iter()
            .map(|identifier| {
                let state = answer(&snapshot, request.profile_id(), identifier);
                if matches!(state, ResolutionState::Pending) {
                    self.hub.resolve_reference(identifier, request.profile_id());
                }
                Resolution::new(identifier, state)
            })
            .collect()
    }

    /// Reads each identifier again after another Space changed it; the
    /// poll announces `changed` when the reads land.
    fn refresh(&self, request: &Refresh) {
        for identifier in request.identifiers() {
            self.hub.refresh_reference(identifier, request.profile_id());
        }
    }
}

fn scope(snapshot: &IrcSnapshot) -> Option<IssueScope> {
    if snapshot.project_keys().is_empty() {
        return None;
    }
    Some(
        IssueScope::new(
            snapshot.profile_id(),
            snapshot.profile_name(),
            snapshot.api_origin(),
        )
        .with_project_keys(snapshot.project_keys().iter().cloned()),
    )
}

fn answer(snapshot: &IrcSnapshot, profile: &str, identifier: &str) -> ResolutionState {
    if profile != snapshot.profile_id() {
        return ResolutionState::Unavailable;
    }
    if let Some(entry) = snapshot
        .cards()
        .iter()
        .find(|entry| entry.identifier() == identifier)
    {
        return entry.card().map_or(ResolutionState::Unavailable, |card| {
            ResolutionState::Available(summary::from_card(snapshot, card))
        });
    }
    if snapshot.reference_error(identifier).is_some() {
        return ResolutionState::Failed;
    }
    let recognizer = subspace_framework::contracts::issues::recognition::ReferenceRecognizer::new(
        snapshot.project_keys().iter().map(String::as_str),
        [],
    );
    if !recognizer.recognises(identifier) {
        return ResolutionState::Unavailable;
    }
    ResolutionState::Pending
}

fn base_summary(snapshot: &IrcSnapshot, card: &ReferenceCard) -> IssueSummary {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("profile", snapshot.profile_id())
        .finish();
    IssueSummary::new(
        card.identifier(),
        card.title(),
        &format!(
            "{}/v1/issues/{}?{query}",
            snapshot.api_origin().trim_end_matches('/'),
            card.identifier()
        ),
        IssueState::new(&card.state().name, state_type(&card.state().kind)),
    )
    .with_markers(card.kind() == "tracker", card.archived())
    .with_assignee(card.assignee())
    .with_summary(card.summary())
}

fn state_type(kind: &str) -> StateType {
    match kind {
        "triage" => StateType::Triage,
        "backlog" => StateType::Backlog,
        "started" => StateType::Started,
        "completed" => StateType::Completed,
        "canceled" => StateType::Canceled,
        "duplicate" => StateType::Duplicate,
        _ => StateType::Unstarted,
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use subspace_framework::contracts::issues::ResolutionState;
    use subspace_framework::dashboard::Dashboard as _;

    use crate::tests::offline::{board_with_hub, offline, settle_board, wait_for};

    #[gpui_kit::test]
    fn channels_navigation_round_trips_a_responder_summary_into_issue_detail(
        cx: &mut TestAppContext,
    ) {
        let fixture = offline();
        let (board, cx) = board_with_hub(&fixture, cx);
        settle_board(&board, cx, |board| board.issue("issue:1").is_some());
        let hub = cx.update(|_, cx| board.read(cx).hub.clone().unwrap());
        hub.resolve_reference("SUB-1", "subspace-api");
        wait_for(|| !hub.irc_snapshot().cards().is_empty());
        let snapshot = hub.irc_snapshot();
        let ResolutionState::Available(summary) = super::answer(&snapshot, "subspace-api", "SUB-1")
        else {
            panic!("the responder did not supply the fixture summary");
        };
        let expected = format!(
            "{}/v1/issues/SUB-1?profile=subspace-api",
            snapshot.api_origin()
        );
        assert_eq!(summary.url, expected);
        let scope = super::scope(&snapshot).unwrap();
        // Channels binds both resolved summaries and pending references
        // through this contract before emitting board navigation.
        for source in [
            summary.url,
            format!("{}/v1/issues/SUB-1", snapshot.api_origin()),
        ] {
            let url = scope.navigation_url(&source).unwrap();
            assert_eq!(url, expected);
            assert!(board.update(cx, |board, cx| board.open_link(&url, cx)));
            settle_board(&board, cx, |board| {
                board
                    .issue_view
                    .snapshot
                    .as_ref()
                    .is_some_and(|issue| issue.identifier() == "SUB-1" && issue.detail().is_some())
            });
        }
        hub.stop();
    }
}
