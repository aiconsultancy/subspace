use crate::Config;
use crate::config::Flow;
use crate::http::{Live, Transport};
use reply::{granted, remaining};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use subspace_account::loopback::{Loopback, Pkce, random_value};
use subspace_account::{
    Account, Cancel, CredentialStore, Error, PendingSlot, ProviderInfo, ServiceToken, Started,
    SystemStore,
};
use subspace_core::Secret;

const CALLBACK: &str = "/callback";
const AUTHORIZE: &str = "/user_management/authorize";
const AUTHENTICATE: &str = "/user_management/authenticate";
const AUTHORIZE_DEVICE: &str = "/user_management/authorize/device";
const EXPIRY_SKEW: Duration = Duration::from_secs(60);
/// Used when an access token carries no readable expiry.
const FALLBACK_LIFETIME: Duration = Duration::from_secs(300);

enum Pending {
    Browser {
        loopback: Loopback,
        pkce: Pkce,
        state: String,
    },
    Device(device::Device),
}

#[derive(Clone)]
struct Session {
    account: Account,
    refresh: Secret,
}

struct Access {
    token: Secret,
    expires_at: Instant,
    wall_deadline: SystemTime,
}

#[derive(Default)]
struct Cache {
    session: Option<Session>,
    access: Option<Access>,
    pending_save: Option<Secret>,
}

/// Serialises renewal and rotating-token persistence. Network calls belong
/// on a background thread; sign-out invalidates in-flight work first.
// qual:api — apps construct it and hand it to the shell as a provider.
pub struct SessionManager {
    config: Config,
    store: Arc<dyn CredentialStore>,
    transport: Arc<dyn Transport>,
    clock: Arc<dyn device::Clock>,
    cache: Mutex<Cache>,
    generation: AtomicU64,
    pending: PendingSlot<Pending>,
}

impl SessionManager {
    /// The OS keychain under the product's namespace.
    pub fn system(config: Config, product: &str) -> Self {
        let store = Arc::new(SystemStore::new(&config.namespace(product)));
        Self::new(config, store)
    }

    pub fn new(config: Config, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            config,
            store,
            transport: Arc::new(Live),
            clock: Arc::new(device::SystemClock),
            cache: Mutex::default(),
            generation: AtomicU64::default(),
            pending: PendingSlot::default(),
        }
    }

    fn authorize_url(&self, redirect: &str, pkce: &Pkce, state: &str) -> Result<String, Error> {
        let mut url =
            url::Url::parse(&self.config.endpoint(AUTHORIZE)).map_err(|_| Error::Unconfigured)?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("response_type", "code")
            .append_pair("provider", "authkit")
            .append_pair("state", state)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    fn redeem(&self, code: &str, pkce: &Pkce) -> Result<(Session, Access), Error> {
        let reply = self.transport.authenticate(
            &self.config.endpoint(AUTHENTICATE),
            serde_json::json!({
                "client_id": self.config.client_id,
                "grant_type": "authorization_code",
                "code": code,
                "code_verifier": pkce.verifier.expose(),
            }),
        )?;
        granted(reply, None)
    }

    fn refresh(&self, session: &Session) -> Result<(Session, Access), Error> {
        if self.config.flow == Flow::Device {
            return self.refresh_device(session);
        }
        let mut body = serde_json::json!({
            "client_id": self.config.client_id,
            "grant_type": "refresh_token",
            "refresh_token": session.refresh.expose(),
        });
        if !session.account.tenant.is_empty() {
            body["organization_id"] = serde_json::json!(session.account.tenant);
        }
        let reply = self
            .transport
            .authenticate(&self.config.endpoint(AUTHENTICATE), body)?;
        granted(reply, Some(&session.account))
    }

    /// Device clients renew with the form-encoded refresh grant: the grant
    /// type, client ID and refresh token, plus the organisation when the
    /// sign-in pinned one.
    fn refresh_device(&self, session: &Session) -> Result<(Session, Access), Error> {
        let mut form = vec![
            ("grant_type", "refresh_token"),
            ("client_id", self.config.client_id.as_str()),
            ("refresh_token", session.refresh.expose()),
        ];
        if !session.account.tenant.is_empty() {
            form.push(("organization_id", session.account.tenant.as_str()));
        }
        let reply = self
            .transport
            .refresh_form(&self.config.endpoint(AUTHENTICATE), &form)?;
        granted(reply, Some(&session.account))
    }

    fn accept(
        &self,
        (session, access): (Session, Access),
        cancel: &Cancel,
        generation: u64,
    ) -> Result<Account, Error> {
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        cancel.check()?;
        if generation != self.generation.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        cancel.begin_commit()?;
        self.store.save(&encode(&session))?;
        let account = session.account.clone();
        *cache = Cache {
            session: Some(session),
            access: Some(access),
            pending_save: None,
        };
        // Leases taken during the switch belong to the previous identity.
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(account)
    }

    fn renew(&self, cache: &mut Cache, generation: u64, minimum: Duration) -> Result<(), Error> {
        flush_save(self.store.as_ref(), cache)?;
        if cache
            .access
            .as_ref()
            .is_some_and(|access| remaining(access) > minimum)
        {
            return Ok(());
        }
        let session = cache.session.as_ref().ok_or(Error::SignInRequired)?;
        let (session, access) = self.refresh(session)?;
        // WorkOS has spent the old refresh token. The cache lock keeps this
        // the same account, so keep the rotated token even when the request
        // went stale or the OS store is temporarily locked; only the stale
        // access token is dropped.
        let current = generation == self.generation.load(Ordering::SeqCst);
        cache.pending_save = Some(encode(&session));
        cache.session = Some(session);
        if current {
            cache.access = Some(access);
        }
        flush_save(self.store.as_ref(), cache)?;
        if current {
            Ok(())
        } else {
            Err(Error::Cancelled)
        }
    }

    /// Ask WorkOS for a device code; the shell opens the verification page
    /// and shows the user code.
    fn start_device(&self) -> Result<Started, Error> {
        let issued = self.clock.now();
        let start = self.transport.authorize_device(
            &self.config.endpoint(AUTHORIZE_DEVICE),
            &self.config.client_id,
        )?;
        let (prompt, device) = device::validate(start, issued, self.config.loopback())?;
        self.pending.start_with_code(
            Some(prompt.url),
            Some(prompt.user_code),
            Pending::Device(device),
        )
    }

    fn load(&self, cache: &mut Cache) -> Result<bool, Error> {
        if cache.session.is_some() {
            return Ok(true);
        }
        let Some(stored) = self.store.load()? else {
            return Ok(false);
        };
        cache.session = Some(decode(&stored)?);
        Ok(true)
    }
}

