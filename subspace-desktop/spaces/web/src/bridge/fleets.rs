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

/// The fleets as the page receives them.
pub(crate) fn current(cx: &mut App) -> Value {
    let service = FleetApi::shared(cx);
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

    use serde_json::{Map, Value, json};
    use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

    use super::shares_fleets;

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
