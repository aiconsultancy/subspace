//! Only an unchanged ID/time anchor lets a partial head preserve the tail and cursor.
use crate::model::{IssueListItem, Page};
use std::collections::HashMap;

fn time(row: &IssueListItem) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(&row.last_activity_at).ok()
}

fn key(row: &IssueListItem) -> (Option<chrono::DateTime<chrono::FixedOffset>>, &str) {
    (time(row), &row.fields.id)
}

fn has_anchor(loaded: &[IssueListItem], incoming: &[IssueListItem]) -> bool {
    incoming.iter().any(|new| {
        loaded.iter().any(|old| {
            old.fields.id == new.fields.id && old.last_activity_at == new.last_activity_at
        })
    })
}

pub(in crate::hub) fn merge(
    loaded: &mut Page<IssueListItem>,
    incoming: Page<IssueListItem>,
    head: bool,
) {
    if head && (incoming.next_cursor.is_none() || !has_anchor(&loaded.items, &incoming.items)) {
        loaded.items.clear();
        loaded.next_cursor.clone_from(&incoming.next_cursor);
    } else if head && let Some(bottom) = incoming.items.iter().min_by_key(|row| key(row)) {
        loaded.items.retain(|row| key(row) < key(bottom));
    }
    let mut rows: HashMap<_, _> = std::mem::take(&mut loaded.items)
        .into_iter()
        .map(|row| (row.fields.id.clone(), row))
        .collect();
    for row in incoming.items {
        let replace = head
            || rows
                .get(&row.fields.id)
                .is_none_or(|old| time(&row) > time(old));
        if replace {
            rows.insert(row.fields.id.clone(), row);
        }
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        time(b)
            .cmp(&time(a))
            .then_with(|| b.fields.id.cmp(&a.fields.id))
    });
    loaded.items = rows;
    if !head {
        loaded.next_cursor = incoming.next_cursor;
    }
}

#[cfg(test)]
mod tests;
