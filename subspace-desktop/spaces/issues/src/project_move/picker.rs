use super::{Issues, Phase};
use gpui_kit::{Context, Window};
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
pub(super) const ID: &str = "issue-move-project";

// qual:allow(coupling, oi) reason: "The native menu coordinates its anchor, retained move scope and one preview request."
pub(super) fn open(board: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let Some(state) = &board.project_move else {
        return;
    };
    if state.busy() || matches!(state.phase, Phase::Unconfirmed(_)) {
        return;
    }
    let source = board
        .issue_view
        .snapshot
        .as_ref()
        .and_then(|s| s.detail())
        .map(|d| d.fields.project_id.as_str());
    let options = board
        .projects
        .iter()
        .filter(|p| p.archived_at.is_none() && Some(p.id.as_str()) != source)
        .map(|p| {
            MenuOption::new(p.id.clone(), format!("{} · {}", p.key, p.name))
                .checked(state.destination.as_ref() == Some(&p.id))
        })
        .collect();
    let anchor = board.menu_anchors.anchor(ID, cx);
    let contents = MenuContents::new("Destination project", "Find a project", options);
    let Some(menu) = board.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe(&menu, |board, _, event: &MenuEvent, cx| {
        match event {
            MenuEvent::Pick(value) => {
                if let Some(state) = &mut board.project_move {
                    state.destination = Some(value.to_string());
                }
                board.menu = None;
                board.load_move_preview(cx);
            }
            MenuEvent::Close => board.menu = None,
            MenuEvent::Query(_) => {}
        }
        cx.notify();
    })
    .detach();
    board.menu = Some(menu);
}
