//! Native credentials stay inside the API transport and are scoped to one origin.
use gpui_kit::App;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use subspace_framework::core::Secret;
use ureq::http::{HeaderValue, Request, Response, StatusCode, Uri, header::AUTHORIZATION};
use ureq::middleware::MiddlewareNext;
use ureq::{Body, SendBody};
use url::Url;

use crate::Error;

static ORIGIN: OnceLock<Url> = OnceLock::new();

struct Account {
    generation: Box<dyn Fn() -> u64 + Send + Sync>,
    ready_generation: fn(&App) -> Option<u64>,
    token: Box<dyn Fn() -> Result<Secret, Error> + Send + Sync>,
}

static ACCOUNT: OnceLock<Account> = OnceLock::new();

/// Installs the configured API base URL once, before Spaces start. It is the
/// server's origin, optionally followed by `/api` (`http://127.0.0.1:3000` and
/// `http://127.0.0.1:3000/api` are the same API). HTTPS is required except on
/// a loopback address, where a local development API listens.
pub fn configure_api_origin(value: &str) -> Result<(), Error> {
    let base = api_base(value)?;
    crate::backend::claim(crate::Backend::Subspace)?;
    install_origin(base)
}

/// Installs a NETT build's API base URL once, before Spaces start, and makes
/// NETT the process's only backend: the Subspace API origin can then never
/// be configured. The same URL rules apply.
pub fn configure_nett_origin(value: &str) -> Result<(), Error> {
    let base = api_base(value)?;
    crate::backend::claim(crate::Backend::Nett)?;
    install_origin(base)
}

fn install_origin(base: Url) -> Result<(), Error> {
    if ORIGIN.get_or_init(|| base.clone()) != &base {
        return Err(Error::InvalidRequest);
    }
    Ok(())
}

/// The normalised `{origin}/api` base of a configured API URL.
pub fn api_base(value: &str) -> Result<Url, Error> {
    let mut base = Url::parse(value.trim()).map_err(|_| Error::InvalidRequest)?;
    if !secure_or_loopback(&base)
        || base.host_str().is_none()
        || !matches!(base.path(), "" | "/" | "/api" | "/api/")
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(Error::InvalidRequest);
    }
    base.set_path("/api");
    Ok(base)
}

/// HTTPS anywhere, or plain HTTP to a loopback address only.
pub(crate) fn secure_or_loopback(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(name)) => name == "localhost",
            None => false,
        },
        _ => false,
    }
}

pub(crate) fn origin() -> Option<&'static Url> {
    ORIGIN.get()
}

/// The build-selected URL, when the product configured Subspace API.
pub fn configured_api_origin() -> Option<&'static str> {
    origin().map(Url::as_str)
}

/// Installs the trusted desktop account callbacks before the API-backed Spaces start.
/// The readiness callback reads local state and must not acquire a token.
pub fn use_native_account(
    generation: impl Fn() -> u64 + Send + Sync + 'static,
    ready_generation: fn(&App) -> Option<u64>,
    token: impl Fn() -> Result<Secret, Error> + Send + Sync + 'static,
) {
    let _ = ACCOUNT.set(Account {
        generation: Box::new(generation),
        ready_generation,
        token: Box::new(token),
    });
}

type Renewal = Box<dyn Fn() -> bool + Send + Sync>;
static RENEWAL: OnceLock<Renewal> = OnceLock::new();

/// Registers how to renew the account's API token on demand, for when the
/// service refuses a token the app still holds as valid. The function
/// returns whether a fresh token is now available.
pub fn use_token_renewal(renew: impl Fn() -> bool + Send + Sync + 'static) {
    let _ = RENEWAL.set(Box::new(renew));
}

/// Renews the account's API token once, as after a refused bearer. False
/// when nothing can renew it. One renewal runs at a time: a caller that
/// waited for another's renewal of the same account, or comes just after
/// one, uses its token. Another account's renewal never stands for this one.
pub(crate) fn renew_token() -> bool {
    static RENEWED: std::sync::Mutex<Option<(Option<u64>, std::time::Instant)>> =
        std::sync::Mutex::new(None);
    let Some(renew) = RENEWAL.get() else {
        return false;
    };
    let mut last = RENEWED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let account = account_generation();
    if recently_renewed(*last, account, std::time::Instant::now()) {
        return true;
    }
    let renewed = renew();
    if renewed {
        *last = Some((account, std::time::Instant::now()));
    }
    renewed
}

