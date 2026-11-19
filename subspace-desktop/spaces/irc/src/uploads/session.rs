//! API identity is revalidated independently of IRC connectivity. No credential enters UI state.
mod demand;
use crate::hub::Hub;
pub(crate) use demand::Demand;
use gpui_kit::{AppContext as _, Context};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use subspace_api_client::{Client, chat_uploads::UploadError};

const RECHECK: Duration = Duration::from_secs(30);
const VALID_FOR: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct Scope {
    client: Arc<Client>,
    user: String,
    epoch: u64,
}
impl Scope {
    pub(crate) fn client(&self) -> Result<&Client, UploadError> {
        if self.client.me().map_err(UploadError::from)?.user_id != self.user {
            return Err(UploadError::Access);
        }
        Ok(&self.client)
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.epoch == other.epoch && self.same_user(other)
    }
    pub(crate) fn same_user(&self, other: &Self) -> bool {
        self.user == other.user && Arc::ptr_eq(&self.client, &other.client)
    }
}
#[derive(Default)]
pub(crate) struct Session {
    client: Option<Arc<Client>>,
    user: Option<String>,
    epoch: u64,
    checked: Option<Instant>,
    checking: bool,
}
impl Session {
    #[cfg(test)]
    pub(crate) fn test_connect(&mut self, client: Client, user: &str) {
        self.client = Some(Arc::new(client));
        self.record(Some(user.into()));
    }
    /// The same client and user in a new session, as after signing in again.
    #[cfg(test)]
    pub(crate) fn test_renew(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.checked = Some(Instant::now());
    }
    pub(crate) fn connect(&mut self) {
        self.client = Client::new().ok().map(Arc::new);
    }
    pub(crate) fn scope(&self) -> Option<Scope> {
        self.checked.filter(|at| at.elapsed() < VALID_FOR)?;
        let client = self.client.clone()?;
        if client.account_changed() {
            return None;
        }
        Some(Scope {
            client,
            user: self.user.clone()?,
            epoch: self.epoch,
        })
    }
    pub(crate) fn matches(&self, scope: &Scope) -> bool {
        self.scope().is_some_and(|current| current.same(scope))
    }
    fn record(&mut self, user: Option<String>) -> bool {
        let expired = self.user.is_some() && self.scope().is_none();
        self.checking = false;
        self.checked = Some(Instant::now());
        let changed = self.user != user || expired;
        if changed {
            self.epoch = self.epoch.wrapping_add(1);
            self.user = user;
        }
        changed
    }
}
pub(crate) fn refresh(hub: &mut Hub, cx: &mut Context<Hub>) {
    if !hub.uploads.demand.active_now() {
        return;
    }
    if hub
        .uploads
        .session
        .client
        .as_ref()
        .is_some_and(|client| client.account_changed())
        && !hub.uploads.session.checking
    {
        hub.uploads.session.connect();
        hub.uploads.session.record(None);
        hub.uploads.session.checked = None;
        hub.uploads.clear();
        cx.notify();
    }
    let session = &mut hub.uploads.session;
    if session.checking || session.checked.is_some_and(|at| at.elapsed() < RECHECK) {
        return;
    }
    let Some(client) = session.client.clone() else {
        return;
    };
    session.checking = true;
    let started_at = Instant::now();
    cx.spawn(async move |this, cx| {
        let identity = cx
            .background_spawn(async move { client.me().map(|me| me.user_id) })
            .await;
        let _ = this.update(cx, |hub, cx| {
            let observation = (started_at, identity);
            hub.observe_initiative_identity(&observation, cx);
            if hub.uploads.session.record(observation.1.ok()) {
                hub.uploads.clear();
                cx.notify();
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::Session;
    use std::sync::Arc;
    use subspace_api_client::Client;
    #[test]
    fn changing_or_losing_identity_invalidates_every_prior_scope() {
        let mut session = Session {
            client: Some(Arc::new(
                Client::loopback("http://127.0.0.1:12345/api", None).unwrap(),
            )),
            ..Default::default()
        };
        assert!(session.record(Some("first".into())));
        let first = session.scope().unwrap();
        assert!(session.matches(&first));
        assert!(!session.record(Some("first".into())));
        assert!(session.matches(&first));
        assert!(session.record(None));
        assert!(!session.matches(&first));
        assert!(session.record(Some("first".into())));
        assert!(
            session.scope().unwrap().same_user(&first),
            "explicit recovery can use the same returning identity"
        );
        assert!(
            !session.matches(&first),
            "the same account returning is a new session"
        );
    }
}
