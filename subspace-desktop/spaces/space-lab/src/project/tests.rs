use super::{
    Component, ComponentKind, InputDeclaration, InputValue, LivePatch, LiveState, ProjectDefinition,
};

pub(super) fn fixture() -> ProjectDefinition {
    let mut definition = ProjectDefinition::new("demo", "python-default", "main.py");
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::Number {
            default: 5.0,
            min: 0.0,
            max: 10.0,
            step: 1.0,
        },
    );
    definition.components.push(Component::new(
        "amount",
        "Amount",
        ComponentKind::Slider {
            input: "amount".into(),
        },
    ));
    definition
}

#[test]
fn parser_rejects_unknown_fields_versions_nonfinite_constraints_and_step_mismatch() {
    let definition = fixture();
    let bytes = serde_json::to_vec(&definition).unwrap();
    assert_eq!(ProjectDefinition::parse(&bytes).unwrap(), definition);
    let mut json = serde_json::to_value(&definition).unwrap();
    json["version"] = 2.into();
    assert!(ProjectDefinition::parse(&serde_json::to_vec(&json).unwrap()).is_err());
    json["version"] = 1.into();
    json["host_callback"] = "run".into();
    assert!(ProjectDefinition::parse(&serde_json::to_vec(&json).unwrap()).is_err());
    let mut invalid = fixture();
    invalid.inputs.insert("amount".into(), numeric(0.5, 10.0));
    assert!(invalid.validate().is_err());
    invalid
        .inputs
        .insert("amount".into(), numeric(0.0, f64::INFINITY));
    assert!(invalid.validate().is_err());
}

fn numeric(default: f64, max: f64) -> InputDeclaration {
    InputDeclaration::Number {
        default,
        min: 0.0,
        max,
        step: 1.0,
    }
}

#[test]
fn enforces_component_and_depth_budgets_at_the_boundary() {
    let mut definition = fixture();
    definition.components = (0..64)
        .map(|ix| {
            Component::new(
                format!("node{ix}"),
                "Text",
                ComponentKind::Text {
                    text: String::new(),
                },
            )
        })
        .collect();
    assert!(definition.validate().is_ok());
    definition
        .components
        .push(Component::new("extra", "Extra", ComponentKind::Chart));
    assert!(definition.validate().is_err());
    let mut node = Component::new("leaf", "Leaf", ComponentKind::Chart);
    for ix in 1..16 {
        node = Component::new(
            format!("level{ix}"),
            "Layout",
            ComponentKind::Column {
                children: vec![node],
            },
        );
    }
    definition.components = vec![node.clone()];
    assert!(definition.validate().is_ok());
    definition.components = vec![Component::new(
        "too-deep",
        "Layout",
        ComponentKind::Row {
            children: vec![node],
        },
    )];
    assert!(definition.validate().is_err());
}

#[test]
fn rejects_invalid_actions_destinations_and_private_source_names() {
    let mut definition = fixture();
    definition.components.push(Component::new(
        "run",
        "Run",
        ComponentKind::Button {
            action: "undeclared".into(),
        },
    ));
    assert!(definition.validate().is_err());
    definition.components.pop();
    for destination in [
        "http://example.com",
        "https://example.com/api",
        "https://user:dummy@example.com",
        "https://example.com?token=test",
    ] {
        definition.destinations = vec![destination.into()];
        assert!(definition.validate().is_err(), "{destination}");
    }
    definition.destinations = vec!["https://example.com:8443".into()];
    assert!(definition.validate().is_ok());
    for path in [
        "/main.py",
        "../main.py",
        "a/../main.py",
        "a//main.py",
        "a\\main.py",
        ".env",
        "nested/.env.py",
        "private/settings.json",
        "node_modules/main.js",
        "dashboard.json",
    ] {
        assert!(super::validate_source_path(path).is_err(), "{path}");
    }
    assert!(super::validate_source_path("src/main.py").is_ok());
    assert!(super::validate_source_path("pnpm-lock.yaml").is_ok());
}

#[test]
fn rejects_duplicate_component_identity_without_replacing_valid_definition() {
    let mut definition = fixture();
    assert!(definition.validate().is_ok());
    definition.components.push(definition.components[0].clone());
    assert!(definition.validate().is_err());
}

#[test]
fn rejects_invalid_defaults_and_wrong_binding_types() {
    let mut definition = fixture();
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::Number {
            default: 11.0,
            min: 0.0,
            max: 10.0,
            step: 1.0,
        },
    );
    assert!(definition.validate().is_err());
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::String {
            default: "one".into(),
            options: vec!["one".into()],
        },
    );
    assert!(definition.validate().is_err());
}

#[test]
fn rejects_source_escape_and_undeclared_entrypoint() {
    let mut definition = fixture();
    definition.files = vec!["../private/value".into()];
    assert!(definition.validate().is_err());
    definition.files = vec!["other.py".into()];
    assert!(definition.validate().is_err());
}

#[test]
fn disabled_controls_reject_committed_changes_and_actions() {
    let mut definition = fixture();
    definition.actions.push("refresh".into());
    definition.components.push(Component::new(
        "refresh",
        "Refresh",
        ComponentKind::Button {
            action: "refresh".into(),
        },
    ));
    let mut state = LiveState::new(definition, "session-one").unwrap();
    let mut patch = LivePatch::default();
    patch.enabled.insert("amount".into(), false);
    patch.enabled.insert("refresh".into(), false);
    state.apply("session-one", 0, patch).unwrap();
    assert!(
        state
            .set_value("amount", InputValue::Number(7.0), 0)
            .is_err()
    );
    assert!(state.validate_action("refresh").is_err());
    assert_eq!(state.values()["amount"], InputValue::Number(5.0));
}
