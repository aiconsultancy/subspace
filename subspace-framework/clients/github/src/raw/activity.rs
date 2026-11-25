use crate::model::{
    CheckRunState, CheckState, EventKind, GhCheck, GhEvent, GhItemDetail, GhReviewer, GhUser,
    ItemKind, ReviewDecision, ReviewerState,
};

use super::projection::{last_rollup, to_item, to_user};
use super::types::{Comment, Nodes, RawCommon, RawContext, RawDetail, RawRequested, Review};

pub(super) fn review_decision(value: Option<&str>) -> Option<ReviewDecision> {
    match value? {
        "APPROVED" => Some(ReviewDecision::Approved),
        "CHANGES_REQUESTED" => Some(ReviewDecision::ChangesRequested),
        "REVIEW_REQUIRED" => Some(ReviewDecision::ReviewRequired),
        _ => None,
    }
}

pub(super) fn rollup_state(state: Option<&str>) -> Option<CheckState> {
    match state? {
        "SUCCESS" => Some(CheckState::Success),
        "FAILURE" | "ERROR" => Some(CheckState::Failure),
        "PENDING" | "EXPECTED" => Some(CheckState::Pending),
        _ => None,
    }
}

fn reviewer_state(state: &str) -> Option<ReviewerState> {
    match state {
        "APPROVED" => Some(ReviewerState::Approved),
        "CHANGES_REQUESTED" => Some(ReviewerState::ChangesRequested),
        "COMMENTED" => Some(ReviewerState::Commented),
        "DISMISSED" => Some(ReviewerState::Dismissed),
        "PENDING" => Some(ReviewerState::Pending),
        _ => None,
    }
}

/// A new request to review does not dismiss an already submitted approval.
pub fn viewer_approved(node: &RawDetail, login: &str) -> bool {
    node.latest_opinionated_reviews
        .iter()
        .flat_map(Nodes::present)
        .chain(node.viewer_latest_review.as_ref())
        .find(|review| {
            review
                .author
                .as_ref()
                .is_some_and(|actor| actor.login.eq_ignore_ascii_case(login))
        })
        .is_some_and(|review| review.state == "APPROVED")
}

pub fn map_reviewers(node: &RawDetail) -> Vec<GhReviewer> {
    let mut reviewers = Vec::new();
    let mut put = |user: GhUser, state: Option<ReviewerState>| {
        let Some(state) = state else { return };
        let key = user.login.to_lowercase();
        match reviewers
            .iter_mut()
            .find(|known: &&mut GhReviewer| known.user.login.to_lowercase() == key)
        {
            Some(known) => *known = GhReviewer { user, state },
            None => reviewers.push(GhReviewer { user, state }),
        }
    };
    let reviews = [&node.latest_reviews, &node.latest_opinionated_reviews];
    for review in reviews.into_iter().flatten().flat_map(Nodes::present) {
        if let Some(author) = &review.author {
            put(to_user(author), reviewer_state(&review.state));
        }
    }
    for request in node.review_requests.iter().flat_map(Nodes::present) {
        let user = match &request.requested_reviewer {
            Some(
                RawRequested::User(actor)
                | RawRequested::Bot(actor)
                | RawRequested::Mannequin(actor),
            ) => to_user(actor),
            Some(RawRequested::Team {
                combined_slug,
                name,
            }) => GhUser {
                login: combined_slug.clone(),
                name: Some(name.clone()),
            },
            Some(RawRequested::Other) | None => continue,
        };
        put(user, Some(ReviewerState::Pending));
    }
    reviewers
}

