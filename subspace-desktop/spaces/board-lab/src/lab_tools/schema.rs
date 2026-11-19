use serde::Deserialize;
use serde_json::json;
use subspace_framework::agent_tools::Arguments;

use crate::contracts::{DashboardLayout, Dataset, EnvironmentSpec, Parameters, Preset};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub action: String,
    pub id: Option<String>,
    pub profile: Option<String>,
    pub preset: Option<Preset>,
    pub parameter: Option<String>,
    pub parameters: Option<Parameters>,
    pub dataset: Option<Dataset>,
    pub environment: Option<String>,
    pub version: Option<String>,
    pub package: Option<String>,
    pub source: Option<String>,
    pub input: Option<serde_json::Value>,
    pub canvas: Option<serde_json::Value>,
    pub title: Option<String>,
    pub destinations: Option<Vec<String>>,
    pub network: Option<bool>,
    pub tool: Option<String>,
    pub services: Option<Vec<String>>,
    pub layout: Option<DashboardLayout>,
    pub environment_spec: Option<EnvironmentSpec>,
}

pub(super) fn arguments(actions: &str) -> Arguments {
    use subspace_framework::agent_tools::schema::{field, single};
    single(vec![
        field(
            "action",
            json!({"type":"string", "enum": actions.split(", ").collect::<Vec<_>>() }),
            true,
        ),
        field("id", json!({"type":"string", "maxLength":100}), false),
        field(
            "profile",
            json!({"type":"string", "maxLength":160,"description":"market, services, typescript or an immutable custom recipe reference"}),
            false,
        ),
        field("preset", json!({"type":"object"}), false),
        field(
            "parameter",
            json!({"type":"string","enum":["date_range","start","end","currency","service","group_by"]}),
            false,
        ),
        field("parameters", json!({"type":"object"}), false),
        field("dataset", json!({"type":"object"}), false),
        field(
            "environment",
            json!({"type":"string", "maxLength":100}),
            false,
        ),
        field("version", json!({"type":"string", "maxLength":80}), false),
        field("package", json!({"type":"string", "maxLength":160}), false),
        field("source", json!({"type":"string", "maxLength":32768}), false),
        field("input", json!({"type":"object"}), false),
        field("canvas", json!({"type":"object"}), false),
        field("title", json!({"type":"string", "maxLength":160}), false),
        field(
            "destinations",
            json!({"type":"array","maxItems":8,"items":{"type":"string","maxLength":253}}),
            false,
        ),
        field("network", json!({"type":"boolean", "default":false}), false),
        field("tool", json!({"type":"string", "maxLength":160}), false),
        field(
            "services",
            json!({"type":"array","maxItems":16,"items":{"type":"string","maxLength":80}}),
            false,
        ),
        field("layout", json!({"type":"object"}), false),
        field(
            "environment_spec",
            json!({"type":"object","additionalProperties":false,"required":["id","title","runtime","version","package_manager","packages"],"properties":{
                "id":{"type":"string","maxLength":48},"title":{"type":"string","maxLength":120},
                "runtime":{"type":"string","enum":["python","node","bun","shell"]},"version":{"type":"string"},
                "package_manager":{"type":"string","enum":["uv","pnpm","bun","mise"]},
                "packages":{"type":"array","maxItems":32,"items":{"type":"string"}},
                "tools":{"type":"array","description":"Initially empty; add CLI tools using add_tool","items":{"type":"string"}}
            }}),
            false,
        ),
    ])
}
