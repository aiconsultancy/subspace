//! The same single-line summary control in full and inline issue editing.
use super::super::IssueEditor;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};
use subspace_api_client::issue_writes::{SUMMARY_LIMIT, summary_length, valid_summary};
use subspace_framework::dashboard::{
    Tokens,
    fields::{self, InputRole},
};

pub(super) fn input(editor: &IssueEditor, role: InputRole, locked: bool, cx: &App) -> Div {
    let tokens = Tokens::current(cx);
    let count = summary_length(&editor.summary.read(cx).value());
    v_flex()
        .min_w_0()
        .gap_1()
        .child(
            fields::input(&editor.summary, role, "Summary")
                .id("issue-edit-summary")
                .disabled(locked),
        )
        .child(fields::note(
            &tokens,
            format!("{count}/{SUMMARY_LIMIT} characters"),
        ))
        .children(validation(editor, &tokens, cx))
}

fn validation(editor: &IssueEditor, tokens: &Tokens, cx: &App) -> Option<AnyElement> {
    if !editor.summary_validation {
        return None;
    }
    if valid_summary(&editor.summary.read(cx).value()) {
        return None;
    }
    let message = editor.summary_error();
    Some(
        fields::field_error(tokens, "issue-summary-error-text", &message)
            .id("issue-summary-error")
            .test_support()
            .aria_label(message)
            .into_any_element(),
    )
}

impl IssueEditor {
    pub(in crate::editing) fn summary_error(&self) -> String {
        if self
            .basis
            .as_ref()
            .is_some_and(|issue| issue.fields.summary.is_some())
        {
            subspace_api_client::issue_writes::IssueSaveError::Summary.to_string()
        } else {
            "Add a summary of the problem this issue solves (up to 300 characters).".into()
        }
    }
}
