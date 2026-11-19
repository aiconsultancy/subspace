//! Terminal group heads from the API carry counts without any rows or cursor.
use super::{ids, reread, shown};
use crate::{
    ListQuery,
    tests::{common_path, connected, row, wait},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Mock;

#[test]
fn a_counts_only_terminal_group_rereads_its_expanded_rows_from_new_pages() {
    let edited = Arc::new(AtomicBool::new(false));
    let serving = edited.clone();
    let mock = Mock::new(move |request| {
        let path = &request.path;
        if path.starts_with("/api/v1/issues/groups") {
            return (
                200,
                json!({"groupBy":"state","groups":[{"stateId":"done","stateType":"completed",
                    "count":3,"items":[],"nextCursor":null}]})
                .to_string(),
            );
        }
        if path.starts_with("/api/v1/issues?") && path.contains("stateId=done") {
            let fresh = serving.load(Ordering::SeqCst);
            let cursor = if fresh { "fresh" } else { "old" };
            let (row_ids, next): (&[&str], _) = if path.contains("cursor=") {
                assert!(path.contains(&format!("cursor={cursor}")));
                (&["3"], None)
            } else {
                (&["1", "2"], Some(cursor))
            };
            let mut rows: Vec<_> = row_ids.iter().map(|id| row(id)).collect();
            if fresh && next.is_some() {
                rows[0]["title"] = "Edited terminal issue".into();
            }
            return (200, crate::tests::page(rows, next));
        }
        common_path(path)
    });
    let hub = connected(&mock);
    let query = ListQuery::new(crate::ListTab::All).project(Some("SUB"));
    hub.activate(query.clone()).unwrap();
    wait(&hub, |snapshot| snapshot.list().is_some());
    assert!(shown(&hub).0.is_empty());
    for loaded in [2, 3] {
        hub.load_more(0).unwrap();
        wait(&hub, |s| s.list().unwrap().groups[0].items.len() == loaded);
    }
    edited.store(true, Ordering::SeqCst);
    reread(&hub, &query);

    let (rows, titles, cursor) = shown(&hub);
    assert_eq!(rows, ids(&["1", "2", "3"]));
    assert_eq!(titles[0], "Edited terminal issue");
    assert_eq!(cursor, None);
    hub.stop();
}
