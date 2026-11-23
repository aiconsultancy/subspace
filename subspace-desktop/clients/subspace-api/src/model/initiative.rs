//! Initiatives the caller belongs to and the fleets they are assigned to.

use serde::Deserialize;

use super::decision::AgentRank;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemberInitiative {
    pub id: String,
    pub name: String,
    pub channels: String,
    /// The fleet the initiative is assigned to, when any.
    #[serde(default, rename = "fleetId")]
    pub fleet_id: Option<String>,
    #[serde(default)]
    pub fleet: Option<Fleet>,
}

/// A fleet: one rear admiral and its captains, the IRC server they use and
/// their standard channels.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Fleet {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub irc_host: Option<String>,
    #[serde(default)]
    pub irc_port: Option<u16>,
    #[serde(default)]
    pub irc_tls: Option<bool>,
    #[serde(default)]
    pub irc_websocket_url: Option<String>,
    #[serde(default)]
    pub rear_admiral_name: Option<String>,
    #[serde(default)]
    pub captain_names: Vec<String>,
    #[serde(default)]
    pub vm_host: Option<String>,
    /// The fleet's Stargate Fleet API address, when the API records one: an
    /// absolute `https` URL, or `http` on loopback, without a trailing
    /// slash. Never a token: each person saves their own on their machine.
    #[serde(default)]
    pub fleet_api_url: Option<String>,
    /// The channel for people, the rear admiral and the captains, when the
    /// fleet records one.
    #[serde(default)]
    pub starbase_channel: Option<String>,
    /// The channel for the rear admiral and its captains, when recorded.
    #[serde(default)]
    pub command_channel: Option<String>,
    /// The fleet's agents in rank and name order.
    #[serde(default)]
    pub agents: Vec<FleetAgent>,
    /// The fleet's ships in name order.
    #[serde(default)]
    pub ships: Vec<FleetShip>,
}

/// One agent of a fleet, as the fleet reports it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct FleetAgent {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    pub rank: AgentRank,
    #[serde(default)]
    pub ship_id: Option<String>,
    #[serde(default)]
    pub ship: Option<String>,
    /// The agent's nick on the fleet's IRC server, which the server ties to
    /// the account of the same name. `None` when the API does not know it.
    #[serde(default)]
    pub irc_nick: Option<String>,
    /// The agent's ship channel, when its ship records one.
    #[serde(default)]
    pub ship_channel: Option<String>,
    #[serde(default)]
    pub available: bool,
}

/// One ship of a fleet.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct FleetShip {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// The ship's channel, when recorded.
    #[serde(default)]
    pub channel: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Fleet, FleetAgent};
    use crate::model::AgentRank;

    #[test]
    fn a_fleet_reports_its_agents_nicks_ships_and_standard_channels() {
        let fleet: Fleet = serde_json::from_value(serde_json::json!({
            "id": "fleet-aurora", "slug": "aurora", "name": "Aurora",
            "starbaseChannel": "#aurora-starbase", "commandChannel": null,
            "agents": [
                {"userId": "u1", "name": "Vega", "rank": "rearAdmiral", "shipId": null,
                 "ship": null, "ircNick": "aurora-admiral", "shipChannel": null,
                 "available": true},
                {"userId": "u2", "name": "Lyra", "rank": "captain", "shipId": "s1",
                 "ship": "Lattice", "ircNick": null, "shipChannel": "#aurora-lyra",
                 "available": false},
                {"userId": "u3", "name": "Wren", "rank": "somethingNew"}
            ],
            "ships": [{"id": "s1", "name": "Lattice", "channel": "#aurora-lyra"}]
        }))
        .unwrap();
        assert_eq!(fleet.starbase_channel.as_deref(), Some("#aurora-starbase"));
        assert_eq!(fleet.command_channel, None);
        let ranks: Vec<AgentRank> = fleet.agents.iter().map(|agent| agent.rank).collect();
        assert_eq!(
            ranks,
            [AgentRank::RearAdmiral, AgentRank::Captain, AgentRank::Other]
        );
        let FleetAgent {
            irc_nick,
            ship_channel,
            ..
        } = &fleet.agents[1];
        assert_eq!(
            (irc_nick, ship_channel.as_deref()),
            (&None, Some("#aurora-lyra"))
        );
        assert_eq!(fleet.ships[0].channel.as_deref(), Some("#aurora-lyra"));
        let legacy: Fleet = serde_json::from_value(serde_json::json!({"id": "old"})).unwrap();
        assert!(legacy.agents.is_empty() && legacy.ships.is_empty());
        assert_eq!(legacy.fleet_api_url, None);
        let addressed: Fleet = serde_json::from_value(serde_json::json!({
            "id": "aurora", "fleetApiUrl": "https://fleet.example.com"
        }))
        .unwrap();
        assert_eq!(
            addressed.fleet_api_url.as_deref(),
            Some("https://fleet.example.com")
        );
    }
}
