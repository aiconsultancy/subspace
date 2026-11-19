//! The GitHub accounts group: Sign in with GitHub, its states, and Sign out,
//! with a fake sign-in and in-memory credentials.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, Entity, TestAppContext, Window, WindowHandle, px, size};
use subspace_core::Secret;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, GitHubAccounts, Runner};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::github_sign_in::{self, GitHubSignIn, Phase};
use subspace_github_auth::{AuthError, Authenticator, Cancel, Pending, Prompt, SignedIn, Started};

use crate::settings::{Settings, SettingsPage};

struct NoGh;

impl Environment for NoGh {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| "/nonexistent/subspace-test-gh".into())
    }
}

impl Runner for NoGh {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("no gh".into())
    }
}

/// Approves at once as `octo-dev`.
struct Approving;

impl Pending for Approving {
    fn finish(self: Box<Self>, _: &Cancel) -> Result<SignedIn, AuthError> {
        Ok(SignedIn {
            login: "octo-dev".into(),
            token: Secret::new("fixture-octo-dev"),
        })
    }
}

impl Authenticator for Approving {
    fn start(&self) -> Result<Started, AuthError> {
        Ok(Started {
            prompt: Prompt::Device {
                user_code: "OCTO-1234".into(),
                verification_uri: "https://github.com/login/device".into(),
            },
            pending: Box::new(Approving),
        })
    }

    fn revoke(&self, _: &Secret) {}
}

struct Fixture {
    settings: WindowHandle<Settings>,
    sign_in: Entity<GitHubSignIn>,
    credentials: Arc<Credentials>,
}

fn open(root: &std::path::Path, cx: &mut TestAppContext) -> Fixture {
    let (settings, sign_in, credentials) = open_with(root, true, cx);
    Fixture {
        settings,
        sign_in: sign_in.unwrap(),
        credentials,
    }
}

/// Settings on a GitHub Space's page, with the app's sign-in when
/// `offered` (the app has an OAuth App).
fn open_with(
    root: &std::path::Path,
    offered: bool,
    cx: &mut TestAppContext,
) -> (
    WindowHandle<Settings>,
    Option<Entity<GitHubSignIn>>,
    Arc<Credentials>,
) {
    let credentials = Arc::new(Credentials::new(&Config::default(), NoGh, NoGh));
    let shared = credentials.clone();
    let path = root.join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let sign_in = cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        let mut board = crate::settings::navigation_tests::registration();
        board.id = "github";
        board.title = "PRs".into();
        board.settings.sections = &[];
        board.settings.features = &[];
        board.connection = subspace_dashboard::settings::ConnectionSettings::new(&[
            subspace_dashboard::settings::ConnectionField::GitHubCredentials,
        ]);
        cx.set_global(crate::registry::IncludedBoards(vec![board]));
        cx.set_global(subspace_dashboard::Services {
            config: Arc::new(Config::default()),
            credentials: shared,
            tools: Default::default(),
            search: None,
            config_file: path.clone(),
        });
        offered.then(|| github_sign_in::install(Arc::new(Approving), cx))
    });
    let file = SettingsFile::open(&path).unwrap();
    let accounts = credentials.github_accounts().unwrap_or_default();
    let settings = cx.add_window(move |window, cx| {
        let mut settings = Settings::new(file, accounts, window, cx);
        settings.page = SettingsPage::Board("github");
        settings
    });
    cx.simulate_window_resize(settings.into(), size(px(1100.), px(1400.)));
    (settings, sign_in, credentials)
}

fn rendered<T>(
    settings: WindowHandle<Settings>,
    cx: &mut TestAppContext,
    check: impl FnOnce(&mut Window, &mut App) -> T,
) -> T {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        check(window, cx)
    })
    .unwrap()
}

fn listed(settings: WindowHandle<Settings>, cx: &mut TestAppContext) -> GitHubAccounts {
    settings
        .read_with(cx, |settings, _| settings.github.accounts.clone())
        .unwrap()
}

