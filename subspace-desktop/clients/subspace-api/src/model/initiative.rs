//! Initiatives the caller belongs to and the fleets they are assigned to.

use serde::Deserialize;

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

/// A fleet: one rear admiral and its captains, and the IRC server they use.
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
}
