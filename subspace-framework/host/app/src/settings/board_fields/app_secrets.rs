//! Secrets a board's app saved itself, beside its declared fields: Settings
//! counts them and can clear them on Save, and never reads their values.

use gpui_kit::Context;
use subspace_core::settings::{SettingsFile, TokenEdit};
use subspace_dashboard::settings::BoardFields;

use super::Field;
use crate::settings::Settings;

/// The keys of the secrets an app saved beside its declared fields, and
/// whether Save clears them. Values never leave the settings file.
pub(in crate::settings) struct AppSecrets {
    pub keys: Vec<String>,
    pub clear: bool,
}

/// The read-only line that counts an app's own secrets.
pub(in crate::settings) fn app_secrets_text(count: usize) -> String {
    match count {
        1 => "This app has saved 1 secret".to_owned(),
        count => format!("This app has saved {count} secrets"),
    }
}

impl AppSecrets {
    /// The saved secrets of `fields`' section that none of them declares.
    pub(in crate::settings) fn of(fields: &BoardFields, file: &SettingsFile) -> Self {
        let declared: Vec<&String> = fields.fields.iter().map(|spec| &spec.key).collect();
        Self::undeclared(file, &fields.section, &declared)
    }

    /// The same after a save, from the drafted fields.
    pub(super) fn load(file: &SettingsFile, section: &[String], fields: &[Field]) -> Self {
        let declared: Vec<&String> = fields.iter().map(|field| &field.spec.key).collect();
        Self::undeclared(file, section, &declared)
    }

    fn undeclared(file: &SettingsFile, section: &[String], declared: &[&String]) -> Self {
        let keys = file
            .board_secret_keys(section)
            .into_iter()
            .filter(|key| !declared.contains(&key))
            .collect();
        Self { keys, clear: false }
    }

    /// Clear each of them on Save, once Clear is chosen.
    pub(in crate::settings) fn edits(&self) -> impl Iterator<Item = (String, TokenEdit)> + '_ {
        self.keys
            .iter()
            .filter(|_| self.clear)
            .map(|key| (key.clone(), TokenEdit::Clear))
    }
}

impl Settings {
    /// Clear the secrets the board's app saved, or undo that choice.
    pub(in crate::settings) fn toggle_app_secrets_clear(
        &mut self,
        board: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(app) = self
            .board_fields
            .get_mut(board)
            .and_then(|draft| draft.app_secrets.as_mut())
        {
            app.clear = !app.clear;
        }
        cx.notify();
    }
}
