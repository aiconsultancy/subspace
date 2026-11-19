//! Sample pull requests and issues for tests.

use subspace_framework::client_github::model::{
    CheckState, GhItem, GhLabel, GhUser, ItemKind, PrState, ReviewDecision,
};

const API: &str = "octo-org/api";
const WEB: &str = "octo-org/web";
const SUBSPACE: &str = "octocat/subspace";

fn user(login: &str, name: Option<&str>) -> GhUser {
    GhUser {
        login: login.into(),
        name: name.map(Into::into),
    }
}

fn me() -> GhUser {
    user("Alex-Example_emu", Some("Alex Example"))
}

fn ann() -> GhUser {
    user("Ann-Perkins_emu", Some("Ann Perkins"))
}

fn label(name: &str, color: &str) -> GhLabel {
    GhLabel {
        name: name.into(),
        color: color.into(),
    }
}

fn base(kind: ItemKind, repo: &str, number: u32, title: &str, updated: &str) -> GhItem {
    let path = match kind {
        ItemKind::Pr => "pull",
        ItemKind::Issue => "issues",
    };
    GhItem {
        id: format!("NODE_{number}"),
        key: format!("{repo}#{number}"),
        kind,
        repo: repo.into(),
        number,
        title: title.into(),
        url: format!("https://github.com/{repo}/{path}/{number}"),
        state: PrState::Open,
        author: Some(me()),
        assignees: Vec::new(),
        labels: Vec::new(),
        comments: 0,
        created_at: "2026-09-20T09:00:00Z".into(),
        updated_at: updated.into(),
        review_decision: None,
        checks: None,
        additions: None,
        deletions: None,
        head_ref: None,
        base_ref: None,
    }
}

/// The fields of a sample pull request.
struct PrSpec<'a> {
    repo: &'a str,
    number: u32,
    title: &'a str,
    updated: &'a str,
    state: PrState,
    author: GhUser,
    review: Option<ReviewDecision>,
    checks: Option<CheckState>,
}

fn pr(spec: PrSpec<'_>) -> GhItem {
    GhItem {
        state: spec.state,
        author: Some(spec.author),
        review_decision: spec.review,
        checks: spec.checks,
        additions: Some(spec.number * 7 % 400 + 12),
        deletions: Some(spec.number * 3 % 90 + 1),
        head_ref: Some(format!("feature/{}", spec.number)),
        base_ref: Some("main".into()),
        ..base(
            ItemKind::Pr,
            spec.repo,
            spec.number,
            spec.title,
            spec.updated,
        )
    }
}

fn issue(repo: &str, number: u32, title: &str, updated: &str, state: PrState) -> GhItem {
    GhItem {
        state,
        author: Some(ann()),
        assignees: vec![me()],
        ..base(ItemKind::Issue, repo, number, title, updated)
    }
}

fn labelled(mut item: GhItem, labels: Vec<GhLabel>, comments: u32) -> GhItem {
    item.labels = labels;
    item.comments = comments;
    item
}

/// The pull requests and the flaky-test issue of the API repository.
fn api_items() -> Vec<GhItem> {
    vec![
        pr(PrSpec {
            repo: API,
            number: 421,
            title: "Add paging to the work items endpoint",
            updated: "2026-09-30T08:15:00Z",
            state: PrState::Open,
            author: ann(),
            review: Some(ReviewDecision::ReviewRequired),
            checks: Some(CheckState::Pending),
        }),
        pr(PrSpec {
            repo: API,
            number: 415,
            title: "Retry token refresh on 401",
            updated: "2026-09-28T13:40:00Z",
            state: PrState::Open,
            author: me(),
            review: Some(ReviewDecision::Approved),
            checks: Some(CheckState::Success),
        }),
        labelled(
            issue(
                API,
                418,
                "Integration tests time out on the ARM runners",
                "2026-09-29T16:10:00Z",
                PrState::Open,
            ),
            vec![label("bug", "d73a4a"), label("ci", "0e8a16")],
            4,
        ),
    ]
}

/// The pull requests of the web repository.
fn web_items() -> Vec<GhItem> {
    vec![
        pr(PrSpec {
            repo: WEB,
            number: 1203,
            title: "Keyboard navigation for the board",
            updated: "2026-09-29T11:05:00Z",
            state: PrState::Open,
            author: me(),
            review: Some(ReviewDecision::ChangesRequested),
            checks: Some(CheckState::Failure),
        }),
        pr(PrSpec {
            repo: WEB,
            number: 1198,
            title: "Spike: virtualised list rows",
            updated: "2026-09-25T17:20:00Z",
            state: PrState::Draft,
            author: me(),
            review: None,
            checks: None,
        }),
    ]
}

/// The merged pull request and the issues of this repository.
fn subspace_items() -> Vec<GhItem> {
    vec![
        pr(PrSpec {
            repo: SUBSPACE,
            number: 12,
            title: "Scaffold the Cargo workspace",
            updated: "2026-09-26T10:00:00Z",
            state: PrState::Merged,
            author: me(),
            review: Some(ReviewDecision::Approved),
            checks: Some(CheckState::Success),
        }),
        labelled(
            issue(
                SUBSPACE,
                7,
                "Document the dashboard interface",
                "2026-09-27T08:30:00Z",
                PrState::Open,
            ),
            vec![label("documentation", "0075ca")],
            0,
        ),
        issue(
            SUBSPACE,
            3,
            "Choose the GitHub data source",
            "2026-09-21T15:45:00Z",
            PrState::Closed,
        ),
    ]
}

pub fn items() -> Vec<GhItem> {
    [api_items(), web_items(), subspace_items()].concat()
}