#[gpui_kit::test]
fn signing_in_lists_the_account_at_once_without_save(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = open(root.path(), cx);
    rendered(fixture.settings, cx, |window, cx| {
        assert_eq!(
            window.find("settings-github-accounts").label(),
            Some("GitHub accounts")
        );
        assert_ne!(
            window.find("settings-github-sign-in").disabled(),
            Some(true)
        );
        assert!(
            window
                .try_find("settings-github-account:octo-dev")
                .is_none()
        );
        window.click("settings-github-sign-in", cx);
    });
    cx.run_until_parked();
    assert_eq!(listed(fixture.settings, cx).logins, ["octo-dev"]);
    rendered(fixture.settings, cx, |window, _| {
        assert!(window.find("settings-github-account:octo-dev").visible());
        assert!(window.find("settings-github-sign-out:octo-dev").visible());
    });
    let token = fixture.credentials.account("octo-dev").unwrap().value;
    assert_eq!(token.expose(), "fixture-octo-dev");
}

#[gpui_kit::test]
fn each_sign_in_state_shows_its_controls_and_cancel_stops_it(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = open(root.path(), cx);
    let show = |phase: Phase, cx: &mut TestAppContext| {
        fixture
            .sign_in
            .update(cx, |sign_in, cx| sign_in.show(phase, cx));
    };
    show(Phase::Browser, cx);
    rendered(fixture.settings, cx, |window, _| {
        assert!(window.find("settings-github-sign-in-cancel").visible());
        assert_eq!(
            window.find("settings-github-sign-in").disabled(),
            Some(true)
        );
    });
    show(
        Phase::Device {
            user_code: "OCTO-1234".into(),
            verification_uri: "https://github.com/login/device".into(),
        },
        cx,
    );
    rendered(fixture.settings, cx, |window, cx| {
        for id in [
            "settings-github-sign-in-code",
            "settings-github-sign-in-copy",
            "settings-github-sign-in-open",
            "settings-github-sign-in-cancel",
        ] {
            assert!(window.find(id).visible(), "{id}");
        }
        window.click("settings-github-sign-in-cancel", cx);
    });
    cx.run_until_parked();
    let phase = fixture
        .sign_in
        .read_with(cx, |sign_in, _| sign_in.phase().clone());
    assert_eq!(phase, Phase::Idle);
    show(Phase::Failed("GitHub could not be reached.".into()), cx);
    rendered(fixture.settings, cx, |window, _| {
        assert!(window.find("settings-github-sign-in-error").visible());
        assert_ne!(
            window.find("settings-github-sign-in").disabled(),
            Some(true)
        );
    });
}

#[gpui_kit::test]
fn signing_out_removes_the_account_and_its_token(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = open(root.path(), cx);
    fixture
        .credentials
        .sign_in("octo-dev", &Secret::new("fixture-octo-dev"))
        .unwrap();
    fixture
        .settings
        .update(cx, |settings, window, cx| {
            settings.refresh_accounts(window, cx)
        })
        .unwrap();
    rendered(fixture.settings, cx, |window, cx| {
        window.click("settings-github-sign-out:octo-dev", cx);
    });
    cx.run_until_parked();
    assert!(listed(fixture.settings, cx).logins.is_empty());
    rendered(fixture.settings, cx, |window, _| {
        assert!(
            window
                .try_find("settings-github-account:octo-dev")
                .is_none()
        );
    });
    assert!(fixture.credentials.account("octo-dev").is_err());
}

#[gpui_kit::test]
fn an_app_without_an_oauth_app_offers_no_sign_in_or_sign_out(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (settings, sign_in, credentials) = open_with(root.path(), false, cx);
    assert!(sign_in.is_none());
    credentials
        .sign_in("octo-dev", &Secret::new("fixture-octo-dev"))
        .unwrap();
    settings
        .update(cx, |settings, window, cx| {
            settings.refresh_accounts(window, cx)
        })
        .unwrap();
    rendered(settings, cx, |window, _| {
        assert!(window.find("settings-github-account:octo-dev").visible());
        assert!(window.try_find("settings-github-sign-in").is_none());
        assert!(
            window
                .try_find("settings-github-sign-out:octo-dev")
                .is_none()
        );
    });
}
