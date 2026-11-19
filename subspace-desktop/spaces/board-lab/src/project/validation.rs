use super::{Component, ComponentKind, InputDeclaration, ProjectDefinition, limits};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn metadata(definition: &ProjectDefinition) -> Result<()> {
    ensure!(
        definition.version == 1,
        "version: only version 1 is supported"
    );
    token(&definition.id).context("id")?;
    token(&definition.environment).context("environment")?;
    text(&definition.title, limits::LABEL_BYTES).context("title")?;
    tokens(&definition.actions, limits::ACTIONS).context("actions")?;
    ensure!(
        definition.environment_keys.len() <= 64,
        "environment_keys: limit is 64"
    );
    unique(&definition.environment_keys).context("environment_keys")?;
    for name in &definition.environment_keys {
        super::input::environment_key(name)?;
    }
    ensure!(
        definition.destinations.len() <= 32,
        "destinations: limit is 32"
    );
    unique(&definition.destinations).context("destinations")?;
    for origin in &definition.destinations {
        destination(origin)?;
    }
    ensure!(
        serde_json::to_vec(definition)?.len() <= limits::DEFINITION_BYTES,
        "definition exceeds 256 KiB"
    );
    Ok(())
}

pub(super) fn sources(definition: &ProjectDefinition) -> Result<()> {
    ensure!(
        !definition.files.is_empty() && definition.files.len() <= limits::SOURCE_FILES,
        "files: expected 1 to 32 source files"
    );
    unique(&definition.files).context("files")?;
    for path in &definition.files {
        super::source::validate_source_path(path)?;
    }
    ensure!(
        definition.files.contains(&definition.entrypoint),
        "entrypoint: must be a declared source file"
    );
    Ok(())
}

pub(super) fn inputs(inputs: &BTreeMap<String, InputDeclaration>) -> Result<()> {
    ensure!(inputs.len() <= limits::INPUTS, "inputs: limit is 64");
    for (key, declaration) in inputs {
        token(key).context("input name")?;
        super::input::declaration(declaration).with_context(|| format!("inputs.{key}"))?;
    }
    Ok(())
}

pub(super) fn tree(
    components: &[Component],
    inputs: &BTreeMap<String, InputDeclaration>,
    actions: &[String],
) -> Result<()> {
    ensure!(!components.is_empty(), "components: must not be empty");
    let mut pending: Vec<_> = components.iter().map(|node| (node, 1)).collect();
    let mut ids = BTreeSet::new();
    while let Some((component, depth)) = pending.pop() {
        ensure!(depth <= limits::DEPTH, "components: maximum depth is 16");
        ensure!(
            ids.insert(&component.id),
            "components.{}: duplicate id",
            component.id
        );
        ensure!(
            ids.len() <= limits::COMPONENTS,
            "components: maximum count is 64"
        );
        component_contract(component, inputs, actions)?;
        pending.extend(component.children().iter().map(|child| (child, depth + 1)));
    }
    Ok(())
}

fn component_contract(
    component: &Component,
    inputs: &BTreeMap<String, InputDeclaration>,
    actions: &[String],
) -> Result<()> {
    token(&component.id).context("component id")?;
    text(&component.label, limits::LABEL_BYTES)
        .with_context(|| format!("components.{}.label", component.id))?;
    match &component.kind {
        ComponentKind::Slider { input } => ensure!(
            matches!(inputs.get(input), Some(InputDeclaration::Number { .. })),
            "{}.input: slider requires a declared number",
            component.id
        ),
        ComponentKind::Select { input } => ensure!(
            matches!(inputs.get(input), Some(InputDeclaration::String { .. })),
            "{}.input: select requires a declared string",
            component.id
        ),
        ComponentKind::Button { action } => ensure!(
            actions.contains(action),
            "{}.action: undeclared action",
            component.id
        ),
        ComponentKind::Grid { columns, .. } => ensure!(
            (1..=12).contains(columns),
            "{}.columns: expected 1 to 12",
            component.id
        ),
        ComponentKind::Text { text } => ensure!(
            text.len() <= limits::TEXT_BYTES,
            "{}.text: exceeds 16 KiB",
            component.id
        ),
        _ => {}
    }
    Ok(())
}

pub(super) fn token(value: &str) -> Result<()> {
    ensure!(
        (1..=64).contains(&value.len())
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
        "expected a safe token (1 to 64 ASCII characters, letter first)"
    );
    Ok(())
}

fn text(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control),
        "expected nonempty text up to {max} bytes without control characters"
    );
    Ok(())
}

fn tokens(values: &[String], limit: usize) -> Result<()> {
    ensure!(values.len() <= limit, "maximum count is {limit}");
    unique(values)?;
    values.iter().try_for_each(|value| token(value))
}

pub(super) fn unique(values: &[String]) -> Result<()> {
    let unique: BTreeSet<_> = values.iter().collect();
    ensure!(
        unique.len() == values.len(),
        "duplicate values are not allowed"
    );
    Ok(())
}

fn destination(origin: &str) -> Result<()> {
    let url = url::Url::parse(origin).context("destination: invalid URL")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "destination: expected an HTTPS origin without credentials, path, query or fragment"
    );
    Ok(())
}
