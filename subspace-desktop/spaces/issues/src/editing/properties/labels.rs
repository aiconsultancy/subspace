//! Keep the user's label toggles when applying a newer issue version.

use std::collections::HashMap;
use subspace_api_client::issue_writes::IssueChoice;

use super::Field;

pub(super) fn keep_edits(
    previous: &[IssueChoice],
    latest: &[IssueChoice],
    edits: &mut HashMap<Field, Vec<IssueChoice>>,
) {
    let Some(selected) = edits.remove(&Field::Labels) else {
        return;
    };
    let merged = merge(previous, &selected, latest);
    if !same_ids(&merged, latest) {
        edits.insert(Field::Labels, merged);
    }
}

fn merge(
    previous: &[IssueChoice],
    selected: &[IssueChoice],
    latest: &[IssueChoice],
) -> Vec<IssueChoice> {
    // Retain the latest labels unless the user explicitly removed one.
    let kept = latest
        .iter()
        .filter(|label| !contains(previous, label) || contains(selected, label));
    // Add only labels the user introduced and the latest issue does not have.
    let added = selected
        .iter()
        .filter(|label| !contains(previous, label) && !contains(latest, label));
    kept.chain(added).cloned().collect()
}

fn contains(labels: &[IssueChoice], label: &IssueChoice) -> bool {
    labels.iter().any(|other| other.id() == label.id())
}

pub(super) fn same_ids(left: &[IssueChoice], right: &[IssueChoice]) -> bool {
    left.len() == right.len() && left.iter().all(|value| contains(right, value))
}

pub(super) fn toggle(selected: &[IssueChoice], choice: Option<IssueChoice>) -> Vec<IssueChoice> {
    let mut selected = selected.to_vec();
    if let Some(choice) = choice {
        if let Some(index) = selected.iter().position(|value| value.id() == choice.id()) {
            selected.remove(index);
        } else {
            selected.push(choice);
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_api_client::{issue_writes::IssueChanges, model::IssueDetail};

    use super::{super::Properties, Field, IssueChoice};

    fn issue(labels: &[&str]) -> IssueDetail {
        let mut issue = crate::tests::offline::row("1", "Fixture issue");
        issue["todos"] = json!([]);
        issue["labels"] = json!(
            labels
                .iter()
                .map(|id| { json!({"id":id,"name":id,"color":"#56789a","state":"active"}) })
                .collect::<Vec<_>>()
        );
        serde_json::from_value(issue).unwrap()
    }

    #[test]
    fn adopting_a_newer_issue_saves_only_the_label_toggles() {
        let original = issue(&["remove-mine", "remove-theirs", "keep"]);
        let mut draft = Properties::new(&[], "project-SUB", Some(&original));
        draft.pick(
            Field::Labels,
            Some(IssueChoice::new("remove-mine", "remove-mine")),
        );
        draft.pick(
            Field::Labels,
            Some(IssueChoice::new("add-mine", "add-mine")),
        );
        let latest = issue(&["remove-mine", "keep", "add-theirs"]);
        draft.install_baseline(&[], "project-SUB", Some(&latest));

        let mut changes = IssueChanges::default();
        draft.install(&mut changes);
        assert_eq!(
            serde_json::to_value(&changes).unwrap(),
            json!({"labelIds":["keep","add-theirs","add-mine"]}),
        );
    }

    #[test]
    fn adopting_labels_already_matching_the_draft_omits_the_field() {
        let original = issue(&["remove-mine"]);
        let mut draft = Properties::new(&[], "project-SUB", Some(&original));
        draft.pick(
            Field::Labels,
            Some(IssueChoice::new("remove-mine", "remove-mine")),
        );
        draft.pick(
            Field::Labels,
            Some(IssueChoice::new("add-mine", "add-mine")),
        );
        let latest = issue(&["add-theirs", "add-mine"]);
        draft.install_baseline(&[], "project-SUB", Some(&latest));

        let mut changes = IssueChanges::default();
        draft.install(&mut changes);
        assert!(changes.is_empty());
    }
}
