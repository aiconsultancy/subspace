//! Retained issue draft construction and projections.
use super::target::{chosen_project, editable_detail};
use super::{
    DESCRIPTION_MARKDOWN, IssueDetail, IssueEditor, IssueWriteScope, Issues, Phase, Properties,
    TextValues, input, input_changed,
};
use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{App, AppContext as _, Context, Entity, ScrollHandle, Window};
use std::collections::HashMap;
use subspace_api_client::{
    hub::HubSnapshot,
    issue_writes::{CreateKey, IssueChanges},
};

impl IssueEditor {
    pub(super) fn opening(
        issues: &Issues,
        edit: bool,
    ) -> Option<(IssueWriteScope, Option<Box<IssueDetail>>, String)> {
        let scope = issues.hub.as_ref()?.issue_write_scope()?;
        let detail = if edit {
            Some(Box::new(editable_detail(issues)?.clone()))
        } else {
            None
        };
        let project = detail
            .as_ref()
            .map(|detail| detail.fields.project_id.clone())
            .or_else(|| chosen_project(issues))?;
        Some((scope, detail, project))
    }

    pub(super) fn new(
        issues: &Issues,
        (scope, basis, project): (IssueWriteScope, Option<Box<IssueDetail>>, String),
        window: &mut Window,
        cx: &mut Context<Issues>,
    ) -> Self {
        let initial = TextValues::from_detail(basis.as_deref());
        let mut properties = Properties::new(&issues.projects, &project, basis.as_deref());
        if let Some(name) = issues
            .issue_view
            .snapshot
            .as_ref()
            .and_then(|open| open.milestone_name())
        {
            properties.name_milestone(name);
        }
        let title = input(&initial.title, "Issue title", window, cx);
        let summary = input(
            &initial.summary,
            "The problem this issue solves",
            window,
            cx,
        );
        let due = input(&initial.due, "YYYY-MM-DD", window, cx);
        let parent = input(&initial.parent, "Issue identifier, e.g. SUB-42", window, cx);
        let description = description_input(initial.description, window, cx);
        let mut subscriptions: Vec<_> = [&title, &summary, &due, &parent]
            .into_iter()
            .map(|state| cx.subscribe_in(state, window, input_changed))
            .collect();
        subscriptions.push(cx.subscribe(&description, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.editor_changed(cx);
            }
        }));
        let previous_focus = window.focused(cx);
        title.update(cx, |input, cx| input.focus(window, cx));
        Self {
            inline: None,
            scope,
            key: CreateKey::new(),
            basis,
            properties,
            catalogues: HashMap::new(),
            title,
            summary,
            summary_validation: false,
            description,
            scroll: ScrollHandle::new(),
            due,
            parent,
            phase: Phase::default(),
            elsewhere: None,
            discard: false,
            previous_focus,
            _subscriptions: subscriptions,
        }
    }

    /// Keeps a newer version of the edited issue for the person to reload;
    /// the draft itself never changes under them. True when it is new.
    pub(super) fn follow_elsewhere(&mut self, snapshot: &HubSnapshot) -> bool {
        let Some(basis) = self.basis.as_ref() else {
            return false;
        };
        let held = self
            .elsewhere
            .as_ref()
            .map_or(basis.fields.version, |newer| newer.fields.version);
        let Some(newer) = snapshot
            .issue()
            .and_then(|open| open.detail())
            .filter(|detail| detail.fields.id == basis.fields.id && detail.fields.version > held)
        else {
            return false;
        };
        self.elsewhere = Some(Box::new(newer.clone()));
        true
    }

    pub(super) fn follow_milestone(&mut self, snapshot: &HubSnapshot) {
        let Some((basis, open)) = self.basis.as_ref().zip(snapshot.issue()) else {
            return;
        };
        if open.detail().is_some_and(|detail| {
            detail.fields.id == basis.fields.id
                && detail.fields.milestone_id == basis.fields.milestone_id
        }) && let Some(name) = open.milestone_name()
        {
            self.properties.name_milestone(name);
        }
    }

    pub(super) fn editing_existing(&self) -> bool {
        self.basis.is_some()
    }

    pub(crate) fn busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Sending | Phase::LoadingConflict | Phase::Checking
        )
    }

    pub(super) fn frozen(&self) -> bool {
        self.busy()
            || matches!(
                self.phase,
                Phase::Unconfirmed | Phase::Checked | Phase::CheckFailed | Phase::SeveralMatches
            )
    }

    pub(super) fn controls_locked(&self) -> bool {
        self.frozen() || self.discard
    }

    pub(super) fn changes(&self, cx: &App) -> IssueChanges {
        let mut changes = IssueChanges::default();
        TextValues::read(self, cx).collect(
            &TextValues::from_detail(self.basis.as_deref()),
            &mut changes,
            !self.editing_existing(),
        );
        self.properties.install(&mut changes);
        changes
    }

    pub(super) fn dirty(&self, cx: &App) -> bool {
        let text = TextValues::read(self, cx);
        let before = TextValues::from_detail(self.basis.as_deref());
        text != before || self.properties.has_edits()
    }
}

fn description_input(
    value: String,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> Entity<EditorState> {
    cx.new(|cx| {
        EditorState::new(window, cx)
            .language(DESCRIPTION_MARKDOWN)
            .line_number(false)
            .folding(false)
            .indent_guides(false)
            .auto_close(false)
            .smart_indent(false)
            .placeholder("Write an issue description")
            .default_value(value)
    })
}