/// Whether `last`, the latest renewal and its account, stands for a renewal
/// `account` asks for at `now`.
fn recently_renewed(
    last: Option<(Option<u64>, std::time::Instant)>,
    account: Option<u64>,
    now: std::time::Instant,
) -> bool {
    const RECENT: std::time::Duration = std::time::Duration::from_secs(5);
    last.is_some_and(|(renewed, at)| {
        renewed == account && now.saturating_duration_since(at) < RECENT
    })
}

/// The signed-in account's generation alone. Reading it notices an account
/// change and drops what was kept for the previous account.
pub(crate) fn account_generation() -> Option<u64> {
    #[cfg(test)]
    if let Some(account) = test_lease::account() {
        return Some(account);
    }
    let generation = ACCOUNT.get().map(|account| (account.generation)())?;
    if SEEN.swap(generation, Ordering::SeqCst) != generation {
        account_changed();
    }
    Some(generation)
}

/// The last account generation seen; `u64::MAX` before the first.
static SEEN: AtomicU64 = AtomicU64::new(u64::MAX);

/// Private state kept per account goes when the account changes: cached
/// file grants, NETT organisations and their caches.
fn account_changed() {
    crate::nett::media::forget_grants();
    crate::nett::scope::forget_account();
}

/// What a client's lease is bound to: the account, and for NETT also the
/// selected organisation, so choosing another one retires every client.
pub(crate) fn generation() -> Option<u64> {
    #[cfg(test)]
    if let Some(lease) = test_lease::get() {
        return Some(lease);
    }
    let account = account_generation()?;
    Some(match crate::backend::backend() {
        crate::Backend::Subspace => account,
        crate::Backend::Nett => account
            .wrapping_shl(32)
            .wrapping_add(crate::nett::scope::generation() & 0xffff_ffff),
    })
}

/// Whether the product has a usable session for this client's account
/// generation. A NETT client's lease also names its organisation, so it is
/// compared with the current lease as a whole.
pub(crate) fn ready(cx: &App, lease: Option<u64>) -> bool {
    ACCOUNT.get().is_some_and(|account| {
        (account.ready_generation)(cx).is_some_and(|ready| {
            (account.generation)() == ready && lease.is_some() && lease == generation()
        })
    })
}

pub(crate) fn agent(lease: Option<u64>) -> ureq::Agent {
    agent_with(lease, crate::backend::backend() == crate::Backend::Nett)
}

/// The native agent, with NETT's flow control when `nett`.
fn agent_with(lease: Option<u64>, nett: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            subspace_framework::core::http::agent()
                .config()
                .tls_config()
                .clone(),
        )
        .proxy(None)
        .https_only(origin().is_some_and(|base| base.scheme() == "https"))
        .http_status_as_error(false)
        .max_redirects(0)
        .middleware(move |request: Request<SendBody>, next: MiddlewareNext| {
            authenticate(request, next, lease, nett)
        })
        .build()
        .into()
}

fn authenticate(
    mut request: Request<SendBody>,
    next: MiddlewareNext,
    lease: Option<u64>,
    nett: bool,
) -> Result<Response<Body>, ureq::Error> {
    // NETT: take a place among the account's requests first, so the lease
    // and bearer below are checked after any wait, just before sending.
    let account = account_generation().unwrap_or_default();
    let permit = if nett {
        match crate::nett::flow::acquire(account) {
            Ok(permit) => Some(permit),
            Err(wait) => return Ok(crate::nett::flow::refused(wait)),
        }
    } else {
        None
    };
    let Some(value) = bearer(request.uri(), lease).map_err(|err| ureq::Error::Other(err.into()))?
    else {
        return Ok(signed_out());
    };
    request.headers_mut().insert(AUTHORIZATION, value);
    crate::nett::add_organisation_header(&mut request);
    let result = next.handle(request);
    if lease != generation() {
        return Err(ureq::Error::Other(
            "The account changed during the request".into(),
        ));
    }
    match (permit, result) {
        (Some(permit), Ok(response)) => Ok(crate::nett::flow::hold(account, permit, response)),
        (_, result) => result,
    }
}

