//! The retained inputs and the fields a person actually changed.

use super::evidence::Evidence;
use super::picker::Picker;

mod rebase;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{App, AppContext as _, Context, Entity, Focusable as _, Window};
use subspace_api_client::decision_writes::DecisionChanges;
use subspace_api_client::model::{DecisionDetail, Project};

use crate::Decisions;
use crate::model::ProjectChoice;

#[derive(Default)]
struct Text {
    title: String,
    date: String,
    decision: String,
    context: String,
    why: String,
    implications: String,
    alternatives: String,
    tags: String,
    issues: String,
    supersedes: String,
}

impl Text {
    // qual:allow(dry, boilerplate) reason: "The retained edit needs an owned copy of the server’s initial fields to write only the changed fields."
    fn from_detail(detail: &DecisionDetail) -> Self {
        Self {
            title: detail.title.clone(),
            date: detail.decided_on.clone(),
            decision: detail.decision.clone(),
            context: detail.context.clone(),
            why: detail.why.clone(),
            implications: detail.implications.clone(),
            alternatives: detail.alternatives.clone(),
            tags: detail.tags.join(", "),
            issues: issue_identifiers(detail),
            supersedes: detail.supersedes_id.clone().unwrap_or_default(),
        }
    }

    fn changes(self, basis: Option<&Self>) -> (DecisionChanges, Option<Vec<String>>) {
        let empty = Self::default();
        let prior = basis.unwrap_or(&empty);
        let change =
            |value: String, previous: &str| (value != previous).then(|| value.trim().to_owned());
        let required = |value: String, previous: &str| {
            if basis.is_none() {
                Some(value.trim().to_owned())
            } else {
                change(value, previous)
            }
        };
        let changes = DecisionChanges {
            title: required(self.title, &prior.title),
            decided_on: required(self.date, &prior.date),
            decision: required(self.decision, &prior.decision),
            context: change(self.context, &prior.context),
            why: change(self.why, &prior.why),
            implications: change(self.implications, &prior.implications),
            alternatives: change(self.alternatives, &prior.alternatives),
            tags: change(self.tags, &prior.tags).map(|text| tag_names(&text)),
            issue_ids: None,
            supersedes_id: change(self.supersedes, &prior.supersedes)
                .map(|id| (!id.is_empty()).then_some(id)),
        };
        (
            changes,
            change(self.issues, &prior.issues).map(|text| issue_keys(&text)),
        )
    }
}

fn issue_keys(text: &str) -> Vec<String> {
    text.split(|character: char| character == ',' || character.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_uppercase)
        .collect()
}

fn tag_names(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) struct Draft {
    original: Option<Text>,
    pub(super) details_open: bool,
    pub(super) evidence_open: bool,
    pub(super) project: Option<Picker>,
    pub(super) fixed_project: Option<(String, String)>,
    pub(super) title: Entity<InputState>,
    pub(super) date: Entity<InputState>,
    pub(super) decision: Entity<TextareaState>,
    pub(super) context: Entity<TextareaState>,
    pub(super) why: Entity<TextareaState>,
    pub(super) implications: Entity<TextareaState>,
    pub(super) alternatives: Entity<TextareaState>,
    pub(super) tags: Entity<InputState>,
    pub(super) issues: Entity<InputState>,
    pub(super) hidden_issues: bool,
    pub(super) supersedes: Entity<InputState>,
    pub(super) predecessor_title: Option<String>,
    pub(super) evidence: Option<Evidence>,
}

