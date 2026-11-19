//! Streamable HTTP on one `/mcp` endpoint. `rmcp` validates the protocol
//! headers, `Host` and `Origin`; this adds the optional bearer token, the
//! endpoint path and the listener.

use crate::options::HttpOptions;
use crate::server::ToolServer;
use bytes::Bytes;
use http::{HeaderMap, Request, Response, StatusCode, header};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use rmcp::transport::StreamableHttpServerConfig;
use rmcp::transport::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_service::Service;

pub const ENDPOINT: &str = "/mcp";
const RATE_LIMIT: usize = 60;
const RATE_WINDOW: Duration = Duration::from_secs(60);

type Body = BoxBody<Bytes, Infallible>;
type Mcp = StreamableHttpService<ToolServer, NeverSessionManager>;

const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];
const LOOPBACK_ORIGINS: [&str; 6] = [
    "http://localhost:*",
    "https://localhost:*",
    "http://127.0.0.1:*",
    "https://127.0.0.1:*",
    "http://[::1]:*",
    "https://[::1]:*",
];

/// A bound listener and the service it serves.
pub struct Endpoint {
    server: ToolServer,
    listener: TcpListener,
    mcp: Mcp,
    token: Option<Arc<str>>,
    rate: Arc<RateLimiter>,
    cancel: CancellationToken,
}

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("{0}")]
    AddrInUse(String),
    #[error("{0}")]
    Other(String),
}

impl BindError {
    fn from_io(address: SocketAddr, error: std::io::Error) -> Self {
        let message = format!("Cannot listen on {address}: {error}");
        match error.kind() {
            std::io::ErrorKind::AddrInUse => Self::AddrInUse(message),
            _ => Self::Other(message),
        }
    }
}

impl Endpoint {
    pub async fn bind(server: ToolServer, options: &HttpOptions) -> Result<Self, BindError> {
        let listener = TcpListener::bind(options.bind)
            .await
            .map_err(|error| BindError::from_io(options.bind, error))?;
        let cancel = CancellationToken::new();
        let config = config(options, cancel.child_token());
        let shutdown_server = server.clone();
        let mcp = StreamableHttpService::new(
            move || Ok(server.clone()),
            Arc::new(NeverSessionManager::default()),
            config,
        );
        Ok(Self {
            listener,
            mcp,
            token: options.token.as_deref().map(Arc::from),
            rate: Arc::new(RateLimiter::default()),
            cancel,
            server: shutdown_server,
        })
    }

    pub fn address(&self) -> Result<SocketAddr, String> {
        self.listener
            .local_addr()
            .map_err(|error| error.to_string())
    }

    /// Serves until `shutdown` completes.
    pub async fn serve(self, shutdown: impl Future<Output = ()>) {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    if let Ok((stream, _)) = accepted {
                        tokio::spawn(connection(stream, self.mcp.clone(), self.token.clone(), self.rate.clone(), self.cancel.child_token()));
                    }
                }
                () = &mut shutdown => break,
            }
        }
        self.server.cancel();
        self.cancel.cancel();
    }
}

/// Stateless requests, an SSE stream per response, and `Host` and `Origin`
/// checks that default to this machine.
fn config(options: &HttpOptions, cancel: CancellationToken) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(false)
        .with_cancellation_token(cancel)
        .with_allowed_hosts(allowed_hosts(options))
        .with_allowed_origins(
            LOOPBACK_ORIGINS
                .iter()
                .map(|origin| (*origin).to_owned())
                .chain(options.origins.iter().cloned()),
        )
        .enforce_origin_validation()
}

fn allowed_hosts(options: &HttpOptions) -> Vec<String> {
    let ip = options.bind.ip();
    let bound = (!ip.is_unspecified() && !ip.is_loopback()).then(|| ip.to_string());
    LOOPBACK_HOSTS
        .iter()
        .map(|host| (*host).to_owned())
        .chain(bound)
        .chain(options.hosts.iter().cloned())
        .collect()
}

async fn connection(
    stream: tokio::net::TcpStream,
    mcp: Mcp,
    token: Option<Arc<str>>,
    rate: Arc<RateLimiter>,
    cancel: CancellationToken,
) {
    let service = hyper::service::service_fn(move |request: Request<Incoming>| {
        let (mut mcp, token, rate) = (mcp.clone(), token.clone(), rate.clone());
        async move {
            Ok::<_, Infallible>(match gate(&request, token.as_deref(), &rate) {
                Some(refused) => refused,
                None => mcp
                    .call(request)
                    .await
                    .unwrap_or_else(|never| match never {}),
            })
        }
    });
    tokio::select! {
        _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service) => {},
        () = cancel.cancelled() => {},
    }
}

