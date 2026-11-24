//! Saved IRC settings and their effective runtime configuration.

use gpui_kit::Context;
use subspace_client_irc::{Command, NetworkConfig, NetworkId};

use super::{Hub, Outbox, Settings, running, save_directories};

impl Hub {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self.outbox, Outbox::Runtime(_))
    }

    pub(crate) fn settings_error(&self) -> Option<String> {
        self.applied.error.clone()
    }

    pub(crate) fn configured_initiative_network(&self) -> Option<NetworkId> {
        self.initiatives.network(&self.running)
    }

    pub(crate) fn serves_any_initiative(
        &self,
        initiatives: &[subspace_api_client::model::MemberInitiative],
    ) -> bool {
        self.initiatives
            .serves_any(initiatives, &self.applied.networks)
    }

    /// Whether the latest read of the memberships, or the account side's
    /// latest request, could not reach the API at all.
    pub(crate) fn api_unreachable(&self, cx: &gpui_kit::App) -> bool {
        self.initiatives.catalog.read(cx).unreachable() || self.managed.unreachable()
    }

    pub(crate) fn missing_initiative_network(&self) -> Option<String> {
        self.initiatives.missing_network(&self.applied.networks)
    }

    /// Configures the runtime with `networks` when they differ from what it
    /// runs. Until the stored buffers are restored, it waits; an account
    /// network held back until its own are read is left out until then.
    /// Until the first read of the memberships settles it runs nothing, and
    /// while Channels is closed it runs none.
    pub(super) fn run(&mut self, networks: Vec<NetworkConfig>) {
        if let Some(waiting) = self.recall.waiting.as_mut() {
            *waiting = networks;
            return;
        }
        if self.initiatives.awaiting_read() {
            return;
        }
        let starting = &self.recall.starting;
        let closed = self.initiatives.closed();
        let networks: Vec<NetworkConfig> = networks
            .into_iter()
            .filter(|network| !closed && !starting.contains(&network.id))
            .collect();
        if networks != self.running {
            self.send(Command::Configure(networks.clone()));
            self.running = networks;
            self.apply_initiative();
        }
    }

    /// Runs the networks the settings name again, after the first read of
    /// the memberships settled or Channels closed or reopened: none while it
    /// is closed, all of them otherwise, each told where to catch up from.
    pub(crate) fn rerun(&mut self) {
        self.run(running::initial(
            &self.applied.networks,
            &self.state.removed,
        ));
        self.send_anchors();
    }

    /// Follows newly saved settings, with the account's networks derived
    /// again beside them.
    pub fn apply(&mut self, settings: Settings, cx: &mut Context<Self>) {
        self.saved = settings;
        self.reconcile(cx);
    }

    /// Follows new effective settings: the runtime reconnects only the
    /// networks whose configuration changed, and the state keeps the
    /// others' buffers. A channel newly added to a network's channels is
    /// wanted again, even after the user left it.
    pub(crate) fn follow(&mut self, settings: Settings, cx: &mut Context<Self>) {
        if settings == self.applied {
            return;
        }
        let started: Vec<NetworkId> = settings
            .networks
            .iter()
            .filter(|network| crate::managed::reserved(&network.id.0))
            .filter(|network| !self.applied.networks.iter().any(|old| old.id == network.id))
            .map(|network| network.id.clone())
            .collect();
        for channel in running::added_channels(&self.applied.networks, &settings.networks) {
            self.state.removed.forget(&channel);
        }
        let held = self.recall.hold(started, &settings.networks);
        let networks = running::next(
            (&self.applied.networks, &settings.networks),
            &self.running,
            &self.state.removed,
        );
        self.run(networks);
        self.state
            .reconfigure(&settings.networks, settings.highlights.clone());
        if self.applied.captain_notifications != settings.captain_notifications {
            self.captain_notifications.pause();
        }
        self.applied = settings;
        self.recall_started(held, cx);
        self.initiatives.forget_rosters();
        self.save_removed(cx);
        save_directories(self, cx);
        cx.notify();
    }
}
