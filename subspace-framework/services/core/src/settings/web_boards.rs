//! Checked immediate web-board edits and narrow rebasing of an open editor.

use super::{SettingsError, SettingsFile};
use crate::config::web_boards::valid_web_board_id;
use serde_json::{Map, Value};

impl SettingsFile {
    /// Raw native entry, including source provenance and unknown fields.
    pub fn web_board(&self, id: &str) -> Option<Map<String, Value>> {
        self.root
            .get("webBoards")?
            .as_object()?
            .get(id)?
            .as_object()
            .cloned()
    }

    /// Immediately writes one complete entry, or unregisters it when absent.
    /// The caller supplies the full raw entry to retain its existing fields.
    /// Board enablement, ordering, other entries and unrelated JSON are untouched.
    // qual:api
    pub fn update_web_board(
        &mut self,
        id: &str,
        entry: Option<Map<String, Value>>,
    ) -> Result<(), SettingsError> {
        if !valid_web_board_id(id) {
            return Err(SettingsError::Validation {
                field: "board id",
                reason: "use a short lower-case name starting with a letter",
            });
        }
        self.check_current()?;
        let mut root = self.root.clone();
        let boards = root
            .entry("webBoards")
            .or_insert_with(|| Value::Object(Map::new()));
        let boards = boards.as_object_mut().ok_or(SettingsError::Invalid)?;
        match entry {
            Some(entry) => {
                boards.insert(id.into(), Value::Object(entry));
            }
            None => {
                boards.remove(id);
            }
        }
        self.write_root(root)
    }

    /// Accepts immediate `webBoards` changes without accepting unrelated edits.
    /// The caller retains its draft inputs. A conflict leaves this snapshot intact.
    pub fn rebase_web_boards(&mut self) -> Result<(), SettingsError> {
        let current = Self::open(&self.file)?;
        validate_rebase(self, &current)?;
        *self = current;
        Ok(())
    }
}

fn validate_rebase(previous: &SettingsFile, current: &SettingsFile) -> Result<(), SettingsError> {
    let mut before = previous.root.clone();
    let mut after = current.root.clone();
    before.remove("webBoards");
    after.remove("webBoards");
    if before != after || (previous.original.is_some() && current.original.is_none()) {
        return Err(SettingsError::Stale);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::settings::{SettingsError, SettingsFile};
    use serde_json::{Map, Value, json};
    use std::fs;

    fn entry(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn immediate_edits_preserve_raw_fields_order_enablement_and_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{"webBoards":{"notes":{"url":"https://notes.example","settings":{"limit":7},"secrets":{"token":"test-only-not-working"},"sourceRecord":{"kind":"folder"}}},"boards":{"notes":false},"boardOrder":["notes"],"unknown":{"keep":true}}"#).unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        let mut notes = file.web_board("notes").unwrap();
        notes.insert("title".into(), json!("New notes"));
        file.update_web_board("notes", Some(notes)).unwrap();
        file.update_web_board("wiki", Some(entry(json!({"url":"https://wiki.example"}))))
            .unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["webBoards"]["notes"]["settings"]["limit"], 7);
        assert_eq!(
            saved["webBoards"]["notes"]["secrets"]["token"],
            "test-only-not-working"
        );
        assert_eq!(
            saved["webBoards"]["notes"]["sourceRecord"]["kind"],
            "folder"
        );
        assert_eq!(saved["boards"]["notes"], false);
        assert_eq!(saved["boardOrder"], json!(["notes"]));
        assert_eq!(saved["unknown"], json!({"keep":true}));
        file.update_web_board("notes", None).unwrap();
        assert!(file.web_board("notes").is_none());
        assert!(file.web_board("wiki").is_some());
    }

    #[test]
    fn invalid_and_stale_immediate_edits_do_not_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, "{}").unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        assert!(
            file.update_web_board("bad", Some(entry(json!({"url":"http://remote.example"}))))
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
        fs::write(&path, r#"{"external":true}"#).unwrap();
        assert!(matches!(
            file.update_web_board("notes", Some(entry(json!({"url":"https://notes.example"})))),
            Err(SettingsError::Stale)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"external":true}"#);
    }

    #[test]
    fn rebase_keeps_a_dirty_draft_and_save_or_cancel_cannot_undo_immediate_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{"repos":["owner/original"]}"#).unwrap();
        let mut editor = SettingsFile::open(&path).unwrap();
        let mut draft = editor.values().clone();
        draft.repos = vec!["owner/dirty".into()];
        let mut manager = SettingsFile::open(&path).unwrap();
        manager
            .update_web_board("notes", Some(entry(json!({"url":"https://notes.example"}))))
            .unwrap();
        editor.rebase_web_boards().unwrap();
        editor.save(&draft).unwrap();
        assert!(
            SettingsFile::open(&path)
                .unwrap()
                .web_board("notes")
                .is_some()
        );
        assert_eq!(
            SettingsFile::open(&path).unwrap().values().repos,
            ["owner/dirty"]
        );
        let mut cancel = SettingsFile::open(&path).unwrap();
        manager = SettingsFile::open(&path).unwrap();
        manager.update_web_board("notes", None).unwrap();
        cancel.rebase_web_boards().unwrap();
        drop(cancel);
        assert!(
            SettingsFile::open(&path)
                .unwrap()
                .web_board("notes")
                .is_none()
        );
    }

    #[test]
    fn rebase_refuses_unrelated_external_edits_without_accepting_the_new_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, "{}").unwrap();
        let mut editor = SettingsFile::open(&path).unwrap();
        fs::write(
            &path,
            r#"{"unknown":7,"webBoards":{"notes":{"url":"https://notes.example"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            editor.rebase_web_boards(),
            Err(SettingsError::Stale)
        ));
        assert!(editor.web_board("notes").is_none());
        assert!(matches!(
            editor.save(&editor.values().clone()),
            Err(SettingsError::Stale)
        ));
    }
}
