//! Retain board-owned controls while keeping Save, Cancel and merging generic.
//! Each editor edits its board's own settings file.

use std::collections::BTreeMap;

use gpui_kit::{App, Context, Subscription, Window};
use serde_json::{Map, Value};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::settings::SettingsEditor;

use crate::BoardRegistration;

pub(super) struct Draft {
    pub id: &'static str,
    pub editor: SettingsEditor,
    initial: Value,
    _subscription: Subscription,
}

/// The editors of `boards`, and why any board's file could not be opened.
/// A board whose file cannot be opened has no editor, so Save leaves it as is.
pub(super) fn load(
    boards: &[BoardRegistration],
    file: &mut SettingsFile,
    form: &super::ServiceForm,
    window: &mut Window,
    cx: &mut Context<super::Settings>,
) -> (Vec<Draft>, Vec<String>) {
    let mut errors = Vec::new();
    let drafts = boards
        .iter()
        .filter_map(|board| {
            let factory = board.settings_editor?;
            let saved = file
                .open_board_file(board.id)
                .map_err(|error| errors.push(error.to_string()))
                .ok()?;
            let editor = factory(saved, form.board_enabled(board.id), window, cx);
            let initial = editor.snapshot(cx);
            let owner = cx.weak_entity();
            let subscription = editor.observe(
                move |cx| {
                    _ = owner.update(cx, |_, cx| cx.notify());
                },
                cx,
            );
            Some(Draft {
                id: board.id,
                editor,
                initial,
                _subscription: subscription,
            })
        })
        .collect();
    (drafts, errors)
}

pub(super) fn snapshots(drafts: &[Draft], cx: &App) -> BTreeMap<&'static str, Value> {
    drafts
        .iter()
        .map(|draft| (draft.id, draft.editor.snapshot(cx)))
        .collect()
}

pub(super) fn changed(drafts: &[Draft], cx: &App) -> bool {
    drafts
        .iter()
        .any(|draft| draft.initial != draft.editor.snapshot(cx))
}

/// The changed editors' values, keyed by board id.
pub(super) fn edits(
    drafts: &[Draft],
    cx: &App,
) -> Result<BTreeMap<String, Map<String, Value>>, String> {
    drafts
        .iter()
        .filter(|draft| draft.initial != draft.editor.snapshot(cx))
        .map(|draft| Ok((draft.id.to_owned(), draft.editor.values(cx)?)))
        .collect()
}

/// The saved settings of each board in `edits`, as its file now holds them.
pub(super) fn saved(
    edits: &BTreeMap<String, Map<String, Value>>,
    file: &SettingsFile,
) -> Vec<(String, Map<String, Value>)> {
    edits
        .keys()
        .map(|id| (id.clone(), file.board_file_values(id)))
        .collect()
}

pub(super) fn accept(drafts: &mut [Draft], cx: &App) {
    for draft in drafts {
        draft.initial = draft.editor.snapshot(cx);
    }
}
