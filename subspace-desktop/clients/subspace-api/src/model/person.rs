//! People and agents as the tracker credits them.
use serde::Deserialize;

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Person {
    pub user_id: String,
    pub display_name: String,
    pub is_former_member: bool,
    pub kind: String,
    pub agent: Option<Agent>,
    /// For a NETT agent, the person it acted for.
    #[serde(default)]
    pub on_behalf_of: Option<Box<Person>>,
}

impl Person {
    /// The name as shown where the person is credited: for a NETT agent,
    /// marked as an agent with the person it acted for; otherwise the name
    /// alone, as Subspace API's people have always been shown.
    pub fn attribution(&self) -> String {
        match &self.on_behalf_of {
            Some(person) => format!("{} (agent, for {})", self.display_name, person.display_name),
            None => self.display_name.clone(),
        }
    }

    /// The name in a list of facts: a former member marked as such, then a
    /// NETT agent's attribution.
    pub fn fact_label(&self) -> String {
        let name = if self.is_former_member {
            format!("{} (former member)", self.display_name)
        } else {
            self.display_name.clone()
        };
        match &self.on_behalf_of {
            Some(person) => format!("{name} (agent, for {})", person.display_name),
            None => name,
        }
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Agent {
    pub rank: Option<String>,
    pub ship: Option<String>,
    /// NETT's fleet for the agent; none for an agent outside a fleet.
    #[serde(default)]
    pub fleet_id: Option<String>,
    /// The model the agent runs, when NETT records it.
    #[serde(default)]
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Person;

    #[test]
    fn subspace_people_and_agents_keep_their_names_and_former_member_mark() {
        // A Subspace API agent that left the organisation: no agent suffix
        // where it is credited, and the former-member mark in facts.
        let agent: Person = serde_json::from_value(serde_json::json!({
            "userId": "agent-1", "displayName": "Enterprise", "isFormerMember": true,
            "kind": "agent", "agent": {"rank": "captain", "ship": "Enterprise"}
        }))
        .unwrap();
        assert_eq!(agent.attribution(), "Enterprise");
        assert_eq!(agent.fact_label(), "Enterprise (former member)");
        let human: Person = serde_json::from_value(serde_json::json!({
            "userId": "u1", "displayName": "Ada", "isFormerMember": false,
            "kind": "human", "agent": null
        }))
        .unwrap();
        assert_eq!(
            (human.attribution(), human.fact_label()),
            ("Ada".into(), "Ada".into())
        );
    }
}
