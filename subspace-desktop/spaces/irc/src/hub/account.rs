//! The settings the runtime follows: the saved networks, then the networks
//! the signed-in account brings (see [`crate::managed`]).

use gpui_kit::Context;

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
}
