//! `irc_write` validation, live checks, previews and whole calls.

use std::sync::mpsc;

use serde_json::json;
use subspace_client_irc::Command;
use subspace_framework::agent_tools::test_support;

use super::super::bridge::Link;
use super::super::tools;
use super::super::write::{self, Write};
use super::{hub_thread, message_to, operation, people, state};
use crate::fixtures::{channel, config, network_id, query};
use crate::state::IrcState;

#[test]
fn operations_are_validated_before_any_connection_is_asked() {
    assert!(write::validate(&[message_to("#subspace", "hello\nworld")]).is_ok());
    assert!(write::validate(&[message_to("bob", "hi")]).is_ok());
    for invalid in [
        message_to("#subspace", "   "),
        message_to("#subspace", &"x".repeat(2001)),
        message_to("#subspace", &"line\n".repeat(11)),
        message_to("#subspace", "\u{1}VERSION\u{1}"),
        message_to("#subspace", "carriage\rreturn"),
        message_to("NickServ", "IDENTIFY secret"),
        message_to("two words", "hi"),
        operation(
            json!({"action":"message","network":"local","target":"#c","text":"a\nb","me":true}),
        ),
        operation(json!({"action":"join","network":"local","channel":"subspace"})),
        operation(json!({"action":"join","network":"local","channel":"#c","key":"has space"})),
        operation(json!({"action":"part","network":"local","channel":"#c","reason":"two\nlines"})),
        operation(json!({"action":"connect","network":" "})),
    ] {
        assert!(
            write::validate(std::slice::from_ref(&invalid)).is_err(),
            "{invalid:?}"
        );
    }
    let many = vec![message_to("#subspace", "hi"); 11];
    assert!(write::validate(&many).unwrap_err().contains("At most 10"));
    let raw: Result<Write, _> = serde_json::from_value(json!({"action":"raw","line":"QUIT"}));
    assert!(raw.is_err());
}

#[test]
fn live_checks_map_each_operation_to_its_runtime_command() {
    let state = state();
    let commands = |operation: Write| {
        let (result, commands) = write::apply(&state, &operation, &people);
        assert_eq!(result.unwrap()["status"], "queued");
        commands
    };
    assert_eq!(
        commands(message_to("#SUBSPACE", "/quit")),
        [Command::Say {
            buffer: channel("#subspace"),
            text: "/quit".into(),
            action: false,
        }]
    );
    assert_eq!(
        commands(operation(
            json!({"action":"message","network":"local","target":"Carol","text":"waves","me":true})
        )),
        [Command::Say {
            buffer: query("carol"),
            text: "waves".into(),
            action: true,
        }]
    );
    assert_eq!(
        commands(message_to("dave", "hi"))[0],
        Command::Say {
            buffer: query("dave"),
            text: "hi".into(),
            action: false,
        }
    );
    assert_eq!(
        commands(operation(
            json!({"action":"join","network":"local","channel":"#new","key":"sesame"})
        )),
        [Command::Join {
            network: network_id(),
            channel: "#new".into(),
            key: Some("sesame".into()),
        }]
    );
    assert_eq!(
        commands(operation(
            json!({"action":"part","network":"local","channel":"#subspace","reason":"bye"})
        )),
        [Command::Part {
            buffer: channel("#subspace"),
            reason: "bye".into(),
        }]
    );
    assert_eq!(
        commands(operation(json!({"action":"disconnect","network":"local"}))),
        [Command::Disconnect {
            network: network_id(),
            reason: String::new(),
        }]
    );
}

#[test]
fn live_checks_refuse_what_the_connection_cannot_do() {
    let state = state();
    let refused = |operation: Write| write::check(&state, &operation, &people).err().unwrap();
    assert!(refused(message_to("#elsewhere", "hi")).contains("Join it first"));
    assert!(
        refused(operation(
            json!({"action":"join","network":"local","channel":"#subspace"})
        ))
        .contains("Already in")
    );
    assert!(
        refused(operation(json!({"action":"connect","network":"local"})))
            .contains("already connected")
    );
    assert!(
        refused(operation(
            json!({"action":"part","network":"other","channel":"#c"})
        ))
        .contains("No IRC network")
    );
    let offline = IrcState::new(&[config()], Vec::new());
    let error = write::check(&offline, &message_to("bob", "hi"), &people)
        .err()
        .unwrap();
    assert_eq!(error, "Acme IRC is not connected.");
    let (result, commands) = write::apply(&offline, &message_to("bob", "hi"), &people);
    assert!(result.is_err() && commands.is_empty());
    assert_eq!(
        write::apply(
            &offline,
            &operation(json!({"action":"connect","network":"local"})),
            &people
        )
        .1,
        [Command::Connect(network_id())]
    );
}

