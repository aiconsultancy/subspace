//! A bounded one-shot loopback callback for browser sign-in with PKCE. The
//! response first streams the waiting page, then fixed completion text once
//! the provider has verified and stored the result.

use crate::{Cancel, Error};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};
use subspace_auth_return::State;
use subspace_core::Secret;

const TIME_LIMIT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(100);
const REQUEST_LIMIT: usize = 16 * 1024;
const READ_BUFFER: usize = 1024;
const REQUEST_PARTS: usize = 3;
const RANDOM_BYTES: usize = 32;

/// A fresh URL-safe random value, for `state`, nonces and verifiers.
pub fn random_value() -> String {
    let bytes: [u8; RANDOM_BYTES] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A PKCE (RFC 7636) code verifier and its S256 challenge.
pub struct Pkce {
    pub verifier: Secret,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        let verifier = random_value();
        Self {
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            verifier: Secret::new(verifier),
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// One listener on IPv4 loopback, never a wildcard interface.
pub struct Loopback {
    listener: TcpListener,
    host: String,
    path: &'static str,
    deadline: Instant,
}

impl Loopback {
    /// Bind `127.0.0.1:port` (`0` for a fresh port). `host_name` is the name
    /// the redirect URI uses, `localhost` or `127.0.0.1`, as the provider's
    /// registration requires; `path` is the callback path.
    pub fn bind(host_name: &str, port: u16, path: &'static str) -> Result<Self, Error> {
        let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|_| Error::Listener)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| Error::Listener)?;
        let port = listener.local_addr().map_err(|_| Error::Listener)?.port();
        Ok(Self {
            listener,
            host: format!("{host_name}:{port}"),
            path,
            deadline: Instant::now() + TIME_LIMIT,
        })
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://{}{}", self.host, self.path)
    }

    /// Wait for the browser to return with `state`. Other requests are
    /// answered with Not found and ignored.
    pub fn wait(&self, cancel: &Cancel, state: &str) -> Result<Reply, Error> {
        loop {
            cancel.check()?;
            if Instant::now() >= self.deadline {
                return Err(Error::TimedOut);
            }
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    if let Some(code) = request(&mut stream, &self.host, self.path, state) {
                        return code.map(|code| Reply { stream, code });
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL)
                }
                Err(_) => return Err(Error::Listener),
            }
        }
    }
}

/// The browser's connection, held open while the provider redeems the code.
pub struct Reply {
    stream: TcpStream,
    code: String,
}

impl Reply {
    /// Show progress, run `exchange` with the authorisation code, then show
    /// the outcome. The browser only reports success after `exchange` does.
    pub fn complete<T>(
        mut self,
        cancel: &Cancel,
        exchange: impl FnOnce(String) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let nonce = random_value();
        begin(&mut self.stream, &nonce);
        let code = std::mem::take(&mut self.code);
        let result = cancel.check().and_then(|()| exchange(code));
        end(&mut self.stream, &nonce, outcome(result.as_ref().err()));
        result
    }
}

fn request(
    stream: &mut TcpStream,
    host: &str,
    path: &str,
    state: &str,
) -> Option<Result<String, Error>> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(POLL)).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .ok()?;
    let head = read_head(stream)?;
    match parse(&head, host, path, state) {
        Callback::Code(code) => Some(Ok(code)),
        Callback::Failed(error) => {
            write_simple(stream, outcome(Some(&error)));
            Some(Err(error))
        }
        Callback::Other => {
            write_simple(stream, State::NotFound);
            None
        }
    }
}

fn read_head(stream: &mut TcpStream) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut head = Vec::new();
    let mut buffer = [0; READ_BUFFER];
    while Instant::now() < deadline && head.len() < REQUEST_LIMIT {
        match stream.read(&mut buffer) {
            Ok(0) => return None,
            Ok(n) => {
                head.extend_from_slice(&buffer[..n]);
                if head.ends_with(b"\r\n\r\n") {
                    return String::from_utf8(head).ok();
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return None,
        }
    }
    None
}

#[derive(Debug, PartialEq, Eq)]
enum Callback {
    Code(String),
    Failed(Error),
    Other,
}

fn local_target(head: &str, expected_host: &str) -> Option<url::Url> {
    let mut lines = head.split("\r\n");
    let request: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    if request.len() != REQUEST_PARTS
        || request[0] != "GET"
        || request[2] != "HTTP/1.1"
        || !request[1].starts_with('/')
        || request[1].starts_with("//")
    {
        return None;
    }
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    if hosts != [expected_host] {
        return None;
    }
    url::Url::parse(&format!("http://{expected_host}{}", request[1])).ok()
}

fn parse(head: &str, expected_host: &str, path: &str, expected_state: &str) -> Callback {
    let Some(url) = local_target(head, expected_host) else {
        return Callback::Other;
    };
    if url.path() != path || url.fragment().is_some() {
        return Callback::Other;
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let values = |name: &str| {
        pairs
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, v)| v.as_ref())
            .collect::<Vec<_>>()
    };
    if values("state").len() != 1 || !equal(values("state")[0], expected_state) {
        return Callback::Other;
    }
    match (values("code").as_slice(), values("error").as_slice()) {
        ([code], []) if !code.is_empty() => Callback::Code((*code).to_owned()),
        ([], [error]) => Callback::Failed(callback_error(error)),
        _ => Callback::Failed(Error::InvalidReply),
    }
}

