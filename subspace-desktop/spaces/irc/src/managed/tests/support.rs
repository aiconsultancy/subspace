//! An offline API, a stand-in account and the hub's timer, for the tests of
//! the account's networks. Fictional accounts and servers only.

use std::sync::{Arc, Mutex};

use gpui_kit::{Entity, VisualTestContext};
use serde_json::{Value, json};
use subspace_api_client::model::MemberInitiative;
use subspace_api_client::{Client, Error};
use subspace_client_irc::{Command, NetworkConfig};
use subspace_framework::client_test_support::{Mock, Request};

use crate::hub::Hub;
use crate::managed::env::Env;

pub(crate) const INITIATIVE: &str = "5d1f0c2e-5b7a-4f0e-9a51-2a8f3c6d7e10";

/// Who the offline API says is signed in, and which servers it vouches for.
#[derive(Clone)]
pub(crate) struct Api {
    pub mock: Arc<Mock>,
    /// `/me`'s answer: the status and the body.
    pub me: Arc<Mutex<(u16, Value)>>,
}

impl Api {
    /// Ada, with the username `ada`, on an API that vouches for `vouched`.
    pub fn new(vouched: &'static [&'static str]) -> Self {
        let me = Arc::new(Mutex::new((200, person("user-ada", Some("ada")))));
        let answer = Arc::clone(&me);
        let mock = Mock::new(move |request| respond(request, &answer, vouched));
        Self {
            mock: Arc::new(mock),
            me,
        }
    }

    pub fn sign_in(&self, status: u16, body: Value) {
        *self.me.lock().unwrap() = (status, body);
    }

    /// The hosts each credential request asked for, in order.
    pub fn asked(&self) -> Vec<String> {
        self.mock
            .requests()
            .iter()
            .filter(|request| request.path == "/api/v1/me/irc-credential")
            .map(|request| {
                let body: Value = serde_json::from_str(&request.body).unwrap();
                body["host"].as_str().unwrap_or_default().to_owned()
            })
            .collect()
    }
}

pub(crate) fn person(id: &str, username: Option<&str>) -> Value {
    json!({"userId": id, "kind": "human", "displayName": "Ada Fixture", "username": username})
}

fn respond(request: &Request, me: &Mutex<(u16, Value)>, vouched: &[&str]) -> (u16, String) {
    if request.path == "/api/v1/me" {
        let (status, body) = me.lock().unwrap().clone();
        return (status, body.to_string());
    }
    let body: Value = serde_json::from_str(&request.body).unwrap_or_default();
    let host = body["host"].as_str().unwrap_or_default();
    if request.path != "/api/v1/me/irc-credential" || !vouched.contains(&host) {
        return (404, r#"{"title":"Not Found"}"#.into());
    }
    let account = me.lock().unwrap().1["username"].clone();
    let expires = chrono::Utc::now() + chrono::TimeDelta::minutes(10);
    let credential = json!({
        "account": account,
        // Names its server, so a login offered elsewhere would show.
        "password": format!("sirc1.for-{}", host.replace(['.', ':'], "-")),
        "expiresAt": expires.to_rfc3339(),
    });
    (200, credential.to_string())
}

/// The account and API the hub's account side uses in a test.
pub(crate) struct TestEnv {
    url: String,
    pub generation: Mutex<Option<u64>>,
    /// False for a build with no API backend.
    pub available: bool,
}

impl Env for TestEnv {
    fn client(&self) -> Result<Client, Error> {
        Client::loopback(&self.url, None)
    }

    fn generation(&self) -> Option<u64> {
        *self.generation.lock().unwrap()
    }

    fn available(&self) -> bool {
        self.available
    }
}

/// The hub's account side talking to `api`, signed in at generation 1.
pub(crate) fn connect(hub: &Entity<Hub>, api: &Api, cx: &mut VisualTestContext) -> Arc<TestEnv> {
    connect_to(hub, api, true, cx)
}

/// [`connect`], in a build with an API backend when `available`.
pub(crate) fn connect_to(
    hub: &Entity<Hub>,
    api: &Api,
    available: bool,
    cx: &mut VisualTestContext,
) -> Arc<TestEnv> {
    let env = Arc::new(TestEnv {
        url: format!("{}/api", api.mock.base_url()),
        generation: Mutex::new(Some(1)),
        available,
    });
    let used: Arc<dyn Env> = env.clone();
    hub.update(cx, |hub, _| hub.managed.use_env_for_test(used));
    env
}

/// The aurora fleet on `aurora` and the boreal fleet on `boreal`, both
/// over TLS on port 6697.
pub(crate) fn fleets(aurora: &str, boreal: Option<&str>) -> Vec<MemberInitiative> {
    let fleet = |id: &str, name: &str, host: &str| {
        json!({"id": id, "slug": id, "name": name, "ircHost": host, "ircPort": 6697,
               "ircTls": true, "starbaseChannel": format!("#{id}-starbase")})
    };
    let mut items = vec![json!({
        "id": INITIATIVE, "name": "Fallow Reach", "channels": "#aurora-nav",
        "fleetId": "aurora", "fleet": fleet("aurora", "Aurora", aurora)
    })];
    if let Some(boreal) = boreal {
        items.push(json!({
            "id": "boreal-initiative", "name": "Boreal", "channels": "",
            "fleetId": "boreal", "fleet": fleet("boreal", "Boreal", boreal)
        }));
    }
    serde_json::from_value(json!(items)).unwrap()
}

/// The fixture person's memberships, as the catalogue read them for
/// `user` at account generation `scope`.
pub(crate) fn member_of(
    hub: &Entity<Hub>,
    (scope, user): (Option<u64>, &str),
    initiatives: Vec<MemberInitiative>,
    cx: &mut VisualTestContext,
) {
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.change_scope_for_test(scope, cx);
        catalog.set_memberships_for_test(Some(user.into()), Ok(initiatives), cx)
    });
    cx.run_until_parked();
}

/// Runs the hub's timer until the account side has nothing left to do.
pub(crate) fn settle(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    for _ in 0..8 {
        hub.update(cx, crate::managed::tick);
        cx.run_until_parked();
    }
}

/// The networks the runtime was last configured with.
pub(crate) fn configured(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> Vec<NetworkConfig> {
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .rev()
            .find_map(|command| match command {
                Command::Configure(networks) => Some(networks.clone()),
                _ => None,
            })
            .unwrap_or_default()
    })
}

pub(crate) fn hosts(networks: &[NetworkConfig]) -> Vec<String> {
    networks
        .iter()
        .map(|network| network.host.clone())
        .collect()
}

/// What each account network's source issues now, by host.
pub(crate) fn issued(networks: &[NetworkConfig]) -> Vec<(String, String, String)> {
    networks
        .iter()
        .filter_map(|network| {
            let source = network.sasl.as_ref()?.credential.as_ref()?;
            let login = subspace_client_irc::test_support::fetch(source).ok()?;
            Some((
                network.host.clone(),
                login.username,
                login.password.expose().to_owned(),
            ))
        })
        .collect()
}
