//! A versioned editor for the project shown in Overview.

mod controls;

use chrono::NaiveDate;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::select::{SearchableVec, SelectItem, SelectState};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement as _, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    TestSupportExt as _, Window,
};
use subspace_api_client::{
    Backend, backend,
    projects::{PlanningProject, ProjectFields},
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, dialog_button},
    dialog::{self, DialogWidth},
    feedback, fields,
};
use subspace_framework::lucide::IconName;

use crate::{Projects, editor};

pub(super) const STAGES: [(&str, &str); 6] = [
    ("backlog", "Backlog"),
    ("planned", "Planned"),
    ("started", "Started"),
    ("paused", "Paused"),
    ("completed", "Completed"),
    ("canceled", "Cancelled"),
];

const PRIORITIES: [(&str, &str); 5] = [
    ("0", "None"),
    ("1", "Urgent"),
    ("2", "High"),
    ("3", "Normal"),
    ("4", "Low"),
];
const DESCRIPTION_MIN_ROWS: usize = 6;
const DESCRIPTION_MAX_ROWS: usize = 12;
const MAX_SUBSPACE_NAME: usize = 120;
const MAX_NETT_NAME: usize = 200;

#[derive(Clone)]
struct Choice {
    key: String,
    label: SharedString,
}

impl SelectItem for Choice {
    type Value = String;

    fn value(&self) -> &Self::Value {
        &self.key
    }

    fn title(&self) -> SharedString {
        self.label.clone()
    }
}

type Choices = Entity<SelectState<SearchableVec<Choice>>>;

pub(super) struct Form {
    before: PlanningProject,
    version: u32,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    status: Choices,
    priority: Choices,
    start: Entity<InputState>,
    target: Entity<InputState>,
}

impl Form {
    fn new(
        project: &PlanningProject,
        version: u32,
        window: &mut Window,
        cx: &mut Context<Projects>,
    ) -> Self {
        let name = editor::input(&project.name, "", window, cx);
        let description = cx.new(|cx| {
            let mut input = TextareaState::new(window, cx);
            input.set_auto_grow(DESCRIPTION_MIN_ROWS, DESCRIPTION_MAX_ROWS, cx);
            input.set_value(
                project.description.as_deref().unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        name.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            before: project.clone(),
            version,
            name,
            description,
            status: select(&STAGES, &project.status, window, cx),
            priority: select(&PRIORITIES, &project.priority.to_string(), window, cx),
            start: editor::input(
                project.start_date.as_deref().unwrap_or_default(),
                "YYYY-MM-DD",
                window,
                cx,
            ),
            target: editor::input(
                project.target_date.as_deref().unwrap_or_default(),
                "YYYY-MM-DD",
                window,
                cx,
            ),
        }
    }

    pub(super) fn write(&self, cx: &App) -> Result<ProjectFields, &'static str> {
        let name = self.name.read(cx).value();
        let name = name.trim();
        let name = (name != self.before.name.trim()).then_some(name);
        if let Some(name) = name {
            validate_name(name, backend())?;
        }
        let description = self.description.read(cx).value();
        let description_changed =
            description.as_str() != self.before.description.as_deref().unwrap_or_default();
        if description_changed
            && (description.contains('\0')
                || description.encode_utf16().count() > editor::MAX_DESCRIPTION_LENGTH)
        {
            return Err("Enter a description of at most 50,000 characters.");
        }
        let status = self
            .status
            .read(cx)
            .selected_value()
            .ok_or("Choose a status.")?;
        let priority = self
            .priority
            .read(cx)
            .selected_value()
            .and_then(|priority| priority.parse().ok())
            .ok_or("Choose a priority.")?;
        let start = parse_date(
            &self.start.read(cx).value(),
            "Enter a valid start date in YYYY-MM-DD format.",
        )?;
        let target = parse_date(
            &self.target.read(cx).value(),
            "Enter a valid target date in YYYY-MM-DD format.",
        )?;
        if let (Some(start), Some(target)) = (start, target)
            && start > target
        {
            return Err("The target date must be on or after the start date.");
        }
        let start_date = start.map(|day| day.to_string());
        let target_date = target.map(|day| day.to_string());
        Ok(ProjectFields {
            version: self.version,
            // The Subspace PUT schema requires the current name on every edit.
            name: name
                .map(str::to_owned)
                .or_else(|| (backend() == Backend::Subspace).then(|| self.before.name.clone())),
            description: description_changed
                .then(|| (!description.is_empty()).then(|| description.to_string())),
            status: (status != &self.before.status).then(|| status.clone()),
            priority: (priority != self.before.priority).then_some(priority),
            start_date: (start_date != self.before.start_date).then_some(start_date),
            target_date: (target_date != self.before.target_date).then_some(target_date),
        })
    }
}

fn validate_name(name: &str, backend: Backend) -> Result<(), &'static str> {
    let (max, message) = match backend {
        Backend::Subspace => (
            MAX_SUBSPACE_NAME,
            "Enter a project name of 1 to 120 characters on one line.",
        ),
        Backend::Nett => (
            MAX_NETT_NAME,
            "Enter a project name of 1 to 200 characters on one line.",
        ),
    };
    if name.is_empty() || name.chars().any(char::is_control) || name.encode_utf16().count() > max {
        return Err(message);
    }
    Ok(())
}

