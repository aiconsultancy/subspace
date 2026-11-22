//! `fleets.get`: the fleets the Subspace API assigns to the person's
//! initiatives, with the address it records for each and, only for that
//! address, the token saved in **Settings › Fleets**
//! (`subspace_client_fleet::FleetApi`). Only the app's own built-in Fleet
//! Console, under its board ID and marked `builtIn`, may ask, and only with
//! its `secrets` grant: no other web Space sees the fleets or their tokens.

use gpui_kit::App;
use serde_json::{Map, Value, json};
use subspace_client_fleet::{FleetApi, SharedFleet};
use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

pub(crate) const NOT_SHARED: &str =
    "Only the built-in Fleet Console reads the fleets of your initiatives.";

/// Whether board `board`, with raw config entry `entry`, is the app's
/// built-in Fleet Console.
pub(crate) fn shares_fleets(board: &WebBoard, entry: Option<&Map<String, Value>>) -> bool {
    board.id == crate::builtin::FLEET_CONSOLE_ID
        && matches!(board.source, WebBoardSource::Folder(_))
        && entry.is_some_and(crate::builtin::is_built_in)
}

/// The fleets as the page receives them. Only the page's Retry
/// (`fleets.retry`, `retry`) reads again a token the keychain could not
/// read, so the person asked for any unlock prompt; the page hears
/// `subspace:fleets-changed` if the read finds something new. A plain
/// `fleets.get`, as the page sends on every `subspace:fleets-changed`,
/// never reads the keychain.
pub(crate) fn current(retry: bool, cx: &mut App) -> Value {
    let service = FleetApi::shared(cx);
    if retry {
        service.update(cx, |service, cx| service.reload_unreadable(cx));
    }
    let fleets = service.read(cx).shared_fleets();
    Value::Array(fleets.iter().map(fleet_json).collect())
}

/// One fleet: `{id, name, endpoint, token, problem}`, with `token` only
/// when it was saved for `endpoint`.
pub(crate) fn fleet_json(fleet: &SharedFleet) -> Value {
    json!({
        "id": fleet.id,
        "name": fleet.name,
        "endpoint": fleet.endpoint,
        "token": fleet.token.as_ref().map(|token| token.expose()),
        "problem": fleet.problem,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gpui_kit::{AppContext as _, Entity, TestAppContext};
    use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore};
    use subspace_framework::core::Secret;
    use subspace_framework::core::vault::{Vault, VaultError};

    use serde_json::{Map, Value, json};
    use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

    use super::{current, shares_fleets};

    /// A keychain that stays locked, counting every read.
    struct Locked(Arc<AtomicUsize>);

    impl Vault for Locked {
        fn load(&self, _: &str) -> Result<Option<Secret>, VaultError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(VaultError)
        }

        fn save(&self, _: &str, _: &Secret) -> Result<(), VaultError> {
            Err(VaultError)
        }

        fn delete(&self, _: &str) -> Result<(), VaultError> {
            Err(VaultError)
        }
    }

    /// The installed fleet service over a locked keychain, and its reads.
    fn locked(cx: &mut TestAppContext) -> (Entity<FleetApi>, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let store = TokenStore::with_vault(Box::new(Locked(reads.clone())));
        let service = cx.new(|_| FleetApi::new(Arc::new(store)));
        service.update(cx, |service, cx| {
            let record =
                FleetRecord::new("aurora", "Aurora", Some("https://fleet.example.com".into()));
            service.set_fleets(vec![record], cx);
        });
        cx.run_until_parked();
        cx.update(|cx| FleetApi::install(service.clone(), cx));
        (service, reads)
    }

    #[gpui_kit::test]
    fn only_a_retry_reads_the_locked_keychain_again_and_nothing_loops(cx: &mut TestAppContext) {
        let (service, reads) = locked(cx);
        assert_eq!(reads.load(Ordering::SeqCst), 1, "the first read");
        let revision = service.read_with(cx, |service, _| service.directory_revision());
        // The page answers every `subspace:fleets-changed` with `fleets.get`,
        // as `attach_fleets` pushes it on each directory revision.
        let asks = Arc::new(AtomicUsize::new(0));
        let _page = cx.update(|cx| {
            let (asks, seen) = (asks.clone(), std::cell::Cell::new(revision));
            cx.observe(&service, move |service, cx| {
                let now = service.read(cx).directory_revision();
                if seen.replace(now) != now && asks.fetch_add(1, Ordering::SeqCst) < 10 {
                    current(false, cx);
                }
            })
        });
        for _ in 0..3 {
            cx.update(|cx| current(false, cx));
            cx.run_until_parked();
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1, "fleets.get never reads");
        cx.update(|cx| current(true, cx));
        cx.run_until_parked();
        assert_eq!(reads.load(Ordering::SeqCst), 2, "the retry reads once");
        assert_eq!(
            asks.load(Ordering::SeqCst),
            0,
            "still locked: nothing changed"
        );
        assert_eq!(
            service.read_with(cx, |service, _| service.directory_revision()),
            revision
        );
        // A second retry at once reads nothing: the service waits a few
        // seconds between reads of one fleet's token.
        cx.update(|cx| current(true, cx));
        cx.run_until_parked();
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    fn board(id: &str, source: WebBoardSource) -> WebBoard {
        WebBoard {
            id: id.to_owned(),
            title: "Fleet Console".to_owned(),
            source,
            settings: Default::default(),
            secrets: Default::default(),
            permissions: Default::default(),
        }
    }

    fn entry(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn only_the_built_in_fleet_console_shares_the_fleets() {
        let folder = WebBoardSource::Folder(PathBuf::from("/apps/fleet-console"));
        let built_in = entry(json!({"path": "/apps/fleet-console", "builtIn": true}));
        assert!(shares_fleets(
            &board("fleet-console", folder.clone()),
            Some(&built_in)
        ));
        // Another Space, even a built-in one, does not.
        assert!(!shares_fleets(
            &board("notes", folder.clone()),
            Some(&built_in)
        ));
        // A Fleet Console the person installed under the same ID does not.
        let installed = entry(json!({"path": "/apps/fleet-console", "builtIn": true,
            "managedSource": {"kind": "folder"}}));
        assert!(!shares_fleets(
            &board("fleet-console", folder.clone()),
            Some(&installed)
        ));
        let plain = entry(json!({"path": "/apps/fleet-console"}));
        assert!(!shares_fleets(
            &board("fleet-console", folder.clone()),
            Some(&plain)
        ));
        assert!(!shares_fleets(&board("fleet-console", folder), None));
        let website = WebBoardSource::Url("https://fleet.example.com".into());
        assert!(!shares_fleets(
            &board("fleet-console", website),
            Some(&built_in)
        ));
    }
}
