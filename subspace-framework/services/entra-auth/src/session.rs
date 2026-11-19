use crate::oidc::{Entra, Granted, Provider, Session};
use crate::{
    Account, Cancel, Config, CredentialStore, Error, ServiceToken, Started, SystemStore, browser,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use subspace_account::{PendingSlot, ProviderInfo};
use subspace_core::Secret;

pub(crate) const EXPIRY_SKEW: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Cache {
    session: Option<Session>,
    access: BTreeMap<String, (Secret, Instant, SystemTime)>,
    pending_save: Option<Secret>,
}

/// Serialises renewal and rotating-token persistence. Network calls belong on a
/// background thread; sign-out invalidates in-flight work before taking the lock.
// qual:api — apps construct it and hand it to the shell as a provider.
pub struct SessionManager {
    config: Config,
    store: Arc<dyn CredentialStore>,
    provider: Arc<dyn Provider>,
    cache: Mutex<Cache>,
    generation: AtomicU64,
    pending: PendingSlot<browser::Pending>,
}

impl SessionManager {
    pub fn system(config: Config, product: &str) -> Self {
        let store = Arc::new(SystemStore::new(&config.namespace(product)));
        Self::new(config, store)
    }

    pub fn new(config: Config, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            provider: Arc::new(Entra(config.clone())),
            config,
            store,
            cache: Mutex::default(),
            generation: AtomicU64::default(),
            pending: PendingSlot::default(),
        }
    }

    pub fn start(&self) -> Result<Started, Error> {
        let (url, pending) = browser::start(&self.config, None)?;
        self.pending.start(Some(url), pending)
    }

    /// Explicit consent/recovery only; token_for never opens a browser.
    // qual:api — explicit interactive recovery for trusted service adapters.
    pub fn start_for(&self, resource: &str) -> Result<Started, Error> {
        let (url, pending) = browser::start(&self.config, Some(resource))?;
        self.pending.start(Some(url), pending)
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Accept and persist before the browser can display success.
    pub fn finish(
        &self,
        started: Started,
        cancel: &Cancel,
        generation: u64,
    ) -> Result<Account, Error> {
        let pending = self.pending.take(&started)?;
        let resource = pending.resource.clone().unwrap_or_default();
        pending.finish(cancel, |granted| {
            self.accept(granted, cancel, generation, &resource)
        })
    }

    fn accept(
        &self,
        granted: Granted,
        cancel: &Cancel,
        generation: u64,
        resource: &str,
    ) -> Result<Account, Error> {
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        cancel.check()?;
        if generation != self.generation() {
            return Err(Error::Cancelled);
        }
        cancel.begin_commit()?;
        self.store.save(&encode(&granted.session))?;
        let account = granted.session.account.clone();
        *cache = cached(granted, resource);
        // Leases taken during the switch belong to the previous identity.
        self.invalidate();
        Ok(account)
    }

    /// Startup restoration is silent; absence of a credential is signed out.
    pub fn restore(&self) -> Result<Option<Account>, Error> {
        self.reconnect()
    }

    /// Renew the account without ever opening a browser.
    pub fn reconnect(&self) -> Result<Option<Account>, Error> {
        let generation = self.generation();
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        if cache.session.is_none() {
            let Some(stored) = self.store.load()? else {
                return Ok(None);
            };
            let session = decode(&stored)?;
            if session.account.tenant != self.config.tenant {
                return Err(Error::InvalidReply);
            }
            cache.session = Some(session);
        }
        self.renew(&mut cache, generation, "", Duration::ZERO)?;
        Ok(cache.session.as_ref().map(|v| v.account.clone()))
    }

    /// Resolve only a product-registered resource. Tokens for the identity
    /// flow and other services can never satisfy this cache lookup.
    // qual:api — resource credentials are consumed by trusted native adapters.
    pub fn token_for(&self, resource: &str) -> Result<ServiceToken, Error> {
        self.token_with_validity(resource, Duration::ZERO)
    }

    /// Refresh a cached token when its remaining lifetime is too short.
    pub fn token_with_validity(
        &self,
        resource: &str,
        minimum: Duration,
    ) -> Result<ServiceToken, Error> {
        self.config.scope(resource)?;
        let generation = self.generation();
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        self.renew(&mut cache, generation, resource, minimum)?;
        if generation != self.generation() {
            return Err(Error::Cancelled);
        }
        let session = cache.session.as_ref().ok_or(Error::SignInRequired)?;
        let (token, expires_at, wall_deadline) =
            cache.access.get(resource).ok_or(Error::Service)?;
        if remaining(*expires_at, *wall_deadline) <= minimum {
            return Err(Error::Service);
        }
        Ok(ServiceToken {
            account: session.account.clone(),
            token: token.clone(),
            expires_at: *expires_at,
        })
    }

    fn renew(
        &self,
        cache: &mut Cache,
        generation: u64,
        resource: &str,
        minimum: Duration,
    ) -> Result<(), Error> {
        self.flush_save(cache)?;
        if cache
            .access
            .get(resource)
            .is_some_and(|(_, expiry, wall)| remaining(*expiry, *wall) > minimum)
        {
            return Ok(());
        }
        let session = cache.session.as_ref().ok_or(Error::SignInRequired)?;
        let scope = if resource.is_empty() {
            None
        } else {
            Some(self.config.scope(resource)?)
        };
        let granted = self.provider.refresh(session, scope)?;
        // The old refresh token may be spent. The cache lock keeps this the
        // same account, so keep the rotated token even when the request went
        // stale or the OS store is temporarily locked; only the stale access
        // token is dropped.
        cache.session = Some(granted.session.clone());
        cache.pending_save = Some(encode(&granted.session));
        self.flush_save(cache)?;
        if generation != self.generation() {
            return Err(Error::Cancelled);
        }
        cache.access.insert(
            resource.into(),
            (
                granted.access,
                granted.expires_at,
                wall_deadline(granted.expires_at),
            ),
        );
        Ok(())
    }

    fn flush_save(&self, cache: &mut Cache) -> Result<(), Error> {
        cache
            .pending_save
            .as_ref()
            .map(|value| self.store.save(value))
            .transpose()?;
        cache.pending_save = None;
        Ok(())
    }

    /// The UI must report deletion errors; it cannot promise persistent sign-out.
    pub fn sign_out(&self) -> Result<(), Error> {
        self.invalidate();
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        self.store.delete()?;
        *cache = Cache::default();
        Ok(())
    }
}

/// The shell drives the Entra session through the provider-neutral account API.
impl subspace_account::Provider for SessionManager {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Microsoft".into(),
            organization: self.config.organization_name().into(),
            detail: Some(("Tenant ID".into(), self.config.tenant_id().into())),
            browser: true,
        }
    }
    fn generation(&self) -> u64 {
        SessionManager::generation(self)
    }
    fn invalidate(&self) {
        SessionManager::invalidate(self)
    }
    fn start(&self) -> Result<Started, Error> {
        SessionManager::start(self)
    }
    fn finish(&self, started: Started, cancel: &Cancel, generation: u64) -> Result<Account, Error> {
        SessionManager::finish(self, started, cancel, generation)
    }
    fn restore(&self) -> Result<Option<Account>, Error> {
        SessionManager::restore(self)
    }
    fn reconnect(&self) -> Result<Option<Account>, Error> {
        SessionManager::reconnect(self)
    }
    fn sign_out(&self) -> Result<(), Error> {
        SessionManager::sign_out(self)
    }
    fn token_with_validity(
        &self,
        resource: &str,
        minimum: Duration,
    ) -> Result<ServiceToken, Error> {
        SessionManager::token_with_validity(self, resource, minimum)
    }
}

