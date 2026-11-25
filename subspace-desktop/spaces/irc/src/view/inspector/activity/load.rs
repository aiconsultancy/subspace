//! An item too large to send inline, read page by page in the
//! background and kept whole in the selection.

use gpui_kit::{AppContext as _, Context, Task};
use subspace_client_fleet::activity::ActivityItem;
use subspace_client_fleet::{Error, FleetApi};

use crate::Irc;

/// An item read in full, or being read.
pub(super) struct Loaded {
    pub(super) result: Option<Result<ActivityItem, Error>>,
    _read: Option<Task<()>>,
}

/// Reads an item that was too large to send inline, page by page.
pub(super) fn load(board: &mut Irc, turn: &str, item: &str, cx: &mut Context<Irc>) {
    let client = board.inspector_activity.as_ref().and_then(|selection| {
        let api = FleetApi::current(cx)?;
        api.read(cx).client(&selection.fleet).ok()
    });
    let Some((selection, client)) = board.inspector_activity.as_mut().zip(client) else {
        return;
    };
    let key = (turn.to_owned(), item.to_owned());
    let (agent, read_key) = (selection.agent.clone(), key.clone());
    let read = cx.spawn(async move |board, cx| {
        let (turn, item) = read_key.clone();
        let result = cx
            .background_spawn(async move { client.activity_item(&agent, &turn, &item) })
            .await;
        _ = board.update(cx, |board, cx| {
            if let Some(loaded) = board
                .inspector_activity
                .as_mut()
                .and_then(|selection| selection.loaded.get_mut(&read_key))
            {
                loaded.result = Some(result);
                cx.notify();
            }
        });
    });
    selection.loaded.insert(
        key,
        Loaded {
            result: None,
            _read: Some(read),
        },
    );
    cx.notify();
}
