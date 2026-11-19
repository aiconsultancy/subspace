//! A capture-only stand-in for the board that answers the `issues` contract,
//! so evidence can show issue references and their tooltips without a live
//! API connection. `SUBSPACE_CAPTURE_ISSUES` turns it on in a debug capture build;
//! release builds never contain it. Every answer is fictional.

use gpui_kit::{App, AppContext as _, Entity, Global};
use subspace_contracts::issues::{
    Changed, CurrentScope, IssuePerson, IssueProgress, IssueScope, IssueState, IssueSummary,
    LatestActivity, PullRequestSummary, Resolution, ResolutionState, Resolve, StateType,
};
use subspace_dashboard::contracts::{self, BoardContracts, Declaration};

const ENV: &str = "SUBSPACE_CAPTURE_ISSUES";
/// The stand-in's board ID in the broker's routing; it has no view.
const BOARD: &str = "capture-issues";
const DECLARATIONS: BoardContracts = BoardContracts::new(&[
    Declaration::responds::<CurrentScope>(),
    Declaration::responds::<Resolve>(),
    Declaration::publishes::<Changed>(),
]);
const RECENT_MINUTES: i64 = 4;
const TRACKER_PROGRESS: (u32, u32) = (3, 7);
const TRACKER_PRS: (u32, u32, u32, u32) = (5, 1, 0, 4);
const API: &str = "https://api.example.test/api";

struct Fixture;

/// Keeps the stand-in alive: the broker holds it only weakly.
struct Kept(#[allow(dead_code)] Entity<Fixture>);
impl Global for Kept {}

fn enabled() -> bool {
    std::env::var_os(ENV).is_some()
}

/// The running boards' routing, with the stand-in while it is on.
pub(crate) fn with_issues(
    mut boards: Vec<(&'static str, BoardContracts)>,
) -> Vec<(&'static str, BoardContracts)> {
    if enabled() {
        boards.push((BOARD, DECLARATIONS));
    }
    boards
}

/// Attaches the answers and announces them, as a starting board does.
pub(crate) fn start(cx: &mut App) {
    if !enabled() {
        return;
    }
    let fixture = cx.new(|_| Fixture);
    let scoped = contracts::respond(BOARD, &fixture, |_, _: &CurrentScope, _| Some(scope()), cx);
    let resolves = contracts::respond(BOARD, &fixture, |_, ask: &Resolve, _| answers(ask), cx);
    if scoped.is_ok() && resolves.is_ok() {
        _ = contracts::publish(BOARD, Changed, cx);
    }
    cx.set_global(Kept(fixture));
}

fn scope() -> IssueScope {
    IssueScope::new("fixture", "Subspace API", API).with_project_keys(["SUB", "OPS"])
}

fn answers(ask: &Resolve) -> Vec<Resolution> {
    ask.identifiers()
        .iter()
        .map(|identifier| Resolution::new(identifier, answer(identifier)))
        .collect()
}

fn summary(identifier: &str, title: &str, state: IssueState) -> IssueSummary {
    IssueSummary::new(
        identifier,
        title,
        &format!("{API}/v1/issues/{identifier}"),
        state,
    )
}

/// The fixture's issues: a started one, a completed archived tracker, one
/// still loading and the rest unavailable.
fn answer(identifier: &str) -> ResolutionState {
    match identifier {
        "SUB-1612" => ResolutionState::Available(
            summary(
                identifier,
                "Show issue references in IRC",
                IssueState::new("In Progress", StateType::Started),
            )
            .with_summary(Some("Keep issue context beside the conversation so people can understand the work without leaving Channels."))
            .with_assignee(Some("protostar"))
            .with_assignee_kind(Some("agent"))
            .with_assignee_rank(Some("captain"))
            .with_owner(Some(IssuePerson::new("rosa", "Rosa Fixture", "human")))
            .with_priority(Some(2))
            .with_latest_activity(Some(LatestActivity::new(
                "comment.created",
                &(chrono::Utc::now() - chrono::Duration::minutes(RECENT_MINUTES)).to_rfc3339(),
            ))),
        ),
        "SUB-1588" => ResolutionState::Available(
            summary(
                identifier,
                "Reference tooltips",
                IssueState::new("Done", StateType::Completed),
            )
            .with_markers(true, true)
            .with_progress(Some(IssueProgress::new(
                TRACKER_PROGRESS.0,
                TRACKER_PROGRESS.1,
            )))
            .with_pull_requests(Some(
                PullRequestSummary::new(TRACKER_PRS.0, TRACKER_PRS.1, TRACKER_PRS.2, TRACKER_PRS.3)
                    .with_checks(Some("success")),
            ))
            .with_due_date(Some("2026-11-20")),
        ),
        "SUB-1700" => ResolutionState::Pending,
        _ => ResolutionState::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use subspace_contracts::issues::{ResolutionState, Resolve};

    use super::answers;

    #[test]
    fn the_fixture_answers_every_identifier_in_order() {
        let answered = answers(&Resolve::new("fixture", ["SUB-1612", "OPS-1", "SUB-1700"]));
        let states: Vec<&str> = answered
            .iter()
            .map(|resolution| match resolution.state {
                ResolutionState::Available(_) => "available",
                ResolutionState::Pending => "pending",
                _ => "unavailable",
            })
            .collect();
        assert_eq!(states, ["available", "unavailable", "pending"]);
    }
}
