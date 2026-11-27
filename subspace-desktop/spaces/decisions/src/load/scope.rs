//! Initiative-scoped decision reads and their combined result.

use std::collections::BTreeSet;

use subspace_api_client::model::{DecisionListItem, Page, Project};
use subspace_api_client::{Backend, DecisionQuery, Error, PageRequest};

use super::{Found, MAX_ITEMS, Plan, all_pages, collect_pages};

pub(super) fn read(
    plan: &Plan,
    projects: &[Project],
    backend: Backend,
    mut page: impl FnMut(&DecisionQuery, &PageRequest) -> Result<Page<DecisionListItem>, Error>,
) -> Result<Found, Error> {
    let Some(initiative) = plan.initiative.as_deref() else {
        return all_pages(&plan.stop, |request| page(&plan.query, request));
    };
    let keys = project_keys(initiative, plan.query.project.as_deref(), projects);
    if keys.is_empty() {
        return Ok(Found {
            items: Vec::new(),
            more: false,
        });
    }
    if backend == Backend::Nett {
        // NETT has no project predicate on this endpoint. Keep reading until
        // enough matching rows arrive, even if earlier pages had none.
        return collect_pages(&plan.stop, usize::MAX, |request| {
            let mut reply = page(&plan.query, request)?;
            reply
                .items
                .retain(|item| keys.contains(item.project_key.as_str()));
            Ok(reply)
        });
    }
    by_project(plan, keys, page)
}

fn project_keys<'a>(
    initiative: &str,
    selected: Option<&str>,
    projects: &'a [Project],
) -> BTreeSet<&'a str> {
    let mut keys: BTreeSet<_> = projects
        .iter()
        .filter(|project| crate::initiative::includes(project, Some(initiative)))
        .map(|project| project.key.as_str())
        .collect();
    if let Some(selected) = selected.and_then(|key| keys.get(key).copied()) {
        keys.retain(|key| *key == selected);
    }
    keys
}

fn by_project(
    plan: &Plan,
    keys: BTreeSet<&str>,
    mut page: impl FnMut(&DecisionQuery, &PageRequest) -> Result<Page<DecisionListItem>, Error>,
) -> Result<Found, Error> {
    let mut result = Found {
        items: Vec::new(),
        more: false,
    };
    let mut query = plan.query.clone();
    for key in keys {
        if plan.stop.stopped() {
            result.more = true;
            break;
        }
        query.project = Some(key.to_owned());
        let mut project = all_pages(&plan.stop, |request| page(&query, request))?;
        result.items.append(&mut project.items);
        // Match the API cursor order across all project reads.
        result
            .items
            .sort_unstable_by(|a, b| (&b.created_at, &b.id).cmp(&(&a.created_at, &a.id)));
        result.more |= project.more || result.items.len() > MAX_ITEMS;
        result.items.truncate(MAX_ITEMS);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::{MAX_PAGES, Plan, Stop};
    use super::read;
    use crate::fixtures;
    use subspace_api_client::Backend;
    use subspace_api_client::model::{DecisionListItem, DecisionStatus, Page, Project};

    const INITIATIVE: &str = "fixture-initiative";

    fn plan() -> Plan {
        Plan {
            query: Default::default(),
            directory: false,
            stop: Stop::default(),
            initiative: Some(INITIATIVE.into()),
            projects: Default::default(),
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
        let mut item =
            fixtures::item_value(id, project, "Fixture decision", "accepted", "2026-11-19");
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
                ("NAV", None) => {
                    reply("NAV", "d2", "2026-11-19T12:00:00Z", Some("nav-next".into()))
                }
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
                ("NAV".into(), Some("nav-next".into())),
                ("WARP".into(), None)
            ]
        );
        let ids: Vec<_> = found.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["d2", "d3", "d1"]);
        assert!(!found.more);
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
        assert!(!empty.more);
    }

    #[test]
    fn nett_scans_past_unrelated_pages_before_applying_the_match_limit() {
        let mut reads = 0;
        let found = read(&plan(), &projects(), Backend::Nett, |query, request| {
            assert!(query.project.is_none());
            assert_eq!(request.cursor, (reads != 0).then(|| reads.to_string()));
            reads += 1;
            if reads <= MAX_PAGES {
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
        assert_eq!(reads, MAX_PAGES + 1);
        assert_eq!(found.items.len(), 1);
        assert_eq!(found.items[0].project_key, "WARP");
        assert!(!found.more);
    }
}
