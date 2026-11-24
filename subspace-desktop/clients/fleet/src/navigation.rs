//! Product-level navigation between Spaces. Only fleet and checkout IDs
//! cross the broker; the responder looks up the fleet's connection itself.

use subspace_framework::contracts::{Contract, Request};
use subspace_framework::dashboard::{BoardNavigation, contracts::MessageKey};
use url::Url;

const ROUTE: &str = "subspace-crew-diff://open";

pub struct CrewDiffNavigation;

impl Contract for CrewDiffNavigation {
    const NAME: &'static str = "crew-diff";
    const VERSION: u32 = 1;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCrewDiff {
    pub fleet: String,
    pub ship: String,
    pub member: String,
    pub project: Option<String>,
}

impl Request for OpenCrewDiff {
    type Contract = CrewDiffNavigation;
    const NAME: &'static str = "open-crew-diff";
    type Response = ();
}

impl OpenCrewDiff {
    /// Declare this request and emit `DashboardEvent::Navigate` with the
    /// returned route. Set `origin` to return to the initiating view.
    pub fn navigation(&self) -> BoardNavigation {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("fleet", &self.fleet)
            .append_pair("ship", &self.ship)
            .append_pair("member", &self.member);
        if let Some(project) = &self.project {
            query.append_pair("project", project);
        }
        BoardNavigation {
            responder: MessageKey::request::<Self>(),
            url: format!("{ROUTE}?{}", query.finish()).into(),
            origin: None,
        }
    }

    pub fn parse(route: &str) -> Option<Self> {
        let (prefix, _) = route.split_once('?')?;
        if prefix != ROUTE {
            return None;
        }
        let url = Url::parse(route).ok()?;
        if url.fragment().is_some() {
            return None;
        }
        let (mut fleet, mut ship, mut member, mut project) = (None, None, None, None);
        for (key, value) in url.query_pairs() {
            let field = match key.as_ref() {
                "fleet" => &mut fleet,
                "ship" => &mut ship,
                "member" => &mut member,
                "project" => &mut project,
                _ => return None,
            };
            if !valid_id(&value) || field.replace(value.into_owned()).is_some() {
                return None;
            }
        }
        Some(Self {
            fleet: fleet?,
            ship: ship?,
            member: member?,
            project,
        })
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::OpenCrewDiff;

    #[test]
    fn navigation_round_trips_both_named_and_default_projects() {
        for project in [None, Some("example".to_owned())] {
            let request = OpenCrewDiff {
                fleet: "example-fleet".into(),
                ship: "example-ship".into(),
                member: "example-crew".into(),
                project,
            };
            assert_eq!(
                OpenCrewDiff::parse(&request.navigation().url),
                Some(request)
            );
        }
        assert!(
            OpenCrewDiff::parse("https://example.com?fleet=example&ship=test&member=crew")
                .is_none()
        );
        assert!(
            OpenCrewDiff::parse("subspace-crew-diff://open?fleet=example&ship=%2fapi&member=crew")
                .is_none()
        );
    }
}
