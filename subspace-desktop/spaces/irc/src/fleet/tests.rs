use serde_json::{Value, json};
use subspace_api_client::model::Fleet;

use super::{Agent, ChannelKind, Rank, Roster};

/// An API agent: `nick` is its IRC nick, when the API knows it.
pub(crate) fn agent(name: &str, rank: &str, nick: Option<&str>, ship: Option<&str>) -> Value {
    json!({
        "userId": format!("user-{name}"), "name": name, "rank": rank,
        "shipId": ship.map(|ship| format!("ship-{ship}")), "ship": ship,
        "ircNick": nick, "shipChannel": null, "available": true,
    })
}

/// A fleet whose record names no channels, so the standard names stand in.
pub(crate) fn fleet(slug: &str, agents: Vec<Value>) -> Fleet {
    fleet_with(slug, agents, json!({}))
}

/// A fleet with `extra` fields, such as its recorded channels.
pub(crate) fn fleet_with(slug: &str, agents: Vec<Value>, extra: Value) -> Fleet {
    let mut value = json!({
        "id": format!("fleet-{slug}"), "slug": slug, "name": slug,
        "ircHost": "irc.example.test", "ircPort": 6697, "ircTls": true,
        "agents": agents,
    });
    if let (Some(fields), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
        fields.extend(extra.clone());
    }
    serde_json::from_value(value).unwrap()
}

/// The aurora fleet: rear admiral Vega, captains Lyra and Orion, Lyra's
/// crew member Wren, and captain Castor, whose nick the API does not know.
pub(crate) fn aurora() -> Fleet {
    fleet(
        "aurora",
        vec![
            agent("admiral-vance", "rearAdmiral", Some("aurora-admiral"), None),
            agent(
                "Captain-Lyra",
                "captain",
                Some("aurora-lyra"),
                Some("Lattice"),
            ),
            agent("Orion", "captain", Some("aurora-orion"), Some("Plover")),
            agent("Wren", "crew", Some("aurora-wren"), Some("Lattice")),
            agent("Castor", "captain", None, Some("Comet")),
        ],
    )
}

fn ship(captain: &str) -> Option<ChannelKind> {
    Some(ChannelKind::Ship {
        captain: captain.to_owned(),
    })
}

#[test]
fn ranks_come_from_the_api_s_nicks_and_never_from_names() {
    let aurora = aurora();
    let roster = Roster::new([&aurora]);
    assert_eq!(roster.rank("aurora-admiral"), Some(Rank::RearAdmiral));
    assert_eq!(roster.rank("Aurora-Lyra"), Some(Rank::Captain), "any case");
    assert_eq!(roster.rank("aurora-orion"), Some(Rank::Captain));
    assert_eq!(roster.rank("aurora-wren"), Some(Rank::Crew));
    for name in [
        "admiral-vance",
        "aurora-admiral-vance",
        "captain-lyra",
        "aurora-captain-lyra",
        "lyra",
        "castor",
        "aurora-castor",
    ] {
        assert_eq!(roster.rank(name), None, "{name}: a name decides nothing");
    }
}

#[test]
fn each_agent_talks_in_its_ship_channel_or_the_starbase() {
    let aurora = aurora();
    let roster = Roster::new([&aurora]);
    let channel = |nick: &str| roster.agent(nick).and_then(|agent| agent.channel.clone());
    assert_eq!(
        channel("aurora-admiral").as_deref(),
        Some("#aurora-starbase")
    );
    assert_eq!(channel("aurora-lyra").as_deref(), Some("#aurora-lyra"));
    assert_eq!(
        channel("aurora-wren").as_deref(),
        Some("#aurora-lyra"),
        "crew talk in their captain's ship channel"
    );
    assert_eq!(
        roster.agent("aurora-orion"),
        Some(&Agent {
            rank: Rank::Captain,
            channel: Some("#aurora-orion".into()),
        })
    );
}

#[test]
fn the_standard_channel_names_stand_in_for_a_record_that_names_none() {
    let aurora = aurora();
    let roster = Roster::new([&aurora]);
    assert_eq!(
        roster.channel("#aurora-starbase"),
        Some(&ChannelKind::Starbase)
    );
    assert_eq!(
        roster.channel("#Aurora-Command"),
        Some(&ChannelKind::Command)
    );
    assert_eq!(roster.channel("#aurora-lyra").cloned(), ship("aurora-lyra"));
    assert_eq!(
        roster.channel("#aurora-orion").cloned(),
        ship("aurora-orion")
    );
    for unknown in [
        "#aurora-wren",
        "#aurora-castor",
        "#aurora-admiral",
        "#aurora-fleet-command",
    ] {
        assert_eq!(roster.channel(unknown), None, "{unknown}");
    }
}

#[test]
fn a_recorded_channel_replaces_its_standard_name() {
    let mut lyra = agent("Lyra", "captain", Some("aurora-lyra"), Some("Lattice"));
    lyra["shipChannel"] = json!("#lattice");
    let orion = agent("Orion", "captain", Some("aurora-orion"), Some("Plover"));
    let aurora = fleet_with(
        "aurora",
        vec![lyra, orion],
        json!({
            "starbaseChannel": "#aurora-base", "commandChannel": "#aurora-bridge",
            "ships": [{"id": "ship-Plover", "name": "Plover", "channel": "#plover"}],
        }),
    );
    let roster = Roster::new([&aurora]);
    assert_eq!(roster.channel("#aurora-base"), Some(&ChannelKind::Starbase));
    assert_eq!(
        roster.channel("#aurora-bridge"),
        Some(&ChannelKind::Command)
    );
    assert_eq!(roster.channel("#lattice").cloned(), ship("aurora-lyra"));
    assert_eq!(
        roster.channel("#plover").cloned(),
        ship("aurora-orion"),
        "the ship's own record"
    );
    for replaced in ["#aurora-starbase", "#aurora-command", "#aurora-lyra"] {
        assert_eq!(roster.channel(replaced), None, "{replaced}");
    }
}

