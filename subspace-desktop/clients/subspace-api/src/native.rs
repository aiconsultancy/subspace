//! Native credentials stay inside the API transport and are scoped to one origin.
use gpui_kit::App;
use std::sync::OnceLock;
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

pub(crate) fn generation() -> Option<u64> {
    ACCOUNT.get().map(|account| (account.generation)())
}

/// Whether the product has a usable session for this client's account generation.
pub(crate) fn ready(cx: &App, lease: Option<u64>) -> bool {
    ACCOUNT.get().is_some_and(|account| {
        (account.ready_generation)(cx)
            .is_some_and(|ready| lease == Some(ready) && (account.generation)() == ready)
    })
}

pub(crate) fn agent(lease: Option<u64>) -> ureq::Agent {
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
            authenticate(request, next, lease)
        })
        .build()
        .into()
}

fn authenticate(
    mut request: Request<SendBody>,
    next: MiddlewareNext,
    lease: Option<u64>,
) -> Result<Response<Body>, ureq::Error> {
    let Some(value) = bearer(request.uri(), lease).map_err(|err| ureq::Error::Other(err.into()))?
    else {
        return Ok(signed_out());
    };
    request.headers_mut().insert(AUTHORIZATION, value);
    let result = next.handle(request);
    if lease != generation() {
        return Err(ureq::Error::Other(
            "The account changed during the request".into(),
        ));
    }
    result
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
        use_native_account,
    };
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
        version.store(2, Ordering::Relaxed);
        assert_eq!(generation(), Some(2));
        cx.update(|app| assert!(!ready(app, lease)));
        assert!(bearer(&stream, lease).unwrap().is_none());
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
