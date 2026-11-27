//! Retained issue draft construction and projections.
use super::target::{NewSubissue, chosen_project, editable_detail};
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
    model::Project,
};

pub(super) struct Opening {
    scope: IssueWriteScope,
    basis: Option<Box<IssueDetail>>,
    project: String,
    subissue_of: Option<NewSubissue>,
}

impl IssueEditor {
    pub(super) fn opening(issues: &Issues, edit: bool) -> Option<Opening> {
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
        Some(Opening {
            scope,
            basis: detail,
            project,
            subissue_of: None,
        })
    }

    pub(super) fn opening_subissue(issues: &Issues, detail: &IssueDetail) -> Option<Opening> {
        Some(Opening {
            scope: issues.hub.as_ref()?.issue_write_scope()?,
            basis: None,
            project: detail.fields.project_id.clone(),
            subissue_of: Some(NewSubissue {
                id: detail.fields.id.clone(),
                identifier: detail.fields.identifier.clone(),
            }),
        })
    }

    pub(super) fn new(
        issues: &Issues,
        Opening {
            scope,
            basis,
            project,
            subissue_of,
        }: Opening,
        window: &mut Window,
        cx: &mut Context<Issues>,
    ) -> Self {
        let initial = TextValues::initial(
            basis.as_deref(),
            subissue_of
                .as_ref()
                .map(|parent| parent.identifier.as_str()),
        );
        let properties = opening_properties(issues, &project, basis.as_deref());
        let title = input(&initial.title, "Issue title", window, cx);
        let summary = input(
            &initial.summary,
            "The problem this issue solves",
            window,
            cx,
        );
        let due = input(&initial.due, "YYYY-MM-DD", window, cx);
        let parent = input(&initial.parent, "Issue identifier, e.g. SUB-42", window, cx);
        let description = description_input(initial.description.clone(), window, cx);
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
            initial_text: initial,
            properties,
            catalogues: HashMap::new(),
            title,
            summary,
            summary_validation: false,
            description,
            scroll: ScrollHandle::new(),
            due,
            parent,
            subissue_of,
            parent_search: None,
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
                Phase::ConfirmingChildren(_)
                    | Phase::Unconfirmed
                    | Phase::Checked
                    | Phase::CheckFailed
                    | Phase::SeveralMatches
            )
    }

    pub(super) fn controls_locked(&self) -> bool {
        self.frozen() || self.discard
    }

    pub(super) fn changes(&self, cx: &App) -> IssueChanges {
        let mut changes = IssueChanges::default();
        TextValues::read(self, cx).collect(
            &self.initial_text,
            &mut changes,
            !self.editing_existing(),
        );
        self.properties.install(&mut changes);
        changes
    }

    pub(super) fn dirty(&self, cx: &App) -> bool {
        TextValues::read(self, cx) != self.initial_text || self.properties.has_edits()
    }

    pub(super) fn keep_edits(
        &mut self,
        projects: &[Project],
        issue: Box<IssueDetail>,
        window: &mut Window,
        cx: &mut Context<Issues>,
    ) {
        let original = self.initial_text.clone();
        let current = TextValues::read(self, cx);
        let latest = TextValues::from_detail(Some(&issue));
        self.replace_unchanged(window, cx, (original, current, latest.clone()));
        self.properties
            .install_baseline(projects, &issue.fields.project_id, Some(&issue));
        self.basis = Some(issue);
        self.initial_text = latest;
        self.elsewhere = None;
    }

    fn replace_unchanged(
        &self,
        window: &mut Window,
        cx: &mut Context<Issues>,
        (original, current, latest): (TextValues, TextValues, TextValues),
    ) {
        for (input, (before, current, latest)) in [
            (&self.title, (original.title, current.title, latest.title)),
            (&self.due, (original.due, current.due, latest.due)),
            (
                &self.parent,
                (original.parent, current.parent, latest.parent),
            ),
        ] {
            if before == current {
                input.update(cx, |input, cx| input.set_value(latest, window, cx));
            }
        }
        if original.summary == current.summary {
            self.summary
                .update(cx, |input, cx| input.set_value(latest.summary, window, cx));
        }
        if original.description == current.description {
            self.description.update(cx, |input, cx| {
                input.set_value(latest.description, window, cx)
            });
        }
    }
}

fn opening_properties(issues: &Issues, project: &str, basis: Option<&IssueDetail>) -> Properties {
    let mut properties = Properties::new(&issues.projects, project, basis);
    let Some(name) = basis.and_then(|_| issues.issue_view.snapshot.as_ref()?.milestone_name())
    else {
        return properties;
    };
    properties.name_milestone(name);
    properties
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
