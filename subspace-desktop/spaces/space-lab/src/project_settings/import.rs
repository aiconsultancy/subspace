use std::io::Read as _;
use std::path::Path;

use gpui_kit::{AppContext as _, Context, Window};

use super::{Choice, ProjectSettings, SettingsEvent, all_approved};
use crate::dotenv::{self, MAX_INPUT_BYTES};
use crate::private_settings::{ImportPreview, PrivateSettings};

fn preview_file(root: &Path, id: &str, path: &Path) -> anyhow::Result<ImportPreview> {
    anyhow::ensure!(
        std::fs::symlink_metadata(path)?.is_file(),
        "import requires a regular file"
    );
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    anyhow::ensure!(metadata.is_file(), "import requires a regular file");
    anyhow::ensure!(
        metadata.len() <= MAX_INPUT_BYTES as u64,
        "import file too large"
    );
    let mut source = String::new();
    file.take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_string(&mut source)?;
    dotenv::parse_reader(source.as_bytes())?;
    PrivateSettings::new(root, id)?.preview_import(&source)
}

impl ProjectSettings {
    pub(super) fn preview_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.metadata.is_none() {
            return;
        }
        let path = self.path.read(cx).value().to_string();
        let root = self.root.clone();
        let id = self.id.clone();
        self.busy = true;
        self.error = None;
        self.preview = None;
        self.approved.clear();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { preview_file(&root, &id, Path::new(&path)) })
                .await;
            _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(preview) => this.preview = Some(preview),
                    Err(_) => {
                        this.error =
                            Some("Could not preview the file. Check the path and dotenv format.")
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn apply_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(metadata) = &self.metadata else {
            return;
        };
        let Some(preview) = &self.preview else {
            return;
        };
        if self.busy
            || !all_approved(preview.conflicts(), &self.approved)
            || self.rows.values().any(|row| row.choice != Choice::Keep)
        {
            return;
        }
        let revision = metadata.revision();
        let Some(preview) = self.preview.take() else {
            return;
        };
        let approved = self.approved.iter().cloned().collect::<Vec<_>>();
        let root = self.root.clone();
        let id = self.id.clone();
        self.busy = true;
        self.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let approved = approved.iter().map(String::as_str).collect::<Vec<_>>();
                    PrivateSettings::new(&root, &id)?.apply_import(revision, preview, &approved)
                })
                .await;
            _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(metadata) => {
                        this.accept_metadata(metadata, window, cx);
                        cx.emit(SettingsEvent::Saved);
                    }
                    Err(_) => {
                        this.error =
                            Some("Could not import. Reload settings and preview the file again.")
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
