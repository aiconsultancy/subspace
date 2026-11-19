use super::{ComponentKind, InputDeclaration, InputValue, ProjectDefinition};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn action_available(declared: &[String], action: &str, enabled: bool) -> Result<()> {
    ensure!(
        declared.iter().any(|name| name == action),
        "undeclared action"
    );
    ensure!(enabled, "action controls are disabled");
    Ok(())
}

/// Runtime fields apply together. Keys of values/options name inputs; keys of
/// enabled/results name stable component IDs. Empty maps leave state unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct LivePatch {
    #[serde(default)]
    pub values: BTreeMap<String, InputValue>,
    #[serde(default)]
    pub options: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,
    #[serde(default)]
    pub results: BTreeMap<String, super::ResultUpdate>,
}

/// The sole accepted state owner. Rejected updates do not mutate any field.
#[derive(Clone, Debug)]
pub struct LiveState {
    definition: ProjectDefinition,
    session_id: String,
    input_revision: u64,
    values: BTreeMap<String, InputValue>,
    options: BTreeMap<String, Vec<String>>,
    enabled: BTreeMap<String, bool>,
    results: BTreeMap<String, super::ResultUpdate>,
}

impl LiveState {
    pub fn new(definition: ProjectDefinition, session_id: impl Into<String>) -> Result<Self> {
        definition.validate()?;
        let session_id = session_id.into();
        super::validation::token(&session_id)?;
        let values = definition
            .inputs
            .iter()
            .map(|(key, input)| (key.clone(), input.default_value()))
            .collect();
        Ok(Self {
            definition,
            session_id,
            input_revision: 0,
            values,
            options: BTreeMap::new(),
            enabled: BTreeMap::new(),
            results: BTreeMap::new(),
        })
    }

    // qual:allow(dry, boilerplate) reason: "Readonly access preserves the validated definition and atomic live-state invariant; public mutable fields would bypass validation."
    pub fn definition(&self) -> &ProjectDefinition {
        &self.definition
    }
    // qual:allow(dry, boilerplate) reason: "Session identity must not be mutable independently of accepted state and its revision."
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    // qual:allow(dry, boilerplate) reason: "Only validated committed input may advance the revision; callers need readonly inspection."
    pub fn input_revision(&self) -> u64 {
        self.input_revision
    }
    // qual:allow(dry, boilerplate) reason: "Readonly access prevents callers bypassing typed input constraints and revision checks."
    pub fn values(&self) -> &BTreeMap<String, InputValue> {
        &self.values
    }
    // qual:allow(dry, boilerplate) reason: "Readonly results preserve all-or-nothing validated patches and their scope and age."
    pub fn results(&self) -> &BTreeMap<String, super::ResultUpdate> {
        &self.results
    }

    pub fn options(&self, key: &str) -> Option<&[String]> {
        self.options
            .get(key)
            .map(Vec::as_slice)
            .or_else(|| match self.definition.inputs.get(key) {
                Some(InputDeclaration::String { options, .. }) => Some(options.as_slice()),
                _ => None,
            })
    }

    pub fn is_enabled(&self, component_id: &str) -> bool {
        self.definition.component(component_id).is_some()
            && self.enabled.get(component_id).copied().unwrap_or(true)
    }

    /// Explicitly rebind after Start/Restart; retained results keep their scope
    /// and age. No user event or execution is emitted by this model.
    #[cfg(test)]
    pub fn bind_session(&mut self, session_id: impl Into<String>) -> Result<()> {
        let session_id = session_id.into();
        super::validation::token(&session_id)?;
        self.session_id = session_id;
        Ok(())
    }

    pub fn set_value(&mut self, key: &str, value: InputValue, expected: u64) -> Result<u64> {
        ensure!(expected == self.input_revision, "input revision conflict");
        ensure!(
            self.binding_enabled(key, false),
            "input controls are disabled"
        );
        let declaration = self
            .definition
            .inputs
            .get(key)
            .context("undeclared input")?;
        super::input::value(declaration, &value, self.options(key))?;
        if self.values.get(key) != Some(&value) {
            let revision = self
                .input_revision
                .checked_add(1)
                .context("input revision exhausted")?;
            self.values.insert(key.into(), value);
            self.input_revision = revision;
        }
        Ok(self.input_revision)
    }

    pub fn validate_action(&self, action: &str) -> Result<()> {
        super::validation::token(action).context("action")?;
        action_available(
            &self.definition.actions,
            action,
            self.binding_enabled(action, true),
        )
    }

    fn binding_enabled(&self, key: &str, action_binding: bool) -> bool {
        let mut pending: Vec<_> = self.definition.components.iter().collect();
        let mut bound = false;
        while let Some(component) = pending.pop() {
            let matches = match &component.kind {
                ComponentKind::Button { action } => action_binding && action == key,
                ComponentKind::Slider { input } | ComponentKind::Select { input } => {
                    !action_binding && input == key
                }
                _ => false,
            };
            if matches {
                bound = true;
                if self.enabled.get(&component.id).copied().unwrap_or(true) {
                    return true;
                }
            }
            pending.extend(component.children());
        }
        !bound
    }

    /// Programmatic synchronization does not create another committed input
    /// event. The revision tracks host user/tool commits, not worker echoes.
    pub fn apply(&mut self, session_id: &str, input_revision: u64, patch: LivePatch) -> Result<()> {
        ensure!(session_id == self.session_id, "stale runtime session");
        ensure!(
            input_revision == self.input_revision,
            "stale runtime input revision"
        );
        ensure!(
            serde_json::to_vec(&patch)?.len() <= super::limits::FRAME_BYTES,
            "patch exceeds frame budget"
        );
        let mut next = self.clone();
        next.merge(patch);
        next.validate_live()?;
        *self = next;
        Ok(())
    }

    fn merge(&mut self, patch: LivePatch) {
        self.values.extend(patch.values);
        self.options.extend(patch.options);
        self.enabled.extend(patch.enabled);
        self.results.extend(patch.results);
    }

    fn validate_live(&self) -> Result<()> {
        self.validate_values()?;
        self.validate_options()?;
        self.validate_enabled()?;
        self.validate_results()
    }

    fn validate_values(&self) -> Result<()> {
        for (key, value) in &self.values {
            let input = self
                .definition
                .inputs
                .get(key)
                .with_context(|| format!("values.{key}: undeclared input"))?;
            super::input::value(input, value, self.options(key))
                .with_context(|| format!("values.{key}"))?;
        }
        Ok(())
    }

    fn validate_options(&self) -> Result<()> {
        for (key, options) in &self.options {
            ensure!(
                matches!(
                    self.definition.inputs.get(key),
                    Some(InputDeclaration::String { .. })
                ),
                "options.{key}: expected a string input"
            );
            super::input::options_contract(options).with_context(|| format!("options.{key}"))?;
        }
        Ok(())
    }

    fn validate_enabled(&self) -> Result<()> {
        for key in self.enabled.keys() {
            let component = self
                .definition
                .component(key)
                .context("enabled: undeclared component")?;
            ensure!(
                matches!(
                    component.kind,
                    ComponentKind::Slider { .. }
                        | ComponentKind::Select { .. }
                        | ComponentKind::Button { .. }
                ),
                "enabled.{key}: not a control"
            );
        }
        Ok(())
    }

    fn validate_results(&self) -> Result<()> {
        for (key, result) in &self.results {
            let component = self
                .definition
                .component(key)
                .with_context(|| format!("results.{key}: undeclared target"))?;
            super::output::validate(result, &component.kind)
                .with_context(|| format!("results.{key}"))?;
        }
        Ok(())
    }
}
