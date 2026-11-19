use super::{Folds, Line, group_key, issue_key, line, list_groups, more_title};
use crate::listing::More;
use crate::listing::tests::group;
use crate::model::StateKind;

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
    let done = group("s9", "Done", StateKind::Completed, 1);
    let groups = list_groups(&[started, done]);
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
