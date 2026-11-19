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
    let channel = match_channel(&initiative.channels, &ship)?;
    Some(Route {
        issue: issue.identifier.clone(),
        ship,
        channel,
        endpoint,
    })
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
