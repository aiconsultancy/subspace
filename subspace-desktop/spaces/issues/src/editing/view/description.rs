//! The shared Markdown editor in the issue form.

use crate::{editing::IssueEditor, view::reveal_focus};
use gpui_kit::{InteractiveElement as _, IntoElement, ParentElement as _, div, rems};
use subspace_framework::dashboard::markdown_editor::MarkdownField;

const HEIGHT_REMS: f32 = 24.;

pub(super) fn render(editor: &IssueEditor, locked: bool) -> impl IntoElement {
    div()
        .id("issue-edit-description")
        .child(reveal_focus::scope(
            "description-input",
            &editor.scroll,
            MarkdownField::new(&editor.description, "issue-description-mode", "Description")
                .h(rems(HEIGHT_REMS))
                .readonly(locked),
        ))
}
