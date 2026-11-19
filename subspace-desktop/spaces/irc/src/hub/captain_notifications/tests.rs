use chrono::{TimeDelta, Utc};
use gpui_kit::TestAppContext;
use serde_json::{Value, json};
use subspace_client_irc::{Command, NetworkId, SendResult, Target};
use subspace_framework::client_github::Client;
use subspace_framework::client_test_support::{Mock, Request};
use subspace_framework::core::{
    Secret,
    config::{Config, GitHub},
};

use super::{
    Monitor, Scope, fetch,
    notice::{Notice, Phase},
    route::{Route, Routes},
};
use crate::hub;

const REPO: &str = "example-org/fixture";
const CHANNEL: &str = "#apollo-phoenix";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn pull(number: u32, body: &str, merged_at: &str) -> Value {
    json!({
        "number": number, "merged_at": merged_at, "merge_commit_sha": SHA,
        "body": body,
        "base": {"repo": {"full_name": REPO}}
    })
}

fn github(request: &Request) -> (u16, String) {
    let resolved = pull(
        7,
        "Closes APL-7. Follow-up to APL-7.",
        "2026-11-18T16:00:10Z",
    );
    let body = match request.path.split('?').next().unwrap_or_default() {
        "/graphql" => json!({"data":{"viewer":{"login":"fixture-user"}}}),
        "/repos/example-org/fixture/pulls" => json!([
            resolved,
            pull(8, "Closes APL-8", "2026-11-18T16:00:20Z"),
            pull(9, "Closes APL-7", "2026-11-18T15:59:00Z"),
            pull(10, "Closes APL-7 and APL-8", "2026-11-18T16:00:40Z"),
        ]),
        "/repos/example-org/fixture/deployments" => json!([
            {"id": 13, "sha": SHA, "environment": "staging", "updated_at": "2026-11-18T16:01:30Z"},
            {"id": 14, "sha": SHA, "environment": "dev", "updated_at": "2026-11-18T16:02:30Z"},
            {"id": 15, "sha": "ffffffffffffffffffffffffffffffffffffffff", "environment": "staging", "updated_at": "2026-11-18T16:02:30Z"}
        ]),
        path if path.contains("/commits/") => json!([resolved]), // Association alone does not match the merge SHA.
        "/repos/example-org/fixture/deployments/13/statuses" => json!([
            {"id": 3, "state":"success", "created_at":"2026-11-18T16:01:30Z"},
            {"id": 2, "state":"in_progress", "created_at":"2026-11-18T16:01:20Z"},
            {"id": 1, "state":"queued", "created_at":"2026-11-18T16:01:10Z"}
        ]),
        "/repos/example-org/fixture/deployments/14/statuses" => json!([
            {"id": 5, "state":"error", "created_at":"2026-11-18T16:02:30Z"},
            {"id": 4, "state":"in_progress", "created_at":"2026-11-18T16:02:20Z"}
        ]),
        _ => return (404, "{}".into()),
    };
    (200, body.to_string())
}

fn route() -> Route {
    Route {
        issue: "APL-7".into(),
        ship: "phoenix".into(),
        channel: CHANNEL.into(),
        endpoint: None,
    }
}

fn api(request: &Request) -> (u16, String) {
    let body = match request.path.split('?').next().unwrap_or_default() {
        "/api/v1/me/initiatives" => json!({"items":[
            {"id":"other-initiative", "name":"Other", "channels":"#other-phoenix, #general-discovery"},
            {"id":"initiative-1", "name":"Apollo", "channels":"#apollo-general #Apollo-Phoenix, #apollo-phoenix-crew"}
        ]}),
        "/api/v1/projects" => json!({"items":[
            {"id":"project-1", "key":"APL", "name":"Apollo", "initiativeId":"initiative-1", "defaultStateId":"state-1"},
            {"id":"project-2", "key":"SUB", "name":"Subspace", "defaultStateId":"state-1"}
        ]}),
        "/api/v1/issues/APL-7" => issue("APL-7", "project-1", Some("phoenix")),
        "/api/v1/issues/APL-8" => issue("APL-8", "project-1", None),
        "/api/v1/issues/APL-9" => issue("APL-9", "project-1", Some("discovery")),
        "/api/v1/issues/SUB-7" => issue("SUB-7", "project-2", Some("phoenix")),
        _ => return (404, "{}".into()),
    };
    (200, body.to_string())
}

fn issue(identifier: &str, project: &str, ship: Option<&str>) -> Value {
    let assignee = ship.map(|ship| {
        json!({
            "userId": "fixture-agent", "displayName": "Fixture agent", "isFormerMember": false,
            "kind": "agent", "agent": {"rank":"captain", "ship":ship}
        })
    });
    json!({
        "id":identifier, "identifier":identifier, "projectId":project,
        "projectKey":identifier.split('-').next(), "type":"issue", "title":"Fixture issue",
        "state":{"id":"state-1","name":"Done","type":"completed"}, "assignee":assignee,
        "labels":[], "priority":2, "blocked":false, "createdAt":"2026-11-01T12:00:00Z",
        "updatedAt":"2026-11-01T12:00:00Z", "version":1, "commentCount":0,
        "todoCounts":{"total":0,"done":0}, "lastActivityAt":"2026-11-16T12:00:00Z", "todos":[]
    })
}

