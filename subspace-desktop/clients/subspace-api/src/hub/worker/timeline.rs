use super::{Job, Shared, Task, lock};
use crate::{
    PageRequest,
    hub::{ReadError, ReadPriority, TimelineSnapshot, timeline::TimelineRequest},
    model::{ActivityKind, ActivityPage, IssueActivity, Page},
};

#[cfg(test)]
mod reload_tests;

pub(super) fn load(shared: &Shared, job: &Job, mut request: TimelineRequest) {
    let Some(sequence) = begin_read(shared, job) else {
        return;
    };
    let result = shared
        .client
        .issue_activity(&request.identifier, &request.page);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    let timeline = &mut issue.timeline;
    timeline.loading = false;
    match result {
        Ok(page) => {
            install(timeline, page, false, sequence);
            let cursor = timeline
                .content
                .page
                .as_ref()
                .and_then(|page| page.next_cursor.clone());
            let under_goal = timeline.content.page.as_ref().is_some_and(|page| {
                page.items
                    .iter()
                    .filter(|entry| entry.activity.kind() == ActivityKind::CommentCreated)
                    .take(request.comment_goal)
                    .count()
                    < request.comment_goal
            });
            if cursor.is_some() && cursor == request.page.cursor {
                timeline.error = Some(ReadError::Network);
            } else if under_goal && request.remaining > 0 && cursor.is_some() {
                request.remaining -= 1;
                request.page.cursor = cursor;
                timeline.loading = true;
                shared.enqueue(&mut state, Task::Timeline(request), ReadPriority::LaterPage);
            }
        }
        Err(error) => {
            timeline.error = Some(error.into());
        }
    }
    state.changed();
}

pub(super) fn refresh(shared: &Shared, job: &Job, identifier: &str) {
    let Some(sequence) = begin_read(shared, job) else {
        return;
    };
    let result = shared
        .client
        .issue_activity(identifier, &PageRequest::default());
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    let failed = result.is_err();
    if let Some(issue) = state.snapshot.issue.as_mut() {
        match result {
            Ok(page) => {
                install(&mut issue.timeline, page, true, sequence);
            }
            Err(error) => {
                issue.timeline.error = Some(error.into());
            }
        }
    }
    for (key, draft) in &mut state.drafts {
        if key.issue == identifier && draft.body().is_empty() && draft.confirmation() > 0 {
            draft.refresh_failed = failed;
        }
    }
    state.changed();
}

/// Reserve provenance before HTTP starts, not when a response happens to finish.
fn begin_read(shared: &Shared, job: &Job) -> Option<u64> {
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return None;
    }
    let content = &mut state.snapshot.issue.as_mut()?.timeline.content;
    content.request_sequence += 1;
    Some(content.request_sequence)
}

fn install(timeline: &mut TimelineSnapshot, incoming: ActivityPage, head: bool, sequence: u64) {
    install_references(&mut timeline.content, &incoming, sequence);
    if let Some(page) = &mut timeline.content.page {
        for row in incoming.items {
            page.items.retain(|old| old.id != row.id);
            page.items.push(row);
        }
        page.items
            .sort_by(|a, b| time(b).cmp(&time(a)).then_with(|| b.id.cmp(&a.id)));
        if !head {
            page.next_cursor = incoming.next_cursor;
        }
    } else {
        timeline.content.page = Some(Page {
            items: incoming.items,
            next_cursor: incoming.next_cursor,
        });
    }
    if let Some(page) = &timeline.content.page {
        timeline.content.projected.retain(|comment| {
            !page.items.iter().any(|entry| {
                entry
                    .comment
                    .as_ref()
                    .is_some_and(|loaded| loaded.id == comment.id)
            })
        });
    }
    timeline.error = None;
}