/// Resolve a current, sensitive bearer for either of the private API transports.
pub(crate) fn bearer(uri: &Uri, lease: Option<u64>) -> Result<Option<HeaderValue>, Error> {
    if !origin().is_some_and(|base| private_api(base, uri)) {
        return Err(Error::InvalidRequest);
    }
    let Some(account) = ACCOUNT.get().filter(|_| lease == generation()) else {
        return Ok(None);
    };
    let token = match (account.token)() {
        Ok(token) if lease == generation() => token,
        Err(Error::Network) => return Err(Error::Network),
        _ => return Ok(None),
    };
    let mut value: HeaderValue = format!("Bearer {}", token.expose())
        .parse()
        .map_err(|_| Error::InvalidRequest)?;
    value.set_sensitive(true);
    Ok(Some(value))
}

fn signed_out() -> Response<Body> {
    let mut response = Response::new(Body::builder().data(Vec::<u8>::new()));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response
}

fn private_api(base: &Url, uri: &Uri) -> bool {
    uri.scheme_str() == Some(base.scheme())
        && uri.authority().map(|authority| authority.as_str()) == Some(base.authority())
        && uri.path().starts_with("/api/v1/")
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    use super::{
        agent, bearer, configure_api_origin, configured_api_origin, generation, private_api, ready,
        recently_renewed, use_native_account,
    };

    #[test]
    fn a_renewal_stands_only_for_the_same_account_and_only_briefly() {
        let now = std::time::Instant::now();
        let last = Some((Some(1), now));
        assert!(recently_renewed(
            last,
            Some(1),
            now + std::time::Duration::from_secs(2)
        ));
        // Another account, just signed in, renews its own token.
        assert!(!recently_renewed(last, Some(2), now));
        assert!(!recently_renewed(
            last,
            Some(1),
            now + std::time::Duration::from_secs(6)
        ));
        assert!(!recently_renewed(None, Some(1), now));
    }
    use subspace_framework::core::Secret;
    use url::Url;

    const FIXTURE_ORIGIN: &str = "https://api.fixture.example.test/api";

    #[gpui_kit::test]
    fn registering_the_native_account_binds_requests_to_its_generation(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let version = Arc::new(AtomicU64::new(1));
        let source = version.clone();
        configure_api_origin(FIXTURE_ORIGIN).unwrap();
        use_native_account(
            move || source.load(Ordering::Relaxed),
            |_| Some(1),
            || Ok(Secret::new("fixture-sse-token")),
        );
        let lease = generation();
        assert_eq!(lease, Some(1));
        cx.update(|app| assert!(ready(app, lease)));
        cx.update(|app| assert!(!ready(app, None)));
        let stream = format!("{FIXTURE_ORIGIN}/v1/updates?project=SUB")
            .parse()
            .unwrap();
        let header = bearer(&stream, lease).unwrap().unwrap();
        assert!(header.is_sensitive());
        assert_eq!(header.to_str().unwrap(), "Bearer fixture-sse-token");
        // What was kept for account 1 goes when the account changes: its
        // cached grants and its NETT organisations, which are not shown to
        // or selectable by the next account.
        crate::nett::media::remember_for_test(FIXTURE_ORIGIN, 1);
        let org = "00000000-0000-0000-0000-0000000000a1";
        let organisations: Vec<crate::nett::Organisation> = serde_json::from_value(
            serde_json::json!([{"id": org.to_uppercase(), "name": "Fixture Works"}]),
        )
        .unwrap();
        crate::nett::scope::observe(Some(1), "user-1", organisations, None);
        assert_eq!(crate::nett::snapshot().organisations.len(), 1);
        assert!(crate::nett::scope::select(org));
        assert_eq!(
            crate::nett::snapshot().selected.as_deref(),
            Some(org.to_uppercase().as_str()),
            "the organisation's own spelling is kept"
        );
        // A reply read under another account is ignored.
        assert!(!crate::nett::scope::observe(
            Some(7),
            "user-7",
            Vec::new(),
            None
        ));
        // A NETT request waiting for a place while the account changes is
        // checked when it gets one, and never sent for the old account.
        let held: Vec<_> = (0..crate::nett::flow::CONCURRENT)
            .map(|_| crate::nett::flow::acquire(1).unwrap())
            .collect();
        let queued = std::thread::spawn(move || {
            super::agent_with(lease, true)
                .get(format!("{FIXTURE_ORIGIN}/v1/me"))
                .call()
                .map(|response| response.status().as_u16())
                .map_err(|_| ())
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        version.store(2, Ordering::Relaxed);
        assert_eq!(generation(), Some(2));
        cx.update(|app| assert!(!ready(app, lease)));
        drop(held);
        assert_eq!(
            queued.join().unwrap(),
            Ok(401),
            "refused before sending, not sent to the old account's host"
        );
        assert!(bearer(&stream, lease).unwrap().is_none());
        assert_eq!(crate::nett::media::cached_for_test(FIXTURE_ORIGIN), 0);
        let after = crate::nett::snapshot();
        assert!(after.organisations.is_empty() && after.selected.is_none());
        assert!(!crate::nett::scope::select(org));
        let result = agent(lease)
            .get(format!("{FIXTURE_ORIGIN}/v1/me"))
            .call()
            .unwrap();
        assert_eq!(result.status().as_u16(), 401);
    }

    #[test]
    fn native_tokens_are_scoped_to_the_private_api() {
        let base = Url::parse(FIXTURE_ORIGIN).unwrap();
        assert!(private_api(
            &base,
            &format!("{FIXTURE_ORIGIN}/v1/me").parse().unwrap()
        ));
        for url in [
            "http://api.fixture.example.test/api/v1/me",
            "https://other.example.test/api/v1/me",
            "https://api.fixture.example.test:8443/api/v1/me",
            "https://api.fixture.example.test.evil.test/api/v1/me",
            "https://api.fixture.example.test/me",
        ] {
            assert!(!private_api(&base, &url.parse().unwrap()));
        }
    }

    #[test]
    fn build_configuration_accepts_only_one_https_api_base() {
        for value in [
            "http://api.fixture.example.test/api",
            "https://user:password@api.fixture.example.test/api",
            "https://api.fixture.example.test/api?query",
            "https://api.fixture.example.test/api#fragment",
            "https://api.fixture.example.test/another-path",
        ] {
            assert!(configure_api_origin(value).is_err());
        }
        for (value, secure) in [
            ("http://127.0.0.1:3000", true),
            ("http://localhost:3000/api", true),
            ("http://[::1]:3000", true),
            ("http://192.168.1.10:3000", false),
        ] {
            let url = Url::parse(value).unwrap();
            assert_eq!(super::secure_or_loopback(&url), secure, "{value}");
        }
        configure_api_origin(&format!("{FIXTURE_ORIGIN}/")).unwrap();
        assert_eq!(configured_api_origin(), Some(FIXTURE_ORIGIN));
        assert!(configure_api_origin("https://other.example.test/api").is_err());
    }
}

/// A lease a unit test moves on its own thread, to retire a client at an
/// exact point: right after a reply was accepted, before what follows it.
#[cfg(test)]
pub(crate) mod test_lease {
    use std::cell::Cell;

    thread_local! {
        static LEASE: Cell<Option<u64>> = const { Cell::new(None) };
        /// The signed-in account, for a test that changes it.
        static ACCOUNT: Cell<Option<u64>> = const { Cell::new(None) };
        /// The lease to move to once the next reply has been read and
        /// accepted.
        pub(crate) static AFTER_REPLY: Cell<Option<u64>> = const { Cell::new(None) };
        /// The lease and account to move to once the next organisation read
        /// has been accepted, as when another account signs in just then.
        pub(crate) static AFTER_READ: Cell<Option<u64>> = const { Cell::new(None) };
    }

    pub(crate) fn set(lease: Option<u64>) {
        LEASE.with(|current| current.set(lease));
    }

    pub(crate) fn get() -> Option<u64> {
        LEASE.with(Cell::get)
    }

    pub(crate) fn set_account(account: Option<u64>) {
        ACCOUNT.with(|current| current.set(account));
    }

    pub(crate) fn account() -> Option<u64> {
        ACCOUNT.with(Cell::get)
    }

    pub(crate) fn reply_accepted() {
        if let Some(lease) = AFTER_REPLY.with(Cell::take) {
            set(Some(lease));
        }
    }

    pub(crate) fn read_accepted() {
        if let Some(next) = AFTER_READ.with(Cell::take) {
            set(Some(next));
            set_account(Some(next));
        }
    }
}