#[test]
fn polls_report_merges_and_each_deployment_transition_in_order() {
    let server = Mock::new(github);
    let config = Config {
        github: GitHub {
            token: Some(Secret::new("fixture-token")),
        },
        ..Default::default()
    };
    let credentials = subspace_framework::agent_tools::test_support::credentials(&config);
    let client = Client::new(credentials).with_api_url(server.base_url());
    let scope = Scope {
        network: NetworkId("fixture".into()),
        account: Some("fixture-user".into()),
        repos: vec![REPO.into()],
    };
    let mock = Mock::new(api);
    let api =
        subspace_api_client::Client::loopback(&format!("{}/api", mock.base_url()), None).unwrap();
    let notices = fetch::read(
        &client,
        &api,
        &scope,
        "2026-11-18T16:00:00Z".parse().unwrap(),
    );
    assert_eq!(
        notices
            .iter()
            .map(|notice| notice.key.phase)
            .collect::<Vec<_>>(),
        [
            Phase::Merged,
            Phase::Started,
            Phase::Succeeded,
            Phase::Started,
            Phase::Failed
        ]
    );
    assert!(
        notices
            .iter()
            .all(|notice| notice.route.channel == CHANNEL && notice.key.number == 7)
    );
    assert_eq!(
        notices[2].text(),
        "@phoenix [Subspace] APL-7 · Deployment succeeded (staging) — https://github.com/example-org/fixture/pull/7"
    );
    assert!(
        !server
            .requests()
            .iter()
            .any(|request| request.path.contains("/deployments/15/statuses"))
    );
}

#[gpui_kit::test]
fn delivery_requires_the_exact_joined_channel_and_sends_each_event_once(cx: &mut TestAppContext) {
    use super::{deliver, scope};
    use std::sync::Arc;
    use subspace_client_irc::{BufferKey, ConnectionStatus, Event};
    use subspace_framework::dashboard::{Catalog, Services};

    let mut settings = crate::tests::settings();
    settings.networks[0].host = "127.0.0.1".into();
    settings.networks[0].port = 6667;
    settings.networks[0].tls = false;
    let network = settings.networks[0].id.clone();
    cx.update(|cx| {
        let config = Config { repos: vec![REPO.into()], ..Default::default() };
        cx.set_global(Services {
            credentials: subspace_framework::agent_tools::test_support::credentials(&config),
            config: Arc::new(config),
            tools: Catalog::new(vec![]).unwrap(),
            search: None,
            config_file: "/fixture/config.json".into(),
        });
        let hub = hub::install_for_test(&settings, cx);
        hub.update(cx, |hub, cx| {
            let time = Utc::now();
            let wanted = scope(hub, cx).unwrap();
            hub.captain_notifications.sync(Some(wanted.clone()), time - TimeDelta::seconds(1));
            let notice = || vec![Notice::merged(REPO, 7, route(), time)];
            hub.ingest(vec![
                Event::Status {
                    network: network.clone(),
                    status: ConnectionStatus::Connected { nick: "fixture-user".into() },
                },
                Event::Joined {
                    buffer: BufferKey { network: network.clone(), target: Target::Channel("#other-phoenix".into()) },
                },
            ], time, cx);
            deliver(hub, &wanted, notice(), cx);
            assert!(!hub.sent().iter().any(|command| matches!(command, Command::SendAll { .. })));

            let target = BufferKey { network: network.clone(), target: Target::Channel(CHANNEL.into()) };
            hub.ingest(vec![Event::Joined { buffer: target.clone() }], time, cx);
            let endpoint = Some(crate::initiatives::Endpoint::parse("ircs://other.example.test:6697").unwrap());
            let remote_route = Route { endpoint, ..route() };
            deliver(hub, &wanted, vec![Notice::merged(REPO, 7, remote_route, time)], cx);
            assert!(!hub.sent().iter().any(|command| matches!(command, Command::SendAll { .. })));
            deliver(hub, &wanted, notice(), cx);
            deliver(hub, &wanted, notice(), cx);

            settings.captain_notifications = false;
            hub.apply(settings, cx);
            deliver(hub, &wanted, vec![Notice::merged(REPO, 8, route(), time)], cx);
            let messages: Vec<_> = hub.sent().iter().filter_map(|command| match command {
                Command::SendAll { buffer, lines, .. } => Some((buffer, lines)),
                _ => None,
            }).collect();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].0, &target);
            assert_eq!(messages[0].1, &["@phoenix [Subspace] APL-7 · PR merged — https://github.com/example-org/fixture/pull/7"]);
        });
    });
}

#[test]
fn only_a_refused_write_is_eligible_for_a_retry() {
    use super::Delivery;
    let now = Utc::now();
    let mut monitor = Monitor::default();
    for (id, result) in [
        (1, SendResult::Written),
        (2, SendResult::Uncertain("lost connection".into())),
        (3, SendResult::Refused("offline".into())),
    ] {
        let notice = Notice::merged(REPO, id as u32, route(), now);
        monitor.deliveries.insert(
            notice.key,
            Delivery {
                time: now,
                pending: Some(id),
            },
        );
        assert!(monitor.answered(id, &result));
    }
    assert_eq!(monitor.deliveries.len(), 2);
    assert!(!monitor.answered(1, &SendResult::Written));
    monitor.sync(None, now + TimeDelta::hours(2));
    assert!(monitor.deliveries.is_empty());
}

#[test]
fn routes_require_one_issue_assignee_and_matching_channel_in_its_project_initiative() {
    let mock = Mock::new(api);
    let api =
        subspace_api_client::Client::loopback(&format!("{}/api", mock.base_url()), None).unwrap();
    let mut routes = Routes::load(&api).unwrap();
    assert_eq!(routes.resolve("Closes APL-7"), Some(route()));
    assert_eq!(
        routes.resolve("Closes APL-9"),
        None,
        "a ship channel listed only under another initiative cannot route"
    );
    for body in [
        "Closes APL-8",
        "Closes SUB-7",
        "Closes APL-7 and APL-8",
        "Closes `APL-7`",
        "Closes apl-7",
        "https://example.test/APL-7",
    ] {
        assert_eq!(routes.resolve(body), None, "{body}");
    }
}
