//! A build with no API backend (`api_backend: "none"`) starts with no
//! sign-in and reaches no API. A child process resolves and installs the
//! launch from stamped settings that name no backend, with the Subspace API
//! debug fixture variables a stray shell or the capture script would set,
//! and drives the shared catalogue and fleet service headless. The fixture
//! records any request; the child keeps its state under a temporary home.

#[allow(dead_code)]
#[path = "../build/settings.rs"]
mod settings;

#[allow(dead_code)]
#[path = "../src/launch.rs"]
mod launch;

#[allow(dead_code)]
#[path = "../src/connection.rs"]
mod connection;

use gpui_kit::TestAppContext;
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::{Capabilities, Client};
use subspace_client_fleet::FleetApi;
use subspace_framework::client_test_support::Mock;

const CHILD: &str = "NO_BACKEND_BUILD_TEST_CHILD";
/// Where each platform keeps a person's state, all moved to a temporary home.
const STATE_VARIABLES: [&str; 8] = [
    "HOME",
    "USERPROFILE",
    "XDG_STATE_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "APPDATA",
    "LOCALAPPDATA",
];
/// A well-formed chat upload reference, which a build with an API opens.
const UPLOAD: &str = "UPL-AQEBAQEBAQEBAQEBAQEBAQ";

/// What a build with no API backend stamps: the backend and nothing else.
fn none_build() -> launch::Stamped {
    launch::Stamped {
        backend: "none",
        ..launch::Stamped::default()
    }
}

#[test]
fn a_build_without_a_backend_offers_no_sign_in_and_reaches_no_api() {
    let api = Mock::new(|_| (200, "[]".to_owned()));
    let fixture = tempfile::tempdir().unwrap();
    let token = fixture.path().join("stray-token");
    std::fs::write(&token, "stray-fixture-token\n").unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("SUBSPACE_") {
            child.env_remove(name);
        }
    }
    for name in STATE_VARIABLES {
        child.env(name, fixture.path());
    }
    let run = child
        .args(["--ignored", "--exact", "launched_without_a_backend"])
        .env(CHILD, "1")
        .env("SUBSPACE_API_URL", format!("{}/api", api.base_url()))
        .env("SUBSPACE_API_ACCESS_JWT_FILE", &token)
        .output()
        .unwrap();
    assert!(
        run.status.success() && String::from_utf8_lossy(&run.stdout).contains("1 passed"),
        "{} {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        api.requests().is_empty(),
        "a build with no API backend sent {} request(s)",
        api.requests().len()
    );
}

#[gpui_kit::test]
#[ignore = "a child process of a_build_without_a_backend_offers_no_sign_in_and_reaches_no_api"]
fn launched_without_a_backend(cx: &mut TestAppContext) {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let api = std::env::var("SUBSPACE_API_URL").unwrap();

    // Launched with the stray settings, the product refuses to start.
    let refused = launch::resolve(&none_build(), std::env::vars()).unwrap_err();
    assert!(
        refused.contains("Unset SUBSPACE_API_ACCESS_JWT_FILE, SUBSPACE_API_URL and start again"),
        "{refused}"
    );
    // A clean run resolves no provider, so the shell is given no account:
    // no Account page and no sign-in prompt.
    let resolved = launch::resolve(&none_build(), Vec::new()).unwrap();
    assert_eq!(resolved.backend, settings::ApiBackend::None);
    assert_eq!(resolved.provider, "");
    assert_eq!(resolved.api_url, "");
    assert_eq!(resolved.nett_api_url, "");
    launch::install(&resolved).unwrap();

    // Nothing can configure an API afterwards, and no client is built even
    // though the debug fixture variables are still in the environment.
    assert!(!subspace_api_client::has_backend());
    assert!(subspace_api_client::configure_api_origin(&api).is_err());
    assert!(subspace_api_client::configure_nett_origin(&api).is_err());
    assert!(Client::new().is_err());

    // Channels: no initiative picker, fleet context, captain notices or
    // uploads, and an upload reference stays the text that was sent.
    assert_eq!(subspace_api_client::capabilities(), Capabilities::NONE);
    let sent = format!("see {UPLOAD}");
    assert!(subspace_api_client::chat_uploads::references(&sent).is_empty());

    // The shared catalogue and the fleet service ask for nothing and find no
    // initiative and no fleet, so Fleet Console has only its own settings.
    let fleets = cx.update(FleetApi::shared);
    let catalog = cx.update(Catalog::shared);
    catalog.update(cx, |catalog, cx| catalog.refresh(cx));
    cx.run_until_parked();
    catalog.read_with(cx, |catalog, _| assert!(catalog.memberships().is_empty()));
    fleets.read_with(cx, |fleets, _| assert!(fleets.shared_fleets().is_empty()));
}