#[test]
fn a_nick_or_channel_that_two_roles_claim_is_unknown() {
    let clash = fleet(
        "aurora",
        vec![
            agent("Vega", "rearAdmiral", Some("aurora-lyra"), None),
            agent("Lyra", "captain", Some("Aurora-Lyra"), Some("Lattice")),
            agent("Mira", "captain", Some("aurora-mira"), Some("Mist")),
        ],
    );
    let roster = Roster::new([&clash]);
    assert_eq!(roster.rank("aurora-lyra"), None);
    assert_eq!(roster.rank("aurora-mira"), Some(Rank::Captain));
    let named_like_the_starbase = fleet(
        "aurora",
        vec![agent(
            "Base",
            "captain",
            Some("aurora-starbase"),
            Some("Hull"),
        )],
    );
    assert_eq!(
        Roster::new([&named_like_the_starbase]).channel("#aurora-starbase"),
        None,
        "a ship named like the Starbase is neither"
    );
    let other = fleet(
        "borealis",
        vec![agent(
            "Lyra",
            "captain",
            Some("aurora-lyra"),
            Some("Lantern"),
        )],
    );
    let aurora = aurora();
    assert_eq!(
        Roster::new([&aurora, &other]).rank("aurora-lyra"),
        None,
        "two fleets claim one nick"
    );
}

#[test]
fn a_rank_needs_the_server_s_account_to_be_the_nick() {
    let aurora = aurora();
    let roster = Roster::new([&aurora]);
    assert_eq!(
        roster.verified("aurora-lyra", Some("Aurora-Lyra")),
        Some(Rank::Captain)
    );
    assert_eq!(roster.verified("aurora-lyra", None), None, "no account");
    assert_eq!(
        roster.verified("aurora-lyra", Some("mira")),
        None,
        "someone else's account behind the agent's nick"
    );
    assert_eq!(roster.verified("mira", Some("mira")), None, "a person");
}

#[test]
fn a_network_that_serves_no_fleet_knows_no_roles() {
    let roster = Roster::new([]);
    assert_eq!(roster, Roster::default());
    assert_eq!(roster.rank("aurora-lyra"), None);
    assert_eq!(roster.channel("#aurora-starbase"), None);
}

#[test]
fn an_unusable_slug_has_no_standard_names_but_keeps_its_agents_and_records() {
    let odd = fleet_with(
        "Not A Slug",
        vec![agent("Lyra", "captain", Some("odd-lyra"), Some("Lattice"))],
        json!({"commandChannel": "#odd-command"}),
    );
    let roster = Roster::new([&odd]);
    assert_eq!(roster.rank("odd-lyra"), Some(Rank::Captain));
    assert_eq!(roster.channel("#odd-command"), Some(&ChannelKind::Command));
    assert_eq!(roster.channel("#Not A Slug-starbase"), None);
}

#[test]
fn ranks_have_their_marks_tones_and_avatars() {
    use subspace_framework::dashboard::Tone;
    use subspace_framework::dashboard::avatar::AgentRank;
    assert_eq!(Rank::Captain.tone(), Some(Tone::Blue));
    assert_eq!(Rank::RearAdmiral.tone(), Some(Tone::Accent));
    assert_eq!(Rank::Crew.tone(), None);
    assert_eq!(Rank::Captain.avatar(), Some(AgentRank::Captain));
    assert_eq!(Rank::Crew.avatar(), None);
    assert_eq!(Rank::Crew.mark(), "CREW");
    assert_eq!(Rank::RearAdmiral.note(), "rear admiral");
}

#[test]
fn a_fleet_that_two_initiatives_share_keeps_its_roles() {
    let (one, two) = (aurora(), aurora());
    let roster = Roster::new([&one, &two]);
    assert_eq!(roster.rank("aurora-lyra"), Some(Rank::Captain));
    assert_eq!(roster.rank("aurora-admiral"), Some(Rank::RearAdmiral));
    assert_eq!(
        roster.channel("#aurora-starbase"),
        Some(&ChannelKind::Starbase)
    );
    assert_eq!(roster.channel("#aurora-lyra").cloned(), ship("aurora-lyra"));
}

#[test]
fn a_ship_whose_captain_is_ambiguous_is_no_ship_channel() {
    let clash = fleet(
        "aurora",
        vec![
            agent("Vega", "rearAdmiral", Some("aurora-lyra"), None),
            agent("Lyra", "captain", Some("aurora-lyra"), Some("Lattice")),
        ],
    );
    let roster = Roster::new([&clash]);
    assert_eq!(roster.rank("aurora-lyra"), None);
    assert_eq!(
        roster.channel("#aurora-lyra"),
        None,
        "no ship for a captain the roster cannot name"
    );
    assert_eq!(
        roster.channel("#aurora-starbase"),
        Some(&ChannelKind::Starbase)
    );
}
