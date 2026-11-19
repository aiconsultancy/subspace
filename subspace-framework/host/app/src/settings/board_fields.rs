//! Drafts of the settings boards declare at run time. Values are saved in
//! the same config file write as the rest of Settings. Saved secrets are
//! never read into an input.

use std::collections::BTreeMap;

use gpui_kit::component::input::InputState;
use gpui_kit::{App, AppContext as _, Context, Entity, Window};
use serde_json::{Map, Value};
use subspace_core::settings::{BoardFieldEdits, SettingsFile, TokenEdit};
use subspace_dashboard::settings::{BoardFields, FieldKind, FieldSpec};

use super::Settings;
use super::form::{Choice, choice, token_edit};
use crate::registry::BoardRegistration;

mod reconcile;
pub(super) use reconcile::reconcile;

/// One board's run-time settings.
pub(super) struct Draft {
    pub id: &'static str,
    section: Vec<String>,
    pub notes: Vec<String>,
    pub fields: Vec<Field>,
    pub permissions: Vec<Grant>,
}

pub(super) struct Grant {
    pub key: String,
    pub label: String,
    pub on: bool,
    initial: bool,
}

pub(super) struct Field {
    pub spec: FieldSpec,
    pub control: Control,
}

/// A field's control and the value it showed when Settings opened or last
/// saved, so only real changes are written.
pub(super) enum Control {
    Text {
        input: Entity<InputState>,
        initial: String,
    },
    Toggle {
        on: bool,
        initial: bool,
    },
    Choice {
        choice: Choice,
        initial: Option<String>,
    },
    Secret {
        input: Entity<InputState>,
        clear: bool,
        saved: bool,
    },
}

pub(super) fn load(
    boards: &[BoardRegistration],
    file: &SettingsFile,
    window: &mut Window,
    cx: &mut Context<Settings>,
) -> Vec<Draft> {
    boards
        .iter()
        .filter_map(|board| Some((board.id, board.fields.as_ref()?()?)))
        .map(|(id, fields)| draft(id, &fields, file, window, cx))
        .collect()
}

fn draft(
    id: &'static str,
    fields: &BoardFields,
    file: &SettingsFile,
    window: &mut Window,
    cx: &mut Context<Settings>,
) -> Draft {
    let saved = file.board_values(&fields.section);
    let controls = fields
        .fields
        .iter()
        .map(|spec| Field {
            control: control(
                spec,
                &saved,
                file.board_secret_present(&fields.section, &spec.key),
                window,
                cx,
            ),
            spec: spec.clone(),
        })
        .collect();
    Draft {
        id,
        section: fields.section.clone(),
        notes: fields.notes.clone(),
        fields: controls,
        permissions: fields
            .permissions
            .iter()
            .map(|permission| {
                let on = file.board_permission(&fields.section, &permission.key);
                Grant {
                    key: permission.key.clone(),
                    label: permission.label.clone(),
                    on,
                    initial: on,
                }
            })
            .collect(),
    }
}

fn control(
    spec: &FieldSpec,
    saved: &Map<String, Value>,
    secret_saved: bool,
    window: &mut Window,
    cx: &mut Context<Settings>,
) -> Control {
    let value = saved.get(&spec.key);
    match &spec.kind {
        FieldKind::Boolean { default } => {
            let on = value.and_then(Value::as_bool).unwrap_or(*default);
            Control::Toggle { on, initial: on }
        }
        FieldKind::Select { options, default } => {
            let initial = value
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| default.clone());
            let choice = choice(options.clone(), initial.as_deref(), false, window, cx);
            Control::Choice { choice, initial }
        }
        FieldKind::Secret => Control::Secret {
            input: secret_input(window, cx),
            clear: false,
            saved: secret_saved,
        },
        _ => {
            let initial = spec.text(value);
            let text = initial.clone();
            let input = cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(text, window, cx);
                input
            });
            Control::Text { input, initial }
        }
    }
}

fn secret_input(window: &mut Window, cx: &mut Context<Settings>) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).masked(true))
}

/// The changes to write, board by board; boards without changes are left
/// out. An invalid value stops the save with a message naming the field.
pub(super) fn edits(drafts: &[Draft], cx: &App) -> Result<Vec<BoardFieldEdits>, String> {
    let mut all = Vec::new();
    for draft in drafts {
        let edit = board_edits(draft, cx)?;
        if !edit.values.is_empty() || !edit.secrets.is_empty() || !edit.permissions.is_empty() {
            all.push(edit);
        }
    }
    Ok(all)
}

