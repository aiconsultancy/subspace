//! Match project_export's user-chosen, atomic, no-clobber file boundary.
use crate::project_view::ProjectView;
use gpui_kit::{AppContext as _, Context};
#[cfg(unix)]
use std::fs::File;
use std::{
    io::{self, Write as _},
    path::Path,
};
use subspace_framework::dashboard::native_grid::{GridExport, GridFormat};

impl ProjectView {
    pub(super) fn export_grid(&mut self, id: &str, event: &GridExport, cx: &mut Context<Self>) {
        let extension = match event.format() {
            GridFormat::Csv => "csv",
            GridFormat::Json => "json",
        };
        let text = match event.snapshot().encode(event.format()) {
            Ok(text) => text,
            Err(error) => {
                self.export_feedback(id, format!("Cannot encode export: {error}"), cx);
                return;
            }
        };
        let name = format!("table-{id}.{extension}");
        let picker =
            cx.prompt_for_new_path(&std::env::current_dir().unwrap_or_default(), Some(&name));
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picker.await else {
                return;
            };
            let written = cx
                .background_spawn(async move { write_new(&path, text.as_bytes()) })
                .await;
            _ = this.update(cx, |this, cx| {
                let message = match written {
                    Ok(()) => "Export saved".to_owned(),
                    Err(error) => format!("Cannot save export: {error}"),
                };
                this.export_feedback(&id, message, cx);
            });
        })
        .detach();
    }

    fn export_feedback(&mut self, id: &str, message: String, cx: &mut Context<Self>) {
        if let Some(grid) = self.grids.get_mut(id) {
            grid.feedback = message.into();
            cx.notify();
        }
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_new;

    #[test]
    fn export_never_overwrites_a_user_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.csv");
        write_new(&path, b"first").unwrap();
        assert!(write_new(&path, b"second").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
