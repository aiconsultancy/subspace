use serde_json::Value;

use super::types::{Nodes, RawActor, RawCommon, RawExtra, RawPrFile};
use crate::model::{GhDiffFile, GhFileStatus, GhItem, GhUser, ItemKind, PrState};

pub(super) fn to_user(actor: &RawActor) -> GhUser {
    GhUser {
        login: actor.login.clone(),
        name: actor.name.clone().filter(|name| !name.is_empty()),
    }
}

pub fn pr_state(state: &str, draft: bool) -> PrState {
    match state {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ if draft => PrState::Draft,
        _ => PrState::Open,
    }
}

pub(super) fn last_rollup(node: &RawCommon) -> Option<&super::types::RawRollup> {
    node.commits
        .as_ref()?
        .nodes
        .last()?
        .as_ref()?
        .commit
        .status_check_rollup
        .as_ref()
}

// qual:allow(dry, boilerplate) reason: "Borrowed GraphQL data must remain usable for detail mapping; the owned GhItem contract requires independent field copies."
pub fn to_item(node: &RawCommon) -> GhItem {
    let is_pr = node.typename == "PullRequest";
    let repo = node.repository.name_with_owner.clone();
    GhItem {
        id: node.id.clone(),
        key: format!("{repo}#{}", node.number),
        kind: if is_pr { ItemKind::Pr } else { ItemKind::Issue },
        repo,
        number: node.number,
        title: node.title.clone(),
        url: node.url.clone(),
        state: if is_pr {
            pr_state(&node.state, node.is_draft.unwrap_or(false))
        } else if node.state == "OPEN" {
            PrState::Open
        } else {
            PrState::Closed
        },
        author: node.author.as_ref().map(to_user),
        assignees: node.assignees.present().map(to_user).collect(),
        labels: node
            .labels
            .iter()
            .flat_map(Nodes::present)
            .cloned()
            .collect(),
        comments: node.comments.total_count,
        created_at: node.created_at.clone(),
        updated_at: node.updated_at.clone(),
        review_decision: super::activity::review_decision(node.review_decision.as_deref())
            .filter(|_| is_pr),
        checks: super::activity::rollup_state(
            last_rollup(node).and_then(|rollup| rollup.state.as_deref()),
        )
        .filter(|_| is_pr),
        additions: node.additions.filter(|_| is_pr),
        deletions: node.deletions.filter(|_| is_pr),
        head_ref: node.head_ref_name.clone().filter(|_| is_pr),
        base_ref: node.base_ref_name.clone().filter(|_| is_pr),
    }
}

pub fn section_items(nodes: &[Value], extras: &[RawExtra]) -> Vec<GhItem> {
    nodes
        .iter()
        .filter_map(|node| serde_json::from_value::<RawCommon>(node.clone()).ok())
        .map(|mut node| {
            let id = node.id.clone();
            for extra in extras.iter().filter(|extra| extra.id == id) {
                node.merge(extra);
            }
            to_item(&node)
        })
        .collect()
}

pub fn extras(nodes: &[Value]) -> Vec<RawExtra> {
    nodes
        .iter()
        .filter_map(|node| serde_json::from_value::<RawExtra>(node.clone()).ok())
        .collect()
}

pub fn assignable_users(nodes: &Nodes<RawActor>) -> Vec<GhUser> {
    nodes.present().map(to_user).collect()
}

pub fn to_diff_file(raw: RawPrFile) -> GhDiffFile {
    let status =
        serde_json::from_value(Value::String(raw.status)).unwrap_or(GhFileStatus::Modified);
    GhDiffFile {
        path: raw.filename,
        previous_path: raw.previous_filename,
        status,
        additions: raw.additions.unwrap_or(0),
        deletions: raw.deletions.unwrap_or(0),
        patch: raw.patch.filter(|patch| !patch.is_empty()),
    }
}
