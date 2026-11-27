use subspace_api_client::issue_writes::IssueChoice;
use subspace_framework::dashboard::menu::MenuOption;

use super::{Field, IssueEditor, Issues};
use crate::editing::properties::{priority_choice, project_choice};

pub(super) fn options(board: &Issues, editor: &IssueEditor, field: Field) -> Vec<MenuOption> {
    let selected = editor.properties.selected(field);
    let mut all = available(board, editor, field);
    if matches!(field, Field::Labels | Field::Milestone | Field::Cycle) {
        let removed: Vec<_> = selected
            .iter()
            .filter(|choice| !all.iter().any(|item| item.id() == choice.id()))
            .cloned()
            .collect();
        all.extend(removed);
    }
    let mut options = Vec::new();
    if field.can_clear() {
        options.push(MenuOption::new("", selection(field, &[])).checked(selected.is_empty()));
    }
    options.extend(all.into_iter().map(|choice| {
        MenuOption::new(choice.id().to_owned(), choice.name().to_owned())
            .checked(selected.iter().any(|item| item.id() == choice.id()))
    }));
    options
}

pub(super) fn available(board: &Issues, editor: &IssueEditor, field: Field) -> Vec<IssueChoice> {
    match field {
        Field::Project => board
            .projects
            .iter()
            .filter(|project| project.archived_at.is_none())
            .map(project_choice)
            .collect(),
        Field::Type => vec![
            IssueChoice::new("issue", "Issue"),
            IssueChoice::new("tracker", "Tracker"),
        ],
        Field::Priority => (0..=i32::from(crate::filters::MAX_PRIORITY))
            .map(priority_choice)
            .collect(),
        _ => field
            .source(&editor.properties, &board.projects)
            .and_then(|source| editor.catalogues.get(&source))
            .into_iter()
            .flatten()
            .filter(|choice| field != Field::Owner || choice.human_id().is_some())
            .cloned()
            .collect(),
    }
}

pub(super) fn selection(field: Field, selected: &[IssueChoice]) -> String {
    if selected.is_empty() {
        return field.empty_name().to_owned();
    }
    if selected.len() > 1 {
        return format!("{} labels", selected.len());
    }
    selected[0].name().to_owned()
}

pub(super) fn display_selection(board: &Issues, editor: &IssueEditor, field: Field) -> String {
    let selected = editor.properties.selected(field);
    if ![Field::Estimate, Field::Milestone, Field::Cycle].contains(&field) {
        return selection(field, selected);
    }
    let named = field
        .source(&editor.properties, &board.projects)
        .and_then(|source| editor.catalogues.get(&source))
        .and_then(|choices| {
            choices
                .iter()
                .find(|choice| selected.iter().any(|s| s.id() == choice.id()))
        });
    named.map_or_else(
        || selection(field, selected),
        |choice| choice.name().to_owned(),
    )
}
