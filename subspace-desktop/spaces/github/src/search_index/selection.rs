use super::{Context, Github, full_text_service, mapping, model};
use crate::{SectionRefresh, references};
use gpui_kit::AppContext as _;
use subspace_framework::dashboard::full_text;

pub(crate) fn account_changed(board: &mut Github, changed: bool, cx: &mut Context<Github>) {
    let Some(service) = full_text_service(cx) else {
        return;
    };
    let Some(client) = board.client.clone() else {
        return;
    };
    let Some(login) = board.account.clone() else {
        return;
    };
    let generation = board.search_generation;
    board._search_selection = Some(cx.spawn(async move |this, cx| {
        let requested = login.clone();
        let cache = service.clone();
        let (ticket, result) = cx
            .background_spawn(async move {
                let ticket = if changed {
                    cache.begin_transition("github")
                } else {
                    cache.begin_activation("github")
                }
                .ok();
                let result = if cache.policy("github").is_ok_and(|policy| policy.enabled) {
                    client.bind_read(&requested).map(Some)
                } else {
                    Ok(None)
                };
                (ticket, result)
            })
            .await;
        let selected = this
            .update(cx, |board, cx| {
                if board.search_generation != generation {
                    return None;
                }
                let bound = match result {
                    Ok(Some(bound)) if model::same_login(bound.account(), &login) => bound,
                    Ok(_) => return None,
                    Err(error) => {
                        super::bind_failed(board, error, cx);
                        return None;
                    }
                };
                let scope = mapping::scope(service.app_id(), bound.endpoint(), bound.account_id());
                select(board, generation, &scope, bound.account(), cx)
                    .then_some(scope)
                    .flatten()
            })
            .ok()
            .flatten();
        if let Some((scope, ticket)) = selected.zip(ticket) {
            cx.background_spawn(async move {
                if let Ok(activation) = service.prepare_activation_ticket(&ticket, &scope) {
                    let _ = service.activate_prepared(&activation);
                }
            })
            .await;
        }
    }));
}

pub(super) fn select(
    board: &mut Github,
    generation: u64,
    scope: &Option<full_text::Scope>,
    verified_login: &str,
    cx: &mut Context<Github>,
) -> bool {
    if board.search_generation != generation {
        return false;
    }
    let Some(scope) = scope.as_ref() else {
        return false;
    };
    if board
        .account
        .as_deref()
        .is_none_or(|login| !model::same_login(login, verified_login))
    {
        return false;
    }
    let previous = board.search_scope.replace(scope.clone());
    if reset_reassigned_login(&mut board.data, previous.as_ref(), scope, verified_login) {
        board.retained_key = None;
        for (login, key) in board.retained_permits.keys() {
            if let Some(account) = board.data.get_mut(login) {
                account.details.remove(key);
            }
        }
        board.retained_permits.clear();
        board.rebuild(cx);
        // Initial scope selection may evict restored rows while live reads
        // for this identity are in flight. Only a verified replacement
        // invalidates those reads and starts fresh ones.
        if previous.is_some() {
            board.advance_read_generation(verified_login.to_lowercase());
            references::forget(board, cx);
            board.load_all(SectionRefresh::Automatic, cx);
        }
    }
    true
}

pub(super) fn reset_reassigned_login(
    data: &mut std::collections::HashMap<String, crate::Account>,
    previous: Option<&full_text::Scope>,
    current: &full_text::Scope,
    login: &str,
) -> bool {
    if previous != Some(current) {
        data.remove(&login.to_lowercase());
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use subspace_framework::dashboard::Dashboard as _;

    use super::{Github, mapping, select};

    #[gpui_kit::test]
    fn a_verified_identity_change_invalidates_reads_for_the_same_login(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let window = cx.add_window(Github::new);
        window
            .update(cx, |board, _, cx| {
                let initial = mapping::scope("fixture", "https://api.github.com", "previous");
                let replacement = mapping::scope("fixture", "https://api.github.com", "current");
                board.account = Some("octo-dev".into());
                let generation = board.search_generation;
                let pending = board.read_generation("octo-dev");
                assert!(select(board, generation, &initial, "octo-dev", cx));
                assert_eq!(board.read_generation("octo-dev"), pending);
                assert!(select(board, generation, &replacement, "octo-dev", cx));
                assert_ne!(board.read_generation("octo-dev"), pending);
                assert_eq!(board.search_scope, replacement);
            })
            .unwrap();
    }
}
