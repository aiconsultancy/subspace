use super::{Context, Github};

pub(super) fn poll(board: &mut Github, cx: &mut Context<Github>) -> bool {
    purge(board, cx);
    !board.retained_permits.is_empty()
}

pub(crate) fn purge(board: &mut Github, cx: &mut Context<Github>) {
    let expired: Vec<_> = board
        .retained_permits
        .iter()
        .filter(|(_, permit)| !permit.valid())
        .map(|(key, _)| key.clone())
        .collect();
    for (account, key) in &expired {
        board
            .retained_permits
            .remove(&(account.clone(), key.clone()));
        if let Some(data) = board.data.get_mut(account) {
            data.details.remove(key);
        }
        if board.retained_key.as_ref() == Some(key) {
            board.retained_key = None;
            board.list.close_detail();
        }
    }
    if !expired.is_empty() {
        board.rebuild(cx);
        cx.notify();
    }
}