impl subspace_account::Provider for SessionManager {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "WorkOS".into(),
            organization: self.config.organization_name().into(),
            detail: None,
            browser: true,
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn start(&self) -> Result<Started, Error> {
        if self.config.flow == Flow::Device {
            return self.start_device();
        }
        let loopback = Loopback::bind("127.0.0.1", self.config.redirect_port, CALLBACK)?;
        let pkce = Pkce::new();
        let state = random_value();
        let url = self.authorize_url(&loopback.redirect_uri(), &pkce, &state)?;
        self.pending.start(
            Some(url),
            Pending::Browser {
                loopback,
                pkce,
                state,
            },
        )
    }

    fn finish(&self, started: Started, cancel: &Cancel, generation: u64) -> Result<Account, Error> {
        match self.pending.take(&started)? {
            Pending::Browser {
                loopback,
                pkce,
                state,
            } => {
                let reply = loopback.wait(cancel, &state)?;
                reply.complete(cancel, |code| {
                    let granted = self.redeem(&code, &pkce)?;
                    self.accept(granted, cancel, generation)
                })
            }
            Pending::Device(device) => {
                let reply = device::poll(
                    self.transport.as_ref(),
                    self.clock.as_ref(),
                    &self.config.endpoint(AUTHENTICATE),
                    &self.config.client_id,
                    &device,
                    cancel,
                )?;
                self.accept(granted(reply, None)?, cancel, generation)
            }
        }
    }

    fn restore(&self) -> Result<Option<Account>, Error> {
        self.reconnect()
    }

    fn reconnect(&self) -> Result<Option<Account>, Error> {
        let generation = self.generation();
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        if !self.load(&mut cache)? {
            return Ok(None);
        }
        self.renew(&mut cache, generation, Duration::ZERO)?;
        Ok(cache
            .session
            .as_ref()
            .map(|session| session.account.clone()))
    }

    fn sign_out(&self) -> Result<(), Error> {
        self.invalidate();
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        self.store.delete()?;
        *cache = Cache::default();
        Ok(())
    }

    fn token_with_validity(
        &self,
        resource: &str,
        minimum: Duration,
    ) -> Result<ServiceToken, Error> {
        self.config.check_resource(resource)?;
        let generation = self.generation();
        let mut cache = self.cache.lock().map_err(|_| Error::Service)?;
        if !self.load(&mut cache)? {
            return Err(Error::SignInRequired);
        }
        self.renew(&mut cache, generation, minimum)?;
        let session = cache.session.as_ref().ok_or(Error::SignInRequired)?;
        let access = cache.access.as_ref().ok_or(Error::Service)?;
        if generation != self.generation() || remaining(access) <= minimum {
            return Err(Error::Service);
        }
        Ok(ServiceToken {
            account: session.account.clone(),
            token: access.token.clone(),
            expires_at: access.expires_at,
        })
    }
}

fn flush_save(store: &dyn CredentialStore, cache: &mut Cache) -> Result<(), Error> {
    if let Some(value) = &cache.pending_save {
        store.save(value)?;
    }
    cache.pending_save = None;
    Ok(())
}

fn encode(session: &Session) -> Secret {
    Secret::new(
        serde_json::json!({"account": session.account, "refresh": session.refresh.expose()})
            .to_string(),
    )
}

fn decode(value: &Secret) -> Result<Session, Error> {
    #[derive(Deserialize)]
    struct Stored {
        account: Account,
        refresh: Secret,
    }
    let stored: Stored = serde_json::from_str(value.expose()).map_err(|_| Error::SignInRequired)?;
    Ok(Session {
        account: stored.account,
        refresh: stored.refresh,
    })
}

mod device;
mod reply;
#[cfg(test)]
mod tests;
