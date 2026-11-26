use subspace_framework::client_github::model::{DiffResponse, GhItem, GhItemDetail};
use subspace_framework::dashboard::{full_text, html};

const MAX_INDEXED_TEXT_BYTES: usize = 48 * 1024;
const TRUNCATION_MARKER: &str = "\n\n[GitHub content truncated to fit the local search cache]";

/// The credential-bound search scope for a verified GitHub account.
/// `pub(crate)`: reused by `agent_tools`'s refresh action so both write
/// paths index into the same scope.
pub(crate) fn scope(app: &str, endpoint: &str, account: &str) -> Option<full_text::Scope> {
    if account.trim().is_empty() || endpoint.trim().is_empty() {
        return None;
    }
    Some(full_text::Scope {
        app: app.into(),
        board: "github".into(),
        endpoint: endpoint.into(),
        tenant: String::new(),
        account: account.into(),
    })
}

/// A fetched item as a retained search document. `pub(crate)`: reused by
/// `agent_tools`'s refresh action so a refreshed item indexes identically
/// to a prefetched or loaded one.
pub(crate) fn document(
    detail: &GhItemDetail,
    refreshed_at: i64,
    account_id: &str,
    login: &str,
) -> Result<full_text::Document, String> {
    document_with_diff(detail, None, refreshed_at, account_id, login)
}

/// `document`, with an optional diff appended to the indexed body.
/// `pub(crate)`: reused by `agent_tools`'s refresh action.
pub(crate) fn document_with_diff(
    detail: &GhItemDetail,
    diff: Option<&DiffResponse>,
    refreshed_at: i64,
    account_id: &str,
    login: &str,
) -> Result<full_text::Document, String> {
    let mut bodies = vec![html::to_markdown_without_images(&detail.body_html)];
    bodies.extend(detail.events.iter().map(|event| {
        format!(
            "{} · {}\n{}",
            event.author.as_ref().map_or("", |author| &author.login),
            event.created_at,
            html::to_markdown_without_images(&event.body_html)
        )
    }));
    if let Some(diff) = diff {
        bodies.extend(diff.files.iter().map(|file| {
            format!(
                "{}\n{}",
                file.path,
                file.patch.as_deref().unwrap_or_default()
            )
        }));
    }
    let metadata = detail
        .item
        .labels
        .iter()
        .map(|label| label.name.as_str())
        .chain(detail.item.assignees.iter().map(|user| user.login.as_str()))
        .chain(detail.item.author.iter().map(|user| user.login.as_str()))
        .chain(
            detail
                .reviewers
                .iter()
                .map(|reviewer| reviewer.user.login.as_str()),
        )
        .chain(detail.check_runs.iter().map(|check| check.name.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let body = bounded(&bodies.join("\n\n"));
    let retained = bounded_detail(detail);
    Ok(full_text::Document {
        id: detail.item.key.clone(),
        title: detail.item.title.clone(),
        body,
        metadata: format!("{} {metadata}", detail.item.repo),
        parent_id: Some(detail.item.repo.clone()),
        kind: "GitHub fetched issue/PR · up to 100 activity entries".into(),
        url: Some(detail.item.url.clone()),
        navigation: detail.item.key.clone(),
        refreshed_at,
        payload: Some(
            serde_json::to_string(&Retained {
                account_id: account_id.into(),
                login: login.to_lowercase(),
                detail: retained,
                diff_indexed: diff.is_some(),
            })
            .map_err(|_| "Invalid retained GitHub item")?,
        ),
    })
}

pub(super) fn list_document(
    item: &GhItem,
    refreshed_at: i64,
    account_id: &str,
    login: &str,
) -> Result<full_text::Document, String> {
    let detail = GhItemDetail {
        item: item.clone(),
        body_html: String::new(),
        reviewers: Vec::new(),
        check_runs: Vec::new(),
        events: Vec::new(),
        viewer_can_review: false,
        viewer_can_comment: false,
        changed_files: None,
        merge_target: None,
        viewer_approved: false,
    };
    document(&detail, refreshed_at, account_id, login)
}

fn bounded_detail(detail: &GhItemDetail) -> GhItemDetail {
    let mut retained = detail.clone();
    retained.body_html = bounded(&retained.body_html);
    let mut used = retained.body_html.len();
    retained.events.retain_mut(|event| {
        if used >= MAX_INDEXED_TEXT_BYTES {
            return false;
        }
        event.body_html = bounded_to(&event.body_html, MAX_INDEXED_TEXT_BYTES - used);
        used += event.body_html.len();
        true
    });
    retained
}

fn bounded(value: &str) -> String {
    bounded_to(value, MAX_INDEXED_TEXT_BYTES)
}

fn bounded_to(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let content_limit = limit.saturating_sub(TRUNCATION_MARKER.len());
    let mut end = content_limit.min(value.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}{}", &value[..end], TRUNCATION_MARKER)
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Retained {
    pub account_id: String,
    pub login: String,
    pub detail: GhItemDetail,
    #[serde(default)]
    pub diff_indexed: bool,
}

pub(super) fn retained(hit: &full_text::Hit) -> Option<Retained> {
    let mut retained: Retained = serde_json::from_str(hit.document.payload.as_deref()?).ok()?;
    let detail = &mut retained.detail;
    if hit.target.scope.board != "github"
        || retained.account_id.trim().is_empty()
        || retained.account_id != hit.target.scope.account
        || retained.login.trim().is_empty()
        || hit.target.id != detail.item.key
        || hit.document.id != detail.item.key
        || hit.document.navigation != detail.item.key
    {
        return None;
    }
    detail.viewer_can_comment = false;
    detail.viewer_can_review = false;
    Some(retained)
}
