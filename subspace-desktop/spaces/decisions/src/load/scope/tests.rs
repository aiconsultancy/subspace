//! Continuations, project ordering and cancellation.

use super::super::{Plan, Stop};
use super::{BATCH, read};
use crate::fixtures;
use subspace_api_client::Backend;
use subspace_api_client::Error;
use subspace_api_client::model::{DecisionListItem, DecisionStatus, Page, Project};

const INITIATIVE: &str = "fixture-initiative";

fn plan() -> Plan {
    Plan {
        query: Default::default(),
        probe: None,
        directory: false,
        stop: Stop::default(),
        initiative: Some(INITIATIVE.into()),
        projects: Default::default(),
        next: None,
    }
}

fn projects() -> [Project; 3] {
    let mut nav = fixtures::project("NAV", "Navigation");
    let mut warp = fixtures::project("WARP", "Warp");
    nav.initiative_id = Some(INITIATIVE.into());
    warp.initiative_id = Some(INITIATIVE.into());
    [nav, warp, fixtures::project("MAP", "Mapping")]
}

fn reply(
    project: &str,
    id: &str,
    created_at: &str,
    next: Option<String>,
) -> Page<DecisionListItem> {
    let mut item = fixtures::item_value(id, project, "Fixture decision", "accepted", "2026-11-19");
    item["createdAt"] = serde_json::json!(created_at);
    serde_json::from_value(serde_json::json!({"items":[item],"nextCursor":next})).unwrap()
}

#[test]
fn initiative_reads_only_its_projects_and_merges_their_cursor_order() {
    let mut plan = plan();
    plan.query.project = Some("MAP".into());
    plan.query.status = vec![DecisionStatus::Accepted];
    plan.query.q = Some("fixture".into());
    let mut asked = Vec::new();
    let found = read(&plan, &projects(), Backend::Subspace, |query, page| {
        assert_eq!(query.q.as_deref(), Some("fixture"));
        assert_eq!(query.status, [DecisionStatus::Accepted]);
        let project = query.project.as_deref().unwrap();
        let cursor = page.cursor.as_deref();
        asked.push((project.to_owned(), page.cursor.clone()));
        Ok(match (project, cursor) {
            ("NAV", None) => reply("NAV", "d2", "2026-11-19T12:00:00Z", Some("nav-next".into())),
            ("NAV", Some("nav-next")) => reply("NAV", "d1", "2026-11-19T10:00:00Z", None),
            ("WARP", None) => reply("WARP", "d3", "2026-11-19T11:00:00Z", None),
            _ => panic!("unexpected request for {project} at {cursor:?}"),
        })
    })
    .unwrap();
    assert_eq!(
        asked,
        [
            ("NAV".into(), None),
            ("WARP".into(), None),
            ("NAV".into(), Some("nav-next".into())),
        ]
    );
    let ids: Vec<_> = found.items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["d2", "d3", "d1"]);
    assert!(found.next.is_none());
}

#[test]
fn a_project_in_the_initiative_narrows_the_read_and_an_empty_scope_reads_nothing() {
    let mut plan = plan();
    plan.query.project = Some("WARP".into());
    let found = read(&plan, &projects(), Backend::Subspace, |query, _| {
        assert_eq!(query.project.as_deref(), Some("WARP"));
        Ok(reply("WARP", "d1", "2026-11-19T11:00:00Z", None))
    })
    .unwrap();
    assert_eq!(found.items.len(), 1);

    let empty = read(&plan, &[], Backend::Subspace, |_, _| {
        panic!("an initiative without readable projects has nothing to read")
    })
    .unwrap();
    assert!(empty.items.is_empty());
    assert!(empty.next.is_none());
}

#[test]
fn nett_scans_past_unrelated_pages_before_applying_the_match_limit() {
    let mut reads = 0;
    let found = read(&plan(), &projects(), Backend::Nett, |query, request| {
        assert!(query.project.is_none());
        assert_eq!(request.cursor, (reads != 0).then(|| reads.to_string()));
        reads += 1;
        if reads <= 10 {
            return Ok(reply(
                "MAP",
                "d1",
                "2026-11-19T12:00:00Z",
                Some(reads.to_string()),
            ));
        }
        Ok(reply("WARP", "d2", "2026-11-19T11:00:00Z", None))
    })
    .unwrap();
    assert_eq!(reads, 11);
    assert_eq!(found.items.len(), 1);
    assert_eq!(found.items[0].project_key, "WARP");
    assert!(found.next.is_none());
}

#[test]
fn the_next_batch_keeps_unconsumed_project_rows() {
    let mut plan = plan();
    let mut reads = Vec::new();
    let mut fetch = |query: &subspace_api_client::DecisionQuery,
                     page: &subspace_api_client::PageRequest| {
        let project = query.project.as_deref().unwrap();
        reads.push((project.to_owned(), page.cursor.clone()));
        let items = match project {
            "NAV" => (51..=150)
                .rev()
                .map(|id| numbered_item(id, project))
                .collect(),
            "WARP" => (1..=50)
                .rev()
                .map(|id| numbered_item(id, project))
                .collect(),
            _ => panic!("unexpected project"),
        };
        Ok(batch(items, None))
    };
    let first = read(&plan, &projects(), Backend::Subspace, &mut fetch).unwrap();
    assert_eq!(first.items.len(), BATCH);
    assert_eq!(first.items[0].id, "00150");
    assert_eq!(first.items.last().unwrap().id, "00051");
    plan.next = first.next;
    assert!(plan.next.is_some());
    let next = read(&plan, &projects(), Backend::Subspace, &mut fetch).unwrap();
    assert_eq!(next.items.len(), 50);
    assert_eq!(next.items[0].id, "00050");
    assert!(next.next.is_none());
    assert_eq!(reads, [("NAV".to_owned(), None), ("WARP".to_owned(), None)]);
}

#[test]
fn a_batch_uses_its_cursor_only_on_the_next_read() {
    let mut plan = plan();
    plan.initiative = None;
    let first = read(&plan, &[], Backend::Subspace, |_, page| {
        assert!(page.cursor.is_none());
        assert_eq!(page.limit as usize, BATCH);
        let items = (1..=BATCH)
            .rev()
            .map(|id| numbered_item(id, "NAV"))
            .collect();
        Ok(batch(items, Some("resume")))
    })
    .unwrap();
    plan.next = first.next;
    let repeat = read(&plan, &[], Backend::Subspace, |_, page| {
        assert_eq!(page.cursor.as_deref(), Some("resume"));
        Ok(reply(
            "NAV",
            "d1",
            "2026-11-19T11:00:00Z",
            Some("resume".into()),
        ))
    });
    assert!(matches!(repeat, Err(Error::InvalidResponse)));
    plan.stop.stop();
    let cancelled = read(&plan, &[], Backend::Subspace, |_, _| {
        panic!("a cancelled batch never asks for a page")
    })
    .unwrap();
    assert!(cancelled.items.is_empty());
}

fn batch(items: Vec<DecisionListItem>, cursor: Option<&str>) -> Page<DecisionListItem> {
    let mut page = serde_json::from_value::<Page<DecisionListItem>>(
        serde_json::json!({ "items": [], "nextCursor": cursor }),
    )
    .unwrap();
    page.items = items;
    page
}

fn numbered_item(number: usize, project: &str) -> DecisionListItem {
    reply(
        project,
        &format!("{number:05}"),
        "2026-11-19T11:00:00Z",
        None,
    )
    .items
    .pop()
    .unwrap()
}
