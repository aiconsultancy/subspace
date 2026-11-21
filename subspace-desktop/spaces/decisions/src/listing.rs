//! The list's lines: the decisions grouped by status, which groups start
//! folded, and the counts the navigator shows. Pure.

use gpui_kit::SharedString;
use subspace_api_client::model::{DecisionListItem, DecisionMakerType, DecisionStatus};
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use crate::model::View;
use crate::words;

/// The groups' order. A status the Space does not know follows them.
pub(crate) const ORDER: [DecisionStatus; 4] = [
    DecisionStatus::Proposed,
    DecisionStatus::Accepted,
    DecisionStatus::Superseded,
    DecisionStatus::Rejected,
];
const ROW_PREFIX: &str = "decision:";
const GROUP_PREFIX: &str = "status:";

/// A decision row's list key.
pub(crate) fn row_key(id: &str) -> SharedString {
    format!("{ROW_PREFIX}{id}").into()
}

/// The decision a row key stands for; none for a group header.
pub(crate) fn decision_id(key: &str) -> Option<&str> {
    key.strip_prefix(ROW_PREFIX)
}

/// Each known status's part of its group key.
const STATUS_KEYS: [(DecisionStatus, &str); 4] = [
    (DecisionStatus::Proposed, "proposed"),
    (DecisionStatus::Accepted, "accepted"),
    (DecisionStatus::Superseded, "superseded"),
    (DecisionStatus::Rejected, "rejected"),
];

fn status_key(status: DecisionStatus) -> &'static str {
    STATUS_KEYS
        .iter()
        .find(|(known, _)| *known == status)
        .map_or("other", |(_, key)| *key)
}

/// A status group's list key.
pub(crate) fn group_key(status: DecisionStatus) -> SharedString {
    format!("{GROUP_PREFIX}{}", status_key(status)).into()
}

/// The status a group key stands for, when it is a known one.
pub(crate) fn group_status(key: &str) -> Option<DecisionStatus> {
    ORDER
        .into_iter()
        .find(|status| group_key(*status).as_ref() == key)
}

/// A status's place in [`ORDER`]; an unknown one comes last.
fn slot(status: DecisionStatus) -> usize {
    ORDER
        .iter()
        .position(|known| *known == status)
        .unwrap_or(ORDER.len())
}

/// The decisions in status groups, in [`ORDER`]; each group keeps the
/// read's order, newest decided first. An empty group is left out.
pub(crate) fn groups(items: &[DecisionListItem]) -> Vec<Group> {
    let mut slots: Vec<Vec<&DecisionListItem>> = vec![Vec::new(); ORDER.len() + 1];
    for item in items {
        if let Some(rows) = slots.get_mut(slot(item.status)) {
            rows.push(item);
        }
    }
    slots
        .iter()
        .enumerate()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(index, rows)| {
            let status = ORDER.get(index).copied().unwrap_or(DecisionStatus::Other);
            group(status, rows)
        })
        .collect()
}

fn group(status: DecisionStatus, items: &[&DecisionListItem]) -> Group {
    Group {
        key: group_key(status),
        icon: Glyph::new("", Tone::Muted),
        name: words::status(status).into(),
        count: Some(items.len().to_string().into()),
        rows: items.iter().map(|item| row(item)).collect(),
    }
}

/// A row: the project key in the id column, then the title. Its lanes are
/// drawn by the Space from the decision itself.
fn row(item: &DecisionListItem) -> Row {
    Row {
        key: row_key(&item.id),
        icon: Glyph::new("", Tone::Muted),
        id: Some(item.project_key.clone().into()),
        title: item.title.clone().into(),
        trailing: Vec::new(),
        done: matches!(
            item.status,
            DecisionStatus::Superseded | DecisionStatus::Rejected
        ),
        indent: 0,
    }
}