fn callback_error(code: &str) -> Error {
    match code {
        "access_denied" => Error::Cancelled,
        "interaction_required" | "login_required" | "consent_required" => Error::SignInRequired,
        "unauthorized_client" => Error::NotPermitted,
        "server_error" | "temporarily_unavailable" => Error::Service,
        _ => Error::InvalidReply,
    }
}

fn equal(left: &str, right: &str) -> bool {
    Sha256::digest(left.as_bytes()) == Sha256::digest(right.as_bytes())
}

fn outcome(error: Option<&Error>) -> State {
    match error {
        None => State::Succeeded,
        Some(Error::Cancelled) => State::Cancelled,
        Some(Error::TimedOut) => State::TimedOut,
        Some(_) => State::Failed,
    }
}

fn begin(stream: &mut TcpStream, nonce: &str) {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nTransfer-Encoding: chunked\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; img-src data:; script-src 'nonce-{nonce}'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(headers.as_bytes());
    chunk(stream, &subspace_auth_return::page(State::Completing));
}

fn end(stream: &mut TcpStream, nonce: &str, state: State) {
    chunk(
        stream,
        &format!(
            "<script nonce=\"{nonce}\">{}</script>",
            subspace_auth_return::completion_script(state)
        ),
    );
    let _ = stream.write_all(b"0\r\n\r\n");
    let _ = stream.flush();
}

fn chunk(stream: &mut TcpStream, text: &str) {
    let _ = stream.write_all(format!("{:x}\r\n{text}\r\n", text.len()).as_bytes());
    let _ = stream.flush();
}

fn write_simple(stream: &mut TcpStream, state: State) {
    let html = subspace_auth_return::page(state);
    let _ = stream.write_all(format!("HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{html}", html.len()).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::{Callback, Loopback, Pkce, parse};
    use crate::Error;

    const HOST: &str = "localhost:8123";
    const PATH: &str = "/auth/callback";

    fn head(target: &str) -> String {
        format!("GET {target} HTTP/1.1\r\nHost: {HOST}\r\n\r\n")
    }

    fn check(target: &str) -> Callback {
        parse(&head(target), HOST, PATH, "expected")
    }

    #[test]
    fn only_the_exact_callback_state_and_local_host_can_complete() {
        assert_eq!(
            check("/auth/callback?code=fixture&state=expected"),
            Callback::Code("fixture".into())
        );
        for target in [
            "/favicon.ico",
            "//attacker.test/auth/callback?code=x&state=expected",
            "/auth/callback?code=x&state=other",
            "/auth/callback?code=x&state=expected&state=expected",
        ] {
            assert_eq!(check(target), Callback::Other);
        }
        assert_eq!(
            parse(
                &head("/auth/callback?code=x&state=expected"),
                "attacker.test",
                PATH,
                "expected"
            ),
            Callback::Other
        );
    }

    #[test]
    fn provider_errors_map_to_fixed_outcomes_without_echoing_details() {
        assert_eq!(
            check("/auth/callback?error=server_error&error_description=do-not-echo&state=expected"),
            Callback::Failed(Error::Service)
        );
        assert_eq!(
            check("/auth/callback?code=x&code=y&state=expected"),
            Callback::Failed(Error::InvalidReply)
        );
        assert_eq!(
            check("/auth/callback?error=access_denied&state=expected"),
            Callback::Failed(Error::Cancelled)
        );
    }

    #[test]
    fn the_listener_names_its_own_port_and_pkce_values_are_fresh() {
        let loopback = Loopback::bind("127.0.0.1", 0, "/callback").unwrap();
        let uri = loopback.redirect_uri();
        assert!(uri.starts_with("http://127.0.0.1:") && uri.ends_with("/callback"));
        assert!(!uri.ends_with(":0/callback"));
        let (first, second) = (Pkce::new(), Pkce::new());
        assert_ne!(first.challenge, second.challenge);
        assert_eq!(first.verifier.expose().len(), 43);
    }
}
