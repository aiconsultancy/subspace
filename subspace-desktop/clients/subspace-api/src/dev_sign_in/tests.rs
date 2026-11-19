use super::{DevSignIn, DevUser, token_url};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::time::Duration;
use subspace_framework::account::{Cancel, Error, Provider as _};

/// One canned reply per connection; returns the request texts.
fn server(replies: usize) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..replies {
            let (mut stream, _) = listener.accept().unwrap();
            seen.push(read_request(&mut stream));
            let claims = URL_SAFE_NO_PAD.encode(r#"{"sub":"user-1","org_id":"org-1"}"#);
            let body = serde_json::json!({
                "accessToken": format!("e30.{claims}.sig"),
                "tokenType": "Bearer",
                "expiresIn": 28800,
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        seen
    });
    (origin, handle)
}

/// The whole request: headers, then a body of its Content-Length. Replying
/// before the body has arrived would close the socket with unread data and
/// reset the client's connection.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let n = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..n]);
        let text = String::from_utf8_lossy(&request);
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length || n == 0 {
                return String::from_utf8_lossy(&request).into_owned();
            }
        } else if n == 0 {
            return text.into_owned();
        }
    }
}

fn user() -> DevUser {
    DevUser {
        email: "developer@example.com".into(),
        name: "Local Developer".into(),
        organisation: "local".into(),
    }
}

#[test]
fn only_a_loopback_api_can_issue_dev_tokens() {
    assert!(DevSignIn::new("https://api.example.com", user()).is_err());
    let base = url::Url::parse("http://127.0.0.1:3000/api").unwrap();
    assert_eq!(
        token_url(&base).unwrap().as_str(),
        "http://127.0.0.1:3000/api/v1/auth/dev/token"
    );
    let local = DevSignIn::new("http://localhost:3000", user()).unwrap();
    // Capture builds may sign in with it because it holds no real account.
    assert!(local.local_development());
}

#[test]
fn blank_dev_identity_fields_use_the_issuers_defaults() {
    let identity = DevUser {
        organisation: "local".into(),
        ..DevUser::default()
    };
    assert_eq!(
        serde_json::to_value(identity).unwrap(),
        serde_json::json!({"organisation": "local"})
    );
}

#[test]
fn signing_in_issues_a_token_without_a_browser_and_sign_out_forgets_it() {
    let (origin, handle) = server(1);
    let provider = DevSignIn::new(&origin, user()).unwrap();
    assert!(!provider.info().browser);
    let started = provider.start().unwrap();
    assert!(started.url.is_none());
    let switching = provider.generation();
    let account = provider
        .finish(started, &Cancel::default(), switching)
        .unwrap();
    assert_ne!(
        provider.generation(),
        switching,
        "committing the identity advances the generation"
    );
    assert_eq!(
        (account.id.as_str(), account.tenant.as_str()),
        ("user-1", "org-1")
    );
    let token = provider
        .token_with_validity("subspace-api", Duration::from_secs(120))
        .unwrap();
    assert!(token.token.expose().starts_with("e30."));
    let request = &handle.join().unwrap()[0];
    assert!(request.starts_with("POST /api/v1/auth/dev/token HTTP/1.1"));
    assert!(request.contains(r#""email":"developer@example.com""#));
    assert!(!request.to_ascii_lowercase().contains("authorization"));
    provider.sign_out().unwrap();
    assert_eq!(
        provider
            .token_with_validity("subspace-api", Duration::ZERO)
            .err(),
        Some(Error::SignInRequired)
    );
}

#[test]
fn startup_restoration_signs_in_silently() {
    let (origin, handle) = server(1);
    let provider = DevSignIn::new(&origin, user()).unwrap();
    assert_eq!(provider.reconnect().unwrap(), None);
    let signed_out = provider.generation();
    assert_eq!(provider.restore().unwrap().unwrap().name, "Local Developer");
    // Reads made while signed out belong to an older account generation.
    let signed_in = provider.generation();
    assert_ne!(signed_in, signed_out);
    provider
        .token_with_validity("subspace-api", Duration::from_secs(60))
        .unwrap();
    assert_eq!(provider.generation(), signed_in);
    assert_eq!(handle.join().unwrap().len(), 1);
}
