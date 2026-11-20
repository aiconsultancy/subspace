//! The page and the full editor share one draft and write boundary.
use super::{Field, Issues};
use gpui_kit::{Context, Window};

#[derive(Clone, Copy)]
pub(crate) enum EditTarget {
    Property(Field),
    Description,
    Summary,
    ProjectMove,
    DueDate,
    Parent,
}

impl EditTarget {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Property(field) => field.facts().1,
            Self::DueDate => "Due date",
            Self::Parent => "Parent",
            Self::ProjectMove => "Project",
            Self::Description => "Description",
            Self::Summary => "Summary",
        }
    }
    pub(crate) fn property(label: &str, tracker: bool) -> Option<Self> {
        Some(match label {
            "State" if !tracker => Self::Property(Field::State),
            "Project"
                if !crate::pickers::teams() && subspace_api_client::capabilities().issue_moves =>
            {
                Self::ProjectMove
            }
            "Team"
                if crate::pickers::teams() && subspace_api_client::capabilities().issue_moves =>
            {
                Self::ProjectMove
            }
            "Priority" => Self::Property(Field::Priority),
            "Assignee" => Self::Property(Field::Assignee),
            "Labels" => Self::Property(Field::Labels),
            "Due date" => Self::DueDate,
            "Milestone" => Self::Property(Field::Milestone),
            "Parent" if !tracker => Self::Parent,
            "Type" => Self::Property(Field::Type),
            "Estimate" if !tracker => Self::Property(Field::Estimate),
            "Cycle" if !tracker => Self::Property(Field::Cycle),
            _ => return None,
        })
    }
}

// qual:allow(coupling, oi) reason: "Field entry points coordinate the retained draft, input focus and native picker anchors in the owning GPUI entity."
impl Issues {
    pub(crate) fn edit_issue_field(
        &mut self,
        target: EditTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_edit_issue() || self.editor.is_some() {
            return;
        }
        if matches!(target, EditTarget::ProjectMove) {
            self.open_project_move(window, cx);
            return;
        }
        self.open_issue_editor(true, window, cx);
        if let Some(editor) = &mut self.editor
            && !matches!(target, EditTarget::Description)
        {
            editor.inline = Some(target);
        }
        let Some(editor) = &self.editor else { return };
        match target {
            EditTarget::ProjectMove => {}
            EditTarget::Summary => editor
                .summary
                .update(cx, |input, cx| input.focus(window, cx)),
            EditTarget::Description => editor
                .description
                .update(cx, |input, cx| input.focus(window, cx)),
            EditTarget::DueDate => editor.due.update(cx, |input, cx| input.focus(window, cx)),
            EditTarget::Parent => editor
                .parent
                .update(cx, |input, cx| input.focus(window, cx)),
            // The shared anchor defers popup layout until this frame's trigger
            // bounds are ready, so focus can transfer in the click itself.
            EditTarget::Property(field) => super::view::open_property(self, field, window, cx),
        }
    }
}
