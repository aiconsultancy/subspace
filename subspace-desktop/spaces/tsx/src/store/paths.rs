//! Where TSX Spaces keeps its files, beside the config file.

use std::path::{Path, PathBuf};

use super::EXTENSION;

/// Where TSX Spaces keeps its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Paths {
    pub(super) root: PathBuf,
}

impl Paths {
    /// `tsx/` beside the config file.
    pub(crate) fn beside(config_file: &Path) -> Self {
        let folder = config_file.parent().unwrap_or_else(|| Path::new(""));
        Self {
            root: folder.join("tsx"),
        }
    }

    pub(super) fn packages(&self) -> PathBuf {
        self.root.join("packages")
    }

    /// Where the installed package of `app_id` is kept.
    pub(crate) fn package_file(&self, app_id: &str) -> PathBuf {
        self.packages().join(format!("{app_id}.{EXTENSION}"))
    }

    pub(super) fn trust(&self) -> PathBuf {
        self.root.join("trust")
    }

    pub(super) fn assets(&self, app_id: &str) -> PathBuf {
        self.root.join("assets").join(app_id)
    }

    pub(super) fn installs(&self) -> PathBuf {
        self.root.join("installs.json")
    }

    /// What Settings shows, written after each verification.
    pub(crate) fn inventory(&self) -> PathBuf {
        self.root.join("inventory.json")
    }

    /// A package's own storage and caches: per app and release key, so a
    /// build signed by another publisher starts with nothing.
    pub(crate) fn data(&self, app_id: &str, key_id: &str) -> PathBuf {
        self.root.join("data").join(app_id).join(key_id)
    }
}
