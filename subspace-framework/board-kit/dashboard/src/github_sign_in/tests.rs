use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use subspace_core::Secret;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner};
use subspace_github_auth::{AuthError, Authenticator, Cancel, Pending, Prompt, SignedIn, Started};

use super::{GitHubSignIn, Phase, SignInEvent, install, shared};
use crate::Services;

struct NoEnvironment;

impl Environment for NoEnvironment {
    /// No gh accounts: gh's configuration directory does not exist.
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| "/nonexistent/subspace-test-gh".into())
    }
}

struct NoRunner;

impl Runner for NoRunner {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("no gh".into())
    }
}

/// A sign-in that shows a device code and then signs `octo-dev` in, or
/// fails to start with `refuse`.
#[derive(Default)]
struct Fake {
    refuse: Option<AuthError>,
    revoked: Mutex<Vec<Secret>>,
}

struct Approved;

impl Pending for Approved {
    fn finish(self: Box<Self>, _: &Cancel) -> Result<SignedIn, AuthError> {
        Ok(SignedIn {
            login: "octo-dev".into(),
            token: Secret::new("fixture-octo-dev"),
        })
    }
}

impl Authenticator for Fake {
    fn start(&self) -> Result<Started, AuthError> {
        if let Some(error) = &self.refuse {
            return Err(error.clone());
        }
        Ok(Started {
            prompt: Prompt::Device {
                user_code: "OCTO-1234".into(),
                verification_uri: "https://github.com/login/device".into(),
            },
            pending: Box::new(Approved),
        })
    }

    fn revoke(&self, token: &Secret) {
        self.revoked.lock().unwrap().push(token.clone());
    }
}

fn setup(fake: Arc<Fake>, cx: &mut TestAppContext) -> (Entity<GitHubSignIn>, Arc<Credentials>) {
    let credentials = Arc::new(Credentials::new(
        &Config::default(),
        NoEnvironment,
        NoRunner,
    ));
    let shared_credentials = credentials.clone();
    let entity = cx.update(|cx| {
        cx.set_global(Services {
            config: Arc::new(Config::default()),
            credentials: shared_credentials,
            tools: Default::default(),
            search: None,
            config_file: "config.json".into(),
        });
        install(fake, cx)
    });
    (entity, credentials)
}

fn events(entity: &Entity<GitHubSignIn>, cx: &mut TestAppContext) -> Arc<Mutex<Vec<SignInEvent>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    cx.update(|cx| {
        cx.subscribe(entity, move |_, event: &SignInEvent, _| {
            sink.lock().unwrap().push(event.clone());
        })
        .detach();
    });
    seen
}

#[gpui_kit::test]
fn a_finished_sign_in_keeps_the_token_in_the_store_and_announces_the_account(
    cx: &mut TestAppContext,
) {
    let (entity, credentials) = setup(Arc::default(), cx);
    let seen = events(&entity, cx);
    entity.update(cx, |sign_in, cx| sign_in.start(cx));
    assert!(entity.read_with(cx, |sign_in, _| sign_in.in_progress()));
    cx.run_until_parked();
    assert_eq!(
        entity.read_with(cx, |sign_in, _| sign_in.phase().clone()),
        Phase::Idle
    );
    assert_eq!(
        *seen.lock().unwrap(),
        [SignInEvent::SignedIn("octo-dev".into())]
    );
    let credential = credentials.github(None).unwrap();
    assert_eq!(credential.value.expose(), "fixture-octo-dev");
    assert!(cx.update(|cx| shared(cx)).is_some());
}

#[gpui_kit::test]
fn a_refused_start_shows_why_in_plain_words(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        refuse: Some(AuthError::DeviceFlowDisabled),
        ..Fake::default()
    });
    let (entity, credentials) = setup(fake, cx);
    entity.update(cx, |sign_in, cx| sign_in.start(cx));
    cx.run_until_parked();
    let phase = entity.read_with(cx, |sign_in, _| sign_in.phase().clone());
    assert_eq!(
        phase,
        Phase::Failed(AuthError::DeviceFlowDisabled.to_string())
    );
    assert!(credentials.github(None).is_err());
}

#[gpui_kit::test]
fn a_cancelled_sign_in_keeps_nothing(cx: &mut TestAppContext) {
    let (entity, credentials) = setup(Arc::default(), cx);
    let seen = events(&entity, cx);
    entity.update(cx, |sign_in, cx| {
        sign_in.start(cx);
        sign_in.cancel(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        entity.read_with(cx, |sign_in, _| sign_in.phase().clone()),
        Phase::Idle
    );
    assert!(seen.lock().unwrap().is_empty());
    assert!(credentials.github(None).is_err());
}

#[gpui_kit::test]
fn signing_out_forgets_the_token_and_revokes_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let (entity, credentials) = setup(fake.clone(), cx);
    credentials
        .sign_in("octo-dev", &Secret::new("fixture-octo-dev"))
        .unwrap();
    let seen = events(&entity, cx);
    entity.update(cx, |sign_in, cx| sign_in.sign_out("octo-dev".into(), cx));
    cx.run_until_parked();
    assert_eq!(
        *seen.lock().unwrap(),
        [SignInEvent::SignedOut("octo-dev".into())]
    );
    assert_eq!(
        *fake.revoked.lock().unwrap(),
        [Secret::new("fixture-octo-dev")]
    );
    assert!(credentials.github(None).is_err());
}

#[gpui_kit::test]
fn the_status_shows_nothing_while_idle_and_something_otherwise(cx: &mut TestAppContext) {
    let phases = [
        Phase::Starting,
        Phase::Browser,
        Phase::Device {
            user_code: "OCTO-1234".into(),
            verification_uri: "https://github.com/login/device".into(),
        },
        Phase::Failed("GitHub could not be reached.".into()),
    ];
    cx.update(|cx| {
        gpui_kit::init(cx);
        let tokens = crate::Tokens::current(cx);
        assert!(super::status("test", &Phase::Idle, &tokens, |_, _, _| {}, cx).is_none());
        for phase in &phases {
            assert!(
                super::status("test", phase, &tokens, |_, _, _| {}, cx).is_some(),
                "{phase:?}"
            );
        }
    });
}

#[gpui_kit::test]
fn an_app_without_an_oauth_app_installs_no_sign_in(cx: &mut TestAppContext) {
    cx.update(|cx| {
        super::init(None, cx);
        assert!(shared(cx).is_none());
        let client = subspace_github_auth::OAuthClient::new("Iv1.example", None);
        super::init(client, cx);
        assert!(shared(cx).is_some());
    });
}
