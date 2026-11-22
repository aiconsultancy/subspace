//! Only active agents count, and a crew member's channel is never guessed
//! between captains.

use serde_json::json;
use subspace_api_client::model::Fleet;

use super::super::Roster;
use super::{agent, fleet, fleet_with};

#[test]
fn an_agent_that_is_no_longer_active_has_no_rank_or_ship() {
    let mut retired = agent("aurora-lyra", "captain", Some("aurora-lyra"), Some("Lyra"));
    retired["available"] = json!(false);
    let aurora = fleet(
        "aurora",
        vec![
            retired,
            agent("Wren", "crew", Some("aurora-wren"), Some("Lyra")),
        ],
    );
    let roster = Roster::new([&aurora]);
    assert_eq!(roster.agent("aurora-lyra"), None, "a retired captain");
    assert_eq!(roster.channel("#aurora-lyra"), None, "nor its ship channel");
    assert_eq!(
        roster.agent("aurora-wren").map(|wren| wren.channel.clone()),
        Some(None),
        "its crew have no captain's channel to go to"
    );
}

#[test]
fn a_crew_member_talks_in_its_ship_s_recorded_channel_else_its_only_captain_s() {
    let crew = || agent("Wren", "crew", Some("aurora-wren"), Some("Lyra"));
    let captains = || {
        vec![
            agent("Lyra", "captain", Some("aurora-lyra"), Some("Lyra")),
            agent("Lyra-two", "captain", Some("aurora-lyra-two"), Some("Lyra")),
            crew(),
        ]
    };
    let channel = |fleet: &Fleet| {
        Roster::new([fleet])
            .agent("aurora-wren")
            .and_then(|wren| wren.channel.clone())
    };
    assert_eq!(
        channel(&fleet("aurora", captains())),
        None,
        "two captains: neither is guessed"
    );
    let recorded = fleet_with(
        "aurora",
        captains(),
        json!({"ships": [{"id": "ship-Lyra", "name": "Lyra", "channel": "#lyra-deck"}]}),
    );
    assert_eq!(channel(&recorded).as_deref(), Some("#lyra-deck"));
    let mut wren = crew();
    wren["shipChannel"] = json!("#lyra-own");
    assert_eq!(
        channel(&fleet("aurora", vec![wren])).as_deref(),
        Some("#lyra-own"),
        "the copy on the crew member's own record"
    );
    let mut retired = agent("Lyra-two", "captain", Some("aurora-lyra-two"), Some("Lyra"));
    retired["available"] = json!(false);
    let one_active = fleet(
        "aurora",
        vec![
            agent("Lyra", "captain", Some("aurora-lyra"), Some("Lyra")),
            retired,
            crew(),
        ],
    );
    assert_eq!(
        channel(&one_active).as_deref(),
        Some("#aurora-lyra"),
        "a retired captain does not count"
    );
}

#[test]
fn a_ship_channel_of_an_agent_follows_the_roster_s_rule() {
    let mut retired = agent("Old", "captain", Some("aurora-old"), Some("Old"));
    retired["available"] = json!(false);
    let aurora = fleet_with(
        "aurora",
        vec![
            agent("Lyra", "captain", Some("aurora-lyra"), Some("Lyra")),
            agent("Wren", "crew", Some("aurora-wren"), Some("Lyra")),
            agent("Orion", "captain", Some("aurora-orion"), Some("Orion")),
            agent("admiral-vance", "rearAdmiral", Some("aurora-admiral"), None),
            retired,
        ],
        json!({"ships": [{"id": "ship-Orion", "name": "Orion", "channel": "#orion-deck"}]}),
    );
    let channel = |user: &str| super::super::ship_channel_of(&aurora, user);
    assert_eq!(channel("user-Lyra").as_deref(), Some("#aurora-lyra"));
    assert_eq!(channel("user-Wren").as_deref(), Some("#aurora-lyra"));
    assert_eq!(channel("user-Orion").as_deref(), Some("#orion-deck"));
    assert_eq!(channel("user-admiral-vance"), None, "no ship");
    assert_eq!(channel("user-Old"), None, "retired");
    assert_eq!(channel("user-unknown"), None);
}
