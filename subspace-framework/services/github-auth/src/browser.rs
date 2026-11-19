//! The browser flow: GitHub's authorize page, then a one-shot listener on
//! `127.0.0.1` for its redirect to `/callback`. For an OAuth App with a
//! loopback callback GitHub accepts any port, so the listener takes a free
//! one. The listener uses `std::net`: it reads one request line on
//! loopback, which needs no HTTP server dependency.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use subspace_core::Secret;
use subspace_core::http::status::{BAD_REQUEST, NOT_FOUND, OK};

use crate::pkce::{self, Pkce};
use crate::token::{self, Reply};
use crate::{AuthError, Cancel, Endpoints, Pending, Prompt, SignedIn, Started};

pub const AUTHORIZE_PATH: &str = "/login/oauth/authorize";
pub const CALLBACK_PATH: &str = "/callback";
/// How often the listener checks for a connection, a cancel or the limit.
const POLL: Duration = Duration::from_millis(100);
/// How long one browser connection may take to send its request.
const READ_LIMIT: Duration = Duration::from_secs(10);
/// The longest request line and header line read.
const LINE_LIMIT: usize = 8 * 1024;
const MAX_HEADER_LINES: usize = 100;

/// What the authorize URL carries.
pub struct Authorize<'a> {
    pub oauth: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub state: &'a str,
    pub challenge: &'a str,
}

/// GitHub's authorize page. `prompt=select_account` lets a person with
/// several GitHub accounts choose one.
pub fn authorize_url(request: &Authorize<'_>) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", request.client_id)
        .append_pair("redirect_uri", request.redirect_uri)
        .append_pair("scope", crate::config::SCOPES)
        .append_pair("state", request.state)
        .append_pair("code_challenge", request.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "select_account")
        .finish();
    format!("{}{AUTHORIZE_PATH}?{query}", request.oauth)
}

/// What the browser's request means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callback {
    /// Not the callback, such as `/favicon.ico`.
    Other,
    Code(Secret),
    Failed(AuthError),
}

/// Reads `GET /callback?code=…&state=…` against the `expected` state.
pub fn parse_callback(target: &str, expected: &str) -> Callback {
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Callback::Other;
    };
    if url.path() != CALLBACK_PATH {
        return Callback::Other;
    }
    let value = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    if value("state").as_deref() != Some(expected) {
        return Callback::Failed(AuthError::StateMismatch);
    }
    match (value("code"), value("error")) {
        (Some(code), None) if !code.is_empty() => Callback::Code(Secret::new(code)),
        (_, error) => Callback::Failed(token::failure(error.as_deref(), None)),
    }
}

/// The request target of an HTTP request line, for `GET` only.
pub fn request_target(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return None;
    };
    Some(target)
}

/// The small page the browser shows after its redirect.
pub fn page(callback: &Callback) -> (u16, String) {
    use subspace_auth_return::State;
    let (status, state) = match callback {
        Callback::Code(_) => (OK, State::Succeeded),
        Callback::Failed(_) => (BAD_REQUEST, State::Failed),
        Callback::Other => (NOT_FOUND, State::NotFound),
    };
    let html = subspace_auth_return::page(state);
    (status, html)
}

/// Starts the browser flow: a listener on a free loopback port and the
/// authorize URL that redirects to it.
pub(crate) fn start(
    client_id: &str,
    secret: &Secret,
    endpoints: &Endpoints,
    time_limit: Duration,
) -> Result<Started, AuthError> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(listener_error)?;
    let port = listener.local_addr().map_err(listener_error)?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let pkce = Pkce::generate();
    let state = pkce::state();
    let url = authorize_url(&Authorize {
        oauth: &endpoints.oauth,
        client_id,
        redirect_uri: &redirect_uri,
        state: &state,
        challenge: &pkce.challenge,
    });
    let pending = BrowserPending {
        listener,
        exchange: Exchange {
            endpoints: endpoints.clone(),
            client_id: client_id.to_owned(),
            secret: secret.clone(),
            redirect_uri,
            verifier: pkce.verifier,
        },
        state,
        deadline: Instant::now() + time_limit,
    };
    Ok(Started {
        prompt: Prompt::Browser { url },
        pending: Box::new(pending),
    })
}

fn listener_error(error: std::io::Error) -> AuthError {
    AuthError::Listener(error.to_string())
}

/// What the code exchange sends.
struct Exchange {
    endpoints: Endpoints,
    client_id: String,
    secret: Secret,
    redirect_uri: String,
    verifier: Secret,
}

impl Exchange {
    fn token(&self, code: &Secret) -> Result<SignedIn, AuthError> {
        let form = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.secret.expose()),
            ("code", code.expose()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("code_verifier", self.verifier.expose()),
        ];
        match token::request(&self.endpoints.oauth, &form)? {
            Reply::Token(granted) => crate::signed_in(&self.endpoints, granted),
            Reply::Failed(error) => Err(error),
            Reply::Pending | Reply::SlowDown(_) => Err(token::failure(None, None)),
        }
    }
}

struct BrowserPending {
    listener: TcpListener,
    exchange: Exchange,
    state: String,
    deadline: Instant,
}

impl Pending for BrowserPending {
    fn finish(self: Box<Self>, cancel: &Cancel) -> Result<SignedIn, AuthError> {
        let code = wait_for_code(&self.listener, &self.state, self.deadline, cancel)?;
        drop(self.listener);
        self.exchange.token(&code)
    }
}

