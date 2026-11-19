use std::collections::BTreeMap;

use serde_yaml_ng::{Mapping, Value};

use super::{
    Definition, DispatchDefinition, DispatchInput, DispatchInputType, Job, JobDependency,
    WorkflowDefinition, WorkflowJob,
};

pub const MAX_WORKFLOW_BYTES: usize = 1024 * 1024;

pub fn parse_workflow_definition(text: &str) -> Definition<WorkflowDefinition> {
    if text.len() > MAX_WORKFLOW_BYTES {
        return unreadable("workflow file exceeds the 1 MB limit");
    }
    let root: Value = match serde_yaml_ng::from_str(text) {
        Ok(value) => value,
        Err(error) => return unreadable(format!("invalid workflow YAML: {error}")),
    };
    let Some(root) = root.as_mapping() else {
        return unreadable("workflow YAML root is not a mapping");
    };
    let dispatch = match parse_dispatch(root) {
        Ok(value) => value,
        Err(reason) => return unreadable(reason),
    };
    let jobs = match parse_jobs(root) {
        Ok(value) => value,
        Err(reason) => return unreadable(reason),
    };
    Definition::Readable(WorkflowDefinition { dispatch, jobs })
}

fn parse_dispatch(root: &Mapping) -> Result<Option<DispatchDefinition>, String> {
    let Some(on) = get(root, "on") else {
        return Ok(None);
    };
    if scalar_is(on, "workflow_dispatch") {
        return Ok(Some(DispatchDefinition::default()));
    }
    let Some(on) = on.as_mapping() else {
        return Ok(None);
    };
    let Some(dispatch) = get(on, "workflow_dispatch") else {
        return Ok(None);
    };
    if dispatch.is_null() {
        return Ok(Some(DispatchDefinition::default()));
    }
    let map = dispatch
        .as_mapping()
        .ok_or_else(|| "workflow_dispatch is not a mapping".to_owned())?;
    let inputs = match get(map, "inputs") {
        None => Vec::new(),
        Some(value) => parse_inputs(value)?,
    };
    Ok(Some(DispatchDefinition { inputs }))
}

fn parse_inputs(value: &Value) -> Result<Vec<DispatchInput>, String> {
    let inputs = value
        .as_mapping()
        .ok_or_else(|| "workflow_dispatch.inputs is not a mapping".to_owned())?;
    inputs
        .iter()
        .map(|(name, definition)| {
            let name = string(name, "dispatch input name")?;
            let fields = definition
                .as_mapping()
                .ok_or_else(|| format!("dispatch input {name} is not a mapping"))?;
            let kind = input_type(fields, &name)?;
            let required = bool_field(fields, "required")?.unwrap_or(false);
            let default = get(fields, "default").map(scalar_string).transpose()?;
            let description = get(fields, "description").map(scalar_string).transpose()?;
            let options = parse_options(fields, &name)?;
            if kind == DispatchInputType::Choice && options.is_empty() {
                return Err(format!("choice input {name} has no options"));
            }
            Ok(DispatchInput {
                name,
                kind,
                required,
                default,
                options,
                description,
            })
        })
        .collect()
}

fn input_type(fields: &Mapping, name: &str) -> Result<DispatchInputType, String> {
    match get(fields, "type")
        .map(|value| string(value, "input type"))
        .transpose()?
        .as_deref()
    {
        None | Some("string") => Ok(DispatchInputType::String),
        Some("boolean") => Ok(DispatchInputType::Boolean),
        Some("choice") => Ok(DispatchInputType::Choice),
        Some("environment") => Ok(DispatchInputType::Environment),
        Some(other) => Err(format!(
            "dispatch input {name} has unsupported type {other}"
        )),
    }
}

fn parse_options(fields: &Mapping, name: &str) -> Result<Vec<String>, String> {
    let Some(value) = get(fields, "options") else {
        return Ok(Vec::new());
    };
    value
        .as_sequence()
        .ok_or_else(|| format!("choice input {name} options are not a list"))?
        .iter()
        .map(scalar_string)
        .collect()
}

fn parse_jobs(root: &Mapping) -> Result<Vec<WorkflowJob>, String> {
    let Some(value) = get(root, "jobs") else {
        return Err("workflow has no jobs mapping".into());
    };
    let jobs = value
        .as_mapping()
        .ok_or_else(|| "workflow jobs is not a mapping".to_owned())?;
    jobs.iter()
        .map(|(key, definition)| {
            let key = string(key, "job key")?;
            let fields = definition
                .as_mapping()
                .ok_or_else(|| format!("job {key} is not a mapping"))?;
            let name = get(fields, "name").map(scalar_string).transpose()?;
            let needs = get(fields, "needs")
                .map(parse_needs)
                .transpose()?
                .unwrap_or_default();
            Ok(WorkflowJob { key, name, needs })
        })
        .collect()
}

fn parse_needs(value: &Value) -> Result<Vec<String>, String> {
    if let Some(value) = value.as_str() {
        return Ok(vec![value.to_owned()]);
    }
    value
        .as_sequence()
        .ok_or_else(|| "job needs is neither a string nor a list".to_owned())?
        .iter()
        .map(|value| string(value, "job need"))
        .collect()
}

// qual:api
pub fn match_job_dependencies(
    jobs: &[Job],
    definitions: &[WorkflowJob],
) -> Definition<Vec<JobDependency>> {
    let mut matched = Vec::with_capacity(jobs.len());
    for job in jobs {
        let candidates = matching_definitions(&job.name, definitions);
        let [definition] = candidates.as_slice() else {
            let reason = if candidates.is_empty() {
                "no match"
            } else {
                "ambiguous match"
            };
            return unreadable(format!("{reason} for API job {:?}", job.name));
        };
        matched.push((job, *definition));
    }
    let by_key = jobs_by_key(&matched);
    let dependencies = matched
        .into_iter()
        .map(|(job, definition)| JobDependency {
            job_id: job.id,
            job_key: definition.key.clone(),
            needs: definition
                .needs
                .iter()
                .flat_map(|key| by_key.get(key).into_iter().flatten().copied())
                .collect(),
        })
        .collect();
    Definition::Readable(dependencies)
}

fn matching_definitions<'a>(name: &str, jobs: &'a [WorkflowJob]) -> Vec<&'a WorkflowJob> {
    let exact: Vec<_> = jobs
        .iter()
        .filter(|job| job.key == name || job.name.as_deref() == Some(name))
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    jobs.iter()
        .filter(|job| {
            let base = job.name.as_deref().unwrap_or(&job.key);
            name.strip_prefix(base)
                .is_some_and(|suffix| suffix.starts_with(" (") && suffix.ends_with(')'))
        })
        .collect()
}

fn jobs_by_key(matched: &[(&Job, &WorkflowJob)]) -> BTreeMap<String, Vec<u64>> {
    let mut result = BTreeMap::<String, Vec<u64>>::new();
    for (job, definition) in matched {
        result
            .entry(definition.key.clone())
            .or_default()
            .push(job.id);
    }
    result
}

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.to_owned()))
}

fn string(value: &Value, what: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} is not a string"))
}

fn scalar_string(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err("workflow input value is not a scalar".into()),
    }
}

fn bool_field(map: &Mapping, key: &str) -> Result<Option<bool>, String> {
    get(map, key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("{key} is not a boolean"))
        })
        .transpose()
}

fn scalar_is(value: &Value, expected: &str) -> bool {
    value.as_str() == Some(expected)
        || value
            .as_sequence()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}

fn unreadable<T>(reason: impl Into<String>) -> Definition<T> {
    Definition::Unreadable(reason.into())
}
