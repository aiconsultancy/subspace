//! Shared data for a disposable workbench, not a Framework API proposal.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Python,
    Node,
    Bun,
    Shell,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSpec {
    pub id: String,
    pub title: String,
    pub runtime: RuntimeKind,
    pub version: String,
    pub package_manager: String,
    pub packages: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub spec: EnvironmentSpec,
    pub ready: bool,
    pub project: PathBuf,
    pub executable: PathBuf,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    pub start: String,
    pub end: String,
    pub currency: String,
    pub service: String,
    pub group_by: String,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            start: "2026-09-01".into(),
            end: "2026-09-30".into(),
            currency: "USD".into(),
            service: "All".into(),
            group_by: "location".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlSpec {
    pub parameter: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlotSeries {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dataset {
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
    pub labels: Vec<String>,
    pub series: Vec<PlotSeries>,
    pub summary: String,
    pub provenance: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub title: String,
    pub environment: String,
    pub recipe: String,
    pub parameters: Parameters,
    pub controls: Vec<ControlSpec>,
    pub view: String,
    #[serde(default)]
    pub snapshot: Option<Dataset>,
    pub revision: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunResult {
    pub dataset: Dataset,
    pub stdout: String,
    pub runtime: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardLayout {
    pub title: String,
    pub mode: String,
    pub order: Vec<String>,
}

impl Default for DashboardLayout {
    fn default() -> Self {
        Self {
            title: "Saved board dashboard".into(),
            mode: "grid".into(),
            order: Vec::new(),
        }
    }
}
