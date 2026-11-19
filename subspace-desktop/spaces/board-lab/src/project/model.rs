use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Shareable definition. Environment is a configured exact-pin environment ID,
/// not a runtime range or an implicit permission grant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ProjectDefinition {
    pub version: u32,
    pub id: String,
    pub title: String,
    pub environment: String,
    pub entrypoint: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, InputDeclaration>,
    pub components: Vec<Component>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub environment_keys: Vec<String>,
    #[serde(default)]
    pub destinations: Vec<String>,
    pub files: Vec<String>,
}

impl ProjectDefinition {
    pub fn new(
        id: impl Into<String>,
        environment: impl Into<String>,
        entrypoint: impl Into<String>,
    ) -> Self {
        let id = id.into();
        let entrypoint = entrypoint.into();
        Self {
            version: 1,
            title: id.clone(),
            id,
            environment: environment.into(),
            files: vec![entrypoint.clone()],
            entrypoint,
            inputs: BTreeMap::new(),
            components: vec![Component::new(
                "heading",
                "Project",
                ComponentKind::Text {
                    text: "New project".into(),
                },
            )],
            actions: Vec::new(),
            environment_keys: Vec::new(),
            destinations: Vec::new(),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= super::limits::DEFINITION_BYTES,
            "definition exceeds 256 KiB"
        );
        let definition: Self = serde_json::from_slice(bytes).context("dashboard.json")?;
        definition.validate()?;
        Ok(definition)
    }

    pub fn validate(&self) -> Result<()> {
        super::validation::metadata(self)?;
        super::validation::sources(self)?;
        super::validation::inputs(&self.inputs)?;
        super::validation::tree(&self.components, &self.inputs, &self.actions)
    }

    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components
            .iter()
            .find_map(|component| component.find(id))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputValue {
    Number(f64),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputDeclaration {
    Number {
        default: f64,
        min: f64,
        max: f64,
        step: f64,
    },
    String {
        default: String,
        options: Vec<String>,
    },
}

impl InputDeclaration {
    pub fn default_value(&self) -> InputValue {
        match self {
            Self::Number { default, .. } => InputValue::Number(*default),
            Self::String { default, .. } => InputValue::String(default.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Component {
    pub id: String,
    pub label: String,
    pub kind: ComponentKind,
}

impl Component {
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: ComponentKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
        }
    }

    pub fn children(&self) -> &[Self] {
        match &self.kind {
            ComponentKind::Row { children }
            | ComponentKind::Column { children }
            | ComponentKind::Grid { children, .. } => children,
            _ => &[],
        }
    }

    fn find(&self, id: &str) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children().iter().find_map(|child| child.find(id))
    }
}

/// Output targets are the IDs of text/chart/table components; input bindings
/// are input-map keys. Controls emit committed changes, never executable code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentKind {
    Row {
        children: Vec<Component>,
    },
    Column {
        children: Vec<Component>,
    },
    Grid {
        columns: u8,
        children: Vec<Component>,
    },
    Text {
        text: String,
    },
    Slider {
        input: String,
    },
    Select {
        input: String,
    },
    Button {
        action: String,
    },
    Chart,
    Table,
}
