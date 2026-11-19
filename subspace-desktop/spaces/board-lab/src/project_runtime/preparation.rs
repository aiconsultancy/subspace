use super::{PreparedSession, proxy::DestinationPolicy};
use crate::environments::{EnvironmentManager, persistent};
use crate::project::{InputValue, LivePatch, LiveState, ProjectDefinition, SourceSnapshot};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

pub(super) fn prepare(
    manager: &EnvironmentManager,
    snapshot: SourceSnapshot,
    selected: BTreeMap<String, String>,
) -> Result<PreparedSession> {
    validate_selected(snapshot.definition(), &selected)?;
    let policy = DestinationPolicy::new(&snapshot.definition().destinations)?;
    let mut runtime = persistent::prepare(&manager.root, &snapshot)?;
    runtime.request.environment = selected.into_iter().collect();
    let values = validated_values(snapshot.definition(), BTreeMap::new())?;
    Ok(PreparedSession {
        snapshot,
        runtime,
        policy,
        values,
    })
}

pub(super) fn validated_values(
    definition: &ProjectDefinition,
    values: BTreeMap<String, InputValue>,
) -> Result<BTreeMap<String, InputValue>> {
    let mut live = LiveState::new(definition.clone(), "preparing-session")?;
    let patch = LivePatch {
        values,
        ..LivePatch::default()
    };
    live.apply("preparing-session", 0, patch)?;
    Ok(live.values().clone())
}

fn validate_selected(
    definition: &ProjectDefinition,
    selected: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!(
        selected.len() <= 64,
        "selected environment exceeds 64 names"
    );
    let mut total = 0usize;
    for (name, value) in selected {
        mxc_probe::validate_environment_name(name)?;
        ensure!(
            definition.environment_keys.contains(name),
            "selected environment name is not declared by project"
        );
        ensure!(
            value.len() <= 16 * 1024 && !value.contains('\0'),
            "invalid selected environment value"
        );
        total = total.saturating_add(name.len()).saturating_add(value.len());
    }
    ensure!(total <= 128 * 1024, "selected environment exceeds 128 KiB");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_selected, validated_values};
    use crate::project::{InputDeclaration, InputValue, ProjectDefinition};
    use std::collections::BTreeMap;

    #[test]
    fn project_runtime_initial_values_merge_defaults_and_reject_unknown_or_invalid_values() {
        let mut definition = ProjectDefinition::new("fixture", "python-data", "main.py");
        definition.inputs.insert(
            "amount".into(),
            InputDeclaration::Number {
                default: 2.0,
                min: 0.0,
                max: 10.0,
                step: 1.0,
            },
        );
        definition.inputs.insert(
            "choice".into(),
            InputDeclaration::String {
                default: "first".into(),
                options: vec!["first".into()],
            },
        );
        let values = validated_values(
            &definition,
            BTreeMap::from([("amount".into(), InputValue::Number(8.0))]),
        )
        .unwrap();
        assert_eq!(values["amount"], InputValue::Number(8.0));
        assert_eq!(values["choice"], InputValue::String("first".into()));
        assert!(
            validated_values(
                &definition,
                BTreeMap::from([("unknown".into(), InputValue::Number(1.0))])
            )
            .is_err()
        );
        assert!(
            validated_values(
                &definition,
                BTreeMap::from([("amount".into(), InputValue::String("wrong".into()))])
            )
            .is_err()
        );
    }

    #[test]
    fn project_runtime_selected_environment_is_declared_reserved_safe_and_bounded_without_value_readback()
     {
        let mut definition = ProjectDefinition::new("fixture", "python-data", "main.py");
        definition.environment_keys.push("SYNTHETIC_TOKEN".into());
        let selected = BTreeMap::from([("SYNTHETIC_TOKEN".into(), "dummy-test-only-value".into())]);
        assert!(validate_selected(&definition, &selected).is_ok());
        assert!(
            validate_selected(
                &definition,
                &BTreeMap::from([("UNDECLARED".into(), "dummy".into())])
            )
            .is_err()
        );
        assert!(
            validate_selected(
                &definition,
                &BTreeMap::from([("PYTHONPATH".into(), "dummy".into())])
            )
            .is_err()
        );
        let error = validate_selected(
            &definition,
            &BTreeMap::from([("SYNTHETIC_TOKEN".into(), "z".repeat(16 * 1024 + 1))]),
        )
        .unwrap_err();
        assert!(!error.to_string().contains(&"z".repeat(100)));
    }
}
