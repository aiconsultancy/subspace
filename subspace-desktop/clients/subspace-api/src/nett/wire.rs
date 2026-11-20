//! NETT's JSON in the desktop's models. NETT's issue, comment and activity
//! shapes are close to the desktop's, so most of the mapping renames a few
//! fields and supplies defaults; decisions, pull requests, images and
//! members have their own, explicit conversions.

use serde_json::{Map, Value, json};

use super::scope::Organisation;
use crate::model::Me;

/// Rewrites a NETT issue-tracking response in place so it reads as the
/// desktop models:
/// - an issue's team is the desktop project (`teamId`, `teamKey`), and
///   NETT's planning project moves to `planningProjectId`;
/// - a person without a kind is human, and a former-member flag defaults
///   to false;
/// - an issue gets the Subspace-only fields empty: no to-dos, no to-do
///   counts, last activity at its last update, and a cycle ID from its
///   cycle.
pub(crate) fn normalise(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(normalise),
        Value::Object(object) => {
            object.values_mut().for_each(normalise);
            team_as_project(object);
            person(object);
            agent_actor(object);
            issue(object);
        }
        _ => {}
    }
}

fn rename(object: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = object.remove(from) {
        object.insert(to.to_owned(), value);
    }
}

fn team_as_project(object: &mut Map<String, Value>) {
    if !object.contains_key("teamId") {
        return;
    }
    if object
        .get("project")
        .is_some_and(|project| project.get("id").is_some() && project.get("name").is_some())
    {
        rename(object, "project", "planningProject");
    }
    rename(object, "projectId", "planningProjectId");
    rename(object, "teamId", "projectId");
    rename(object, "teamKey", "projectKey");
}

fn person(object: &mut Map<String, Value>) {
    if object.contains_key("userId")
        && object.contains_key("displayName")
        && !object.contains_key("kind")
    {
        object.insert("kind".into(), json!("human"));
        object
            .entry("isFormerMember")
            .or_insert_with(|| json!(false));
        if object.get("displayName").is_some_and(Value::is_null) {
            object.insert("displayName".into(), json!(""));
        }
    }
}

/// The desktop's person for an agent actor: the agent's name, rank, ship,
/// fleet and model, marked as an agent, with the person it acted for from
/// the actor or else from `fallback`, the field the agent is shown in.
fn agent_person(actor: &Value, fallback: Option<&Value>) -> Option<Value> {
    if actor.get("type").and_then(Value::as_str) != Some("agent") {
        return None;
    }
    let agent = actor.get("agent")?;
    let field = |key: &str| agent.get(key).cloned().unwrap_or(Value::Null);
    let human = |person: &&Value| {
        person.is_object() && person.get("kind").and_then(Value::as_str) != Some("agent")
    };
    // The actor's own person, unless an earlier pass already put the agent
    // there, in which case the person it acted for went with it.
    let own = actor.get("person");
    let acted_for = own
        .filter(human)
        .or_else(|| {
            own.and_then(|person| person.get("onBehalfOf"))
                .filter(human)
        })
        .or(fallback.filter(human))
        .cloned()
        .map(|mut person| {
            if let Some(object) = person.as_object_mut() {
                object.remove("onBehalfOf");
                self::person(object);
            }
            person
        })
        .unwrap_or(Value::Null);
    Some(json!({
        "userId": field("id"),
        "displayName": agent.get("name").and_then(Value::as_str).unwrap_or("Agent"),
        "isFormerMember": agent.get("isActive").and_then(Value::as_bool) == Some(false),
        "kind": "agent",
        "agent": {
            "rank": field("rank"),
            "ship": field("ship"),
            "fleetId": field("fleetId"),
            "model": field("model"),
        },
        "onBehalfOf": acted_for,
    }))
}