/// Answers browser connections until one brings the callback, the time
/// limit passes or the sign-in is cancelled.
fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    deadline: Instant,
    cancel: &Cancel,
) -> Result<Secret, AuthError> {
    listener.set_nonblocking(true).map_err(listener_error)?;
    let (sender, receiver) = mpsc::channel();
    loop {
        if cancel.is_cancelled() {
            return Err(AuthError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AuthError::TimedOut);
        }
        accept_one(listener, state, &sender)?;
        match receiver.recv_timeout(POLL) {
            Ok(Callback::Code(code)) => return Ok(code),
            Ok(Callback::Failed(error)) => return Err(error),
            Ok(Callback::Other) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(AuthError::Cancelled),
        }
    }
}

/// Hands a waiting connection to its own thread, so a browser's idle
/// speculative connection cannot hold up the real one.
fn accept_one(
    listener: &TcpListener,
    state: &str,
    sender: &Sender<Callback>,
) -> Result<(), AuthError> {
    match listener.accept() {
        Ok((stream, _)) => {
            let (state, sender) = (state.to_owned(), sender.clone());
            std::thread::spawn(move || {
                if let Some(callback) = answer(stream, &state) {
                    let _ = sender.send(callback);
                }
            });
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(error) => Err(listener_error(error)),
    }
}

/// Reads one request, answers it with a page and gives what it meant.
fn answer(stream: TcpStream, state: &str) -> Option<Callback> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(READ_LIMIT)).ok()?;
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let line = read_line(&mut reader)?;
    skip_headers(&mut reader)?;
    let callback =
        request_target(&line).map_or(Callback::Other, |target| parse_callback(target, state));
    write_page(stream, &callback);
    Some(callback)
}

fn read_line(reader: &mut BufReader<TcpStream>) -> Option<String> {
    let mut line = String::new();
    let read = std::io::Read::take(reader, LINE_LIMIT as u64)
        .read_line(&mut line)
        .ok()?;
    (read > 0).then_some(line)
}

fn skip_headers(reader: &mut BufReader<TcpStream>) -> Option<()> {
    for _ in 0..MAX_HEADER_LINES {
        if read_line(reader)?.trim().is_empty() {
            return Some(());
        }
    }
    None
}

fn write_page(mut stream: TcpStream, callback: &Callback) {
    let (status, html) = page(callback);
    let reason = match status {
        OK => "OK",
        BAD_REQUEST => "Bad Request",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        html.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(html.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::{Authorize, Callback, authorize_url, page, parse_callback, request_target};
    use crate::AuthError;
    use subspace_core::Secret;

    #[test]
    fn the_authorize_url_asks_for_pkce_scopes_and_an_account_choice() {
        let url = authorize_url(&Authorize {
            oauth: "https://github.com",
            client_id: "client",
            redirect_uri: "http://127.0.0.1:5000/callback",
            state: "state-value",
            challenge: "challenge-value",
        });
        let parsed = url::Url::parse(&url).unwrap();
        assert_eq!(parsed.path(), "/login/oauth/authorize");
        let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        let expected = [
            ("client_id", "client"),
            ("redirect_uri", "http://127.0.0.1:5000/callback"),
            ("scope", "repo read:org"),
            ("state", "state-value"),
            ("code_challenge", "challenge-value"),
            ("code_challenge_method", "S256"),
            ("prompt", "select_account"),
        ];
        assert_eq!(
            pairs,
            expected.map(|(key, value)| (key.to_owned(), value.to_owned()))
        );
    }

    #[test]
    fn the_callback_needs_the_expected_state() {
        assert_eq!(
            parse_callback("/callback?code=abc&state=good", "good"),
            Callback::Code(Secret::new("abc"))
        );
        assert_eq!(
            parse_callback("/callback?code=abc&state=bad", "good"),
            Callback::Failed(AuthError::StateMismatch)
        );
        assert_eq!(
            parse_callback("/callback?code=abc", "good"),
            Callback::Failed(AuthError::StateMismatch)
        );
        assert_eq!(
            parse_callback("/callback?error=access_denied&state=good", "good"),
            Callback::Failed(AuthError::Denied)
        );
        assert_eq!(parse_callback("/favicon.ico", "good"), Callback::Other);
        assert_eq!(
            request_target("GET /callback?x=1 HTTP/1.1\r\n"),
            Some("/callback?x=1")
        );
        assert_eq!(request_target("POST /callback HTTP/1.1"), None);
    }

    #[test]
    fn pages_say_what_happened_without_echoing_the_request() {
        let (status, html) = page(&Callback::Code(Secret::new("code-value")));
        assert_eq!(status, 200);
        assert!(html.contains("Signed in to Subspace") && html.contains("close this tab"));
        assert!(!html.contains("code-value"));
        let (status, html) = page(&Callback::Failed(AuthError::StateMismatch));
        assert_eq!(status, 400);
        assert!(html.contains("Sign-in did not finish"));
        assert_eq!(page(&Callback::Other).0, 404);
        for callback in [
            Callback::Code(Secret::new("code-value")),
            Callback::Failed(AuthError::StateMismatch),
            Callback::Other,
        ] {
            let (_, html) = page(&callback);
            assert!(html.contains("alt=\"Subspace\""));
            assert_eq!(html.matches("src=\"data:image/svg+xml;base64,").count(), 1);
            assert!(!html.contains("src=\"http") && !html.contains("{{"));
        }
    }
}