fn board_edits(draft: &Draft, cx: &App) -> Result<BoardFieldEdits, String> {
    let mut values = BTreeMap::new();
    let mut secrets = BTreeMap::new();
    for field in &draft.fields {
        let key = field.spec.key.clone();
        match field_edit(field, cx)? {
            Edit::None => {}
            Edit::Value(value) => {
                values.insert(key, value);
            }
            Edit::Secret(edit) => {
                secrets.insert(key, edit);
            }
        }
    }
    Ok(BoardFieldEdits {
        section: draft.section.clone(),
        values,
        secrets,
        permissions: draft
            .permissions
            .iter()
            .filter(|grant| grant.on != grant.initial)
            .map(|grant| (grant.key.clone(), grant.on))
            .collect(),
    })
}

enum Edit {
    None,
    Value(Option<Value>),
    Secret(TokenEdit),
}

fn field_edit(field: &Field, cx: &App) -> Result<Edit, String> {
    Ok(match &field.control {
        Control::Text { input, initial } => {
            let text = input.read(cx).value().to_string();
            if text == *initial {
                Edit::None
            } else {
                Edit::Value(field.spec.parse_text(&text)?)
            }
        }
        Control::Toggle { on, initial } if on != initial => Edit::Value(Some(Value::Bool(*on))),
        Control::Choice { choice, initial } => {
            let selected = choice.read(cx).selected_value().cloned();
            if selected == *initial {
                Edit::None
            } else {
                Edit::Value(selected.map(Value::String))
            }
        }
        Control::Secret { input, clear, .. } => match token_edit(&input.read(cx).value(), *clear) {
            TokenEdit::Keep => Edit::None,
            edit => Edit::Secret(edit),
        },
        Control::Toggle { .. } => Edit::None,
    })
}

/// Whether any run-time field changed. An invalid value counts, so Save
/// reports it.
pub(super) fn changed(drafts: &[Draft], cx: &App) -> bool {
    edits(drafts, cx).map_or(true, |edits| !edits.is_empty())
}

/// After a successful save: the shown values become the saved ones, and
/// secret inputs are emptied.
pub(super) fn accept(
    drafts: &mut [Draft],
    file: &SettingsFile,
    window: &mut Window,
    cx: &mut Context<Settings>,
) {
    for draft in drafts {
        for grant in &mut draft.permissions {
            grant.initial = grant.on;
        }
        for field in &mut draft.fields {
            accept_field(field, file, &draft.section, window, cx);
        }
    }
}

fn accept_field(
    field: &mut Field,
    file: &SettingsFile,
    section: &[String],
    window: &mut Window,
    cx: &mut Context<Settings>,
) {
    match &mut field.control {
        Control::Text { input, initial } => *initial = input.read(cx).value().to_string(),
        Control::Toggle { on, initial } => *initial = *on,
        Control::Choice { choice, initial } => *initial = choice.read(cx).selected_value().cloned(),
        Control::Secret {
            input,
            clear,
            saved,
        } => {
            *clear = false;
            *saved = file.board_secret_present(section, &field.spec.key);
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }
}

impl Settings {
    pub(super) fn set_board_grant(&mut self, board: usize, key: &str, on: bool) {
        if let Some(grant) = self
            .board_fields
            .get_mut(board)
            .and_then(|draft| draft.permissions.iter_mut().find(|grant| grant.key == key))
        {
            grant.on = on;
        }
    }
    pub(super) fn set_field_toggle(&mut self, board: usize, field: usize, value: bool) {
        if let Some(Control::Toggle { on, .. }) = self.field_control(board, field) {
            *on = value;
        }
    }

    /// Clear a saved secret, or undo that choice, and empty its input.
    pub(super) fn toggle_field_clear(
        &mut self,
        board: usize,
        field: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Control::Secret { input, clear, .. }) = self.field_control(board, field) {
            *clear = !*clear;
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.secret_hint = None;
        cx.notify();
    }

    fn field_control(&mut self, board: usize, field: usize) -> Option<&mut Control> {
        self.board_fields
            .get_mut(board)?
            .fields
            .get_mut(field)
            .map(|field| &mut field.control)
    }
}
