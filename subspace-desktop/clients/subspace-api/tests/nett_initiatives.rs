//! The shared initiative catalogue across a NETT organisation switch: what
//! organisation A serves and lists goes, and organisation B, which does not
//! serve initiative channels, is read afresh with its own capability list.
//! Its own process, because the backend and the account are chosen once.

use gpui_kit::TestAppContext;
use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::{Client, nett};
use subspace_framework::core::Secret;

struct Mock {
    child: Child,
    origin: String,
}

impl Mock {
    fn start() -> Self {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/nett_api_test_server.py");
        let mut child = Command::new(
            subspace_framework::core::path::find_tool("python3")
                .expect("python3 runs the NETT mock"),
        )
        .arg("-I")
        .arg(&script)
        .args(["--port", "0", "--serve", "initiativeChannels"])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("python3 runs the NETT mock");
        let mut line = String::new();
        if let Some(stdout) = child.stdout.take() {
            let _ = BufReader::new(stdout).read_line(&mut line);
        }
        let origin = line
            .trim()
            .strip_prefix("NETT fixture listening on ")
            .map(str::to_owned)
            .expect("the NETT mock started");
        Self { child, origin }
    }

    fn hook(&self, path: &str, body: &Value) {
        ureq::post(&format!("{}/__fixture/{path}", self.origin))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .unwrap();
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
    let mock = Mock::start();
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
        assert!(
            catalog
                .memberships()
                .iter()
                .all(|i| i.channels.is_empty() && i.fleet.is_none())
        );
    });
}
