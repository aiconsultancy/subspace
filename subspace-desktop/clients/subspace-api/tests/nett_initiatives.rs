//! The shared initiative catalogue across a NETT organisation switch: what
//! organisation A serves and lists goes, and organisation B, which does not
//! serve initiative channels, is read afresh with its own capability list.
//! Its own process, because the backend and the account are chosen once.

use gpui_kit::TestAppContext;
use serde_json::{Value, json};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::{Client, nett};
use subspace_framework::core::Secret;

#[path = "../src/tests/nett/process.rs"]
mod process;
use process::NettProcess as Mock;

/// Fixture traffic stays on loopback: no proxy from the environment sees it.
fn direct() -> ureq::Agent {
    ureq::Agent::config_builder().proxy(None).build().into()
}

impl Mock {
    fn hook(&self, path: &str, body: &Value) {
        direct()
            .post(&format!("{}/__fixture/{path}", self.origin))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .unwrap();
    }
}

fn organisation(name: &str) -> String {
    nett::snapshot()
        .organisations
        .into_iter()
        .find(|org| org.name == name)
        .unwrap()
        .id
}

#[gpui_kit::test]
fn the_catalogue_follows_an_organisation_switch_and_what_b_serves(cx: &mut TestAppContext) {
    let mock = Mock::start(&["initiativeChannels"], &[]);
    subspace_api_client::configure_nett_origin(&format!("{}/api", mock.origin)).unwrap();
    subspace_api_client::use_native_account(
        || 1,
        |_| None,
        || Ok(Secret::new("nett-fixture-access")),
    );
    Client::new().unwrap().me().unwrap();
    let (works, second) = (
        organisation("Fixture Works"),
        organisation("Second Fixture Org"),
    );
    mock.hook("capabilities", &json!({"org": second, "features": []}));

    // Organisation A serves initiative channels: its initiatives carry them.
    assert!(nett::select_organisation(&works));
    let catalog = cx.update(Catalog::shared);
    catalog.update(cx, |catalog, cx| catalog.refresh(cx));
    cx.run_until_parked();
    catalog.read_with(cx, |catalog, _| {
        let platform = catalog
            .memberships()
            .iter()
            .find(|i| i.name == "Platform")
            .expect("A's initiatives are listed");
        assert_eq!(platform.channels, "#platform");
    });
    assert!(subspace_api_client::capabilities().initiative_channels);
    catalog.update(cx, |catalog, cx| {
        let id = catalog
            .memberships()
            .iter()
            .find(|i| i.name == "Platform")
            .unwrap()
            .id
            .clone();
        catalog.pick(&id, cx);
    });

    // Organisation B does not: nothing of A's catalogue or selection
    // survives, and B is read with its own capability list.
    assert!(nett::select_organisation(&second));
    catalog.update(cx, |catalog, cx| catalog.refresh(cx));
    cx.run_until_parked();
    assert!(!subspace_api_client::capabilities().initiative_channels);
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.selected().is_none());
        assert!(catalog.memberships().iter().all(|i| i.name != "Platform"));
        // B's own initiative is listed, as B serves it: without channels.
        let office = catalog
            .memberships()
            .iter()
            .find(|i| i.name == "Office")
            .expect("B's initiatives are listed");
        assert!(office.channels.is_empty() && office.fleet.is_none());
        assert!(
            catalog
                .memberships()
                .iter()
                .all(|i| i.channels.is_empty() && i.fleet.is_none())
        );
    });
}
