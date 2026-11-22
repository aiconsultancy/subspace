use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::project::{InputValue, ProjectDefinition, ProjectRevision};
use crate::project_service::InputRevision;

#[derive(Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Read {
    Schemas,
    List,
    Project { id: String },
    Source { id: String, path: String },
    State { id: String },
    SettingsNames { id: String },
}

#[derive(Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Write {
    Create {
        definition: ProjectDefinition,
        files: BTreeMap<String, String>,
    },
    Save {
        definition: ProjectDefinition,
        expected: ProjectRevision,
    },
    EditFile {
        id: String,
        path: String,
        source: String,
        expected: ProjectRevision,
    },
    Apply {
        id: String,
        expected: ProjectRevision,
    },
    Start {
        id: String,
        expected: ProjectRevision,
    },
    Stop {
        id: String,
    },
    Close {
        id: String,
    },
    SetValue {
        id: String,
        key: String,
        value: InputValue,
        expected: InputRevision,
    },
    Invoke {
        id: String,
        name: String,
        expected: InputRevision,
    },
    Export {
        id: String,
        expected: ProjectRevision,
    },
}

#[derive(Clone)]
pub(super) enum Request {
    Read(Read),
    Write(Box<Write>),
}

impl Request {
    pub fn parse(value: Value, write: bool) -> Result<Self, String> {
        if write {
            serde_json::from_value(value).map(Self::Write)
        } else {
            serde_json::from_value(value).map(Self::Read)
        }
        .map_err(|_| "Invalid project action or arguments".into())
    }
}