fn wall_deadline(expiry: Instant) -> SystemTime {
    SystemTime::now() + expiry.saturating_duration_since(Instant::now())
}

fn remaining(expiry: Instant, wall_deadline: SystemTime) -> Duration {
    expiry.saturating_duration_since(Instant::now()).min(
        wall_deadline
            .duration_since(SystemTime::now())
            .unwrap_or_default(),
    )
}

fn cached(granted: Granted, resource: &str) -> Cache {
    Cache {
        session: Some(granted.session),
        access: BTreeMap::from([(
            resource.into(),
            (
                granted.access,
                granted.expires_at,
                wall_deadline(granted.expires_at),
            ),
        )]),
        pending_save: None,
    }
}

fn encode(session: &Session) -> Secret {
    Secret::new(serde_json::json!({"account":session.account,"subject":session.subject,"refresh":session.refresh.expose(),"nonce":session.nonce.expose()}).to_string())
}

fn decode(value: &Secret) -> Result<Session, Error> {
    #[derive(Deserialize)]
    struct Stored {
        account: Account,
        subject: String,
        refresh: Secret,
        nonce: Secret,
    }
    let stored: Stored = serde_json::from_str(value.expose()).map_err(|_| Error::SignInRequired)?;
    Ok(Session {
        account: stored.account,
        subject: stored.subject,
        refresh: stored.refresh,
        nonce: stored.nonce,
    })
}

#[cfg(test)]
mod tests;
