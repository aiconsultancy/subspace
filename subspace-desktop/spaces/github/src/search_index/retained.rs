use super::{
    Context, DashboardEvent, Github, Group, Toast, Window, full_text, full_text_service, mapping,
    model, rows,
};

pub(crate) fn append_retained(board: &Github, groups: &mut Vec<Group>) {
    let Some(key) = &board.retained_key else {
        return;
    };
    if groups
        .iter()
        .flat_map(|group| &group.rows)
        .any(|row| row.key.as_ref() == key)
    {
        return;
    }
    let Some(item) = board.find_item(key) else {
        return;
    };
    groups.push(Group {
        key: "github-retained".into(),
        name: "Retained search result".into(),
        icon: subspace_framework::dashboard::Glyph::new(
            "⌕",
            subspace_framework::dashboard::Tone::Muted,
        ),
        count: Some("1".into()),
        rows: vec![rows::row(
            item,
            board.account.as_deref().unwrap_or_default(),
            board.now,
        )],
    });
}

pub(crate) fn open(
    board: &mut Github,
    hit: &full_text::Hit,
    permit: full_text::ReadPermit,
    window: &mut Window,
    cx: &mut Context<Github>,
) {
    if !permit.valid() {
        return;
    }
    let Some(service) = full_text_service(cx) else {
        return;
    };
    let Some(retained) = mapping::retained(hit) else {
        return;
    };
    if hit.target.scope.app != service.app_id()
        || board
            .account
            .as_deref()
            .is_some_and(|account| !model::same_login(account, &retained.login))
        || board
            .search_scope
            .as_ref()
            .is_some_and(|scope| scope != &hit.target.scope)
    {
        return;
    }
    let detail = retained.detail;
    if !is_pr_detail(&detail) {
        // The board is PRs-only (REQ-02): a full-text hit retained from
        // before that change, or from a different reader, never opens an
        // Issue here. Do not purge the index entry or the reader's search
        // history; only refuse to surface it at this boundary.
        return;
    }
    let key = detail.item.key.clone();
    let watch = board.retained_permits.is_empty();
    board
        .retained_permits
        .insert((retained.login.to_lowercase(), key.clone()), permit);
    if watch {
        board.retained_watch = Some(subspace_framework::dashboard::retained::watch_retained(
            cx,
            super::permits::poll,
        ));
    }
    board._detail_task = None;
    board.search_generation = board.search_generation.wrapping_add(1);
    board.account = Some(retained.login.clone());
    board.search_scope = Some(hit.target.scope.clone());
    board
        .data
        .entry(retained.login.to_lowercase())
        .or_default()
        .details
        .insert(key.clone(), detail);
    board.retained_key = Some(key.clone());
    board.detail_key = Some(key.clone());
    board.diff = None;
    show_retained_item(board, &key, window, cx);
}

fn show_retained_item(
    board: &mut Github,
    key: &str,
    window: &mut Window,
    cx: &mut Context<Github>,
) {
    crate::navigation::selection_changed(board, Some(key), cx);
    board.rebuild(cx);
    board
        .list
        .set_status(subspace_framework::dashboard::list::LoadStatus::Ready);
    board.list.reveal(key);
    board.focus.focus(window, cx);
    cx.emit(DashboardEvent::Opened(key.into()));
    cx.emit(DashboardEvent::Toast(Toast::info(
        "Retained GitHub item · refresh for live content",
    )));
    cx.notify();
}

/// REQ-02: the board is PRs-only, so a retained full-text hit only opens
/// when its original item was a pull request.
fn is_pr_detail(detail: &subspace_framework::client_github::model::GhItemDetail) -> bool {
    detail.item.kind == subspace_framework::client_github::model::ItemKind::Pr
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::model::ItemKind;

    use super::is_pr_detail;

    #[test]
    fn a_retained_issue_detail_is_rejected_but_a_pull_request_is_not() {
        let items = crate::sample::items();
        let pr = items
            .iter()
            .find(|item| item.kind == ItemKind::Pr)
            .cloned()
            .unwrap();
        let issue = items
            .iter()
            .find(|item| item.kind == ItemKind::Issue)
            .cloned()
            .unwrap();
        let detail_of = |item| subspace_framework::client_github::model::GhItemDetail {
            item,
            body_html: String::new(),
            reviewers: vec![],
            check_runs: vec![],
            events: vec![],
            viewer_can_review: false,
            viewer_can_comment: false,
            changed_files: None,
            merge_target: None,
            viewer_approved: false,
        };
        assert!(is_pr_detail(&detail_of(pr)));
        assert!(!is_pr_detail(&detail_of(issue)));
    }
}
