use super::{Folds, Line, group_key, issue_key, line, list_groups, more_title, visible_page_near};
use crate::fixtures::issue;
use crate::listing::More;
use crate::listing::tests::group;
use crate::model::{StateKind, SubIssues};
use serde_json::json;

#[test]
fn keys_name_groups_issues_and_more_lines() {
    assert_eq!(line(&group_key("s1")), Some(Line::Group("s1")));
    assert_eq!(line(&issue_key("id-SUB-1")), Some(Line::Issue("id-SUB-1")));
    assert_eq!(line("more:s1"), Some(Line::More("s1")));
    assert_eq!(line("something"), None);
}

#[test]
fn groups_carry_their_totals_rows_and_more_line() {
    let mut started = group("s1", "In Progress", StateKind::Started, 2);
    started.total = Some(40);
    started.more = More::Available;
    let mut done = group("s9", "Done", StateKind::Completed, 0);
    done.rows = vec![issue(
        "SUB-9",
        "Closed issue",
        ("Done", "completed"),
        json!({}),
    )];
    done.total = Some(1);
    let groups = list_groups(&[started, done], SubIssues::Hidden, &[false, true]).groups;
    assert_eq!(groups[0].name.as_ref(), "In Progress");
    assert_eq!(groups[0].count.as_deref(), Some("40"));
    let titles: Vec<_> = groups[0]
        .rows
        .iter()
        .map(|row| row.title.as_ref())
        .collect();
    assert_eq!(titles, ["Sample issue", "Sample issue", "Show more"]);
    assert_eq!(groups[0].rows[0].key.as_ref(), "issue:id-SUB-100");
    assert!(groups[0].rows[0].id.is_none(), "the identifier is a lane");
    assert!(!groups[0].rows[0].done);
    assert!(groups[1].rows[0].done, "a Done row reads muted");
}

#[test]
fn nested_rows_follow_their_parent_across_states_and_keep_their_own_status() {
    let mut started = group("started", "In Progress", StateKind::Started, 1);
    started.rows.extend([
        child("SUB-1", "SUB-100", ("In Progress", "started")),
        child("SUB-2", "SUB-1", ("In Progress", "started")),
    ]);
    started.total = Some(3);
    let mut done = group("done", "Done", StateKind::Completed, 0);
    done.rows = vec![
        child("SUB-3", "SUB-100", ("Done", "completed")),
        child("SUB-4", "missing", ("Done", "completed")),
    ];
    done.total = Some(5);
    done.more = More::Available;

    let projection = list_groups(&[started, done], SubIssues::Nested, &[false, true]);
    let rows = &projection.groups[0].rows;
    let order: Vec<_> = rows
        .iter()
        .map(|row| (row.key.as_ref(), row.indent))
        .collect();
    assert_eq!(
        order,
        [
            ("issue:id-SUB-100", 0),
            ("issue:id-SUB-1", 1),
            ("issue:id-SUB-2", 2),
            ("issue:id-SUB-3", 1)
        ]
    );
    assert!(rows[3].done);
    assert_eq!(
        projection.guides[&rows[2].key], 1,
        "the rail continues past the grandchild"
    );
    assert_eq!(projection.groups[0].count.as_deref(), Some("1"));
    let done = &projection.groups[1];
    assert_eq!(done.count.as_deref(), Some("4"));
    assert_eq!(done.rows[0].key.as_ref(), "issue:id-SUB-4");
    assert_eq!(done.rows[0].indent, 0);
    assert_eq!(done.rows[1].key.as_ref(), "more:done");
}

#[test]
fn a_matching_child_stays_in_an_open_group_while_its_parents_group_is_folded() {
    let mut todo = group("todo", "Todo", StateKind::Unstarted, 0);
    todo.rows = vec![child("SUB-1", "SUB-9", ("Todo", "unstarted"))];
    todo.total = Some(1);
    let mut done = group("done", "Done", StateKind::Completed, 0);
    done.rows = vec![issue(
        "SUB-9",
        "Closed parent",
        ("Done", "completed"),
        json!({}),
    )];
    done.total = Some(1);
    let groups = [todo, done];
    let folded = list_groups(&groups, SubIssues::Nested, &[false, true]).groups;
    assert_eq!(folded[0].rows[0].key, issue_key("id-SUB-1"));
    assert_eq!(folded[0].rows[0].indent, 0);
    let open = list_groups(&groups, SubIssues::Nested, &[false, false]).groups;
    assert!(open[0].rows.is_empty());
    assert_eq!(open[1].rows[1].key, issue_key("id-SUB-1"));
    assert_eq!(open[1].rows[1].indent, 1);
}

#[test]
fn a_page_follows_the_last_displayed_issue_after_nesting_reorders_the_source() {
    let mut started = group("started", "In Progress", StateKind::Started, 2);
    started
        .rows
        .push(child("SUB-3", "SUB-100", ("In Progress", "started")));
    started.more = More::Available;
    let groups = list_groups(&[started], SubIssues::Nested, &[false]).groups;
    assert_eq!(
        visible_page_near(&groups, "issue:id-SUB-101", 1),
        Some("started")
    );
    assert_eq!(visible_page_near(&groups, "issue:id-SUB-3", 1), None);
}

fn child(id: &str, parent: &str, state: (&str, &str)) -> subspace_api_client::model::IssueListItem {
    issue(
        id,
        "Child issue",
        state,
        json!({"parentId": format!("id-{parent}")}),
    )
}

#[test]
fn the_more_line_says_what_the_next_page_is_doing() {
    assert_eq!(more_title(More::Available), Some("Show more"));
    assert_eq!(more_title(More::Loading), Some("Loading more…"));
    assert_eq!(more_title(More::Failed), Some("Couldn't load more"));
    assert_eq!(more_title(More::None), None);
    assert_eq!(more_title(More::Unread), None);
}

#[test]
fn done_cancelled_and_duplicate_start_folded_and_hand_folds_are_kept() {
    let mut folds = Folds::default();
    assert!(folds.is_folded("done", StateKind::Completed));
    assert!(folds.is_folded("cancelled", StateKind::Canceled));
    assert!(folds.is_folded("duplicate", StateKind::Duplicate));
    assert!(!folds.is_folded("todo", StateKind::Unstarted));

    folds.record("done", StateKind::Completed, false);
    folds.record("todo", StateKind::Unstarted, true);
    assert!(!folds.is_folded("done", StateKind::Completed));
    assert!(folds.is_folded("todo", StateKind::Unstarted));
    assert_eq!(folds.unfolded, ["done"]);
    assert_eq!(folds.folded, ["todo"]);

    folds.record("done", StateKind::Completed, true);
    folds.record("todo", StateKind::Unstarted, false);
    assert_eq!(
        folds,
        Folds::default(),
        "back to the defaults saves nothing"
    );
}
