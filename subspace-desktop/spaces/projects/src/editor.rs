//! Retained milestone fields and the removal confirmation.

use chrono::NaiveDate;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::{Disableable as _, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_api_client::projects::{Milestone, MilestoneChange, MilestoneFields};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, dialog_button},
    dialog::{self, DialogWidth},
    feedback,
    fields::{self, InputRole},
};
use subspace_framework::lucide::IconName;

use crate::{Projects, Write, project_editor};

const DESCRIPTION_MIN_ROWS: usize = 5;
const DESCRIPTION_MAX_ROWS: usize = 10;
const MAX_NAME_LENGTH: usize = 200;
pub(super) const MAX_DESCRIPTION_LENGTH: usize = 50_000;

pub(super) enum Editor {
    Form(Box<Form>),
    Project(Box<project_editor::Form>),
    Delete {
        milestone: Milestone,
        focus: FocusHandle,
    },
}

pub(super) struct Form {
    milestone: Option<Milestone>,
    name: Entity<InputState>,
    date: Entity<InputState>,
    description: Entity<TextareaState>,
}

impl Form {
    fn new(milestone: Option<Milestone>, window: &mut Window, cx: &mut Context<Projects>) -> Self {
        let value = milestone.as_ref();
        let name = input(
            value.map(|row| row.name.as_str()).unwrap_or_default(),
            "",
            window,
            cx,
        );
        let date = input(
            value
                .and_then(|row| row.target_date.as_deref())
                .unwrap_or_default(),
            "YYYY-MM-DD",
            window,
            cx,
        );
        let description = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx);
            state.set_auto_grow(DESCRIPTION_MIN_ROWS, DESCRIPTION_MAX_ROWS, cx);
            state.set_value(
                value
                    .and_then(|row| row.description.as_deref())
                    .unwrap_or_default(),
                window,
                cx,
            );
            state
        });
        name.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            milestone,
            name,
            date,
            description,
        }
    }

    fn write(&self, cx: &App) -> Result<Write, &'static str> {
        let name = self.name.read(cx).value();
        let date = self.date.read(cx).value();
        let description = self.description.read(cx).value();
        let fields = fields_for(&name, &date, &description, subspace_api_client::backend())?;
        Ok(match self.milestone.clone() {
            Some(row) => Write::Change(row, MilestoneChange::Edit(fields)),
            None => Write::Add(fields),
        })
    }
}

fn fields_for(
    name: &str,
    date: &str,
    description: &str,
    backend: subspace_api_client::Backend,
) -> Result<MilestoneFields, &'static str> {
    let name = name.trim();
    let date = date.trim();
    if name.is_empty()
        || name.contains(['\n', '\r', '\0'])
        || name.encode_utf16().count() > MAX_NAME_LENGTH
    {
        return Err("Enter a name of 1 to 200 characters on one line.");
    }
    if date.is_empty() {
        if backend == subspace_api_client::Backend::Nett {
            return Err("Enter a target date.");
        }
    } else if !NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok_and(|day| day.to_string() == date)
    {
        return Err("Enter a valid target date in YYYY-MM-DD format.");
    }
    if description.contains('\0') || description.encode_utf16().count() > MAX_DESCRIPTION_LENGTH {
        return Err("Enter a description of at most 50,000 characters.");
    }
    Ok(MilestoneFields {
        name: name.to_owned(),
        target_date: (!date.is_empty()).then(|| date.to_owned()),
        description: (!description.is_empty()).then(|| description.to_owned()),
    })
}

pub(super) fn input(
    value: &str,
    hint: &str,
    window: &mut Window,
    cx: &mut Context<Projects>,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(hint.to_owned());
        state.set_value(value, window, cx);
        state
    })
}

impl Projects {
    pub(super) fn open_editor(
        &mut self,
        row: Option<Milestone>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.may_write() {
            self.error = None;
            self.editor = Some(Editor::Form(Box::new(Form::new(row, window, cx))));
            cx.notify();
        }
    }

    pub(super) fn confirm_remove(
        &mut self,
        row: Milestone,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.may_write() {
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            self.error = None;
            self.editor = Some(Editor::Delete {
                milestone: row,
                focus,
            });
            cx.notify();
        }
    }

