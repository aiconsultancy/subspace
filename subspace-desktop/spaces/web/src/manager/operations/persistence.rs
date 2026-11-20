use gpui_kit::{Context, Window};
use subspace_framework::core::settings::SettingsFile;

use super::super::model::{bind_page_secrets, install_entry, named_entry};
use super::{Manager, config_path};

impl Manager {
    pub(in crate::manager) fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_account(cx);
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        if !self.epoch.accepts(pending.ticket) {
            return;
        }
        let mut previous = pending.file.web_board(&pending.id).unwrap_or_default();
        let source = pending.prepared.source();
        bind_page_secrets(&mut previous, &source);
        let result = serde_json::to_value(pending.prepared.metadata())
            .map_err(|error| error.to_string())
            .and_then(|metadata| {
                let entry = named_entry(install_entry(previous, &source, metadata), &pending.title);
                pending
                    .file
                    .update_web_board(&pending.id, Some(entry))
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => {
                pending.prepared.retain();
                self.after_save(window, cx);
            }
            Err(error) => self.fail(error, cx),
        }
    }

    pub(in crate::manager) fn remove_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.remove.take() else { return };
        self.epoch.invalidate();
        self.pending = None;
        self.offer = None;
        let result = config_path(cx).and_then(|path| {
            let mut file = SettingsFile::open(&path).map_err(|error| error.to_string())?;
            file.update_web_board(&id, None)
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => self.after_save(window, cx),
            Err(error) => self.fail(error, cx),
        }
    }
}
