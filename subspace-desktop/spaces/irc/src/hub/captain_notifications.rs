// qual:allow(srp, max_independent_clusters=3) reason: "The monitor schedules one background read and hands its notices to the owning Channels runtime."
//! The process-owned Channels monitor. It sends on the connection the
//! selected initiative's channels use (its fleet's server, or the default
//! Subspace IRC server), to channels that are already joined.

mod fetch;
mod notice;
mod route;
mod state;
#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use gpui_kit::{App, AppContext as _, Context};
use subspace_client_irc::{BufferKey, Command, ConnectionStatus, NetworkId, Target};
use subspace_framework::client_github::{Client, lifecycle::REFRESH_INTERVAL};
use subspace_framework::core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_framework::dashboard::{prefs, refresh, services};

use super::Hub;
use notice::Notice;
pub(super) use state::Monitor;
use state::{Active, Delivery};

const LOOKBACK: TimeDelta = TimeDelta::hours(1);
const CHECK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    network: NetworkId,
    account: Option<String>,
    repos: Vec<String>,
}

fn scope(hub: &Hub, cx: &App) -> Option<Scope> {
    if !hub.applied.captain_notifications || !refresh::enabled(crate::ID, cx) {
        return None;
    }
    let network = hub.configured_initiative_network()?;
    let mut repos = services(cx)?
        .config
        .repos
        .iter()
        .map(|repo| repo.to_ascii_lowercase())
        .collect::<Vec<_>>();
    repos.sort_unstable();
    repos.dedup();
    (!repos.is_empty()).then(|| Scope {
        network,
        account: prefs::get(cx, GITHUB_ACCOUNT_KEY),
        repos,
    })
}

fn connected(hub: &Hub, scope: &Scope) -> bool {
    hub.state
        .network(&scope.network)
        .is_some_and(|network| matches!(network.status, ConnectionStatus::Connected { .. }))
}

pub(super) fn tick(hub: &mut Hub, cx: &mut Context<Hub>) {
    if hub
        .captain_notifications
        .last_check
        .is_some_and(|last| last.elapsed() < CHECK_INTERVAL)
    {
        return;
    }
    hub.captain_notifications.last_check = Some(Instant::now());
    let Some(services) = services(cx) else { return };
    let scope = scope(hub, cx);
    let now = Utc::now();
    let ready = scope.as_ref().is_some_and(|scope| connected(hub, scope));
    hub.captain_notifications.sync(scope, now);
    let Some(active) = hub.captain_notifications.active.as_mut() else {
        return;
    };
    if active.schedule.ready(
        active.started.elapsed(),
        ready && active.fetch.is_none(),
        REFRESH_INTERVAL,
    ) {
        let client = Client::new(services.credentials.clone());
        start_read(active, client, now, cx);
    }
}

fn start_read(active: &mut Active, client: Client, now: DateTime<Utc>, cx: &mut Context<Hub>) {
    let scope = active.scope.clone();
    let since = active.since.max(now - LOOKBACK);
    active.fetch = Some(cx.spawn(async move |this, cx| {
        let read_scope = scope.clone();
        let notices = cx
            .background_spawn(async move {
                subspace_api_client::Client::new().map_or_else(
                    |_| Vec::new(),
                    |api| fetch::read(&client, &api, &read_scope, since),
                )
            })
            .await;
        _ = this.update(cx, |hub, cx| deliver(hub, &scope, notices, cx));
    }));
}

fn deliver(hub: &mut Hub, read_scope: &Scope, notices: Vec<Notice>, cx: &App) {
    if scope(hub, cx).as_ref() != Some(read_scope) {
        hub.captain_notifications.pause();
        return;
    }
    let Some(active) = hub.captain_notifications.active.as_mut() else {
        return;
    };
    active.fetch = None;
    let since = active.since.max(Utc::now() - LOOKBACK);
    if !connected(hub, read_scope) {
        return;
    }
    for notice in notices {
        if notice.occurred_at > since
            && serves_route(hub, &read_scope.network, &notice.route)
            && joined_channel(hub, &read_scope.network, &notice.route.channel)
        {
            send(hub, &read_scope.network, notice);
        }
    }
}

fn serves_route(hub: &Hub, network: &NetworkId, route: &route::Route) -> bool {
    hub.running
        .iter()
        .find(|config| &config.id == network)
        .is_some_and(|config| {
            hub.initiatives
                .serves_route(config, route.endpoint.as_ref())
        })
}

fn joined_channel(hub: &Hub, network: &NetworkId, channel: &str) -> bool {
    hub.state.buffers().any(|(key, buffer)| {
        buffer.joined
            && &key.network == network
            && matches!(&key.target, Target::Channel(name) if name.eq_ignore_ascii_case(channel))
    })
}

fn send(hub: &mut Hub, network: &NetworkId, notice: Notice) {
    if hub
        .captain_notifications
        .deliveries
        .contains_key(&notice.key)
    {
        return;
    }
    let buffer = BufferKey {
        network: network.clone(),
        target: Target::Channel(notice.route.channel.clone()),
    };
    let id = hub.state.next_send();
    let lines = vec![notice.text()];
    hub.captain_notifications.deliveries.insert(
        notice.key,
        Delivery {
            time: notice.occurred_at,
            pending: Some(id),
        },
    );
    hub.send(Command::SendAll { buffer, id, lines });
}
