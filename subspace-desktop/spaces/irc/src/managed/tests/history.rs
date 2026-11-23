//! An account network's stored history: kept under an id only that person
//! and server have, never deleted when the account changes, restored when
//! the network runs again in the same session, and shown in the buffer
//! list and in search only while its person is signed in.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_client_irc::{BufferKey, Command, Line, NetworkId, Target};
use subspace_framework::dashboard::full_text::{Filters, Query, Ranking, RecordQuery, Service};

use super::support::{Api, TestEnv, connect, fleets, member_of, person, settle};
use crate::fixtures::{channel, message};
use crate::hub::{self, Hub};
use crate::initiatives::Endpoint;
use crate::managed::ids::network_id;
use crate::store::tests::{recent, service};
use crate::store::{self, Link, Op};
use crate::tests::{mount_hub, settings};

fn nav(network: &NetworkId) -> BufferKey {
    BufferKey {
        network: network.clone(),
        target: Target::Channel("#aurora-nav".into()),
    }
}

fn publish(service: &Service, lines: Vec<Line>) {
    let errors = Link::new(service.clone()).run(vec![Op::Publish(lines)], chrono::Utc::now());
    assert!(errors.is_empty(), "{errors:?}");
}

/// Every stored line's text: what is kept, shown or not.
fn kept(service: &Service) -> usize {
    let query = RecordQuery {
        filters: Filters::default(),
        before: None,
        limit: 1000,
    };
    service
        .records(&store::scope(service.app_id()), &query)
        .map_or(0, |page| page.documents.len())
}

/// What a search for `text` finds, by body.
fn found(service: &Service, text: &str) -> Vec<String> {
    let query = Query {
        text: text.into(),
        boards: vec![crate::ID.into()],
        limit: 20,
        ranking: Ranking::Bm25,
    };
    let mut bodies: Vec<String> = service
        .search(&query)
        .map(|response| {
            response
                .hits
                .into_iter()
                .map(|hit| hit.document.body)
                .collect()
        })
        .unwrap_or_default();
    bodies.sort();
    bodies
}

