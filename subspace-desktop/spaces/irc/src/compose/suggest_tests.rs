use subspace_client_irc::{Event, Member};

use super::{Facts, Offer, Person, fold, suggest};
use crate::fixtures::{at, channel, config, member, message, query};
use crate::state::IrcState;

fn person(nick: &str, prefix: Option<char>) -> Person {
    Person {
        nick: nick.into(),
        prefix,
        away: false,
        rank: None,
        account: None,
    }
}

/// `person` signed in to the account of its own nick.
fn signed(nick: &str, prefix: Option<char>) -> Person {
    Person {
        account: Some(nick.into()),
        ..person(nick, prefix)
    }
}

fn facts() -> Facts {
    Facts {
        tracker: None,
        people: vec![
            person("galactica-captain", Some('@')),
            person("galactica-pilot", None),
            person("gaius", Some('+')),
            person("starbuck", None),
        ],
        everyone: vec![person("galactica-captain", None), person("apollo", None)],
        channels: vec!["#ship-galactica".into(), "#ship-pegasus".into()],
        joined: vec!["#ship-galactica".into()],
        ask: None,
    }
}

fn inserts(offer: Option<Offer>) -> Vec<String> {
    offer.map_or_else(Vec::new, |offer| {
        offer
            .choices
            .into_iter()
            .map(|choice| choice.insert)
            .collect()
    })
}

#[test]
fn an_at_sign_offers_matching_members_and_inserts_the_mention() {
    let offer = suggest("@gal", 4, &facts());
    assert_eq!(offer.as_ref().map(|offer| offer.range.clone()), Some(0..4));
    assert_eq!(
        inserts(offer),
        ["@galactica-captain ", "@galactica-pilot "],
        "the owner's example: @gal shows galactica-* members"
    );
    assert_eq!(
        inserts(suggest("@", 1, &facts())).len(),
        4,
        "a bare @ offers everyone"
    );
    assert_eq!(inserts(suggest("@zz", 3, &facts())), Vec::<String>::new());
}

#[test]
fn the_word_at_the_cursor_completes_in_the_middle_of_a_message() {
    let text = "ping @GAI about it";
    let offer = suggest(text, 9, &facts());
    assert_eq!(offer.as_ref().map(|offer| offer.range.clone()), Some(5..9));
    assert_eq!(inserts(offer), ["@gaius "], "case is ignored");
    assert_eq!(
        suggest(text, text.len(), &facts()),
        None,
        "a plain word opens nothing"
    );
}

#[test]
fn rows_show_the_mode_prefix_and_speak_rank_and_away() {
    let mut facts = facts();
    facts.people[0].away = true;
    let offer = suggest("@galactica-c", 12, &facts).unwrap_or_else(|| panic!("an offer"));
    let item = &offer.choices[0].item;
    assert_eq!(item.label().as_ref(), "galactica-captain");
    assert_eq!(
        item.spoken_label().as_ref(),
        "galactica-captain, operator, away"
    );
}

#[test]
fn a_fleet_agent_s_row_notes_its_rank_as_the_member_list_does() {
    let aurora: subspace_api_client::model::Fleet = serde_json::from_value(serde_json::json!({
        "id": "aurora", "slug": "aurora", "agents": [
            {"name": "Vance", "rank": "rearAdmiral", "ircNick": "aurora-admiral", "available": true},
            {"name": "Lyra", "rank": "captain", "ircNick": "aurora-lyra", "available": true},
            {"name": "Orion", "rank": "captain", "ircNick": "aurora-orion", "available": true}
        ]
    }))
    .unwrap();
    let roster = crate::fleet::Roster::new([&aurora]);
    let facts = Facts {
        people: vec![
            signed("aurora-lyra", Some('@')),
            signed("aurora-admiral", None),
            person("aurora-orion", None),
            person("lyra", None),
        ],
        ..Facts::default()
    }
    .ranked(&roster);
    let spoken = |query: &str| {
        suggest(query, query.len(), &facts)
            .map(|offer| offer.choices[0].item.spoken_label().to_string())
    };
    assert_eq!(
        spoken("@aurora-l").as_deref(),
        Some("aurora-lyra, operator, captain")
    );
    assert_eq!(
        spoken("@aurora-a").as_deref(),
        Some("aurora-admiral, rear admiral")
    );
    assert_eq!(spoken("@lyra").as_deref(), Some("lyra"), "a person");
    assert_eq!(
        spoken("@aurora-o").as_deref(),
        Some("aurora-orion"),
        "no account, no rank"
    );
}

#[test]
fn folding_follows_irc_casemapping() {
    assert_eq!(fold("Gal[A]\\~"), "gal{a}|^");
    let facts = Facts {
        people: vec![person("dr{zoe}", None)],
        ..Facts::default()
    };
    assert_eq!(inserts(suggest("@DR[Z", 5, &facts)), ["@dr{zoe} "]);
}

