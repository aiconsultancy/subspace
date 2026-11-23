//! Match the issue body reference, current assignee and project initiative.

use std::collections::HashMap;

use subspace_api_client::{
    Client,
    model::{IssueFields, MemberInitiative, Project},
};
use subspace_framework::contracts::issues::recognition::ReferenceRecognizer;

use crate::initiatives::{Endpoint, channel_names};

pub(super) struct Routes<'a> {
    client: &'a Client,
    projects: Vec<Project>,
    initiatives: Vec<MemberInitiative>,
    references: ReferenceRecognizer,
    cache: HashMap<String, Option<Route>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Route {
    pub issue: String,
    pub ship: String,
    pub channel: String,
    pub endpoint: Option<Endpoint>,
}

impl<'a> Routes<'a> {
    pub(super) fn load(client: &'a Client) -> Option<Self> {
        let initiatives = client.member_initiatives().ok()?;
        if initiatives.is_empty() {
            return None;
        }
        let projects = client.all_projects().ok()?;
        let references =
            ReferenceRecognizer::new(projects.iter().map(|project| project.key.as_str()), []);
        Some(Self {
            client,
            projects,
            initiatives,
            references,
            cache: HashMap::new(),
        })
    }

    pub(super) fn resolve(&mut self, body: &str) -> Option<Route> {
        let identifier = unique_reference(&self.references, body)?;
        self.cache
            .entry(identifier.clone())
            .or_insert_with(|| {
                let issue = self.client.issue(&identifier).ok()?;
                find(&issue.fields, &self.projects, &self.initiatives)
            })
            .clone()
    }
}

fn unique_reference(references: &ReferenceRecognizer, body: &str) -> Option<String> {
    let references = references.find(body);
    let identifier = references.first()?.identifier();
    references
        .iter()
        .all(|reference| reference.identifier() == identifier)
        .then(|| identifier.to_owned())
}

fn find(
    issue: &IssueFields,
    projects: &[Project],
    initiatives: &[MemberInitiative],
) -> Option<Route> {
    let assignee = issue
        .assignee
        .as_ref()
        .filter(|person| person.kind == "agent" && !person.is_former_member)?;
    let ship = assignee
        .agent
        .as_ref()?
        .ship
        .as_deref()?
        .to_ascii_lowercase();
    let project = projects
        .iter()
        .find(|project| project.id == issue.project_id && project.archived_at.is_none())?;
    let initiative = initiatives
        .iter()
        .find(|initiative| Some(initiative.id.as_str()) == project.initiative_id.as_deref())?;
    let endpoint = Endpoint::from_fleet(initiative);
    if initiative.fleet_id.is_some() && endpoint.is_none() {
        return None;
    }
    let channel = destination(initiative, &assignee.user_id, &ship)?;
    Some(Route {
        issue: issue.identifier.clone(),
        ship,
        channel,
        endpoint,
    })
}

/// Where a notice for `assignee`'s ship goes: the ship channel the fleet's
/// roster gives the agent, as conversations use it (the ship's recorded
/// channel, else its captain's standard name). Only when the fleet does not
/// list the agent at all, as from an API that reports no agents, does the
/// initiative's one `#<initiative>-<ship>` channel stand in.
fn destination(initiative: &MemberInitiative, assignee: &str, ship: &str) -> Option<String> {
    let listed = initiative
        .fleet
        .as_ref()
        .filter(|fleet| fleet.agents.iter().any(|agent| agent.user_id == assignee));
    match listed {
        Some(fleet) => crate::fleet::ship_channel_of(fleet, assignee),
        None => match_channel(&initiative.channels, ship),
    }
}

fn match_channel(channels: &str, ship: &str) -> Option<String> {
    let mut channels = channel_names(channels)
        .into_iter()
        .map(|channel| channel.to_ascii_lowercase())
        .filter(|channel| ship_channel(channel, ship));
    let first = channels.next()?;
    channels.all(|channel| channel == first).then_some(first)
}

