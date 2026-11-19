//! The browser and device flows end to end against a fake GitHub on
//! loopback. No request leaves the machine.

mod fake;

use std::time::Duration;

use subspace_core::Secret;
use subspace_github_auth::{
    AuthError, Authenticator as _, Cancel, Endpoints, GitHubOAuth, OAuthClient, Prompt, pkce,
};

use fake::{FakeGitHub, Options};

fn oauth(github: &FakeGitHub, secret: Option<&str>) -> GitHubOAuth {
    let client = OAuthClient {
        id: "fake-client".into(),
        secret: secret.map(Secret::new),
    };
    let endpoints = Endpoints {
        oauth: github.url(),
        api: github.url(),
    };
    GitHubOAuth::new(client, endpoints).with_time_limit(Duration::from_secs(10))
}

/// The authorize URL's query value `name`.
fn query(url: &str, name: &str) -> String {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap()
}

/// What a browser does after GitHub approves: follow the redirect.
fn follow_redirect(url: &str, state: &str) -> std::thread::JoinHandle<(u16, String)> {
    let redirect = format!(
        "{}?code=fake-code&state={state}",
        query(url, "redirect_uri")
    );
    std::thread::spawn(move || {
        let mut response = ureq::get(&redirect)
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .unwrap();
        let status = response.status().as_u16();
        (status, response.body_mut().read_to_string().unwrap())
    })
}

fn browser_url(prompt: &Prompt) -> String {
    match prompt {
        Prompt::Browser { url } => url.clone(),
        Prompt::Device { .. } => panic!("expected the browser flow"),
    }
}

#[test]
fn the_browser_flow_exchanges_the_code_with_pkce_and_names_the_account() {
    let github = FakeGitHub::start(Options::default());
    let started = oauth(&github, Some("fake-secret")).start().unwrap();
    let url = browser_url(&started.prompt);
    assert!(url.starts_with(&format!("{}/login/oauth/authorize?", github.url())));
    assert!(query(&url, "redirect_uri").starts_with("http://127.0.0.1:"));
    let browser = follow_redirect(&url, &query(&url, "state"));
    let signed_in = started.pending.finish(&Cancel::default()).unwrap();
    assert_eq!(signed_in.login, "octo-dev");
    assert_eq!(signed_in.token, Secret::new("fixture-octo-dev"));
    let (status, page) = browser.join().unwrap();
    assert_eq!(status, 200);
    assert!(page.contains("Signed in to Subspace"));
    let form = github.token_form();
    assert_eq!(form["client_id"], "fake-client");
    assert_eq!(form["client_secret"], "fake-secret");
    assert_eq!(form["code"], "fake-code");
    assert_eq!(form["redirect_uri"], query(&url, "redirect_uri"));
    assert_eq!(
        pkce::challenge(&form["code_verifier"]),
        query(&url, "code_challenge")
    );
}

#[test]
fn a_reply_with_another_state_is_refused_before_any_exchange() {
    let github = FakeGitHub::start(Options::default());
    let started = oauth(&github, Some("fake-secret")).start().unwrap();
    let browser = follow_redirect(&browser_url(&started.prompt), "forged");
    let error = started.pending.finish(&Cancel::default()).unwrap_err();
    assert_eq!(error, AuthError::StateMismatch);
    assert_eq!(browser.join().unwrap().0, 400);
    assert!(github.token_form().is_empty());
}

#[test]
fn a_browser_sign_in_stops_at_its_time_limit_or_when_cancelled() {
    let github = FakeGitHub::start(Options::default());
    let quick = oauth(&github, Some("fake-secret")).with_time_limit(Duration::from_millis(300));
    let error = quick.start().unwrap().pending.finish(&Cancel::default());
    assert_eq!(error.unwrap_err(), AuthError::TimedOut);

    let started = oauth(&github, Some("fake-secret")).start().unwrap();
    let cancel = Cancel::default();
    let waiting = {
        let cancel = cancel.clone();
        std::thread::spawn(move || started.pending.finish(&cancel))
    };
    std::thread::sleep(Duration::from_millis(200));
    cancel.cancel();
    assert_eq!(waiting.join().unwrap().unwrap_err(), AuthError::Cancelled);
}

#[test]
fn a_token_without_both_scopes_is_refused() {
    let github = FakeGitHub::start(Options {
        scope: "repo",
        ..Options::default()
    });
    let started = oauth(&github, Some("fake-secret")).start().unwrap();
    let url = browser_url(&started.prompt);
    let browser = follow_redirect(&url, &query(&url, "state"));
    let error = started.pending.finish(&Cancel::default()).unwrap_err();
    browser.join().unwrap();
    assert_eq!(error, AuthError::MissingScopes("read:org".into()));
}

#[test]
fn without_a_secret_the_device_flow_polls_until_the_code_is_entered() {
    let github = FakeGitHub::start(Options {
        pending_polls: 2,
        ..Options::default()
    });
    let started = oauth(&github, None).start().unwrap();
    assert_eq!(
        started.prompt,
        Prompt::Device {
            user_code: "OCTO-1234".into(),
            verification_uri: format!("{}/login/device", github.url()),
        }
    );
    let signed_in = started.pending.finish(&Cancel::default()).unwrap();
    assert_eq!(signed_in.login, "octo-dev");
    assert_eq!(github.polls(), 3);
    let form = github.token_form();
    assert_eq!(
        form["grant_type"],
        "urn:ietf:params:oauth:grant-type:device_code"
    );
    assert!(!form.contains_key("client_secret"));
}

#[test]
fn a_disabled_device_flow_says_so() {
    let github = FakeGitHub::start(Options {
        device_disabled: true,
        ..Options::default()
    });
    let error = oauth(&github, None).start().err().unwrap();
    assert_eq!(error, AuthError::DeviceFlowDisabled);
}
