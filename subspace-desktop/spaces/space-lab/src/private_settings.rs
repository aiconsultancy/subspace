//! Private per-project environment values; never included in project source.

mod disk;
mod readback;
mod redaction;

use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use subspace_framework::core::Secret;

pub struct PrivateSettings {
    directory: PathBuf,
}

#[derive(Debug)]
pub enum Edit {
    Keep,
    Replace(Secret),
    Clear,
}

#[derive(Debug)]
pub struct Metadata {
    revision: u64,
    names: Vec<String>,
}

impl Metadata {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

pub struct Selection(BTreeMap<String, String>);

impl fmt::Debug for Selection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Selection")
            .field("names", &self.0.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Selection {
    pub fn into_values(self) -> BTreeMap<String, String> {
        self.0
    }
}

pub struct ImportPreview {
    values: BTreeMap<String, String>,
    names: Vec<String>,
    conflicts: Vec<String>,
    revision: u64,
    directory: PathBuf,
}

impl fmt::Debug for ImportPreview {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportPreview")
            .field("names", &self.names)
            .field("conflicts", &self.conflicts)
            .field("revision", &self.revision)
            .finish()
    }
}

impl ImportPreview {
    pub fn names(&self) -> &[String] {
        &self.names
    }
    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }
}

impl PrivateSettings {
    pub fn new(board_root: &Path, project_id: &str) -> Result<Self> {
        ensure!(board_root.is_absolute(), "board root must be absolute");
        ensure!(
            !project_id.is_empty()
                && project_id.len() <= 128
                && project_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "invalid project id"
        );
        let directory = disk::prepare(board_root, project_id)?;
        Ok(Self { directory })
    }

    pub fn metadata(&self) -> Result<Metadata> {
        let _lock = disk::lock(&self.directory)?;
        Ok(disk::read(&self.directory)?.metadata())
    }

    pub fn update(
        &self,
        expected_revision: u64,
        edits: BTreeMap<String, Edit>,
    ) -> Result<Metadata> {
        let _lock = disk::lock(&self.directory)?;
        let mut record = disk::read(&self.directory)?;
        ensure!(
            record.revision == expected_revision,
            "private settings revision conflict"
        );
        for (name, edit) in edits {
            mxc_probe::validate_environment_name(&name)?;
            match edit {
                Edit::Keep => {}
                Edit::Clear => {
                    record.values.remove(&name);
                }
                Edit::Replace(value) => {
                    record.values.insert(name, value.expose().to_owned());
                }
            }
        }
        validate(&record.values)?;
        record.revision = record
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("private settings revision exhausted"))?;
        disk::write(&self.directory, &record)?;
        Ok(record.metadata())
    }

    pub fn selected(&self, names: &[&str]) -> Result<Selection> {
        let _lock = disk::lock(&self.directory)?;
        let record = disk::read(&self.directory)?;
        let mut selected = BTreeMap::new();
        ensure!(
            names.len() <= crate::dotenv::MAX_VARIABLES,
            "too many selected names"
        );
        for name in names {
            mxc_probe::validate_environment_name(name)?;
            let value = record
                .values
                .get(*name)
                .ok_or_else(|| anyhow::anyhow!("selected variable not found"))?;
            ensure!(
                selected.insert((*name).to_owned(), value.clone()).is_none(),
                "duplicate selected name"
            );
        }
        validate(&selected)?;
        Ok(Selection(selected))
    }

    pub fn preview_import(&self, source: &str) -> Result<ImportPreview> {
        let values = crate::dotenv::parse(source)?.into_values();
        validate(&values)?;
        let _lock = disk::lock(&self.directory)?;
        let record = disk::read(&self.directory)?;
        let names = values.keys().cloned().collect::<Vec<_>>();
        let conflicts = names
            .iter()
            .filter(|name| record.values.contains_key(*name))
            .cloned()
            .collect();
        Ok(ImportPreview {
            values,
            names,
            conflicts,
            revision: record.revision,
            directory: self.directory.clone(),
        })
    }

    pub fn apply_import(
        &self,
        expected_revision: u64,
        preview: ImportPreview,
        approved_replacements: &[&str],
    ) -> Result<Metadata> {
        ensure!(
            preview.directory == self.directory,
            "import preview belongs to another project"
        );
        ensure!(
            preview.revision == expected_revision,
            "import preview is stale"
        );
        let approved: BTreeSet<&str> = approved_replacements.iter().copied().collect();
        ensure!(
            approved.len() == approved_replacements.len()
                && approved == preview.conflicts.iter().map(String::as_str).collect(),
            "import conflicts require explicit approval"
        );
        let edits = preview
            .values
            .into_iter()
            .map(|(name, value)| (name, Edit::Replace(Secret::new(value))))
            .collect();
        self.update(expected_revision, edits)
    }
}

fn validate(values: &BTreeMap<String, String>) -> Result<()> {
    ensure!(
        values.len() <= crate::dotenv::MAX_VARIABLES,
        "too many environment values"
    );
    let mut total = 0usize;
    for (name, value) in values {
        mxc_probe::validate_environment_name(name)?;
        ensure!(
            value.len() <= crate::dotenv::MAX_VALUE_BYTES && !value.contains('\0'),
            "invalid environment value"
        );
        total = total.saturating_add(name.len()).saturating_add(value.len());
    }
    ensure!(
        total <= crate::dotenv::MAX_INPUT_BYTES,
        "environment values exceed total limit"
    );
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "private_settings/tests.rs"]
mod tests;

#[cfg(all(test, not(unix)))]
mod unsupported_tests {
    use super::PrivateSettings;

    #[test]
    fn private_store_fails_closed_when_acl_support_is_unavailable() {
        let root = tempfile::tempdir().unwrap();
        assert!(PrivateSettings::new(root.path(), "project").is_err());
    }
}
