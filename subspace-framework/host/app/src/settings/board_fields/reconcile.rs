//! Preserve controls for fields that survive a configured-board replacement.

use super::{Draft, Field, Grant, Settings, control, draft};
use crate::registry::BoardRegistration;
use gpui_kit::{Context, Window};
use std::collections::BTreeMap;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::settings::BoardFields;

pub(in crate::settings) fn reconcile(
    drafts: &mut Vec<Draft>,
    boards: &[BoardRegistration],
    file: &SettingsFile,
    window: &mut Window,
    cx: &mut Context<Settings>,
) {
    let mut retained: BTreeMap<_, _> = std::mem::take(drafts)
        .into_iter()
        .map(|draft| (draft.id, draft))
        .collect();
    for board in boards {
        let Some(fields) = board.fields.as_ref().and_then(|source| source()) else {
            continue;
        };
        let next = match retained.remove(board.id) {
            Some(mut previous) if previous.section == fields.section => {
                reconcile_fields(&mut previous, &fields, file, window, cx);
                previous
            }
            _ => draft(board.id, &fields, file, window, cx),
        };
        drafts.push(next);
    }
}

fn reconcile_fields(
    draft: &mut Draft,
    metadata: &BoardFields,
    file: &SettingsFile,
    window: &mut Window,
    cx: &mut Context<Settings>,
) {
    let mut fields: BTreeMap<_, _> = std::mem::take(&mut draft.fields)
        .into_iter()
        .map(|field| (field.spec.key.clone(), field))
        .collect();
    let saved = file.board_values(&metadata.section);
    for spec in &metadata.fields {
        let control = match fields.remove(&spec.key) {
            Some(previous) if previous.spec.kind == spec.kind => previous.control,
            _ => control(
                spec,
                &saved,
                file.board_secret_present(&metadata.section, &spec.key),
                window,
                cx,
            ),
        };
        draft.fields.push(Field {
            spec: spec.clone(),
            control,
        });
    }
    let mut grants: BTreeMap<_, _> = std::mem::take(&mut draft.permissions)
        .into_iter()
        .map(|grant| (grant.key.clone(), grant))
        .collect();
    for permission in &metadata.permissions {
        let on = file.board_permission(&metadata.section, &permission.key);
        let mut grant = grants.remove(&permission.key).unwrap_or_else(|| Grant {
            key: permission.key.clone(),
            label: permission.label.clone(),
            on,
            initial: on,
        });
        grant.label.clone_from(&permission.label);
        draft.permissions.push(grant);
    }
    draft.notes.clone_from(&metadata.notes);
}
