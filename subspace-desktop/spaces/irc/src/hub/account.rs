//! The settings the runtime follows: the saved networks, then the networks
//! the signed-in account brings (see [`crate::managed`]).

use gpui_kit::Context;
use subspace_client_irc::{BufferKey, Command, NetworkId};

use super::{Hub, Settings};

impl Hub {
    /// The settings the runtime follows: the saved ones, then the
    /// account's networks.
    pub(crate) fn effective(&self) -> Settings {
        let mut settings = self.saved.clone();
        settings
            .networks
            .extend(self.managed.networks().iter().cloned());
        settings
    }

    /// Derives the account's networks again and has the runtime follow
    /// them with the saved ones. Unchanged networks keep their connection.
    pub(crate) fn reconcile(&mut self, cx: &mut Context<Self>) {
        let catalog = self.initiatives.catalog.read(cx);
        self.managed.rederive(
            &self.saved.networks,
            (catalog.memberships(), catalog.verified_for()),
        );
        self.follow(self.effective(), cx);
    }

    /// The account's networks for Settings.
    pub(crate) fn account_networks(&self) -> crate::managed::AccountNetworks {
        self.managed.shown()
    }

    /// Notes the buffers of account networks that `next` no longer runs,
    /// so their stored lines can be deleted if another person signs in.
    pub(crate) fn retire(&mut self, next: &Settings) {
        let gone = |network: &NetworkId| {
            crate::managed::reserved(&network.0)
                && !next.networks.iter().any(|kept| &kept.id == network)
        };
        let retired: Vec<BufferKey> = self
            .state
            .buffers()
            .map(|(key, _)| key)
            .filter(|key| gone(&key.network))
            .cloned()
            .collect();
        self.recall.retired.extend(retired);
    }

    /// Restores the stored buffers of account networks that run now, and
    /// tells the runtime where each one's history resumes.
    pub(crate) fn restore_parked(&mut self) {
        let (running, parked): (Vec<_>, Vec<_>) = std::mem::take(&mut self.recall.parked)
            .into_iter()
            .partition(|buffer| self.state.network(&buffer.buffer.network).is_some());
        self.recall.parked = parked;
        let mut networks: Vec<NetworkId> = Vec::new();
        for buffer in running {
            if !networks.contains(&buffer.buffer.network) {
                networks.push(buffer.buffer.network.clone());
            }
            self.state
                .restore_buffer(&buffer.buffer, buffer.lines, &buffer.ended);
        }
        self.recall.keep_endings(&mut self.state);
        for network in networks {
            let anchors = self.state.anchors(&network);
            if !anchors.is_empty() {
                self.send(Command::Anchors { network, anchors });
            }
        }
    }

    /// The person with the API's user ID `person` signed in: deletes the stored
    /// lines of every other person's account networks, so neither history
    /// nor search shows them, and keeps this person's for their networks.
    pub(crate) fn keep_history_of(&mut self, person: &str) {
        let user = crate::managed::digest(person);
        let others =
            |network: &NetworkId| crate::managed::owner(network).is_some_and(|owner| owner != user);
        let (gone, parked): (Vec<_>, Vec<_>) = std::mem::take(&mut self.recall.parked)
            .into_iter()
            .partition(|buffer| others(&buffer.buffer.network));
        self.recall.parked = parked;
        let retired = std::mem::take(&mut self.recall.retired);
        let forgotten = gone
            .into_iter()
            .map(|buffer| buffer.buffer)
            .chain(retired.into_iter().filter(|key| others(&key.network)));
        for buffer in forgotten {
            self.recall.forget(&buffer);
        }
    }
}