fn ship_channel(channel: &str, ship: &str) -> bool {
    let Some(name) = channel.strip_prefix('#') else {
        return false;
    };
    let Some(initiative) = name
        .strip_suffix(ship)
        .and_then(|prefix| prefix.strip_suffix('-'))
    else {
        return false;
    };
    !ship.is_empty()
        && !initiative.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod slug_tests {
    use super::find;
    use subspace_api_client::model::{IssueListItem, MemberInitiative, Project};

    #[test]
    fn subspace_api_initiatives_keep_their_own_prefix_whatever_the_fleet_slug() {
        let issue: IssueListItem = serde_json::from_value(serde_json::json!({
            "id": "APL-7", "identifier": "APL-7", "projectId": "project-1", "projectKey": "APL",
            "type": "issue", "title": "Fixture", "state": {"id": "s", "name": "Done", "type": "completed"},
            "assignee": {"userId": "agent", "displayName": "Agent", "isFormerMember": false,
                         "kind": "agent", "agent": {"rank": "captain", "ship": "phoenix"}},
            "labels": [], "priority": 2, "blocked": false, "createdAt": "2026-11-01T12:00:00Z",
            "updatedAt": "2026-11-01T12:00:00Z", "version": 1, "commentCount": 0,
            "todoCounts": {"total": 0, "done": 0}, "lastActivityAt": "2026-11-01T12:00:00Z"
        }))
        .unwrap();
        let projects: Vec<Project> = serde_json::from_value(serde_json::json!([
            {"id": "project-1", "key": "APL", "name": "Apollo", "initiativeId": "initiative-1",
             "defaultStateId": "s"}
        ]))
        .unwrap();
        // The fleet's slug differs from the channels' initiative prefix.
        let initiatives: Vec<MemberInitiative> = serde_json::from_value(serde_json::json!([
            {"id": "initiative-1", "name": "Apollo", "channels": "#apollo-general #apollo-phoenix",
             "fleetId": "fleet-1", "fleet": {"id": "fleet-1", "slug": "aurora",
             "ircHost": "irc.example.test", "ircPort": 6697, "ircTls": true}}
        ]))
        .unwrap();
        let route = find(&issue, &projects, &initiatives).expect("the default build still routes");
        assert_eq!(route.channel, "#apollo-phoenix");
    }

    #[test]
    fn a_fleet_that_lists_the_captain_routes_to_its_ship_s_recorded_channel_else_its_nick() {
        let issue: IssueListItem = serde_json::from_value(serde_json::json!({
            "id": "APL-7", "identifier": "APL-7", "projectId": "project-1", "projectKey": "APL",
            "type": "issue", "title": "Fixture", "state": {"id": "s", "name": "Done", "type": "completed"},
            "assignee": {"userId": "agent", "displayName": "Agent", "isFormerMember": false,
                         "kind": "agent", "agent": {"rank": "captain", "ship": "phoenix"}},
            "labels": [], "priority": 2, "blocked": false, "createdAt": "2026-11-01T12:00:00Z",
            "updatedAt": "2026-11-01T12:00:00Z", "version": 1, "commentCount": 0,
            "todoCounts": {"total": 0, "done": 0}, "lastActivityAt": "2026-11-01T12:00:00Z"
        }))
        .unwrap();
        let projects: Vec<Project> = serde_json::from_value(serde_json::json!([
            {"id": "project-1", "key": "APL", "name": "Apollo", "initiativeId": "initiative-1",
             "defaultStateId": "s"}
        ]))
        .unwrap();
        let initiatives = |channel: serde_json::Value| -> Vec<MemberInitiative> {
            serde_json::from_value(serde_json::json!([
                {"id": "initiative-1", "name": "Apollo", "channels": "#apollo-general #apollo-phoenix",
                 "fleetId": "fleet-1", "fleet": {"id": "fleet-1", "slug": "aurora",
                 "ircHost": "irc.example.test", "ircPort": 6697, "ircTls": true,
                 "agents": [{"userId": "agent", "name": "Phoenix", "rank": "captain",
                             "shipId": "s1", "ship": "phoenix", "ircNick": "aurora-phoenix",
                             "available": true}],
                 "ships": [{"id": "s1", "name": "phoenix", "channel": channel}]}}
            ]))
            .unwrap()
        };
        let channel =
            |recorded| find(&issue, &projects, &initiatives(recorded)).map(|route| route.channel);
        assert_eq!(
            channel(serde_json::json!("#phoenix-deck")).as_deref(),
            Some("#phoenix-deck"),
            "the ship's recorded channel, as its conversation uses"
        );
        assert_eq!(
            channel(serde_json::Value::Null).as_deref(),
            Some("#aurora-phoenix"),
            "the roster's own fallback, never the initiative's derived name"
        );
    }
}
