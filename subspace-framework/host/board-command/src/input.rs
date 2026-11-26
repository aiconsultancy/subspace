use crate::tree::Route;
use serde_json::{Map, Value, json};
use std::io::Read;
use usage::parse::{ParseOutput, ParseValue};

pub(crate) fn switch(output: &ParseOutput, name: &str) -> bool {
    output
        .flags
        .iter()
        .any(|(flag, value)| flag.name == name && matches!(value, ParseValue::Bool(true)))
}

pub(crate) fn arguments(route: &Route, output: &ParseOutput) -> Result<Value, String> {
    let mut fields = Map::new();
    let mut operations_file = None;
    for (flag, value) in &output.flags {
        let ParseValue::String(raw) = value else {
            continue;
        };
        // Only a batched route declares `--operations-file`; on another it is
        // a product flag of the same name.
        if route.batched && flag.name == "operations-file" {
            operations_file = Some(raw);
            continue;
        }
        // Only the fields the route declares as flags: a product's root
        // flags parse on the same line but are not the tool's input.
        let Some(schema) = route.field(&flag.name) else {
            continue;
        };
        fields.insert(flag.name.clone(), field_value(raw, schema)?);
    }
    if let Some(path) = operations_file {
        if !fields.is_empty() {
            return Err("--operations-file cannot be combined with operation fields.".into());
        }
        return read_batch(path);
    }
    if let Some(action) = &route.action {
        fields.insert("action".into(), json!(action));
    }
    if route.batched {
        Ok(json!({"operations":[fields]}))
    } else {
        Ok(Value::Object(fields))
    }
}

fn field_value(raw: &str, schema: &Value) -> Result<Value, String> {
    if schema["type"] == "string" {
        return Ok(Value::String(raw.into()));
    }
    serde_json::from_str(raw).map_err(|error| format!("Expected a JSON value: {error}"))
}

fn read_batch(path: &str) -> Result<Value, String> {
    const LIMIT: u64 = 1024 * 1024;
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Cannot read operations file: {error}"))?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Operations file exceeds 1 MiB.".into());
    }
    let operations: Vec<Value> =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if operations.is_empty() || operations.len() > subspace_agent_tools::MAX_BATCH {
        return Err("Batch requires 1..20 operations.".into());
    }
    Ok(json!({"operations":operations}))
}
