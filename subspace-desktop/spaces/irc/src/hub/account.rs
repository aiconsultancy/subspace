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
        self.managed
            .rederive(&self.saved.networks, catalog.memberships());
        self.follow(self.effective(), cx);
    }

    /// Sets the account as a check would, for tests: `client` stands for
    /// the API the account's networks ask for logins.
    #[cfg(test)]
    pub(crate) fn sign_in_for_test(
        &mut self,
        account: crate::managed::Account,
        client: Option<std::sync::Arc<subspace_api_client::Client>>,
        cx: &mut Context<Self>,
    ) {
        self.managed.settle_for_test(account, client);
        self.reconcile(cx);
    }
}