/// Whether group `group` starts folded: Superseded and Rejected in All
/// decisions, unless a search may be looking for something in them.
pub(crate) fn starts_folded(view: View, searching: bool, group: &str) -> bool {
    view == View::All
        && !searching
        && [DecisionStatus::Superseded, DecisionStatus::Rejected]
            .into_iter()
            .any(|status| group_key(status).as_ref() == group)
}

pub(crate) fn count_status(items: &[DecisionListItem], status: DecisionStatus) -> usize {
    items.iter().filter(|item| item.status == status).count()
}

pub(crate) fn count_maker(items: &[DecisionListItem], kind: DecisionMakerType) -> usize {
    items
        .iter()
        .filter(|item| item.made_by_type == kind)
        .count()
}

pub(crate) fn count_project(items: &[DecisionListItem], key: &str) -> usize {
    items.iter().filter(|item| item.project_key == key).count()
}

#[cfg(test)]
mod tests {
    use subspace_api_client::model::{DecisionMakerType, DecisionStatus};

    use super::{
        count_maker, count_project, count_status, decision_id, group_key, group_status, groups,
        row_key, starts_folded,
    };
    use crate::fixtures;
    use crate::model::View;

    #[test]
    fn decisions_group_by_status_in_the_lifecycle_order() {
        let groups = groups(&fixtures::items());
        let names: Vec<_> = groups.iter().map(|group| group.name.as_ref()).collect();
        assert_eq!(names, ["Proposed", "Accepted", "Superseded", "Rejected"]);
        let accepted: Vec<_> = groups[1].rows.iter().map(|row| row.key.as_ref()).collect();
        assert_eq!(
            accepted,
            ["decision:d2", "decision:d3", "decision:d4", "decision:d5"],
            "a group keeps the read's order, newest decided first"
        );
        assert_eq!(groups[1].count.as_deref(), Some("4"));
        assert_eq!(groups[2].rows[0].id.as_deref(), Some("NAV"));
        assert!(
            groups[2].rows[0].done,
            "a superseded decision reads quietly"
        );
        assert!(!groups[0].rows[0].done);
    }

    #[test]
    fn an_empty_status_has_no_group() {
        let accepted: Vec<_> = fixtures::items()
            .into_iter()
            .filter(|item| item.status == DecisionStatus::Accepted)
            .collect();
        let groups = groups(&accepted);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key.as_ref(), "status:accepted");
        assert!(super::groups(&[]).is_empty());
    }

    #[test]
    fn keys_read_back_as_decisions_and_statuses() {
        assert_eq!(decision_id(&row_key("d2")), Some("d2"));
        assert_eq!(decision_id("status:accepted"), None);
        assert_eq!(
            group_status(&group_key(DecisionStatus::Rejected)),
            Some(DecisionStatus::Rejected)
        );
        assert_eq!(group_status("status:other"), None);
    }

    #[test]
    fn superseded_and_rejected_start_folded_only_in_all_decisions_without_a_search() {
        let superseded = group_key(DecisionStatus::Superseded);
        let rejected = group_key(DecisionStatus::Rejected);
        let accepted = group_key(DecisionStatus::Accepted);
        assert!(starts_folded(View::All, false, &superseded));
        assert!(starts_folded(View::All, false, &rejected));
        assert!(!starts_folded(View::All, false, &accepted));
        assert!(!starts_folded(View::All, true, &superseded), "searching");
        assert!(!starts_folded(View::Superseded, false, &superseded));
    }

    #[test]
    fn counts_split_the_loaded_decisions() {
        let items = fixtures::items();
        assert_eq!(count_status(&items, DecisionStatus::Accepted), 4);
        assert_eq!(count_status(&items, DecisionStatus::Rejected), 1);
        assert_eq!(count_maker(&items, DecisionMakerType::Human), 2);
        assert_eq!(count_maker(&items, DecisionMakerType::Agent), 6);
        assert_eq!(count_project(&items, "NAV"), 5);
        assert_eq!(count_project(&items, "WARP"), 3);
    }
}