impl Draft {
    pub(super) fn new(
        board: &Decisions,
        editing: Option<&DecisionDetail>,
        replacing: Option<&DecisionDetail>,
        window: &mut Window,
        cx: &mut Context<Decisions>,
    ) -> Option<Self> {
        let original = editing.map(Text::from_detail);
        let fresh = Text {
            date: chrono::Local::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string(),
            supersedes: replacing
                .map(|detail| detail.id.clone())
                .unwrap_or_default(),
            ..Text::default()
        };
        let value = original.as_ref().unwrap_or(&fresh);
        let project = editing
            .or(replacing)
            .map(|detail| (detail.project_id.clone(), detail.project_key.clone()));
        let picker = project.is_none().then(|| project_picker(board, window, cx));
        let evidence = if editing.is_none() {
            Some(Evidence::new(
                board.session.account.as_ref()?,
                &board.session.human_makers,
                window,
                cx,
            ))
        } else {
            None
        };
        let title = input(&value.title, "Decision title", window, cx);
        title.read(cx).focus_handle(cx).focus(window, cx);
        Some(Self {
            details_open: false,
            evidence_open: false,
            project: picker,
            fixed_project: project,
            title,
            date: input(&value.date, "YYYY-MM-DD", window, cx),
            decision: paragraph(&value.decision, "What was decided?", window, cx),
            context: paragraph(&value.context, "", window, cx),
            why: paragraph(&value.why, "", window, cx),
            implications: paragraph(&value.implications, "", window, cx),
            alternatives: paragraph(&value.alternatives, "", window, cx),
            tags: input(&value.tags, "e.g. architecture, release", window, cx),
            issues: input(
                &value.issues,
                "Issue identifiers, e.g. EX-42, EX-43",
                window,
                cx,
            ),
            hidden_issues: editing.is_some_and(hidden_issues),
            supersedes: input(&value.supersedes, "Accepted decision ID", window, cx),
            predecessor_title: replacing.map(|detail| detail.title.clone()),
            evidence,
            original,
        })
    }

    pub(super) fn changes(&self, cx: &App) -> (DecisionChanges, Option<Vec<String>>) {
        let value = |field: &Entity<InputState>| field.read(cx).value().to_string();
        let paragraph = |field: &Entity<TextareaState>| field.read(cx).value().to_string();
        let (changes, issues) = Text {
            title: value(&self.title),
            date: value(&self.date),
            decision: paragraph(&self.decision),
            context: paragraph(&self.context),
            why: paragraph(&self.why),
            implications: paragraph(&self.implications),
            alternatives: paragraph(&self.alternatives),
            tags: value(&self.tags),
            issues: value(&self.issues),
            supersedes: value(&self.supersedes),
        }
        .changes(self.original.as_ref());
        (changes, issues.filter(|_| !self.hidden_issues))
    }

    pub(super) fn project_id(&self, cx: &App) -> Option<String> {
        if let Some((id, _)) = &self.fixed_project {
            return Some(id.clone());
        }
        self.project.as_ref()?.value(cx).map(str::to_owned)
    }
}

pub(super) fn issue_identifiers(detail: &DecisionDetail) -> String {
    detail
        .issues
        .items
        .iter()
        .map(|issue| issue.identifier.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn hidden_issues(detail: &DecisionDetail) -> bool {
    usize::try_from(detail.issues.count).is_ok_and(|count| count > detail.issues.items.len())
}

fn project_picker(board: &Decisions, window: &mut Window, cx: &mut Context<Decisions>) -> Picker {
    let active = |project: &&Project| project.archived_at.is_none();
    let selected = match &board.choice.project {
        ProjectChoice::Project(key) => board
            .session
            .projects
            .iter()
            .filter(active)
            .find(|project| &project.key == key)
            .map(|project| project.id.as_str()),
        ProjectChoice::All => None,
    };
    let choices = board
        .session
        .projects
        .iter()
        .filter(active)
        .map(|project| {
            (
                format!("{} · {}", project.key, project.name),
                project.id.clone(),
            )
        })
        .collect();
    Picker::new(choices, selected, window, cx)
}

pub(super) fn input(
    value: &str,
    hint: &str,
    window: &mut Window,
    cx: &mut Context<Decisions>,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(hint.to_owned());
        state.set_value(value, window, cx);
        state
    })
}

pub(super) fn paragraph(
    value: &str,
    hint: &str,
    window: &mut Window,
    cx: &mut Context<Decisions>,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        let mut state = TextareaState::new(window, cx).placeholder(hint.to_owned());
        state.set_auto_grow(2, 5, cx);
        state.set_value(value, window, cx);
        state
    })
}
