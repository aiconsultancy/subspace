//! Show a local file in the system file manager: Finder, File Explorer or
//! the desktop's file browser.

use std::path::{Path, PathBuf};

use gpui_kit::{App, ElementId};
use subspace_lucide::IconName;

use crate::action::{Intent, action_button};
use crate::icon_button::IconButton;

/// The platform's name for the command.
pub const fn label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in File Explorer"
    } else {
        "Show in Folder"
    }
}

/// What to show for `path`: the file itself, else the nearest folder above
/// it that exists. The file manager cannot select a missing file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(PathBuf),
    Folder(PathBuf),
}

pub fn target(path: &Path, exists: impl Fn(&Path) -> bool) -> Option<Target> {
    if exists(path) {
        return Some(Target::File(path.to_owned()));
    }
    path.ancestors()
        .skip(1)
        .find(|folder| !folder.as_os_str().is_empty() && exists(folder))
        .map(|folder| Target::Folder(folder.to_owned()))
}

/// Selects `path` in the file manager, or opens its folder when the file
/// does not exist yet.
pub fn reveal(path: &Path, cx: &App) {
    match target(path, Path::exists) {
        Some(Target::File(file)) => cx.reveal_path(&file),
        Some(Target::Folder(folder)) => cx.open_with_system(&folder),
        None => {}
    }
}

/// A folder-glyph action that reveals `path`, labelled for the platform.
pub fn button(id: impl Into<ElementId>, path: PathBuf) -> IconButton {
    action_button(id, Intent::Neutral, Some(IconName::FolderOpen), label())
        .on_click(move |_, _, cx| reveal(&path, cx))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Target, label, target};

    #[test]
    fn a_missing_file_reveals_its_nearest_existing_folder() {
        let file = Path::new("/home/fixture/.config/subspace/config.json");
        assert_eq!(target(file, |_| true), Some(Target::File(file.to_owned())));
        let folder = target(file, |path| path == Path::new("/home/fixture"));
        assert_eq!(folder, Some(Target::Folder("/home/fixture".into())));
        assert_eq!(target(Path::new("config.json"), |_| false), None);
    }

    #[test]
    fn the_label_names_the_platform_file_manager() {
        assert!(label().starts_with("Show in "));
    }
}