fn held(hub: &Entity<Hub>, buffer: &BufferKey, cx: &VisualTestContext) -> Vec<String> {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(buffer)
            .map(|held| {
                held.entries
                    .iter()
                    .map(|entry| entry.line.text.clone())
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Signs `who` in at `generation`, with their memberships on the aurora
/// server, as a switch or a fresh sign-in does.
fn sign_in_as(
    hub: &Entity<Hub>,
    (api, env): (&Api, &TestEnv),
    (who, generation): ((&str, &str), u64),
    cx: &mut VisualTestContext,
) {
    api.sign_in(200, person(who.0, Some(who.1)));
    *env.generation.lock().unwrap() = Some(generation);
    settle(hub, cx);
    member_of(
        hub,
        (Some(generation), who.0),
        fleets("127.0.0.1", None),
        cx,
    );
    settle(hub, cx);
}

fn sign_out(
    hub: &Entity<Hub>,
    (api, env): (&Api, &TestEnv),
    generation: u64,
    cx: &mut VisualTestContext,
) {
    api.sign_in(401, json!({}));
    *env.generation.lock().unwrap() = Some(generation);
    settle(hub, cx);
}

/// A hub keeping messages in a fresh store that holds a line of Ada's
/// account network on the aurora server and one of the saved network,
/// signed in as Ada at generation 1.
fn ada_signed_in(
    cx: &mut TestAppContext,
) -> (
    Service,
    tempfile::TempDir,
    Entity<Hub>,
    Api,
    std::sync::Arc<TestEnv>,
    &mut VisualTestContext,
) {
    let (root, service) = service();
    publish(
        &service,
        vec![
            recent(message(&nav(&ada()), "lyra", "launch window for ada", 1)),
            recent(message(
                &channel("#subspace"),
                "alice",
                "launch window saved",
                2,
            )),
        ],
    );
    let store = service.clone();
    let (_board, hub, cx) = mount_hub(cx, move |cx| {
        hub::install_with_store_for_test(&settings(), store, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        found(&service, "launch"),
        ["launch window saved"],
        "nobody signed in: only saved networks' lines are found"
    );
    let api = Api::new(&["127.0.0.1"]);
    let env = connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(held(&hub, &nav(&ada()), cx), ["launch window for ada"]);
    assert_eq!(found(&service, "launch").len(), 2);
    (service, root, hub, api, env, cx)
}

fn ada() -> NetworkId {
    network_id("user-ada", &aurora())
}

fn aurora() -> Endpoint {
    Endpoint {
        host: "127.0.0.1".into(),
        port: 6697,
        tls: true,
    }
}

#[gpui_kit::test]
fn switching_to_another_person_and_back_keeps_and_hides_the_history(cx: &mut TestAppContext) {
    let (service, _root, hub, api, env, cx) = ada_signed_in(cx);
    sign_in_as(&hub, (&api, &env), (("user-bob", "bob"), 2), cx);
    assert!(held(&hub, &nav(&ada()), cx).is_empty());
    assert!(held(&hub, &nav(&network_id("user-bob", &aurora())), cx).is_empty());
    assert_eq!(found(&service, "launch"), ["launch window saved"]);
    assert_eq!(kept(&service), 2, "nothing is deleted");
    sign_in_as(&hub, (&api, &env), (("user-ada", "ada"), 3), cx);
    assert_eq!(held(&hub, &nav(&ada()), cx), ["launch window for ada"]);
    assert_eq!(found(&service, "launch").len(), 2);
    hub.read_with(cx, |hub, _| {
        let anchors = hub
            .sent()
            .iter()
            .filter(
                |command| matches!(command, Command::Anchors { network, .. } if *network == ada()),
            )
            .count();
        assert_eq!(anchors, 2, "each time her network starts");
    });
}

#[gpui_kit::test]
fn signing_out_and_in_again_restores_the_history_without_a_restart(cx: &mut TestAppContext) {
    let (service, _root, hub, api, env, cx) = ada_signed_in(cx);
    sign_out(&hub, (&api, &env), 2, cx);
    assert!(held(&hub, &nav(&ada()), cx).is_empty());
    assert_eq!(found(&service, "launch"), ["launch window saved"]);
    sign_in_as(&hub, (&api, &env), (("user-ada", "ada"), 3), cx);
    assert_eq!(held(&hub, &nav(&ada()), cx), ["launch window for ada"]);
    assert_eq!(kept(&service), 2);
}

/// Whether her network starts before its history is back.
fn configures_ada(command: &Command) -> bool {
    matches!(command, Command::Configure(networks)
        if networks.iter().any(|network| network.id == ada()))
}

/// Watches every change of the hub for her network configured while one
/// of `buffers` is not restored yet.
fn watch_early(
    hub: &Entity<Hub>,
    buffers: Vec<BufferKey>,
    cx: &mut VisualTestContext,
) -> std::rc::Rc<std::cell::Cell<bool>> {
    let early = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = std::rc::Rc::clone(&early);
    cx.update(|_, cx| {
        cx.observe(hub, move |hub, cx| {
            let hub = hub.read(cx);
            let configured = hub.sent().iter().any(configures_ada);
            let restored = buffers.iter().all(|buffer| {
                hub.state
                    .buffer(buffer)
                    .is_some_and(|held| !held.entries.is_empty())
            });
            seen.set(seen.get() || (configured && !restored));
        })
        .detach();
    });
    early
}

/// The conversations her network's first command after it starts says
/// where to resume, before it can connect.
fn anchored(hub: &Entity<Hub>, cx: &VisualTestContext) -> Vec<Target> {
    hub.read_with(cx, |hub, _| {
        let sent = hub.sent();
        let started = sent
            .iter()
            .position(configures_ada)
            .expect("her network runs");
        let next = sent[started + 1..].iter().find(|command| {
            command.network() == Some(&ada()) && !matches!(command, Command::SetAutojoin { .. })
        });
        let Some(Command::Anchors { anchors, .. }) = next else {
            panic!("her network's next command is not its anchors: {sent:?}");
        };
        let mut targets: Vec<Target> = anchors.iter().map(|anchor| anchor.target.clone()).collect();
        targets.sort_by_key(|target| format!("{target:?}"));
        targets
    })
}

#[gpui_kit::test]
fn an_account_network_connects_only_once_its_history_is_restored(cx: &mut TestAppContext) {
    let lyra = BufferKey {
        network: ada(),
        target: Target::Query("lyra".into()),
    };
    let (_root, service) = service();
    publish(
        &service,
        vec![
            recent(message(&nav(&ada()), "lyra", "launch window for ada", 1)),
            recent(message(&lyra, "lyra", "docking at nine", 2)),
        ],
    );
    let (_board, hub, cx) = mount_hub(cx, move |cx| {
        hub::install_with_store_for_test(&settings(), service, cx)
    });
    cx.run_until_parked();
    let early = watch_early(&hub, vec![nav(&ada()), lyra.clone()], cx);
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert!(!early.get(), "configured before its history was restored");
    assert_eq!(held(&hub, &lyra, cx), ["docking at nine"]);
    assert_eq!(
        anchored(&hub, cx),
        [Target::Channel("#aurora-nav".into()), lyra.target],
        "the channel and the private conversation"
    );
}
