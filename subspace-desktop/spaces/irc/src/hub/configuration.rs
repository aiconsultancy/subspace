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

    pub(crate) fn missing_initiative_network(&self) -> Option<String> {
        self.initiatives.missing_network(&self.applied.networks)
    }

    /// Configures the runtime with `networks` when they differ from what it
    /// runs. Until the stored buffers are restored, it waits.
    pub(super) fn run(&mut self, networks: Vec<NetworkConfig>) {
        if let Some(waiting) = self.recall.waiting.as_mut() {
            *waiting = networks;
            return;
        }
        if networks != self.running {
            self.send(Command::Configure(networks.clone()));
            self.running = networks;
            self.apply_initiative(false);
        }
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
        self.recall_started(started, cx);
        self.initiatives.forget_rosters();
        self.save_removed(cx);
        save_directories(self, cx);
        cx.notify();
    }
}
