//! A native board's own settings file ([`crate::config::board_file`]),
//! edited inside the Settings transaction. Its top-level object is the
//! board's settings; keys a save does not set stay as they are.

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde_json::{Map, Value};

use super::{SettingsError, read_existing};
use crate::state;

#[cfg(unix)]
const PRIVATE_DIR_MODE: u32 = 0o700;

pub(super) struct BoardFile {
    file: PathBuf,
    original: Option<String>,
    root: Map<String, Value>,
}

impl BoardFile {
    pub(super) fn open(file: PathBuf) -> Result<Self, SettingsError> {
        let original = read_existing(&file)?;
        let root = match original.as_deref().map(serde_json::from_str).transpose()? {
            None => Map::new(),
            Some(Value::Object(root)) => root,
            Some(_) => return Err(SettingsError::Invalid),
        };
        Ok(Self {
            file,
            original,
            root,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.file
    }

    pub(super) fn values(&self) -> &Map<String, Value> {
        &self.root
    }

    /// The file's object with `changes` set, unless the file changed since
    /// it was opened.
    pub(super) fn edited(
        &self,
        changes: &Map<String, Value>,
    ) -> Result<Map<String, Value>, SettingsError> {
        if read_existing(&self.file)? != self.original {
            return Err(SettingsError::Stale);
        }
        let mut root = self.root.clone();
        root.extend(changes.clone());
        Ok(root)
    }

    /// Writes `root` owner-only, in an owner-only folder.
    pub(super) fn write(&mut self, root: Map<String, Value>) -> Result<(), SettingsError> {
        let text = serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n";
        if let Some(dir) = self.file.parent() {
            create_private_dir(dir)?;
        }
        state::write_atomic(&self.file, &text)?;
        self.original = Some(text);
        self.root = root;
        Ok(())
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        builder.mode(PRIVATE_DIR_MODE);
        builder.create(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(PRIVATE_DIR_MODE))
    }
    #[cfg(not(unix))]
    builder.create(dir)
}
