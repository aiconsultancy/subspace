use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, AppContext as _, Context, Entity, Window};
use subspace_api_client::{
    issue_writes::{IssueChanges, IssueProperty},
    model::IssueDetail,
};

use super::{IssueEditor, Issues};

pub(super) fn input(
    value: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(placeholder.to_owned());
        state.set_value(value, window, cx);
        state
    })
}

pub(super) fn input_changed(
    issues: &mut Issues,
    input: &Entity<InputState>,
    event: &InputEvent,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    match event {
        InputEvent::Change => {
            if let Some(editor) = &mut issues.editor
                && input == &editor.summary
            {
                editor.summary_validation = true;
            }
            issues.editor_changed(cx);
        }
        InputEvent::PressEnter { .. } => issues.save_issue_editor(window, cx),
        _ => {}
    }
}

#[derive(Default, PartialEq, Eq)]
pub(super) struct TextValues {
    pub(super) title: String,
    pub(super) summary: String,
    pub(super) description: String,
    pub(super) due: String,
    pub(super) parent: String,
}

impl TextValues {
    pub(super) fn read(editor: &IssueEditor, cx: &App) -> Self {
        Self {
            title: editor.title.read(cx).value().to_string(),
            summary: editor.summary.read(cx).value().to_string(),
            description: editor.description.read(cx).value().to_string(),
            due: editor.due.read(cx).value().trim().to_owned(),
            parent: editor.parent.read(cx).value().trim().to_owned(),
        }
    }

    pub(super) fn from_detail(detail: Option<&IssueDetail>) -> Self {
        detail.map_or_else(Self::default, |detail| Self {
            title: detail.fields.title.clone(),
            summary: detail.fields.summary.clone().unwrap_or_default(),
            description: detail.description.clone().unwrap_or_default(),
            due: detail.fields.due_date.clone().unwrap_or_default(),
            parent: detail
                .parent
                .as_ref()
                .map(|parent| parent.identifier.clone())
                .or_else(|| detail.fields.parent_id.clone())
                .unwrap_or_default(),
        })
    }

    pub(super) fn collect(self, basis: &Self, changes: &mut IssueChanges, create: bool) {
        if create || self.title != basis.title {
            changes.set(IssueProperty::Title(self.title));
        }
        if create || self.summary != basis.summary {
            changes.set(IssueProperty::Summary(self.summary));
        }
        if self.description != basis.description {
            changes.set(IssueProperty::Description(nonempty(self.description)));
        }
        if self.due != basis.due {
            changes.set(IssueProperty::DueDate(nonempty(self.due)));
        }
        if self.parent != basis.parent {
            changes.set(IssueProperty::Parent(nonempty(self.parent)));
        }
    }
}

fn nonempty(text: String) -> Option<String> {
    (!text.trim().is_empty()).then_some(text)
}
