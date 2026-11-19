use gpui_kit::{AppContext as _, Context, Entity};
use subspace_api_client::issue_writes::{ChoiceSource, IssueChoice, IssueSaveError};
use subspace_framework::dashboard::menu::Menu;

use super::super::super::{Field, IssueEditor};
use super::options;
use crate::Issues;

pub(super) fn load(
    board: &Issues,
    editor: &IssueEditor,
    menu: Entity<Menu>,
    field: Field,
    cx: &mut Context<Issues>,
) {
    let source = field.source(&editor.properties, &board.projects);
    let scope = editor.scope.clone();
    if let (Some(source), Some(hub)) = (source, board.hub.clone()) {
        let key = editor.key.value();
        menu.update(cx, |menu, cx| {
            menu.set_note(Some("Loading options…".into()), cx)
        });
        cx.spawn(async move |board, cx| {
            let requested = source.clone();
            let result = cx
                .background_spawn(async move { hub.issue_choices(&scope, &requested) })
                .await;
            let _ = board.update(cx, |board, cx| {
                if board
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.key.value() == key)
                    && board.menu.as_ref() == Some(&menu)
                {
                    loaded(board, field, &menu, (source, result), cx);
                }
            });
        })
        .detach();
    }
}

fn loaded(
    board: &mut Issues,
    field: Field,
    menu: &Entity<Menu>,
    (source, result): (ChoiceSource, Result<Vec<IssueChoice>, IssueSaveError>),
    cx: &mut Context<Issues>,
) {
    let Some(editor) = board.editor.as_mut() else {
        return;
    };
    let note = match result {
        Ok(choices) => {
            editor.catalogues.insert(source, choices);
            None
        }
        Err(_) => {
            editor.catalogues.remove(&source);
            Some("Options unavailable. Close and open the picker to retry.".into())
        }
    };
    let Some(editor) = &board.editor else {
        return;
    };
    let result = options(board, editor, field);
    menu.update(cx, |menu, cx| {
        menu.set_options(result, cx);
        menu.set_note(note, cx);
    });
}
