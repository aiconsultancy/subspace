use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};

use super::ProjectService;
use crate::project::{LoadedProject, ProjectDefinition, ProjectRevision};

impl ProjectService {
    pub fn root(&self) -> &Path {
        self.store.root()
    }

    pub fn list(&self) -> Result<Vec<LoadedProject>> {
        self.store.list()
    }

    pub fn create(
        &self,
        definition: &ProjectDefinition,
        files: BTreeMap<String, Vec<u8>>,
    ) -> Result<LoadedProject> {
        self.store.create(definition, files)
    }

    pub fn load(&self, id: &str) -> Result<LoadedProject> {
        self.store.load(id)
    }

    pub fn save(
        &self,
        definition: &ProjectDefinition,
        expected: &ProjectRevision,
    ) -> Result<LoadedProject> {
        self.change_source(&definition.id, || self.store.save(definition, expected))
    }

    pub fn edit_file(
        &self,
        id: &str,
        path: &str,
        bytes: &[u8],
        expected: &ProjectRevision,
    ) -> Result<LoadedProject> {
        self.change_source(id, || self.store.edit_file(id, path, bytes, expected))
    }

    pub fn export(&self, id: &str, expected: &ProjectRevision, output: &Path) -> Result<()> {
        let snapshot = self.store.snapshot(id, expected)?;
        let settings = crate::private_settings::PrivateSettings::new(self.root(), id)?;
        let metadata = settings.metadata()?;
        let names = metadata
            .names()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let values = settings
            .selected(&names)?
            .into_values()
            .into_values()
            .collect::<Vec<_>>();
        let mut files = snapshot.definition().files.clone();
        files.insert(0, "dashboard.json".into());
        crate::project_export::export_project(snapshot.directory(), &files, output, &values)
            .context("Cannot export this project")
    }
}