    pub(super) fn dismiss_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.write_task.is_none() && self.editor.take().is_some() {
            if !self.blocked_write {
                self.error = None;
            }
            self.focus.focus(window, cx);
            self.reconcile_selection(cx);
            cx.notify();
        }
    }

    pub(super) fn submit_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let write = match self.editor.as_ref() {
            Some(Editor::Form(form)) => form.write(cx),
            Some(Editor::Project(form)) => form.write(cx).map(Write::Project),
            Some(Editor::Delete { milestone, .. }) => {
                Ok(Write::Change(milestone.clone(), MilestoneChange::Delete))
            }
            None => return,
        };
        match write {
            Ok(write) => self.send(write, window, cx),
            Err(error) => {
                self.error = Some(error.to_owned());
                cx.notify();
            }
        }
    }
}

pub(super) fn render(
    this: &Projects,
    tokens: &Tokens,
    cx: &mut Context<Projects>,
) -> Option<AnyElement> {
    let editor = this.editor.as_ref()?;
    let busy = this.write_task.is_some();
    let (title, action, icon, intent, body) = match editor {
        Editor::Project(form) => return Some(project_editor::render(this, form, tokens, cx)),
        Editor::Form(form) => (
            if form.milestone.is_some() {
                "Edit milestone"
            } else {
                "New milestone"
            },
            "Save milestone",
            IconName::Check,
            Intent::Primary,
            form_body(form, busy),
        ),
        Editor::Delete { milestone, .. } => (
            "Delete milestone",
            "Delete milestone",
            IconName::Trash,
            Intent::Reject,
            div()
                .child(format!(
                    "Delete “{}”? Issues linked to it will remain in the project.",
                    milestone.name
                ))
                .into_any_element(),
        ),
    };
    let mut frame = dialog::frame(tokens, DialogWidth::D600, title)
        .id("projects-editor")
        .test_support()
        .role(Role::Dialog)
        .aria_label(title);
    if let Editor::Delete { focus, .. } = editor {
        frame = frame.track_focus(focus);
    }
    Some(dialog::backdrop(
        tokens,
        frame
            .child(body)
            .children(
                this.error
                    .as_ref()
                    .map(|error| fields::field_error(tokens, "milestone-error", error)),
            )
            .children(initiative_warning(this, tokens))
            .children(
                busy.then(|| {
                    feedback::loading_line(tokens, "milestone-saving", "Saving milestone…")
                }),
            )
            .child(footer(this, (action, icon, intent), cx)),
    ))
}

pub(super) fn initiative_warning(this: &Projects, tokens: &Tokens) -> Option<Div> {
    let record = this.record.as_ref()?.as_ref().ok()?;
    (!this.in_scope(&record.project)).then(|| {
        fields::field_error(
            tokens,
            "milestone-initiative-changed",
            "The selected initiative changed. Copy any edits before closing this form.",
        )
    })
}

fn footer(
    this: &Projects,
    (label, icon, intent): (&'static str, IconName, Intent),
    cx: &Context<Projects>,
) -> Div {
    dialog::footer()
        .child(
            dialog_button("milestone-cancel", Intent::Neutral, None, "Cancel")
                .disabled(this.write_task.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.dismiss_editor(window, cx))),
        )
        .child(
            dialog_button("milestone-save", intent, Some(icon), label)
                .disabled(!this.may_write())
                .on_click(cx.listener(|this, _, window, cx| this.submit_editor(window, cx))),
        )
}

fn form_body(form: &Form, busy: bool) -> AnyElement {
    v_flex()
        .gap_3()
        .child(fields::form_field(
            "Name",
            fields::input(&form.name, InputRole::Configuration, "Name").disabled(busy),
        ))
        .child(fields::form_field(
            "Target date",
            fields::input(&form.date, InputRole::Configuration, "Target date").disabled(busy),
        ))
        .child(fields::form_field(
            "Description",
            fields::textarea(&form.description, "Description").disabled(busy),
        ))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::fields_for;
    use subspace_api_client::Backend;

    #[test]
    fn subspace_can_clear_a_date_and_nett_requires_one() {
        assert!(fields_for(" Plan ", "", "", Backend::Nett).is_err());
        let fields = fields_for(" Plan ", "", "", Backend::Subspace).unwrap();
        assert_eq!(fields.name, "Plan");
        assert_eq!(fields.target_date, None);
        assert_eq!(fields.description, None);
        assert!(fields_for("Plan", "2026-02-29", "", Backend::Subspace).is_err());
        assert!(fields_for("Plan", "2028-02-29", "", Backend::Nett).is_ok());
    }
}
