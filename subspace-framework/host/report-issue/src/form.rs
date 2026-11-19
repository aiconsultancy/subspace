use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::select::SelectState;
use gpui_kit::{App, AppContext as _, Entity, Window};
use subspace_client_github::issue_report::{Diagnostics, Draft, IssueType};

const DESCRIPTION_ROWS: usize = 3;
const STEPS_ROWS: usize = 10;
const PREVIEW_ROWS: usize = 12;

pub(super) struct Form {
    pub title: Entity<InputState>,
    pub description: Entity<TextareaState>,
    pub steps: Entity<TextareaState>,
    pub kind: Entity<SelectState<Vec<String>>>,
    pub preview_title: Entity<TextareaState>,
    pub preview_body: Entity<TextareaState>,
}

impl Form {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            title: cx.new(|cx| InputState::new(window, cx)),
            description: textarea(DESCRIPTION_ROWS, window, cx),
            steps: textarea(STEPS_ROWS, window, cx),
            kind: cx.new(|cx| {
                SelectState::new(
                    ["Bug", "UX", "Feature request", "Other"]
                        .map(String::from)
                        .to_vec(),
                    None,
                    window,
                    cx,
                )
            }),
            preview_title: textarea(1, window, cx),
            preview_body: textarea(PREVIEW_ROWS, window, cx),
        }
    }

    pub fn draft(&self, cx: &App) -> Draft {
        Draft::new(self.description.read(cx).value().to_string())
            .with_title(self.title.read(cx).value().to_string())
            .with_steps(self.steps.read(cx).value().to_string())
            .with_issue_type(issue_type(self.kind.read(cx).selected_value()))
    }

    pub fn values(&self, cx: &App) -> [String; 4] {
        let draft = self.draft(cx);
        [
            draft.title().into(),
            draft.description().into(),
            draft.steps().into(),
            draft.issue_type().name().into(),
        ]
    }

    pub fn set_preview(&self, title: &str, body: &str, window: &mut Window, cx: &mut App) {
        self.preview_title
            .update(cx, |input, cx| input.set_value(title, window, cx));
        self.preview_body
            .update(cx, |input, cx| input.set_value(body, window, cx));
    }
}

fn textarea(rows: usize, window: &mut Window, cx: &mut App) -> Entity<TextareaState> {
    cx.new(|cx| {
        let mut state = TextareaState::new(window, cx);
        state.set_auto_grow(rows, rows, cx);
        state
    })
}

fn issue_type(value: Option<&String>) -> IssueType {
    match value.map(String::as_str) {
        Some("Bug") => IssueType::Bug,
        Some("UX") => IssueType::Ux,
        Some("Feature request") => IssueType::FeatureRequest,
        Some("Other") => IssueType::Other,
        _ => IssueType::Unknown,
    }
}

pub(super) fn diagnostic_fields(diagnostics: &Diagnostics) -> Vec<(&str, &str)> {
    let fields = [
        ("Product", Some(diagnostics.product_id())),
        ("App version", Some(diagnostics.app_version())),
        ("Build channel", diagnostics.channel()),
        ("Commit", diagnostics.commit()),
        ("OS", diagnostics.os()),
        ("Architecture", diagnostics.architecture()),
        ("Active board", diagnostics.active_board()),
        ("Theme", diagnostics.theme_id()),
        ("Icon size", diagnostics.icon_size()),
    ];
    let paths = diagnostics
        .paths()
        .into_iter()
        .flat_map(|paths| paths.fields());
    fields
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| (label, value)))
        .chain(paths)
        .collect()
}

#[cfg(test)]
pub(super) fn diagnostic_text(diagnostics: &Diagnostics) -> String {
    diagnostic_fields(diagnostics)
        .into_iter()
        .map(|(label, value)| format!("{label}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}