pub fn check_state(context: &RawContext) -> CheckRunState {
    match context {
        RawContext::StatusContext { state, .. } => match rollup_state(Some(state)) {
            Some(CheckState::Success) => CheckRunState::Success,
            Some(CheckState::Failure) => CheckRunState::Failure,
            Some(CheckState::Pending) | None => CheckRunState::Pending,
        },
        RawContext::CheckRun { status, .. } if status != "COMPLETED" => CheckRunState::Pending,
        RawContext::CheckRun { conclusion, .. } => match conclusion.as_deref() {
            Some("SUCCESS") => CheckRunState::Success,
            Some("NEUTRAL" | "STALE") => CheckRunState::Neutral,
            Some("SKIPPED") => CheckRunState::Skipped,
            Some("CANCELLED") => CheckRunState::Cancelled,
            _ => CheckRunState::Failure,
        },
        RawContext::Other => CheckRunState::Pending,
    }
}

pub fn map_checks(node: &RawCommon) -> Vec<GhCheck> {
    let Some(contexts) = last_rollup(node).and_then(|rollup| rollup.contexts.as_ref()) else {
        return Vec::new();
    };
    contexts
        .present()
        .filter_map(|context| {
            let (name, url) = match context {
                RawContext::CheckRun {
                    name, details_url, ..
                } => (name, details_url),
                RawContext::StatusContext {
                    context,
                    target_url,
                    ..
                } => (context, target_url),
                RawContext::Other => return None,
            };
            Some(GhCheck {
                name: name.clone(),
                state: check_state(context),
                url: url.clone(),
            })
        })
        .collect()
}

struct EventContent {
    created_at: String,
    body_html: String,
    review_state: Option<ReviewerState>,
}

fn event(
    kind: EventKind,
    author: Option<GhUser>,
    content: EventContent,
    url: Option<String>,
) -> GhEvent {
    GhEvent {
        kind,
        author,
        created_at: content.created_at,
        body_html: content.body_html,
        review_state: content.review_state,
        url,
    }
}

fn comment_event(comment: &Comment) -> GhEvent {
    event(
        EventKind::Comment,
        comment.author.as_ref().map(to_user),
        EventContent {
            created_at: comment.created_at.clone(),
            body_html: comment.body_html.clone(),
            review_state: None,
        },
        Some(comment.url.clone()),
    )
}

fn review_event(review: &Review) -> GhEvent {
    event(
        EventKind::Review,
        review.author.as_ref().map(to_user),
        EventContent {
            created_at: review
                .submitted_at
                .clone()
                .or_else(|| review.created_at.clone())
                .unwrap_or_default(),
            body_html: review.body_html.clone().unwrap_or_default(),
            review_state: reviewer_state(&review.state),
        },
        review.url.clone(),
    )
}

pub fn map_events(node: &RawDetail) -> Vec<GhEvent> {
    let comments = node
        .comment_events
        .iter()
        .flat_map(Nodes::present)
        .map(comment_event);
    let reviews = node
        .reviews
        .iter()
        .flat_map(Nodes::present)
        .filter(|review| {
            review.state != "PENDING"
                && (review.state != "COMMENTED"
                    || !review.body.as_deref().unwrap_or("").trim().is_empty())
        })
        .map(review_event);
    let mut events: Vec<GhEvent> = comments.chain(reviews).collect();
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    events.truncate(100);
    events
}

pub fn to_detail(node: &RawDetail) -> GhItemDetail {
    let item = to_item(&node.common);
    let is_pr = item.kind == ItemKind::Pr;
    let open = matches!(
        item.state,
        crate::model::PrState::Open | crate::model::PrState::Draft
    );
    GhItemDetail {
        body_html: node.body_html.clone(),
        reviewers: if is_pr {
            map_reviewers(node)
        } else {
            Vec::new()
        },
        check_runs: if is_pr {
            map_checks(&node.common)
        } else {
            Vec::new()
        },
        events: map_events(node),
        viewer_can_review: is_pr && open && !node.viewer_did_author,
        viewer_can_comment: node.viewer_can_react && !node.locked,
        changed_files: if is_pr { node.changed_files } else { None },
        merge_target: None,
        viewer_approved: false,
        item,
    }
}