/// Agent attribution, once NETT records it. The agent is shown where its
/// work appears, and the person it acted for stays with it:
/// - an activity entry's `attribution` names the agent, while its legacy
///   actor is that person (an older shape put the agent on the actor);
/// - a comment's `actor` is the agent that wrote it, its `actualAuthor`
///   the person;
/// - an issue's `createdByActor` is the agent that created it.
fn agent_actor(object: &mut Map<String, Value>) {
    let legacy = object.get("person").cloned();
    if let Some(person) = object
        .get("attribution")
        .and_then(|actor| agent_person(actor, legacy.as_ref()))
    {
        object.insert("type".into(), json!("agent"));
        object.insert("person".into(), person);
    } else if object.get("type").and_then(Value::as_str) == Some("agent")
        && let Some(person) = agent_person(&Value::Object(object.clone()), None)
    {
        object.insert("person".into(), person);
    }
    if object.contains_key("actualAuthor")
        && let Some(author) = object
            .get("actor")
            .and_then(|actor| agent_person(actor, object.get("actualAuthor")))
    {
        object.insert("actualAuthor".into(), author);
    }
    if let Some(creator) = object
        .get("createdByActor")
        .and_then(|actor| agent_person(actor, object.get("createdBy")))
    {
        object.insert("createdBy".into(), creator);
    }
}

fn issue(object: &mut Map<String, Value>) {
    let shaped = ["identifier", "state", "projectKey", "version"]
        .iter()
        .all(|key| object.contains_key(*key));
    if !shaped {
        return;
    }
    object
        .entry("todoCounts")
        .or_insert_with(|| json!({"total": 0, "done": 0}));
    object.entry("todos").or_insert_with(|| json!([]));
    object.entry("commentCount").or_insert_with(|| json!(0));
    object.entry("labels").or_insert_with(|| json!([]));
    object.entry("blocked").or_insert_with(|| json!(false));
    if !object.contains_key("lastActivityAt") {
        let updated = object.get("updatedAt").cloned().unwrap_or(Value::Null);
        object.insert("lastActivityAt".into(), updated);
    }
    if object.get("cycleId").is_none_or(Value::is_null) {
        let cycle = object
            .get("cycle")
            .and_then(|cycle| cycle.get("id"))
            .cloned()
            .unwrap_or(Value::Null);
        object.insert("cycleId".into(), cycle);
    }
}

