//! A NETT build reaches only NETT. The product resolves and installs its
//! launch as `main` does, from a NETT build's stamped settings, while the
//! process environment also holds the Subspace API debug variables a stray
//! shell would. Every construction path the Issues and Channels Spaces use
//! must then reach only NETT; a Subspace API sentinel records any request.

#[allow(dead_code)]
#[path = "../build/settings.rs"]
mod settings;

#[allow(dead_code)]
#[path = "../src/launch.rs"]
mod launch;

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

#[gpui_kit::test]
fn a_nett_build_reaches_only_nett(cx: &mut TestAppContext) {
    let subspace = Mock::new(|_| (200, "{}".to_owned()));
    // Installing remembers organisation choices beside the app's state, so
    // the state directory is a temporary one, never the person's.
    let home = tempfile::tempdir().unwrap();
    let token = home.path().join("stray-token");
    std::fs::write(&token, "stray-fixture-token\n").unwrap();
    // SAFETY: set before any other thread in this test process reads them.
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("XDG_STATE_HOME", home.path());
        std::env::set_var("SUBSPACE_API_URL", format!("{}/api", subspace.base_url()));
        std::env::set_var("SUBSPACE_API_ACCESS_JWT_FILE", &token);
    }
    let nett_mock = Mock::new(|request| nett(&request.path));
    let nett_url = format!("{}/api", nett_mock.base_url());

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
        [("SUBSPACE_NETT_API_URL".to_owned(), nett_url.clone())],
    )
    .unwrap();
    assert_eq!(resolved.api_url, "");
    launch::install(&resolved).unwrap();
    subspace_api_client::use_native_account(|| 1, |_| None, || Ok(Secret::new("nett-token")));
    assert_eq!(subspace_api_client::backend(), Backend::Nett);
    assert!(
        subspace_api_client::configure_api_origin(&format!("{}/api", subspace.base_url())).is_err(),
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
    assert!(first.api_origin().starts_with(nett_mock.base_url()));
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

    // The initiative catalogue Channels and Issues share reads NETT too.
    let catalog = cx.update(Catalog::shared);
    catalog.update(cx, |catalog, cx| catalog.refresh(cx));
    cx.run_until_parked();
    catalog.read_with(cx, |catalog, _| {
        assert_eq!(catalog.hint(), None, "the catalogue read completed");
        assert!(catalog.memberships().is_empty());
    });

    assert!(
        subspace.requests().is_empty(),
        "a NETT build sent {} request(s) to Subspace API",
        subspace.requests().len()
    );
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