/// A missing reference is a versioned tombstone, so delayed reads cannot revive it.
fn install_references(
    content: &mut crate::hub::state::issue::TimelineContent,
    incoming: &ActivityPage,
    sequence: u64,
) {
    let ids: std::collections::HashSet<_> = incoming
        .items
        .iter()
        .filter_map(IssueActivity::decision_id)
        .chain(
            incoming
                .references
                .decisions
                .iter()
                .map(|decision| decision.id.as_str()),
        )
        .collect();
    for id in ids {
        if content
            .decision_sequences
            .get(id)
            .is_some_and(|installed| *installed > sequence)
        {
            continue;
        }
        content.decision_sequences.insert(id.to_owned(), sequence);
        match incoming
            .references
            .decisions
            .iter()
            .find(|decision| decision.id == id)
        {
            Some(decision) => {
                content.decisions.insert(id.to_owned(), decision.clone());
            }
            None => {
                content.decisions.remove(id);
            }
        }
    }
}

fn time(row: &IssueActivity) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(&row.display_at).ok()
}

#[cfg(test)]
mod tests {
    use super::install;
    use crate::{hub::TimelineSnapshot, model::ActivityPage};
    use serde_json::json;

    fn response(event: usize, title: Option<&str>, status: &str) -> ActivityPage {
        let id = format!("00000000-0000-0000-0000-{event:012}");
        serde_json::from_value(json!({"items":[{"id":format!("event-{event}"), "kind":"decision.logged",
            "occurredAt":"2026-11-16T12:00:00Z", "displayAt":"2026-11-16T12:00:00Z",
            "actor":{"type":"system","label":"Fixture service"}, "data":{"decisionId":id}}],
            "nextCursor":"older", "references":{"decisions":title.map(|title| vec![json!({"id":id,"title":title,"status":status})]).unwrap_or_default()}})).unwrap()
    }

    #[test]
    fn same_event_refresh_retires_only_its_missing_reference() {
        let mut timeline = TimelineSnapshot::default();
        install(
            &mut timeline,
            response(1, Some("Head"), "proposed"),
            false,
            1,
        );
        install(
            &mut timeline,
            response(2, Some("Older"), "accepted"),
            false,
            2,
        );
        install(&mut timeline, response(1, None, "proposed"), true, 3);
        assert!(
            timeline
                .decision("00000000-0000-0000-0000-000000000001")
                .is_none()
        );
        assert_eq!(
            timeline
                .decision("00000000-0000-0000-0000-000000000002")
                .unwrap()
                .title,
            "Older"
        );
        assert_eq!(timeline.page().unwrap().items.len(), 2);
    }

    #[test]
    fn delayed_old_response_cannot_overwrite_or_revive_new_reference() {
        let mut timeline = TimelineSnapshot::default();
        install(
            &mut timeline,
            response(1, Some("New title"), "accepted"),
            true,
            2,
        );
        install(
            &mut timeline,
            response(1, Some("Old title"), "proposed"),
            false,
            1,
        );
        let decision = timeline
            .decision("00000000-0000-0000-0000-000000000001")
            .unwrap();
        assert_eq!(decision.title, "New title");
        assert_eq!(decision.status, crate::model::DecisionStatus::Accepted);
        install(&mut timeline, response(1, None, "proposed"), true, 4);
        install(
            &mut timeline,
            response(1, Some("Delayed"), "accepted"),
            true,
            3,
        );
        assert!(
            timeline
                .decision("00000000-0000-0000-0000-000000000001")
                .is_none()
        );
    }

    #[test]
    fn later_page_can_replace_a_shared_decision_title_and_status() {
        let mut timeline = TimelineSnapshot::default();
        install(
            &mut timeline,
            response(1, Some("Old"), "proposed"),
            false,
            1,
        );
        let mut older_page = response(1, Some("Retitled"), "accepted");
        older_page.items[0].id = "older-event".into();
        older_page.next_cursor = None;
        install(&mut timeline, older_page, false, 2);
        let page = timeline.page().unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page.next_cursor.is_none());
        let decision = timeline
            .decision("00000000-0000-0000-0000-000000000001")
            .unwrap();
        assert_eq!(decision.title, "Retitled");
        assert_eq!(decision.status, crate::model::DecisionStatus::Accepted);
    }
}
