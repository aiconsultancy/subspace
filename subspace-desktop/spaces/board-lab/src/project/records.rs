use super::ProjectDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ProjectRevision {
    pub definition: String,
    pub source: String,
}

impl ProjectRevision {
    pub fn new(definition: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            definition: definition.into(),
            source: source.into(),
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LoadedProject {
    pub definition: ProjectDefinition,
    pub revision: ProjectRevision,
    pub files: BTreeMap<String, Vec<u8>>,
    pub directory: PathBuf,
}

impl LoadedProject {
    pub fn new(
        definition: ProjectDefinition,
        revision: ProjectRevision,
        files: BTreeMap<String, Vec<u8>>,
        directory: PathBuf,
    ) -> Self {
        Self {
            definition,
            revision,
            files,
            directory,
        }
    }
}
