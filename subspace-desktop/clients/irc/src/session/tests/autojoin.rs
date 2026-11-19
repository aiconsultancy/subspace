use super::{
    Carry, Command, Event, NetworkId, Session, channel, command, config, feed, registered, sent,
};

fn select(channels: Option<&[&str]>) -> Command {
    Command::SetAutojoin {
        network: NetworkId("local".into()),
        channels: channels.map(|values| values.iter().map(|value| (*value).to_owned()).collect()),
    }
}

#[test]
fn replaces_only_autojoin_channels_and_restores_local_channels() {
    let mut session = registered("");
    feed(&mut session, &[":subspace!u@h JOIN #subspace"]);
    let manual = |name: &str| Command::Join {
        network: NetworkId("local".into()),
        channel: name.to_owned(),
        key: None,
    };
    assert_eq!(
        sent(&command(&mut session, manual("#manual"))),
        ["JOIN #manual"]
    );
    feed(&mut session, &[":subspace!u@h JOIN #manual"]);
    let first = command(&mut session, select(Some(&["#one", "#manual"])));
    assert_eq!(sent(&first), ["PART #subspace", "JOIN #one"]);
    assert!(
        first
            .events
            .contains(&Event::AutojoinRetired(channel("#subspace")))
    );
    feed(
        &mut session,
        &[":subspace!u@h JOIN #one", ":subspace!u@h PART #subspace"],
    );
    let local = command(&mut session, select(None));
    assert_eq!(sent(&local), ["PART #one", "JOIN #subspace"]);
    assert!(
        local
            .events
            .contains(&Event::AutojoinRetired(channel("#one")))
    );
}

#[test]
fn rapid_switch_discards_stale_joins_and_requeues_a_return_before_part_ack() {
    let mut session = registered("");
    assert_eq!(
        sent(&command(&mut session, select(Some(&["#one"])))),
        ["PART #subspace", "JOIN #one"]
    );
    assert_eq!(
        sent(&command(&mut session, select(Some(&["#two"])))),
        ["PART #one", "JOIN #two"]
    );
    let stale = feed(&mut session, &[":subspace!u@h JOIN #one"]);
    assert!(
        !stale
            .events
            .iter()
            .any(|event| matches!(event, Event::Joined { .. }))
    );
    assert_eq!(
        sent(&command(&mut session, select(Some(&["#one"])))),
        ["PART #two", "JOIN #one"]
    );
}

#[test]
fn offline_switch_prunes_the_previous_managed_channels_from_reconnect() {
    let mut session = registered("");
    feed(
        &mut session,
        &[":subspace!u@h JOIN #subspace", ":subspace!u@h JOIN #manual"],
    );
    let mut carry = session.into_carry();
    let config = config();
    let retired = super::super::configure_autojoin(&mut carry, &config, &select(Some(&["#two"])));
    assert_eq!(retired, [Event::AutojoinRetired(channel("#subspace"))]);
    let mut session = Session::new(config, carry);
    assert_eq!(
        sent(&feed(&mut session, &[":srv 001 subspace :Welcome"])),
        ["JOIN #two", "JOIN #manual"]
    );
}

#[test]
fn registered_switch_respects_rfc1459_channel_equivalence() {
    let mut conf = config();
    conf.channels = vec!["#deck[1]".into()];
    let mut session = Session::new(conf, Carry::default());
    feed(
        &mut session,
        &[
            ":srv 001 subspace :Welcome",
            ":srv 005 subspace CASEMAPPING=rfc1459 :supported",
        ],
    );
    feed(&mut session, &[":subspace!u@h JOIN #deck[1]"]);
    let selected = command(&mut session, select(Some(&["#deck{1}", "#two"])));
    assert_eq!(sent(&selected), ["JOIN #two"]);
    assert!(selected.events.is_empty());
}

#[test]
fn offline_retirement_uses_the_previous_server_mapping_and_buffer_spelling() {
    let mut conf = config();
    conf.channels = vec!["#deck[1]".into()];
    let mut session = Session::new(conf.clone(), Carry::default());
    feed(
        &mut session,
        &[
            ":srv 001 subspace :Welcome",
            ":srv 005 subspace CASEMAPPING=rfc1459 :supported",
            ":subspace!u@h JOIN #deck{1}",
        ],
    );
    let mut carry = session.into_carry();
    assert_eq!(carry.rejoin, ["#deck{1}"]);
    let events = super::super::configure_autojoin(&mut carry, &conf, &select(Some(&["#two"])));
    assert_eq!(events, [Event::AutojoinRetired(channel("#deck{1}"))]);
    let mut session = Session::new(conf, carry);
    assert_eq!(
        sent(&feed(&mut session, &[":srv 001 subspace :Welcome"])),
        ["JOIN #two"]
    );
}
