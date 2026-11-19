//! The open chat tabs of one account, saved beside `state.json` so a restart
//! reopens the conversations Zack had open.

use serde::{Deserialize, Serialize};

use super::{Path, SessionId, fs, parse_session_id, state};

/// The ordered sessions of the open tabs, and the active one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SavedTabs {
    #[serde(default)]
    pub(super) sessions: Vec<String>,
    #[serde(default)]
    pub(super) active: Option<String>,
}

impl SavedTabs {
    /// The saved sessions Subspace will act on, in order, without duplicates.
    pub(super) fn session_ids(&self) -> Vec<SessionId> {
        let mut ids: Vec<SessionId> = Vec::new();
        for id in &self.sessions {
            let Ok(parsed) = parse_session_id(id) else {
                continue;
            };
            if ids.iter().any(|open| open.as_str() == parsed.as_str()) {
                continue;
            }
            ids.push(parsed);
        }
        ids
    }

    /// Where the active session sits in `sessions`, defaulting to the first.
    pub(super) fn active_index(&self) -> usize {
        let ids = self.session_ids();
        self.active
            .as_ref()
            .and_then(|active| ids.iter().position(|id| id.as_str() == active))
            .unwrap_or(0)
    }
}

pub(super) fn tabs_file_name(login: &str) -> String {
    format!("copilot-tabs-{}.json", login.to_ascii_lowercase())
}

pub(super) fn read_tabs(path: &Path) -> SavedTabs {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(super) fn save_tabs(path: &Path, tabs: &SavedTabs) -> Result<(), String> {
    let text = serde_json::to_string(tabs)
        .map_err(|_| "Cannot encode the open Copilot chats.".to_string())?;
    state::write_atomic(path, &text).map_err(|_| "Cannot save the open Copilot chats.".to_string())
}

/// Before tabs, one file held the single open chat. It becomes the one saved
/// tab, and is then removed so the migration runs once.
pub(super) fn migrate_single_chat(tabs: SavedTabs, previous: &Path) -> SavedTabs {
    if !tabs.sessions.is_empty() {
        return tabs;
    }
    let saved = fs::read_to_string(previous)
        .ok()
        .and_then(|text| serde_json::from_str::<String>(&text).ok());
    let _ = fs::remove_file(previous);
    match saved {
        Some(id) => SavedTabs {
            sessions: vec![id.clone()],
            active: Some(id),
        },
        None => tabs,
    }
}

#[cfg(test)]
mod tests {
    use super::{SavedTabs, migrate_single_chat, read_tabs, save_tabs, tabs_file_name};
    use std::fs;

    #[test]
    fn each_account_keeps_its_own_open_tabs() {
        assert_eq!(tabs_file_name("Work_Shell"), "copilot-tabs-work_shell.json");
    }

    #[test]
    fn only_well_formed_session_ids_are_reopened_and_each_only_once() {
        let tabs = SavedTabs {
            sessions: vec![
                "abc-123".into(),
                "abc-123".into(),
                "../other".into(),
                "def-456".into(),
            ],
            active: Some("def-456".into()),
        };
        let ids: Vec<String> = tabs
            .session_ids()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        assert_eq!(ids, ["abc-123", "def-456"]);
        assert_eq!(tabs.active_index(), 1);
    }

    #[test]
    fn an_unknown_active_tab_falls_back_to_the_first() {
        let tabs = SavedTabs {
            sessions: vec!["abc-123".into()],
            active: Some("gone".into()),
        };
        assert_eq!(tabs.active_index(), 0);
        assert_eq!(SavedTabs::default().active_index(), 0);
    }

    #[test]
    fn saved_tabs_round_trip_and_damaged_files_start_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("copilot-tabs-work.json");
        assert_eq!(read_tabs(&file), SavedTabs::default());
        let tabs = SavedTabs {
            sessions: vec!["abc-123".into()],
            active: Some("abc-123".into()),
        };
        save_tabs(&file, &tabs).unwrap();
        assert_eq!(read_tabs(&file), tabs);
        fs::write(&file, "not json").unwrap();
        assert_eq!(read_tabs(&file), SavedTabs::default());
    }

    #[test]
    fn the_previous_single_chat_becomes_the_one_tab_and_is_migrated_once() {
        let dir = tempfile::tempdir().unwrap();
        let previous = dir.path().join("copilot-session-work.json");
        fs::write(&previous, "\"abc-123\"").unwrap();
        let migrated = migrate_single_chat(SavedTabs::default(), &previous);
        assert_eq!(migrated.sessions, ["abc-123"]);
        assert_eq!(migrated.active.as_deref(), Some("abc-123"));
        assert!(!previous.exists());
        assert_eq!(
            migrate_single_chat(SavedTabs::default(), &previous),
            SavedTabs::default()
        );
    }

    #[test]
    fn saved_tabs_are_not_replaced_by_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let previous = dir.path().join("copilot-session-work.json");
        fs::write(&previous, "\"old\"").unwrap();
        let tabs = SavedTabs {
            sessions: vec!["abc-123".into()],
            active: None,
        };
        assert_eq!(migrate_single_chat(tabs.clone(), &previous), tabs);
        assert!(previous.exists(), "nothing to migrate, nothing removed");
    }
}