#[test]
fn no_message_goes_to_a_fleet_agent_s_nick_but_its_channel_is_fine() {
    let state = state();
    let agents = |network: &subspace_client_irc::NetworkId, nick: &str| {
        (*network == network_id() && nick.eq_ignore_ascii_case("bob"))
            .then(|| "bob is a fleet agent. Agents talk in channels, where everyone can see: ask in #subspace.".to_owned())
    };
    let error = write::check(&state, &message_to("Bob", "hi"), &agents)
        .err()
        .unwrap();
    assert!(error.contains("bob is a fleet agent"), "{error}");
    let (result, commands) = write::apply(&state, &message_to("bob", "hi"), &agents);
    assert!(result.is_err() && commands.is_empty(), "nothing is sent");
    let me = operation(
        json!({"action":"message","network":"local","target":"bob","text":"waves","me":true}),
    );
    assert!(write::check(&state, &me, &agents).is_err(), "nor an action");
    assert!(write::check(&state, &message_to("#subspace", "hi"), &agents).is_ok());
    assert!(write::check(&state, &message_to("mira", "hi"), &agents).is_ok());
}

#[test]
fn the_preview_names_who_sends_and_every_line() {
    let operations = [
        message_to("#subspace", "first\nsecond"),
        operation(
            json!({"action":"message","network":"local","target":"bob","text":"waves","me":true}),
        ),
        operation(json!({"action":"join","network":"local","channel":"#new","key":"sesame"})),
        operation(json!({"action":"part","network":"local","channel":"#old"})),
    ];
    let labels: Vec<String> =
        std::iter::repeat_n("Acme IRC (local) as subspace".to_owned(), 4).collect();
    let text = write::describe(&operations, &labels);
    assert!(text.contains("sent as you"));
    assert!(text.contains(
        "\n1. Acme IRC (local) as subspace: message to #subspace\n   > first\n   > second"
    ));
    assert!(text.contains("\n2. Acme IRC (local) as subspace: action (/me) in bob\n   > waves"));
    assert!(text.contains("\n3. Acme IRC (local) as subspace: join #new with a channel key"));
    assert!(!text.contains("sesame"));
    assert!(text.contains("\n4. Acme IRC (local) as subspace: leave #old"));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_approval_covers_the_batch_and_it_runs_in_order() {
    let (link, hub) = hub_thread(state());
    let tools = tools(link, None);
    let (approval, invocation) = test_support::invocation(
        json!({"operations":[
            {"action":"message","network":"local","target":"#subspace","text":"hello\nworld"},
            {"action":"join","network":"local","channel":"#new"},
        ]}),
        true,
    );
    let value = tools[1].call(invocation).await.unwrap();
    drop(tools);
    assert_eq!(value["approved"], true);
    assert_eq!(value["results"][0]["success"], true);
    assert_eq!(value["results"][1]["value"]["target"], "#new");
    let summaries = approval.summaries();
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].contains("   > world"), "{}", summaries[0]);
    let sent = hub.join().unwrap();
    assert_eq!(
        sent,
        [
            Command::Say {
                buffer: channel("#subspace"),
                text: "hello\nworld".into(),
                action: false,
            },
            Command::Join {
                network: network_id(),
                channel: "#new".into(),
                key: None,
            },
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_or_refused_batch_sends_nothing() {
    let (link, hub) = hub_thread(state());
    let tools = tools(link, None);
    let batch = json!({"operations":[{"action":"message","network":"local","target":"#subspace","text":"hi"}]});
    let (_, denied) = test_support::invocation(batch, false);
    assert_eq!(tools[1].call(denied).await.unwrap()["approved"], false);
    let (approval, refused) = test_support::invocation(
        json!({"operations":[
            {"action":"message","network":"local","target":"#subspace","text":"hi"},
            {"action":"message","network":"local","target":"#elsewhere","text":"hi"},
        ]}),
        true,
    );
    let error = tools[1].call(refused).await.unwrap_err();
    assert!(
        error.starts_with("Operation 2: Not in #elsewhere"),
        "{error}"
    );
    assert!(approval.summaries().is_empty());
    let (_, networks) = test_support::invocation(json!({"action":"networks"}), true);
    assert_eq!(
        tools[0].call(networks).await.unwrap()["networks"][0]["id"],
        "local"
    );
    drop(tools);
    assert!(hub.join().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_board_that_is_not_running_answers_clearly() {
    let (sender, receiver) = mpsc::channel();
    drop(receiver);
    let tools = tools(Link::Fixed(sender), None);
    let (_, invocation) = test_support::invocation(json!({"action":"networks"}), true);
    let error = tools[0].call(invocation).await.unwrap_err();
    assert!(error.contains("Channels Space is not running"), "{error}");
}