fn parse_date(value: &str, invalid: &'static str) -> Result<Option<NaiveDate>, &'static str> {
    let text = value.trim();
    if text.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .filter(|date| date.to_string() == text)
        .map(Some)
        .ok_or(invalid)
}

fn select(
    options: &[(&str, &str)],
    selected: &str,
    window: &mut Window,
    cx: &mut Context<Projects>,
) -> Choices {
    let choices: Vec<_> = options
        .iter()
        .map(|(key, label)| Choice {
            key: (*key).into(),
            label: SharedString::from((*label).to_owned()),
        })
        .collect();
    cx.new(|cx| {
        let mut state = SelectState::new(SearchableVec::new(choices), None, window, cx);
        state.set_selected_value(&selected.to_owned(), window, cx);
        state
    })
}

// qual:allow(coupling, oi) reason: "The project editor binds the selected API record to the Space's shared dialog."
impl Projects {
    pub(super) fn open_project_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.editable_project() else {
            return;
        };
        let Some(version) = project.version else {
            return;
        };
        let form = Form::new(project, version, window, cx);
        self.error = None;
        self.editor = Some(editor::Editor::Project(Box::new(form)));
        cx.notify();
    }
}

pub(super) fn render(
    this: &Projects,
    form: &Form,
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> AnyElement {
    let busy = this.write_task.is_some();
    dialog::backdrop(
        tokens,
        dialog::frame(tokens, DialogWidth::D600, "Edit project")
            .id("project-editor")
            .test_support()
            .role(Role::Dialog)
            .aria_label("Edit project")
            .child(controls::form_body(form, busy))
            .children(
                this.error
                    .as_ref()
                    .map(|error| fields::field_error(tokens, "project-error", error)),
            )
            .children(editor::initiative_warning(this, tokens))
            .children(
                busy.then(|| feedback::loading_line(tokens, "project-saving", "Saving project…")),
            )
            .child(
                dialog::footer()
                    .child(
                        dialog_button("project-cancel", Intent::Neutral, None, "Cancel")
                            .disabled(busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_editor(window, cx)),
                            ),
                    )
                    .child(
                        dialog_button(
                            "project-save",
                            Intent::Primary,
                            Some(IconName::Check),
                            "Save changes",
                        )
                        .disabled(this.editable_project().is_none())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.submit_editor(window, cx)),
                        ),
                    ),
            ),
    )
    .into_any_element()
}