/// The caller from `/auth/context`, or `None` when the context has no user.
pub(crate) fn me(context: &Value) -> Option<Me> {
    let user = context.get("user");
    let id = context
        .get("userId")
        .or_else(|| user.and_then(|user| user.get("id")))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?;
    let text = |key| {
        user.and_then(|user| user.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let name = [text("firstName"), text("lastName")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let display_name = if name.is_empty() { text("email") } else { name };
    serde_json::from_value(json!({"userId": id, "kind": "human", "displayName": display_name})).ok()
}

/// The accessible organisations from `/auth/context`.
pub(crate) fn organisations(context: &Value) -> Vec<Organisation> {
    context
        .get("organisations")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// A NETT initiative's picker label: its `path` of ancestor names ending with
/// its own, as `Platform › Desktop`, or its name when there is no path.
/// Path entries are names or `{name}` objects.
pub(crate) fn initiative_label(item: &Value) -> String {
    let names: Vec<&str> = item
        .get("path")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.as_str().or_else(|| part.get("name")?.as_str()))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    if names.is_empty() {
        item.get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    } else {
        names.join(" › ")
    }
}

/// Labels a NETT initiative with channels by its path, in place.
pub(crate) fn label_initiative(item: &mut Value) {
    let label = initiative_label(item);
    if let Some(object) = item.as_object_mut() {
        object.insert("name".into(), json!(label));
    }
}

/// A NETT initiative as a desktop initiative: no channels and no fleet.
pub(crate) fn initiative(item: &Value) -> Option<Value> {
    let id = item.get("id")?.as_str()?;
    if item.get("archived").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(json!({
        "id": id,
        "name": initiative_label(item),
        "channels": "",
    }))
}

/// A linked pull request in the desktop's shape. NETT keeps the target
/// branch and completion disposition; fields it lacks are empty.
pub(crate) fn pull_request(item: &Value) -> Value {
    let repository = item.get("repository").cloned().unwrap_or_default();
    let full_name = repository
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let owner = full_name.split_once('/').map_or("", |(owner, _)| owner);
    let name = full_name.rsplit('/').next().unwrap_or(full_name);
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
    json!({
        "id": text("id"),
        "repository": {
            "owner": owner,
            "name": name,
            "fullName": full_name,
            "url": repository.get("url").and_then(Value::as_str).unwrap_or_default(),
        },
        "number": item.get("number").cloned().unwrap_or(json!(0)),
        "url": text("url"),
        "title": text("title"),
        "authorLogin": item.get("authorLogin").cloned().unwrap_or(Value::Null),
        "state": text("state"),
        "isDraft": item.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        "headBranch": "",
        "baseBranch": text("targetBranch"),
        "referenceKind": text("referenceKind"),
        "referenceSources": [],
        "referenceActive": item.get("referenceActive").and_then(Value::as_bool).unwrap_or(false),
        "completion": item.get("completionDisposition").and_then(Value::as_str).unwrap_or("none"),
        "mergedAt": null,
        "closedAt": null,
        "githubUpdatedAt": text("lastObservedAt"),
        "lastObservedAt": text("lastObservedAt"),
    })
}

/// A NETT decision as the desktop's decision row. NETT records human
/// deciders, a planning project and an initiative; it has no tags, agent
/// makers or evidence, which stay empty.
pub(crate) fn decision(item: &Value) -> Value {
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
    let decider = item
        .get("deciders")
        .and_then(Value::as_array)
        .and_then(|deciders| deciders.first())
        .cloned()
        .unwrap_or_default();
    let decider_id = decider.get("userId").and_then(Value::as_str);
    let issues = item.get("issues").cloned().unwrap_or_default();
    let issue_items: Vec<Value> = issues
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|issue| {
            json!({
                "id": issue.get("id").and_then(Value::as_str).unwrap_or_default(),
                "identifier": issue.get("identifier").and_then(Value::as_str).unwrap_or_default(),
                "title": issue.get("title").and_then(Value::as_str).unwrap_or_default(),
            })
        })
        .collect();
    let count = issues
        .get("count")
        .and_then(Value::as_i64)
        .unwrap_or(issue_items.len() as i64);
    let summary_id = |key: &str| {
        item.get(key)
            .and_then(|summary| summary.get("id"))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let recorded_by = item
        .get("createdByUserId")
        .and_then(Value::as_str)
        .or(decider_id)
        .unwrap_or_default();
    let mut row = json!({
        "id": text("id"),
        "title": text("title"),
        "status": text("status"),
        "decidedOn": text("decidedOn"),
        "projectId": text("projectId"),
        "projectKey": "",
        "project": item.get("project").cloned().unwrap_or(Value::Null),
        "supersedesId": summary_id("supersedes"),
        "supersededById": summary_id("supersededBy"),
        "supersedes": item.get("supersedes").cloned().unwrap_or(Value::Null),
        "supersededBy": item.get("supersededBy").cloned().unwrap_or(Value::Null),
        "issues": {"items": issue_items, "count": count},
        "tags": [],
        "madeByType": "human",
        "madeByUserId": decider_id,
        "recordedByUserId": recorded_by,
        "version": item.get("version").cloned().unwrap_or(Value::Null),
        "revisionNumber": item.get("revisionNumber").cloned().unwrap_or(json!(1)),
        "createdAt": text("createdAt"),
        "updatedAt": text("updatedAt"),
    });
    if item.get("decision").is_some() {
        let detail = json!({
            "decision": text("decision"),
            "context": text("context"),
            "why": text("why"),
            "implications": text("implications"),
            "alternatives": "",
            "acceptedAt": item.get("acceptedAt").cloned().unwrap_or(Value::Null),
            "acceptedByUserId": item.get("acceptedByUserId").cloned().unwrap_or(Value::Null),
            "provenance": {"madeByType": "human", "madeByUserId": decider_id},
            "recordedBy": {
                "userId": decider_id.unwrap_or(recorded_by),
                "displayName": decider.get("displayName").and_then(Value::as_str).unwrap_or_default(),
                "available": decider.get("available").and_then(Value::as_bool).unwrap_or(true),
                "kind": "human",
            },
        });
        if let (Some(row), Value::Object(detail)) = (row.as_object_mut(), detail) {
            row.extend(detail);
        }
    }
    row
}

/// A NETT image occurrence as the desktop's image metadata. Bytes come
/// through a file grant, so no URL or digest is kept.
pub(crate) fn image(issue: &str, item: &Value) -> Option<Value> {
    let media = item.get("media")?;
    let source = item.get("source").cloned().unwrap_or_default();
    let mut author = source
        .get("author")
        .cloned()
        .unwrap_or_else(|| json!({"userId": "", "displayName": "", "isFormerMember": false}));
    normalise(&mut author);
    Some(json!({
        "id": media.get("mediaId")?.as_str()?,
        "issueId": issue,
        "fileName": media.get("fileName").and_then(Value::as_str).unwrap_or_default(),
        "contentType": media.get("contentType").and_then(Value::as_str).unwrap_or_default(),
        "sizeBytes": media.get("sizeBytes").cloned().unwrap_or(json!(0)),
        "url": "",
        "sha256": "",
        "createdAt": source.get("createdAt").and_then(Value::as_str).unwrap_or_default(),
        "uploadedBy": author,
    }))
}

/// A member as an assignee choice: the user ID and a display name.
pub(crate) fn member(item: &Value) -> Option<(String, String)> {
    let status = item
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active");
    if !status.eq_ignore_ascii_case("active") {
        return None;
    }
    let id = item.get("userId")?.as_str()?.to_owned();
    let text = |key| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
    };
    let name = [text("firstName"), text("lastName")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some((
        id,
        if name.is_empty() {
            text("email").to_owned()
        } else {
            name
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{decision, me, normalise, pull_request};
    use crate::model::{
        Comment, DecisionDetail, IssueDetail, IssueListItem, PullRequest, WorkflowState,
    };
    use serde_json::json;

    fn nett_row(id: &str) -> serde_json::Value {
        json!({
            "id": id, "identifier": "OPS-1", "teamId": "team-1", "teamKey": "OPS",
            "type": "issue", "title": "Fixture", "state": {"id": "s", "name": "Todo", "type": "unstarted"},
            "assignee": {"userId": "u1", "displayName": "Ada Fixture", "isFormerMember": false},
            "priority": 2, "estimate": null, "cycle": {"id": "c1", "number": 3, "state": "current"},
            "dueDate": null, "labels": [], "projectId": "planning-1", "milestoneId": null,
            "parentId": null, "blocked": false, "archivedAt": null,
            "createdAt": "2026-11-01T12:00:00Z", "updatedAt": "2026-11-02T12:00:00Z",
            "version": 4, "progress": null, "commentCount": 1
        })
    }

    #[test]
    fn a_nett_issue_reads_as_a_desktop_issue_with_its_team_as_the_project() {
        let mut value = nett_row("i1");
        value["project"] = json!({"id": "planning-1", "name": "Launch"});
        normalise(&mut value);
        assert_eq!(value["planningProjectId"], "planning-1");
        assert_eq!(value["planningProject"]["name"], "Launch");
        let row: IssueListItem = serde_json::from_value(value).unwrap();
        assert_eq!(row.project_id, "team-1");
        assert_eq!(row.project_key, "OPS");
        assert_eq!(row.assignee.as_ref().unwrap().kind, "human");
        assert_eq!(row.last_activity_at, "2026-11-02T12:00:00Z");
        assert_eq!(row.cycle_id.as_deref(), Some("c1"));
        assert_eq!(row.todo_counts.total, 0);
        assert!(row.summary.is_none() && row.owner.is_none());

        let mut detail = nett_row("i1");
        detail.as_object_mut().unwrap().extend(
            json!({"description": "Text", "createdBy": {"userId": "u1", "displayName": "Ada"},
                   "children": {"items": [nett_row("i2")], "nextCursor": null},
                   "relations": {"items": [], "nextCursor": null}, "parent": null})
            .as_object()
            .unwrap()
            .clone(),
        );
        normalise(&mut detail);
        let detail: IssueDetail = serde_json::from_value(detail).unwrap();
        assert_eq!(detail.children.unwrap().items[0].project_key, "OPS");
        assert!(detail.todos.is_empty());
    }

    #[test]
    fn workflow_states_comments_and_people_map_without_loss() {
        let mut state = json!({"id": "s1", "teamId": "team-1", "name": "Todo", "type": "unstarted",
            "position": 2, "isDefault": true, "issueCount": 4});
        normalise(&mut state);
        let state: WorkflowState = serde_json::from_value(state).unwrap();
        assert_eq!(state.project_id, "team-1");

        let mut comment = json!({"id": "c1", "issueId": "i1", "parentCommentId": null, "body": "Hi",
            "version": 1, "actualAuthor": {"userId": "u1", "displayName": "Ada", "isFormerMember": false},
            "createdAt": "2026-11-01T12:00:00Z", "displayCreatedAt": "2026-11-01T12:00:00Z",
            "editedAt": null, "deletedAt": null, "resolved": false, "resolvedBy": null,
            "resolvedAt": null, "replyCount": 0, "hasReplies": false, "media": [], "mentions": []});
        normalise(&mut comment);
        let comment: Comment = serde_json::from_value(comment).unwrap();
        assert_eq!(comment.actual_author.kind, "human");
    }

    #[test]
    fn decisions_keep_human_deciders_and_leave_subspace_fields_empty() {
        let value = decision(&json!({
            "id": "d1", "title": "Use the queue", "status": "accepted", "decidedOn": "2026-11-10",
            "deciders": [{"userId": "u1", "displayName": "Ada", "available": true}],
            "projectId": null, "project": null,
            "issues": {"availability": "available", "items": [{"id": "i1", "identifier": "OPS-1", "title": "T"}], "hasMore": false},
            "version": 2, "revisionNumber": 1, "decision": "We use it.", "context": "", "why": "Speed",
            "implications": "", "createdByUserId": "u1", "createdAt": "2026-11-10T09:00:00Z",
            "updatedAt": "2026-11-10T09:00:00Z"
        }));
        let detail: DecisionDetail = serde_json::from_value(value).unwrap();
        assert_eq!(detail.item.issues.count, 1);
        assert!(detail.item.tags.is_empty());
        assert_eq!(detail.item.project_key, "");
        assert_eq!(detail.item.made_by_user_id.as_deref(), Some("u1"));
        assert_eq!(detail.recorded_by.display_name, "Ada");
        assert_eq!(detail.why, "Speed");
    }

    #[test]
    fn agent_actors_read_as_agents_with_their_rank_and_ship() {
        let actor = json!({"type": "agent", "source": "mcp",
            "person": {"userId": "u1", "displayName": "Ada Fixture", "isFormerMember": false},
            "agent": {"id": "a1", "name": "Enterprise", "rank": "captain", "ship": "Enterprise",
                      "fleetId": "f1", "model": "m", "isActive": true},
            "relayedCrew": {"name": "Riker", "model": "m"}, "label": null});
        let mut comment = json!({"id": "c1", "issueId": "i1", "parentCommentId": null, "body": "Done",
            "version": 1, "actualAuthor": {"userId": "u1", "displayName": "Ada Fixture", "isFormerMember": false},
            "actor": actor.clone(), "relayedCrew": {"name": "Riker", "model": "m"},
            "createdAt": "2026-11-01T12:00:00Z", "displayCreatedAt": "2026-11-01T12:00:00Z",
            "editedAt": null, "deletedAt": null, "resolved": false, "resolvedBy": null,
            "resolvedAt": null, "replyCount": 0, "hasReplies": false, "media": [], "mentions": []});
        normalise(&mut comment);
        let comment: Comment = serde_json::from_value(comment).unwrap();
        assert_eq!(comment.actual_author.kind, "agent");
        assert_eq!(comment.actual_author.display_name, "Enterprise");
        assert_eq!(
            comment
                .actual_author
                .agent
                .as_ref()
                .unwrap()
                .ship
                .as_deref(),
            Some("Enterprise")
        );
        assert_eq!(comment.relayed_crew.unwrap().name, "Riker");
        // The person the agent acted for stays with it.
        let acted_for = comment.actual_author.on_behalf_of.as_deref().unwrap();
        assert_eq!(acted_for.display_name, "Ada Fixture");
        assert_eq!(acted_for.kind, "human");
        assert_eq!(
            comment.actual_author.attribution(),
            "Enterprise (agent, for Ada Fixture)"
        );

        let mut entry = json!({"actor": actor});
        normalise(&mut entry);
        assert_eq!(entry["actor"]["person"]["displayName"], "Enterprise");
        assert_eq!(entry["actor"]["person"]["kind"], "agent");
        assert_eq!(
            entry["actor"]["person"]["onBehalfOf"]["displayName"],
            "Ada Fixture"
        );
    }

    #[test]
    fn agents_without_fleet_or_ship_keep_the_person_they_acted_for() {
        let crew = json!({"type": "agent", "source": "mcp",
            "person": {"userId": "u2", "displayName": "Grace Fixture", "isFormerMember": false},
            "agent": {"id": "a2", "name": "Wesley", "rank": "crew", "ship": null,
                      "fleetId": null, "model": null, "isActive": true},
            "relayedCrew": null, "label": null});
        // An activity entry keeps its legacy actor, the person, and adds the
        // agent as its attribution.
        let mut entry = json!({"actor": {"type": "human",
            "person": {"userId": "u2", "displayName": "Grace Fixture", "isFormerMember": false},
            "attribution": crew.clone()}});
        normalise(&mut entry);
        let person = &entry["actor"]["person"];
        assert_eq!(entry["actor"]["type"], "agent");
        assert_eq!(person["displayName"], "Wesley");
        assert!(person["agent"]["ship"].is_null() && person["agent"]["fleetId"].is_null());
        assert_eq!(person["onBehalfOf"]["displayName"], "Grace Fixture");
        let person: crate::model::Person = serde_json::from_value(person.clone()).unwrap();
        assert_eq!(person.attribution(), "Wesley (agent, for Grace Fixture)");
        assert_eq!(person.fact_label(), "Wesley (agent, for Grace Fixture)");
        let agent = person.agent.unwrap();
        assert_eq!((agent.rank.as_deref(), agent.ship), (Some("crew"), None));
        // An issue created by an agent shows it, for the person.
        let mut created = json!({"createdByActor": crew,
            "createdBy": {"userId": "u2", "displayName": "Grace Fixture", "isFormerMember": false}});
        normalise(&mut created);
        assert_eq!(created["createdBy"]["displayName"], "Wesley");
        assert_eq!(created["createdBy"]["onBehalfOf"]["userId"], "u2");
    }

    #[test]
    fn initiatives_are_labelled_by_their_path_or_name() {
        use super::{initiative, initiative_label};
        let nested = json!({"id": "i1", "name": "Desktop", "path": ["Platform", "Desktop"]});
        assert_eq!(initiative_label(&nested), "Platform › Desktop");
        let listed = json!({"id": "i1", "name": "Desktop", "archived": false,
            "path": [{"id": "p", "name": "Platform"}, {"id": "i1", "name": "Desktop"}]});
        assert_eq!(initiative(&listed).unwrap()["name"], "Platform › Desktop");
        assert_eq!(initiative_label(&json!({"name": "Platform"})), "Platform");
        assert_eq!(
            initiative_label(&json!({"name": "Platform", "path": []})),
            "Platform"
        );
    }

    #[test]
    fn pull_requests_and_the_caller_map_from_nett_shapes() {
        let pr: PullRequest = serde_json::from_value(pull_request(&json!({
            "id": "p1", "repository": {"id": "r", "name": "example/app", "url": "https://example.com/app"},
            "number": 12, "url": "https://example.com/app/pull/12", "title": "Fix", "authorLogin": "octo",
            "state": "open", "isDraft": false, "targetBranch": "main", "referenceKind": "closing",
            "referenceActive": true, "lastObservedAt": "2026-11-01T12:00:00Z"
        })))
        .unwrap();
        assert_eq!(pr.repository.owner, "example");
        assert_eq!(pr.base_branch, "main");
        let caller =
            me(&json!({"userId": "u1", "user": {"firstName": "Ada", "lastName": "Fixture"}}))
                .unwrap();
        assert_eq!(caller.display_name, "Ada Fixture");
        assert!(me(&json!({"userId": null})).is_none());
    }
}
