//! A NETT build reaches only NETT. The product resolves and installs a
//! launch from stamped NETT settings in one child process. Another child
//! exercises the Issues and Channels construction paths with account
//! persistence off. Both inherit the Subspace API debug settings a
//! stray shell would have, with a sentinel to record any requests, and
//! both keep their state under a temporary home, never the person's.

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
use std::time::{Duration, Instant};
use subspace_api_client::chat_uploads::UploadError;
use subspace_api_client::hub::{Hub, HubStatus};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::issue_writes::IssueSaveError;
use subspace_api_client::{Backend, Client};
use subspace_framework::client_test_support::Mock;
use subspace_framework::core::Secret;

const ORG: &str = "00000000-0000-0000-0000-0000000000a1";
const CLIENT: &str = "client_00000000000000000000000000";
const CHILD: &str = "NETT_BUILD_TEST_CHILD";
/// The temporary home a child keeps its state under.
const HOME: &str = "NETT_BUILD_TEST_HOME";
/// Where each platform keeps a person's state, configuration and data.
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

fn nett(path: &str) -> (u16, String) {
    let path = path.split('?').next().unwrap_or_default();
    let team = r#"{"id":"00000000-0000-0000-0000-0000000000b1","key":"OPS","name":"Operations","defaultStateId":"00000000-0000-0000-0000-0000000000c1","archivedAt":null,"states":[{"id":"00000000-0000-0000-0000-0000000000c1","teamId":"00000000-0000-0000-0000-0000000000b1","name":"Todo","type":"unstarted","position":1,"isDefault":true}]}"#;
    match path {
        "/api/v1/auth/context" => (
            200,
            format!(
                r#"{{"userId":"00000000-0000-0000-0000-0000000000d1","user":{{"firstName":"Ada","lastName":"Fixture"}},"organisations":[{{"id":"{ORG}","name":"Fixture Works","isPersonalOrg":false}}]}}"#
            ),
        ),
        p if p == format!("/api/v1/orgs/{ORG}/issue-teams") => {
            (200, format!(r#"{{"items":[{team}],"nextCursor":null}}"#))
        }
        p if p == format!("/api/v1/orgs/{ORG}/issue-teams/OPS") => (200, team.to_owned()),
        p if p.starts_with(&format!("/api/v1/orgs/{ORG}/")) => {
            (200, r#"{"items":[],"nextCursor":null}"#.to_owned())
        }
        _ => (404, "{}".to_owned()),
    }
}

/// A NETT build's stamped settings: WorkOS sign-in with a placeholder
/// client, and the NETT API URL left to the run.
fn nett_build() -> launch::Stamped {
    launch::Stamped {
        backend: "nett",
        provider: "workos",
        workos_client_id: CLIENT,
        ..launch::Stamped::default()
    }
}

#[test]
fn a_nett_build_reaches_only_nett() {
    let subspace = Mock::new(|_| (200, "{}".to_owned()));
    let fixture = tempfile::tempdir().unwrap();
    let token = fixture.path().join("stray-token");
    std::fs::write(&token, "stray-fixture-token\n").unwrap();
    let subspace_url = format!("{}/api", subspace.base_url());
    for phase in ["install", "exercise"] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("SUBSPACE_") {
                child.env_remove(name);
            }
        }
        // Installing turns on organisation memory beside the app's state:
        // every place a platform keeps state is a temporary one.
        let home = fixture.path().join(phase);
        std::fs::create_dir(&home).unwrap();
        for name in STATE_VARIABLES {
            child.env(name, &home);
        }
        child.env(HOME, &home);
        let run = child
            .args([
                "--ignored",
                "--exact",
                "launched_with_stray_subspace_settings",
            ])
            .env(CHILD, phase)
            .env("SUBSPACE_API_URL", &subspace_url)
            .env("SUBSPACE_API_ACCESS_JWT_FILE", &token)
            .output()
            .unwrap();
        assert!(
            run.status.success() && String::from_utf8_lossy(&run.stdout).contains("1 passed"),
            "{phase}: {} {}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
    }
    assert!(
        subspace.requests().is_empty(),
        "a NETT build sent {} request(s) to Subspace API",
        subspace.requests().len()
    );
}

#[gpui_kit::test]
#[ignore = "a child process of a_nett_build_reaches_only_nett, with its environment"]
fn launched_with_stray_subspace_settings(cx: &mut TestAppContext) {
    let Ok(phase) = std::env::var(CHILD) else {
        return;
    };
    let subspace_api = std::env::var("SUBSPACE_API_URL").unwrap();
    let nett_mock = Mock::new(|request| nett(&request.path));
    let nett_url = format!("{}/api", nett_mock.base_url());

    assert_state_isolated();
    let resolved = resolve_nett_launch(&nett_url);
    if phase == "install" {
        launch::install(&resolved).unwrap();
        assert_eq!(subspace_api_client::backend(), Backend::Nett);
        subspace_api_client::configure_nett_origin(&nett_url).unwrap();
        return;
    }
    assert_eq!(phase, "exercise");
    assert_nett_account_and_features(&resolved, &subspace_api, nett_mock.base_url());
    assert_issues_read_nett();
    assert_catalogue_reads_nett(cx);
    assert_nett_requests_are_scoped(&nett_mock);
}

/// The app's state, organisation memory included, resolves under the
/// child's temporary home. Windows reads its state folder from the
/// account's known folders, which no variable moves; there the children
/// still select no organisation, so nothing is written.
fn assert_state_isolated() {
    let home = std::env::var_os(HOME).expect("the child's temporary home");
    if cfg!(windows) {
        return;
    }
    let state = subspace_framework::core::state::path().expect("a state directory");
    assert!(
        state.starts_with(&home),
        "state resolves under the temporary home, not {}",
        state.display()
    );
}

fn resolve_nett_launch(nett_url: &str) -> launch::Launch {
    // Launched with that environment, the product refuses to start and
    // installs nothing.
    let refused = launch::resolve(&nett_build(), std::env::vars()).unwrap_err();
    assert!(
        refused.contains("SUBSPACE_API_ACCESS_JWT_FILE, SUBSPACE_API_URL"),
        "{refused}"
    );
    assert_eq!(subspace_api_client::backend(), Backend::Subspace);

    // A clean run resolves and installs only the NETT origin, as `main`
    // does; the stray variables stay in the process for the libraries.
    let resolved = launch::resolve(
        &nett_build(),
        [("SUBSPACE_NETT_API_URL".to_owned(), nett_url.to_owned())],
    )
    .unwrap();
    assert_eq!(resolved.api_url, "");
    assert_eq!(resolved.nett_api_url, nett_url);
    resolved
}

fn assert_nett_account_and_features(resolved: &launch::Launch, subspace_api: &str, origin: &str) {
    // The installer enables on-disk organisation choices. Use the same
    // resolved origin here and keep the account journey in memory on every OS.
    subspace_api_client::configure_nett_origin(&resolved.nett_api_url).unwrap();
    subspace_api_client::use_native_account(|| 1, |_| None, || Ok(Secret::new("nett-token")));
    assert_eq!(subspace_api_client::backend(), Backend::Nett);
    assert!(
        subspace_api_client::configure_api_origin(subspace_api).is_err(),
        "the Subspace API origin can never be configured in a NETT build"
    );
    let capabilities = subspace_api_client::capabilities();
    assert!(
        !capabilities.chat_uploads
            && !capabilities.initiative_channels
            && !capabilities.captain_notices,
        "Channels keeps uploads, the initiative picker and captain notices off"
    );
    assert_eq!(subspace_api_client::service_name(), "NETT");

    // The first client has no organisation; reading the caller selects the
    // only one, which retires that client.
    let first = Client::new().unwrap();
    assert_eq!(first.service(), Backend::Nett);
    assert!(first.api_origin().starts_with(origin));
    assert_eq!(first.me().unwrap().display_name, "Ada Fixture");
    assert!(first.account_changed());
    let client = Client::new().unwrap();
    assert_eq!(client.service(), Backend::Nett);
    assert_eq!(client.organisation(), Some(ORG));
    assert!(!client.account_changed());

    // Features NETT has not advertised refuse before sending anything.
    assert_eq!(
        client.chat_upload("UPL-AAAAAAAAAAAAAAAAAAAAAA").err(),
        Some(UploadError::Unavailable)
    );
    assert_eq!(
        client.preview_issue_move("x", "y", 1).err(),
        Some(IssueSaveError::Unavailable)
    );
    assert!(client.fleet("fleet").is_err());
}

fn assert_issues_read_nett() {
    // The hub the Issues Space starts reads NETT.
    let hub = Hub::start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while hub.snapshot().projects().is_empty() {
        assert!(Instant::now() < deadline, "{:?}", hub.snapshot().status());
        assert_ne!(hub.snapshot().status(), &HubStatus::Stopped);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(hub.snapshot().projects()[0].key, "OPS");
    hub.stop();
}

fn assert_catalogue_reads_nett(cx: &mut TestAppContext) {
    // The initiative catalogue Channels and Issues share reads NETT too.
    let catalog = cx.update(Catalog::shared);
    catalog.update(cx, |catalog, cx| catalog.refresh(cx));
    cx.run_until_parked();
    catalog.read_with(cx, |catalog, _| {
        assert_eq!(catalog.hint(), None, "the catalogue read completed");
        assert!(catalog.memberships().is_empty());
    });
}

fn assert_nett_requests_are_scoped(nett_mock: &Mock) {
    let nett_requests = nett_mock.requests();
    assert!(nett_requests.iter().all(|request| {
        request
            .headers
            .contains(&("authorization".into(), "Bearer nett-token".into()))
    }));
    assert!(nett_requests.iter().any(|request| {
        request.path.starts_with(&format!("/api/v1/orgs/{ORG}/"))
            && request.headers.contains(&("x-org-id".into(), ORG.into()))
    }));
    assert!(
        nett_requests
            .iter()
            .any(|request| request.path.contains("initiatives")),
        "the catalogue asked NETT for initiatives"
    );
}