#[test]
fn a_leading_slash_offers_the_commands_the_composer_handles() {
    let offer = suggest("/jo", 3, &facts()).unwrap_or_else(|| panic!("an offer"));
    assert_eq!(offer.choices.len(), 1);
    assert_eq!(offer.choices[0].insert, "/join ");
    let item = &offer.choices[0].item;
    assert_eq!(item.label().as_ref(), "/join");
    assert_eq!(
        item.spoken_label().as_ref(),
        "/join <#channel> [key], Join a channel"
    );
    assert_eq!(
        inserts(suggest("/q", 2, &facts())),
        ["/query ", "/quit ", "/raw "],
        "an alias such as /quote matches its command"
    );
    assert_eq!(
        suggest("//jo", 4, &facts()),
        None,
        "// sends a literal slash"
    );
    assert_eq!(suggest("say /jo", 7, &facts()), None, "only at the start");
}

#[test]
fn a_commands_first_argument_completes_channels_or_nicks() {
    assert_eq!(
        inserts(suggest("/join ", 6, &facts())),
        ["#ship-galactica ", "#ship-pegasus "],
        "join offers every channel the network knows"
    );
    assert_eq!(
        inserts(suggest("/JOIN ship-p", 12, &facts())),
        ["#ship-pegasus "]
    );
    assert_eq!(
        inserts(suggest("/part ", 6, &facts())),
        ["#ship-galactica "],
        "part offers the channels we are in"
    );
    assert_eq!(inserts(suggest("/msg ap", 7, &facts())), ["apollo "]);
    assert_eq!(
        inserts(suggest("/query @ga", 10, &facts())),
        ["galactica-captain "]
    );
    assert_eq!(suggest("/me wa", 6, &facts()), None, "free text");
    assert_eq!(
        suggest("/msg apollo hi", 14, &facts()),
        None,
        "only the first argument"
    );
}

#[test]
fn a_hash_offers_joined_channels_in_text() {
    assert_eq!(
        inserts(suggest("see #sh", 7, &facts())),
        ["#ship-galactica "]
    );
}

fn joined_state() -> IrcState {
    let room = channel("#ship-galactica");
    let mut state = IrcState::new(&[config()], Vec::new());
    let members: Vec<Member> = ["galactica-pilot", "galactica-captain", "apollo", "subspace"]
        .into_iter()
        .map(|nick| member(nick, ""))
        .collect();
    let events = [
        Event::Joined {
            buffer: room.clone(),
        },
        Event::Members {
            buffer: room.clone(),
            members,
        },
        Event::Line(message(&room, "apollo", "first", 1)),
        Event::Line(message(&room, "galactica-captain", "report", 2)),
        Event::Line(message(&room, "subspace", "on it", 3)),
        Event::Line(message(&query("starbuck"), "starbuck", "psst", 4)),
    ];
    for event in events {
        state.apply(event, at(5));
    }
    state
}

#[test]
fn people_come_recent_speakers_first_then_by_name_without_us() {
    let state = joined_state();
    let facts = Facts::gather(&state, &channel("#ship-galactica"));
    let nicks: Vec<&str> = facts
        .people
        .iter()
        .map(|person| person.nick.as_str())
        .collect();
    assert_eq!(nicks, ["galactica-captain", "apollo", "galactica-pilot"]);
    assert!(
        facts
            .everyone
            .iter()
            .any(|person| person.nick == "starbuck"),
        "command arguments reach the whole network"
    );
    assert_eq!(facts.joined, ["#ship-galactica"]);
}

#[test]
fn a_private_conversation_offers_the_other_person() {
    let state = joined_state();
    let facts = Facts::gather(&state, &query("starbuck"));
    assert_eq!(inserts(suggest("@", 1, &facts)), ["@starbuck "]);
}

#[test]
fn a_fleet_channel_offers_ask_before_the_irc_client_s_commands() {
    let facts = facts().asking(Some("aurora-lyra"));
    let offer = suggest("/", 1, &facts).unwrap_or_else(|| panic!("an offer"));
    let labels: Vec<String> = offer
        .choices
        .iter()
        .map(|choice| choice.item.label().to_string())
        .collect();
    assert_eq!(labels.first().map(String::as_str), Some("/ask"));
    assert!(labels.len() > 1, "the client's commands follow");
    let ask = &offer.choices[0];
    assert_eq!(ask.insert, "/ask ");
    assert_eq!(
        ask.item.spoken_label().as_ref(),
        "/ask <text>, Ask aurora-lyra and track the answer, a command of this channel"
    );
    assert_eq!(
        inserts(suggest("/as", 3, &facts))
            .first()
            .map(String::as_str),
        Some("/ask ")
    );
    assert!(
        !inserts(suggest("/jo", 3, &facts)).contains(&"/ask ".to_owned()),
        "only when the name matches"
    );
}

#[test]
fn ask_is_not_offered_outside_a_fleet_channel() {
    assert!(!inserts(suggest("/", 1, &facts())).contains(&"/ask ".to_owned()));
}
