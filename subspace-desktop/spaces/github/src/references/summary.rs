//! A loaded pull request as the `github@1` contract summarises it: the
//! client's model mapped onto the contract's own types.

use subspace_framework::client_github::model::{
    CheckRunState, CheckState, GhCheck, GhItem, PrState, ReviewDecision,
};
use subspace_framework::contracts::github::recognition::PullRequestLink;
use subspace_framework::contracts::github::{
    CheckCounts, CheckStatus, PullRequestDetails, PullRequestStatus, ReviewStatus,
};

/// The short summary of a loaded pull request, with its check counts when
/// its check runs were read with it.
pub(super) fn details(item: &GhItem, runs: &[GhCheck]) -> PullRequestDetails {
    let url =
        PullRequestLink::from_key(&item.key).map_or_else(|| item.url.clone(), |link| link.url());
    let updated = Some(item.updated_at.as_str()).filter(|at| !at.is_empty());
    PullRequestDetails::new(&item.key, &item.title, &url, status(item.state))
        .with_author(item.author.as_ref().map(|author| author.login.as_str()))
        .with_review(item.review_decision.map(review))
        .with_checks(item.checks.map(checks))
        .with_check_counts(check_counts(runs))
        .with_updated_at(updated)
        .with_branches(item.head_ref.as_deref(), item.base_ref.as_deref())
        .with_changes(item.additions, item.deletions)
}

/// How many of `runs` passed, out of how many: a check passes unless it
/// failed, was cancelled or is still running, as the detail's check summary
/// counts them. `None` when no runs were read.
fn check_counts(runs: &[GhCheck]) -> Option<CheckCounts> {
    let passed = runs
        .iter()
        .filter(|run| {
            matches!(
                run.state,
                CheckRunState::Success | CheckRunState::Neutral | CheckRunState::Skipped
            )
        })
        .count();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    (!runs.is_empty()).then(|| CheckCounts::new(count(passed), count(runs.len())))
}

// qual:allow(dry, boilerplate) reason: "Maps the GitHub client's model onto the independent public contract enum; the contract crate must not depend on the client, so neither can derive the other."
fn status(state: PrState) -> PullRequestStatus {
    match state {
        PrState::Open => PullRequestStatus::Open,
        PrState::Draft => PullRequestStatus::Draft,
        PrState::Merged => PullRequestStatus::Merged,
        PrState::Closed => PullRequestStatus::Closed,
    }
}

// qual:allow(dry, repeated_matches) reason: "Maps review decisions to contract values. Copilot Agents maps its own unrelated enums to UI icons and tones."
fn review(decision: ReviewDecision) -> ReviewStatus {
    match decision {
        ReviewDecision::Approved => ReviewStatus::Approved,
        ReviewDecision::ChangesRequested => ReviewStatus::ChangesRequested,
        ReviewDecision::ReviewRequired => ReviewStatus::ReviewRequired,
    }
}

// qual:allow(dry, repeated_matches) reason: "Maps check results to contract values. This enum is independent of review decisions and Copilot Agents UI state."
fn checks(state: CheckState) -> CheckStatus {
    match state {
        CheckState::Success => CheckStatus::Success,
        CheckState::Failure => CheckStatus::Failure,
        CheckState::Pending => CheckStatus::Pending,
    }
}
