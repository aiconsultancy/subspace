//! A fake GitHub on loopback: the token and device-code endpoints and
//! `GET /user`, enough for one sign-in.

use std::collections::BTreeMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
pub struct Options {
    /// The granted scopes the token answer names.
    pub scope: &'static str,
    /// How many device polls answer `authorization_pending` first.
    pub pending_polls: usize,
    pub device_disabled: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scope: "repo,read:org",
            pending_polls: 0,
            device_disabled: false,
        }
    }
}

#[derive(Default)]
struct Seen {
    token_form: BTreeMap<String, String>,
    polls: AtomicUsize,
}

pub struct FakeGitHub {
    address: String,
    seen: Arc<Mutex<Seen>>,
}

impl FakeGitHub {
    pub fn start(options: Options) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let shared = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                serve(stream, options, &shared);
            }
        });
        Self { address, seen }
    }

    pub fn url(&self) -> String {
        self.address.clone()
    }

    /// The last form posted to the token endpoint.
    pub fn token_form(&self) -> BTreeMap<String, String> {
        self.seen.lock().unwrap().token_form.clone()
    }

    pub fn polls(&self) -> usize {
        self.seen.lock().unwrap().polls.load(Ordering::Relaxed)
    }
}

fn serve(stream: TcpStream, options: Options, seen: &Mutex<Seen>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let answer = route(&path, &body, options, seen);
    respond(stream, &answer.to_string());
}

fn route(path: &str, body: &[u8], options: Options, seen: &Mutex<Seen>) -> serde_json::Value {
    let form: BTreeMap<String, String> = url::form_urlencoded::parse(body).into_owned().collect();
    match path {
        "/login/device/code" if options.device_disabled => {
            serde_json::json!({"error": "device_flow_disabled", "error_description": "Device Flow must be explicitly enabled for this App"})
        }
        "/login/device/code" => serde_json::json!({
            "device_code": "fake-device-code",
            "user_code": "OCTO-1234",
            "verification_uri": format!("{}/login/device", "http://ADDRESS"),
            "expires_in": 900,
            "interval": 0,
        }),
        "/login/oauth/access_token" => token(form, options, seen),
        "/user" => serde_json::json!({"login": "octo-dev", "id": 1}),
        _ => serde_json::json!({"message": "Not Found"}),
    }
}

fn token(
    form: BTreeMap<String, String>,
    options: Options,
    seen: &Mutex<Seen>,
) -> serde_json::Value {
    let mut seen = seen.lock().unwrap();
    let device = form.contains_key("device_code");
    seen.token_form = form;
    if device && seen.polls.fetch_add(1, Ordering::Relaxed) < options.pending_polls {
        return serde_json::json!({"error": "authorization_pending"});
    }
    serde_json::json!({"access_token": "fixture-octo-dev", "token_type": "bearer", "scope": options.scope})
}

fn respond(mut stream: TcpStream, body: &str) {
    let local = stream.local_addr().unwrap();
    let body = body.replace("http://ADDRESS", &format!("http://{local}"));
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
}
