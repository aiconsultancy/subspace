use super::merge;
use crate::model::{IssueListItem, Page};

fn page(rows: &[(&str, u8)], cursor: Option<&str>) -> Page<IssueListItem> {
    let fixture = serde_json::json!({"items":[crate::tests::row("1")]});
    let items: Vec<_> = rows
        .iter()
        .map(|(id, hour)| {
            let mut row = fixture["items"][0].clone();
            row["id"] = (*id).into();
            row["lastActivityAt"] = format!("2026-11-15T{hour:02}:00:00Z").into();
            row
        })
        .collect();
    serde_json::from_value(serde_json::json!({"items":items,"nextCursor":cursor})).unwrap()
}

fn ids(page: &Page<IssueListItem>) -> Vec<&str> {
    page.items
        .iter()
        .map(|row| row.fields.id.as_str())
        .collect()
}

#[test]
fn disjoint_head_adopts_its_cursor_including_after_an_exhausted_first_read() {
    for cursor in [Some("old-tail"), None] {
        let mut loaded = page(&[("a", 9), ("b", 8)], cursor);
        merge(
            &mut loaded,
            page(&[("x", 12), ("y", 11)], Some("new-overflow")),
            true,
        );
        assert_eq!(ids(&loaded), ["x", "y"]);
        assert_eq!(loaded.next_cursor.as_deref(), Some("new-overflow"));
    }
}

#[test]
fn moved_overlap_cannot_hide_overflow_with_or_without_an_old_cursor() {
    let names: Vec<_> = (1..=49).map(|n| format!("n{n:02}")).collect();
    let mut rows = vec![("a", 12)];
    rows.extend(names.iter().map(|id| (id.as_str(), 11)));
    for cursor in [Some("after-c"), None] {
        let mut loaded = page(&[("a", 9), ("b", 8), ("c", 7)], cursor);
        merge(&mut loaded, page(&rows, Some("new-overflow")), true);
        assert_eq!(loaded.items.len(), 50);
        assert_eq!(ids(&loaded)[0], "a");
        assert!(!ids(&loaded).contains(&"b"));
        assert!(!ids(&loaded).contains(&"c"));
        assert_eq!(loaded.next_cursor.as_deref(), Some("new-overflow"));
    }
}

#[test]
fn a_shared_row_with_changed_activity_is_not_an_anchor() {
    let mut loaded = page(&[("a", 9), ("b", 8), ("c", 7)], Some("after-c"));
    merge(
        &mut loaded,
        page(&[("a", 12), ("new", 11)], Some("after-new")),
        true,
    );
    assert_eq!(ids(&loaded), ["a", "new"]);
    assert_eq!(loaded.next_cursor.as_deref(), Some("after-new"));
}

#[test]
fn complete_head_is_the_entire_result_even_when_old_rows_were_below_it() {
    let mut loaded = page(&[("a", 9), ("b", 8), ("c", 7)], Some("tail"));
    merge(&mut loaded, page(&[("a", 9)], None), true);
    assert_eq!(ids(&loaded), ["a"]);
    assert!(loaded.next_cursor.is_none());
    merge(&mut loaded, page(&[], None), true);
    assert!(loaded.items.is_empty());
}

#[test]
fn missing_rows_inside_head_range_are_removed_while_older_rows_and_cursor_stay() {
    let mut loaded = page(
        &[("a", 12), ("removed", 11), ("b", 10), ("c", 9)],
        Some("old-tail"),
    );
    merge(
        &mut loaded,
        page(&[("a", 12), ("b", 10)], Some("head-cursor")),
        true,
    );
    assert_eq!(ids(&loaded), ["a", "b", "c"]);
    assert_eq!(loaded.next_cursor.as_deref(), Some("old-tail"));
}

#[test]
fn head_is_authoritative_even_when_activity_moves_backwards_and_ids_break_ties() {
    let mut loaded = page(
        &[("z", 12), ("a", 11), ("b", 9), ("older", 8)],
        Some("tail"),
    );
    let mut head = page(&[("a", 11), ("b", 9), ("z", 9)], Some("head"));
    head.items[2].fields.title = "Authoritative lower time".into();
    merge(&mut loaded, head, true);
    assert_eq!(ids(&loaded), ["a", "z", "b", "older"]);
    assert_eq!(loaded.items[1].fields.title, "Authoritative lower time");
    assert_eq!(
        loaded.items[1].last_activity_at.as_str(),
        "2026-11-15T09:00:00Z"
    );
}
