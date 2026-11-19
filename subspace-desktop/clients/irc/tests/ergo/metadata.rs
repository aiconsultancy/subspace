//! Status propagation against the real local Ergo fixture.

use subspace_client_irc::{Event, Target};

use super::{Client, join, registered_client, server, suffix};

fn wait_status(client: &mut Client, from: usize, nick: &str, status: Option<&str>) {
    let event = client.wait(from, "nickname status", |event| {
        matches!(event, Event::NickStatus { nick: seen, status: text, .. } if seen == nick && text.as_deref() == status)
    });
    assert_eq!(
        event,
        Event::NickStatus {
            network: client.network(),
            nick: nick.into(),
            status: status.map(str::to_string),
        }
    );
}

fn set_status(client: &mut Client, status: Option<&str>) {
    let from = client.mark();
    let command = status.map_or_else(
        || "/raw METADATA * SET status".into(),
        |text| format!("/raw METADATA * SET status :{text}"),
    );
    client.input(Target::Server, &command);
    let nick = client.config.nickname.clone();
    // Ergo echoes SET to its originator as 761/766. Waiting also puts a
    // barrier between a set and another client's join or sync request.
    wait_status(client, from, &nick, status);
}

#[test]
fn subscribed_clients_receive_status_set_change_and_clear() {
    let Some(server) = server() else {
        eprintln!("IRC_TEST_SERVER is not set; skipping the Ergo test");
        return;
    };
    let suffix = suffix();
    let channel = format!("#status-{suffix}");
    let mut a = registered_client("a", &format!("sta{suffix}"), &server);
    let mut b = registered_client("b", &format!("stb{suffix}"), &server);
    join(&mut a, &channel);
    let from = join(&mut b, &channel);
    b.wait(from, "B members", |event| {
        matches!(event, Event::Members { .. })
    });
    for value in [Some("Thinking…"), Some("Running a command"), None] {
        let from = b.mark();
        set_status(&mut a, value);
        wait_status(&mut b, from, &a.config.nickname, value);
    }
    assert!(b.events.iter().any(|event| matches!(event, Event::Capabilities { enabled, .. } if enabled.iter().any(|cap| cap == "draft/metadata-2"))));
}

#[test]
fn joining_during_a_status_delivers_the_current_value() {
    let Some(server) = server() else {
        eprintln!("IRC_TEST_SERVER is not set; skipping the Ergo test");
        return;
    };
    let suffix = suffix();
    let channel = format!("#snapshot-{suffix}");
    let mut a = registered_client("a", &format!("sna{suffix}"), &server);
    let mut b = registered_client("b", &format!("snb{suffix}"), &server);
    let from = join(&mut a, &channel);
    a.wait(from, "A joined", |event| {
        matches!(event, Event::Joined { .. })
    });
    set_status(&mut a, Some("Already thinking…"));
    let from = join(&mut b, &channel);
    wait_status(&mut b, from, &a.config.nickname, Some("Already thinking…"));
    assert!(
        b.events[from..]
            .iter()
            .any(|event| matches!(event, Event::Joined { .. }))
    );
}
