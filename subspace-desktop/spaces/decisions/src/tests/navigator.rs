use super::{Entry, Loaded, makers, projects, views};
use crate::fixtures;
use crate::model::{Maker, ProjectChoice, View};

fn counts<T>(entries: &[Entry<T>]) -> Vec<Option<&str>> {
    entries.iter().map(|entry| entry.count.as_deref()).collect()
}

fn selected<T: Clone>(entries: &[Entry<T>]) -> Vec<T> {
    entries
        .iter()
        .filter(|entry| entry.selected)
        .map(|entry| entry.value.clone())
        .collect()
}

#[test]
fn every_view_counts_its_status_while_all_decisions_is_shown() {
    let items = fixtures::items();
    let loaded = Some(Loaded {
        items: &items,
        more: false,
    });
    let entries = views(View::All, loaded);
    let labels: Vec<_> = entries.iter().map(|entry| entry.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "All decisions",
            "Proposed",
            "Accepted",
            "Rejected",
            "Superseded"
        ]
    );
    assert_eq!(
        counts(&entries),
        [Some("8"), Some("1"), Some("4"), Some("1"), Some("2")]
    );
    assert_eq!(selected(&entries), [View::All]);
}

#[test]
fn a_status_view_counts_only_itself() {
    let items: Vec<_> = fixtures::items()
        .into_iter()
        .filter(|item| item.id == "d6" || item.id == "d7")
        .collect();
    let loaded = Some(Loaded {
        items: &items,
        more: true,
    });
    let entries = views(View::Superseded, loaded);
    assert_eq!(counts(&entries), [None, None, None, None, Some("2+")]);
    assert_eq!(selected(&entries), [View::Superseded]);
    assert_eq!(
        counts(&views(View::All, None)),
        [None; 5],
        "nothing read yet"
    );
}

#[test]
fn people_and_agents_split_the_decisions_anyone_made() {
    let items = fixtures::items();
    let loaded = Some(Loaded {
        items: &items,
        more: false,
    });
    let entries = makers(Maker::Anyone, loaded);
    assert_eq!(counts(&entries), [Some("8"), Some("2"), Some("6")]);
    assert_eq!(selected(&makers(Maker::Agents, loaded)), [Maker::Agents]);
}

#[test]
fn projects_list_every_project_then_each_active_one() {
    let items = fixtures::items();
    let loaded = Some(Loaded {
        items: &items,
        more: false,
    });
    let mut retired = fixtures::project("OLD", "Retired");
    retired.archived_at = Some("2026-10-01T00:00:00Z".into());
    let mut known = fixtures::projects();
    known.push(retired);
    let entries = projects(&ProjectChoice::All, &known, loaded);
    let labels: Vec<_> = entries.iter().map(|entry| entry.label.as_str()).collect();
    assert_eq!(
        labels,
        ["All projects", "NAV · Navigation", "WARP · Warp drive"],
        "an archived project is left out"
    );
    assert_eq!(counts(&entries), [Some("8"), Some("5"), Some("3")]);
    let archived = projects(&ProjectChoice::Project("OLD".into()), &known, None);
    assert_eq!(
        archived.len(),
        4,
        "the chosen project shows even when archived"
    );
    let gone = projects(&ProjectChoice::Project("GONE".into()), &known, None);
    assert_eq!(
        selected(&gone),
        [ProjectChoice::Project("GONE".into())],
        "a chosen project the account no longer lists still shows as chosen"
    );
}