/// The refusal for a request outside the endpoint or without the token.
fn gate<B>(
    request: &Request<B>,
    token: Option<&str>,
    rate: &RateLimiter,
) -> Option<Response<Body>> {
    if request.uri().path() != ENDPOINT {
        return Some(plain(
            StatusCode::NOT_FOUND,
            "Not Found: the MCP endpoint is /mcp",
        ));
    }
    if !rate.allow() {
        return Some(rate_limited());
    }
    match token {
        Some(token) if !authorized(request.headers(), token) => Some(unauthorized()),
        _ => None,
    }
}

#[derive(Default)]
struct RateLimiter {
    state: Mutex<Option<(Instant, usize)>>,
}

impl RateLimiter {
    fn allow(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let now = Instant::now();
        match *state {
            Some((started, count)) if now.duration_since(started) < RATE_WINDOW => {
                if count >= RATE_LIMIT {
                    false
                } else {
                    *state = Some((started, count + 1));
                    true
                }
            }
            _ => {
                *state = Some((now, 1));
                true
            }
        }
    }
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|presented| same(presented.as_bytes(), token.as_bytes()))
}

/// Compares without stopping at the first difference.
fn same(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn unauthorized() -> Response<Body> {
    let mut response = plain(
        StatusCode::UNAUTHORIZED,
        "Unauthorized: a bearer token is required",
    );
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer"),
    );
    response
}

fn rate_limited() -> Response<Body> {
    plain(
        StatusCode::TOO_MANY_REQUESTS,
        "Too Many Requests: MCP HTTP rate limit exceeded (60 requests per minute)",
    )
}

fn plain(status: StatusCode, text: &'static str) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from_static(text.as_bytes())).boxed());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::{RateLimiter, allowed_hosts, authorized, gate, same};
    use crate::options::HttpOptions;
    use http::{HeaderMap, HeaderValue, Request, StatusCode};

    fn options(bind: &str, hosts: &[&str]) -> HttpOptions {
        HttpOptions {
            bind: bind.parse().unwrap(),
            token: None,
            origins: Vec::new(),
            hosts: hosts.iter().map(|host| (*host).to_owned()).collect(),
        }
    }

    #[test]
    fn hosts_cover_loopback_the_bound_address_and_extra_names() {
        assert_eq!(
            allowed_hosts(&options("127.0.0.1:7331", &[])),
            ["localhost", "127.0.0.1", "::1"]
        );
        assert_eq!(
            allowed_hosts(&options("192.0.2.4:7331", &["mcp.example"])),
            ["localhost", "127.0.0.1", "::1", "192.0.2.4", "mcp.example"]
        );
        assert_eq!(allowed_hosts(&options("0.0.0.0:7331", &[])).len(), 3);
    }

    #[test]
    fn only_the_exact_bearer_token_passes() {
        let mut headers = HeaderMap::new();
        assert!(!authorized(&headers, "secret"));
        headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
        assert!(authorized(&headers, "secret"));
        for wrong in ["Bearer secre", "Bearer secret2", "bearer secret", "secret"] {
            headers.insert("authorization", HeaderValue::from_static(wrong));
            assert!(!authorized(&headers, "secret"), "{wrong}");
        }
        assert!(same(b"abc", b"abc") && !same(b"abc", b"abd") && !same(b"ab", b"abc"));
    }

    #[test]
    fn the_gate_refuses_other_paths_and_missing_tokens() {
        let request = |path: &str| Request::post(path).body(()).unwrap();
        let rate = RateLimiter::default();
        let other = gate(&request("/other"), None, &rate).unwrap();
        assert_eq!(other.status(), StatusCode::NOT_FOUND);
        assert!(gate(&request("/mcp"), None, &rate).is_none());
        let refused = gate(&request("/mcp"), Some("secret"), &rate).unwrap();
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(refused.headers()["www-authenticate"], "Bearer");
    }

    #[test]
    fn the_bind_rate_limit_refuses_the_next_request() {
        let rate = RateLimiter::default();
        for _ in 0..60 {
            assert!(rate.allow());
        }
        assert!(!rate.allow());
    }
}
