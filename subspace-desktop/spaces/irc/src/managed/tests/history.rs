//! An account network's stored history: kept under an id only that person
//! and server have, restored when the network runs again, and deleted
//! once another person signs in, so neither history nor search shows it.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Command, Line, NetworkId, Target};
use subspace_framework::dashboard::full_text::{Filters, RecordQuery, Service};

use super::support::{Api, connect, fleets, member_of, settle};
use crate::fixtures::message;
use crate::hub::{self, Hub};
use crate::initiatives::Endpoint;
use crate::managed::plan::network_id;
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

fn stored_texts(service: &Service) -> Vec<String> {
    let query = RecordQuery {
        filters: Filters::default(),
        before: None,
        limit: 1000,
    };
    let mut texts: Vec<String> = service
        .records(&store::scope(service.app_id()), &query)
        .map(|page| {
            page.documents
                .into_iter()
                .map(|document| document.body)
                .collect()
        })
        .unwrap_or_default();
    texts.sort();
    texts
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

#[gpui_kit::test]
fn an_account_network_s_history_returns_with_it_and_another_person_s_is_deleted(
    cx: &mut TestAppContext,
) {
    let (_root, service) = service();
    let endpoint = Endpoint {
        host: "127.0.0.1".into(),
        port: 6697,
        tls: true,
    };
    let (ada, bob) = (
        network_id("user-ada", &endpoint),
        network_id("user-bob", &endpoint),
    );
    publish(
        &service,
        vec![
            recent(message(&nav(&ada), "lyra", "for ada", 1)),
            recent(message(&nav(&bob), "lyra", "for bob", 2)),
        ],
    );
    let store = service.clone();
    let (_board, hub, cx) = mount_hub(cx, move |cx| {
        hub::install_with_store_for_test(&settings(), store, cx)
    });
    cx.run_until_parked();
    assert!(held(&hub, &nav(&ada), cx).is_empty(), "not running yet");
    let api = Api::new(&["127.0.0.1"]);
    connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    assert_eq!(
        held(&hub, &nav(&ada), cx),
        ["for ada"],
        "restored with its network"
    );
    assert!(held(&hub, &nav(&bob), cx).is_empty());
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().iter().any(|command| matches!(
            command,
            Command::Anchors { network, .. } if *network == ada
        )));
    });
    hub.update(cx, |hub, cx| hub.flush_store(true, cx));
    cx.run_until_parked();
    assert_eq!(
        stored_texts(&service),
        ["for ada"],
        "another person's account network history is deleted"
    );
}
